use std::time::Duration;

use anyhow::Result;
use async_stream::try_stream;
use async_trait::async_trait;
use futures::TryStreamExt;
use reqwest::Client;
use serde_json::{Value, json, to_string_pretty};
use tokio_stream::StreamExt;
use tokio_util::{
    codec::{FramedRead, LinesCodec},
    io::StreamReader,
};
use tracing::{debug, error, trace};

use crate::ai::{
    provider::{CompletionResponse, MessageStream, Provider, ProviderError},
    types::{FunctionCall, Message, MessageRole, ToolCall, Usage},
};

const OPENROUTER_API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

pub struct OpenRouterProvider {
    client: Client,
    model: String,
}

impl OpenRouterProvider {
    pub fn new(model: impl Into<String>, api_key: impl Into<String>) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        let api_key = api_key.into();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&format!("Bearer {}", api_key))?,
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .default_headers(headers)
            .build()?;

        Ok(Self {
            client,
            model: model.into(),
        })
    }

    fn create_request_payload(
        &self,
        system_prompt: &str,
        messages: &[Message],
        tools: Option<&[rmcp::model::Tool]>,
        stream: bool,
    ) -> Value {
        let mut chat_messages = Vec::new();

        if !system_prompt.is_empty() {
            chat_messages.push(json!({
                "role": "system",
                "content": system_prompt
            }));
        }

        for msg in messages {
            match msg.role {
                MessageRole::System => {
                    if let Some(content) = &msg.content {
                        chat_messages.push(json!({
                            "role": "system",
                            "content": content
                        }));
                    }
                }
                MessageRole::User => {
                    if let Some(content) = &msg.content {
                        chat_messages.push(json!({
                            "role": "user",
                            "content": content
                        }));
                    }
                }
                MessageRole::Assistant => {
                    let mut assistant_msg = json!({
                        "role": "assistant"
                    });
                    if let Some(content) = &msg.content {
                        assistant_msg["content"] = json!(content);
                    }
                    if let Some(tool_calls) = &msg.tool_calls {
                        let tc: Vec<Value> = tool_calls
                            .iter()
                            .map(|tc| {
                                json!({
                                    "id": tc.id,
                                    "type": "function",
                                    "function": {
                                        "name": tc.function.name,
                                        "arguments": tc.function.arguments
                                    }
                                })
                            })
                            .collect();
                        assistant_msg["tool_calls"] = json!(tc);
                    }
                    chat_messages.push(assistant_msg);
                }
                MessageRole::Tool => {
                    if let Some(content) = &msg.content
                        && let Some(tool_call_id) = &msg.tool_call_id
                    {
                        chat_messages.push(json!({
                            "role": "tool",
                            "tool_call_id": tool_call_id,
                            "content": content
                        }));
                    }
                }
            }
        }

        let mut payload = json!({
            "model": self.model,
            "messages": chat_messages,
            "stream": stream,
        });

        if let Some(tools) = tools
            && !tools.is_empty()
        {
            let openai_tools: Vec<Value> = tools
                .iter()
                .map(|tool| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description.as_deref().unwrap_or(""),
                            "parameters": serde_json::Value::Object(tool.input_schema.as_ref().clone())
                        }
                    })
                })
                .collect();
            payload["tools"] = json!(openai_tools);
            payload["tool_choice"] = json!("auto");
        }

        payload
    }

    fn parse_response(&self, response: &Value) -> Result<CompletionResponse, ProviderError> {
        let choices = response
            .get("choices")
            .and_then(|c| c.as_array())
            .ok_or_else(|| {
                error!(
                    "No choices array in OpenRouter response: {:?}",
                    to_string_pretty(&response)
                );
                ProviderError::InvalidRequest("No choices array in response".to_string())
            })?;

        let first_choice = choices.first().ok_or_else(|| {
            error!(
                "Empty choices array in OpenRouter response: {:?}",
                to_string_pretty(&response)
            );
            ProviderError::InvalidRequest("Empty choices array in response".to_string())
        })?;

        let message_obj = first_choice.get("message").ok_or_else(|| {
            error!(
                "No message in choice in OpenRouter response: {:?}",
                to_string_pretty(&response)
            );
            ProviderError::InvalidRequest("No message in choice".to_string())
        })?;

        let content = message_obj
            .get("content")
            .and_then(|c| c.as_str())
            .map(String::from);

        let tool_calls: Vec<ToolCall> = message_obj
            .get("tool_calls")
            .and_then(|tc| tc.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|tc| {
                        let id = tc.get("id")?.as_str()?;
                        let function = tc.get("function")?;
                        let name = function.get("name")?.as_str()?;
                        let arguments = function.get("arguments")?.as_str()?;
                        Some(ToolCall {
                            id: id.to_string(),
                            tool_type: "function".to_string(),
                            function: FunctionCall {
                                name: name.to_string(),
                                arguments: arguments.to_string(),
                            },
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let message = if !tool_calls.is_empty() {
            Message::assistant_with_tools(content, tool_calls)
        } else {
            Message::assistant(content.unwrap_or_default())
        };

        let usage = response
            .get("usage")
            .map(|u| Usage {
                prompt_tokens: u.get("prompt_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32,
                completion_tokens: u
                    .get("completion_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0) as u32,
                total_tokens: u.get("total_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32,
            })
            .unwrap_or_default();

        trace!(
            "Parsed OpenRouter response successfully: {:?}",
            to_string_pretty(&response)
        );

        Ok(CompletionResponse { message, usage })
    }
}

#[async_trait]
impl Provider for OpenRouterProvider {
    async fn complete(
        &self,
        system_prompt: &str,
        messages: &[Message],
        tools: Option<&[rmcp::model::Tool]>,
    ) -> Result<CompletionResponse, ProviderError> {
        debug!(
            "OpenRouter API call - model: {}, messages: {}, tools: {}",
            self.model,
            messages.len(),
            tools.map_or(0, |t| t.len())
        );

        let payload = self.create_request_payload(system_prompt, messages, tools, false);
        trace!(
            "OpenRouter request payload: {}",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        );

        let response = self
            .client
            .post(OPENROUTER_API_URL)
            .json(&payload)
            .send()
            .await
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

        debug!("OpenRouter API response status: {}", response.status());

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            debug!("OpenRouter API error response: {}", error_text);
            return Err(ProviderError::RequestFailed(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let response_text = response
            .text()
            .await
            .map_err(|e| ProviderError::InvalidRequest(e.to_string()))?;
        trace!("OpenRouter raw response: {}", response_text);

        let json_response: Value = serde_json::from_str(&response_text)
            .map_err(|e| ProviderError::InvalidRequest(e.to_string()))?;

        let completion_response = self.parse_response(&json_response)?;
        debug!(
            "OpenRouter response parsed successfully - usage: {:?}",
            completion_response.usage
        );

        crate::ai::metrics::record_token_usage(
            "openrouter",
            &self.model,
            &completion_response.usage,
        );

        if let Some(tool_calls) = &completion_response.message.tool_calls {
            debug!(
                "OpenRouter response contains {} tool calls",
                tool_calls.len()
            );
        }

        Ok(completion_response)
    }

    async fn stream(
        &self,
        system_prompt: &str,
        messages: &[Message],
        tools: Option<&[rmcp::model::Tool]>,
    ) -> Result<MessageStream, ProviderError> {
        let model = self.model.clone();
        let payload = self.create_request_payload(system_prompt, messages, tools, true);

        let response = self
            .client
            .post(OPENROUTER_API_URL)
            .json(&payload)
            .send()
            .await
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ProviderError::RequestFailed(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let stream = response.bytes_stream().map_err(std::io::Error::other);
        let stream_reader = StreamReader::new(stream);
        let mut lines = FramedRead::new(stream_reader, LinesCodec::new());

        Ok(Box::pin(try_stream! {
            let mut content_buffer = String::new();
            let mut tool_calls_map: std::collections::HashMap<usize, ToolCall> =
                std::collections::HashMap::new();
            let mut final_usage = Usage::default();

            while let Some(line) = lines.next().await {
                let line = line.map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

                if line.is_empty() || !line.starts_with("data: ") {
                    continue;
                }

                let data = &line[6..];
                if data == "[DONE]" {
                    break;
                }

                let chunk: Value = serde_json::from_str(data)
                    .map_err(|e| ProviderError::InvalidRequest(e.to_string()))?;

                if let Some(choices) = chunk.get("choices").and_then(|c| c.as_array()) {
                    for choice in choices {
                        if let Some(delta) = choice.get("delta") {
                            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                content_buffer.push_str(content);
                            }

                            if let Some(tool_calls) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
                                for tc in tool_calls {
                                    let index = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;

                                    let entry = tool_calls_map.entry(index).or_insert_with(|| ToolCall {
                                        id: String::new(),
                                        tool_type: "function".to_string(),
                                        function: FunctionCall {
                                            name: String::new(),
                                            arguments: String::new(),
                                        },
                                    });

                                    if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                                        entry.id = id.to_string();
                                    }
                                    if let Some(function) = tc.get("function") {
                                        if let Some(name) = function.get("name").and_then(|n| n.as_str()) {
                                            entry.function.name.push_str(name);
                                        }
                                        if let Some(args) = function.get("arguments").and_then(|a| a.as_str()) {
                                            entry.function.arguments.push_str(args);
                                        }
                                    }
                                }
                            }
                        }

                        if choice.get("finish_reason").and_then(|f| f.as_str()).is_some() {
                            if let Some(usage) = chunk.get("usage") {
                                final_usage = Usage {
                                    prompt_tokens: usage.get("prompt_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32,
                                    completion_tokens: usage.get("completion_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32,
                                    total_tokens: usage.get("total_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32,
                                };
                            }
                        }
                    }
                }
            }

            let mut tool_calls: Vec<ToolCall> = tool_calls_map.into_values().collect();
            tool_calls.sort_by(|a, b| a.id.cmp(&b.id));

            crate::ai::metrics::record_token_usage("openrouter", &model, &final_usage);

            let message = if !tool_calls.is_empty() {
                Message::assistant_with_tools(
                    if content_buffer.is_empty() { None } else { Some(content_buffer) },
                    tool_calls
                )
            } else {
                Message::assistant(content_buffer)
            };

            yield CompletionResponse {
                message,
                usage: final_usage,
            };
        }))
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
