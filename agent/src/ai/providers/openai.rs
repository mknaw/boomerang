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
use tracing::{debug, trace};

use crate::ai::{
    provider::{CompletionResponse, MessageStream, Provider, ProviderError},
    types::{Message, MessageRole, ToolCall, Usage},
};

pub struct OpenAIProvider {
    client: Client,
    model: String,
}

impl OpenAIProvider {
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
        let mut input_items = Vec::new();

        // Convert messages to input format
        for msg in messages {
            match msg.role {
                MessageRole::User => {
                    if let Some(content) = &msg.content {
                        input_items.push(json!({
                            "role": "user",
                            "content": content
                        }));
                    }
                }
                MessageRole::Assistant => {
                    if let Some(content) = &msg.content {
                        input_items.push(json!({
                            "role": "assistant",
                            "content": content
                        }));
                    }
                    if let Some(tool_calls) = &msg.tool_calls {
                        for tool_call in tool_calls {
                            input_items.push(json!({
                                "type": "function_call",
                                "call_id": tool_call.id,
                                "name": tool_call.function.name,
                                "arguments": tool_call.function.arguments
                            }));
                        }
                    }
                }
                MessageRole::Tool => {
                    if let Some(content) = &msg.content
                        && let Some(tool_call_id) = &msg.tool_call_id
                    {
                        input_items.push(json!({
                            "type": "function_call_output",
                            "call_id": tool_call_id,
                            "output": content
                        }));
                    }
                }
                MessageRole::System => {
                    // System messages are handled via instructions parameter
                }
            }
        }

        let mut payload = json!({
            "model": self.model,
            "input": input_items,
            "stream": stream,
        });

        // Add instructions (system prompt)
        if !system_prompt.is_empty() {
            payload["instructions"] = json!(system_prompt);
        }

        if let Some(tools) = tools
            && !tools.is_empty()
        {
            let openai_tools: Vec<Value> = tools
                .iter()
                .map(|tool| {
                    json!({
                        "type": "function",
                        "name": tool.name,
                        "description": tool.description.as_deref().unwrap_or(""),
                        "parameters": serde_json::Value::Object(tool.input_schema.as_ref().clone())
                    })
                })
                .collect();
            payload["tools"] = json!(openai_tools);
            payload["tool_choice"] = json!("auto");
        }

        payload
    }

    fn parse_response(&self, response: &Value) -> Result<CompletionResponse, ProviderError> {
        debug!(
            "Parsing OpenAI Responses API response: {:?}",
            to_string_pretty(&response)
        );

        // Parse the output array
        let output = response
            .get("output")
            .and_then(|o| o.as_array())
            .ok_or_else(|| {
                ProviderError::InvalidRequest("No output array in response".to_string())
            })?;

        let mut content: Option<String> = None;
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        for item in output {
            match item.get("type").and_then(|t| t.as_str()) {
                Some("message") => {
                    if let Some(role) = item.get("role").and_then(|r| r.as_str())
                        && role == "assistant"
                        && let Some(content_array) = item.get("content").and_then(|c| c.as_array())
                    {
                        for content_item in content_array {
                            if content_item.get("type").and_then(|t| t.as_str())
                                == Some("output_text")
                                && let Some(text) =
                                    content_item.get("text").and_then(|t| t.as_str())
                            {
                                content = Some(text.to_string());
                            }
                        }
                    }
                }
                Some("function_call") => {
                    if let (Some(call_id), Some(name), Some(args)) = (
                        item.get("call_id").and_then(|i| i.as_str()),
                        item.get("name").and_then(|n| n.as_str()),
                        item.get("arguments").and_then(|a| a.as_str()),
                    ) {
                        tool_calls.push(ToolCall {
                            id: call_id.to_string(),
                            tool_type: "function".to_string(),
                            function: crate::ai::types::FunctionCall {
                                name: name.to_string(),
                                arguments: args.to_string(),
                            },
                        });
                    }
                }
                _ => {
                    // Handle other output types if needed
                }
            }
        }

        let message = if !tool_calls.is_empty() {
            Message::assistant_with_tools(content, tool_calls)
        } else {
            Message::assistant(content.unwrap_or_default())
        };

        let usage = response
            .get("usage")
            .map(|u| Usage {
                prompt_tokens: u.get("input_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32,
                completion_tokens: u.get("output_tokens").and_then(|t| t.as_u64()).unwrap_or(0)
                    as u32,
                total_tokens: u.get("total_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32,
            })
            .unwrap_or_default();

        Ok(CompletionResponse { message, usage })
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    async fn complete(
        &self,
        system_prompt: &str,
        messages: &[Message],
        tools: Option<&[rmcp::model::Tool]>,
    ) -> Result<CompletionResponse, ProviderError> {
        debug!(
            "OpenAI API call - model: {}, messages: {}, tools: {}",
            self.model,
            messages.len(),
            tools.map_or(0, |t| t.len())
        );

        let payload = self.create_request_payload(system_prompt, messages, tools, false);
        trace!(
            "OpenAI request payload: {}",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        );

        debug!("Making OpenAI API request to responses");
        let response = self
            .client
            .post("https://api.openai.com/v1/responses")
            .json(&payload)
            .send()
            .await
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

        debug!("OpenAI API response status: {}", response.status());

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            debug!("OpenAI API error response: {}", error_text);
            return Err(ProviderError::RequestFailed(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let response_text = response
            .text()
            .await
            .map_err(|e| ProviderError::InvalidRequest(e.to_string()))?;
        trace!("OpenAI raw response: {}", response_text);

        let json_response: Value = serde_json::from_str(&response_text)
            .map_err(|e| ProviderError::InvalidRequest(e.to_string()))?;

        let completion_response = self.parse_response(&json_response)?;
        debug!(
            "OpenAI response parsed successfully - usage: {:?}",
            completion_response.usage
        );

        crate::ai::metrics::record_token_usage("openai", &self.model, &completion_response.usage);

        if let Some(tool_calls) = &completion_response.message.tool_calls {
            debug!("OpenAI response contains {} tool calls", tool_calls.len());
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
            .post("https://api.openai.com/v1/responses")
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
            let mut tool_calls_buffer: Vec<ToolCall> = Vec::new();

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

                // Parse streaming output
                if let Some(output) = chunk.get("output").and_then(|o| o.as_array()) {
                    for item in output {
                        match item.get("type").and_then(|t| t.as_str()) {
                            Some("message") => {
                                if let Some(role) = item.get("role").and_then(|r| r.as_str())
                                    && role == "assistant"
                                    && let Some(content_array) = item.get("content").and_then(|c| c.as_array())
                                {
                                    for content_item in content_array {
                                        if content_item.get("type").and_then(|t| t.as_str()) == Some("output_text")
                                            && let Some(text) = content_item.get("text").and_then(|t| t.as_str())
                                        {
                                            content_buffer.push_str(text);
                                        }
                                    }
                                }
                            }
                            Some("function_call") => {
                                if let (Some(call_id), Some(name), Some(args)) = (
                                    item.get("call_id").and_then(|i| i.as_str()),
                                    item.get("name").and_then(|n| n.as_str()),
                                    item.get("arguments").and_then(|a| a.as_str()),
                                ) {
                                    tool_calls_buffer.push(ToolCall {
                                        id: call_id.to_string(),
                                        tool_type: "function".to_string(),
                                        function: crate::ai::types::FunctionCall {
                                            name: name.to_string(),
                                            arguments: args.to_string(),
                                        },
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                }

                if chunk.get("status").and_then(|s| s.as_str()) == Some("completed") {
                    let message = if !tool_calls_buffer.is_empty() {
                        Message::assistant_with_tools(
                            if content_buffer.is_empty() { None } else { Some(content_buffer.clone()) },
                            tool_calls_buffer.clone()
                        )
                    } else {
                        Message::assistant(content_buffer.clone())
                    };

                    let usage = chunk.get("usage").map(|u| Usage {
                        prompt_tokens: u.get("input_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32,
                        completion_tokens: u.get("output_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32,
                        total_tokens: u.get("total_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32,
                    }).unwrap_or_default();

                    crate::ai::metrics::record_token_usage("openai", &model, &usage);

                    yield CompletionResponse { message, usage };
                    return;
                }
            }

            let final_message = if !tool_calls_buffer.is_empty() {
                Message::assistant_with_tools(
                    if content_buffer.is_empty() { None } else { Some(content_buffer) },
                    tool_calls_buffer
                )
            } else {
                Message::assistant(content_buffer)
            };

            yield CompletionResponse {
                message: final_message,
                usage: Usage::default()
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
