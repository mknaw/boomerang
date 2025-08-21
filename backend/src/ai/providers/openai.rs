use std::time::Duration;
use async_trait::async_trait;
use anyhow::Result;
use reqwest::Client;
use serde_json::{json, Value};
use async_stream::try_stream;
use futures::TryStreamExt;
use tokio_util::{codec::{FramedRead, LinesCodec}, io::StreamReader};
use tokio_stream::StreamExt;
use tracing::{debug, trace};

use crate::ai::{
    provider::{Provider, ProviderError, CompletionResponse, MessageStream},
    types::{Message, MessageRole, Usage, ToolCall},
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
        let mut api_messages = vec![json!({
            "role": "system",
            "content": system_prompt
        })];

        for msg in messages {
            let mut api_msg = json!({
                "role": match msg.role {
                    MessageRole::System => "system",
                    MessageRole::User => "user", 
                    MessageRole::Assistant => "assistant",
                    MessageRole::Tool => "tool",
                }
            });

            if let Some(content) = &msg.content {
                api_msg["content"] = json!(content);
            }

            if let Some(tool_calls) = &msg.tool_calls {
                api_msg["tool_calls"] = json!(tool_calls);
            }

            if let Some(tool_call_id) = &msg.tool_call_id {
                api_msg["tool_call_id"] = json!(tool_call_id);
            }

            api_messages.push(api_msg);
        }

        let mut payload = json!({
            "model": self.model,
            "messages": api_messages,
            "stream": stream,
        });

        if let Some(tools) = tools {
            if !tools.is_empty() {
                let openai_tools: Vec<Value> = tools.iter().map(|tool| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description.as_deref().unwrap_or(""),
                            "parameters": serde_json::Value::Object(tool.input_schema.as_ref().clone())
                        }
                    })
                }).collect();
                payload["tools"] = json!(openai_tools);
                payload["tool_choice"] = json!("auto");
            }
        }

        payload
    }

    fn parse_response(&self, response: &Value) -> Result<CompletionResponse, ProviderError> {
        let choices = response
            .get("choices")
            .and_then(|c| c.as_array())
            .ok_or_else(|| ProviderError::InvalidRequest("No choices in response".to_string()))?;

        let choice = choices
            .first()
            .ok_or_else(|| ProviderError::InvalidRequest("Empty choices array".to_string()))?;

        let api_message = choice
            .get("message")
            .ok_or_else(|| ProviderError::InvalidRequest("No message in choice".to_string()))?;

        let content = api_message.get("content").and_then(|c| c.as_str()).map(String::from);
        let tool_calls = api_message.get("tool_calls").and_then(|tc| {
            tc.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|call| {
                        let id = call.get("id")?.as_str()?.to_string();
                        let tool_type = call.get("type")?.as_str()?.to_string();
                        let function = call.get("function")?;
                        let name = function.get("name")?.as_str()?.to_string();
                        let arguments = function.get("arguments")?.as_str()?.to_string();

                        Some(ToolCall {
                            id,
                            tool_type,
                            function: crate::ai::types::FunctionCall { name, arguments },
                        })
                    })
                    .collect()
            })
        });

        let message = if let Some(tool_calls) = tool_calls {
            Message::assistant_with_tools(content, tool_calls)
        } else {
            Message::assistant(content.unwrap_or_default())
        };

        let usage = response
            .get("usage")
            .map(|u| Usage {
                prompt_tokens: u.get("prompt_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32,
                completion_tokens: u.get("completion_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32,
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
        debug!("OpenAI API call - model: {}, messages: {}, tools: {}", 
               self.model, messages.len(), tools.map_or(0, |t| t.len()));
        
        let payload = self.create_request_payload(system_prompt, messages, tools, false);
        trace!("OpenAI request payload: {}", serde_json::to_string_pretty(&payload).unwrap_or_default());

        debug!("Making OpenAI API request to chat/completions");
        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
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
                status,
                error_text
            )));
        }

        let response_text = response.text().await
            .map_err(|e| ProviderError::InvalidRequest(e.to_string()))?;
        trace!("OpenAI raw response: {}", response_text);

        let json_response: Value = serde_json::from_str(&response_text)
            .map_err(|e| ProviderError::InvalidRequest(e.to_string()))?;

        let completion_response = self.parse_response(&json_response)?;
        debug!("OpenAI response parsed successfully - usage: {:?}", completion_response.usage);
        
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
        let payload = self.create_request_payload(system_prompt, messages, tools, true);

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
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
                status,
                error_text
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

                if let Some(choices) = chunk.get("choices").and_then(|c| c.as_array()) {
                    for choice in choices {
                        if let Some(delta) = choice.get("delta") {
                            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                content_buffer.push_str(content);
                            }

                            if let Some(tool_calls) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
                                for (i, tool_call) in tool_calls.iter().enumerate() {
                                    while tool_calls_buffer.len() <= i {
                                        tool_calls_buffer.push(ToolCall {
                                            id: String::new(),
                                            tool_type: "function".to_string(),
                                            function: crate::ai::types::FunctionCall {
                                                name: String::new(),
                                                arguments: String::new(),
                                            },
                                        });
                                    }

                                    if let Some(id) = tool_call.get("id").and_then(|i| i.as_str()) {
                                        tool_calls_buffer[i].id = id.to_string();
                                    }

                                    if let Some(function) = tool_call.get("function") {
                                        if let Some(name) = function.get("name").and_then(|n| n.as_str()) {
                                            tool_calls_buffer[i].function.name = name.to_string();
                                        }
                                        if let Some(args) = function.get("arguments").and_then(|a| a.as_str()) {
                                            tool_calls_buffer[i].function.arguments.push_str(args);
                                        }
                                    }
                                }
                            }
                        }

                        if choice.get("finish_reason").is_some() {
                            let message = if !tool_calls_buffer.is_empty() {
                                Message::assistant_with_tools(
                                    if content_buffer.is_empty() { None } else { Some(content_buffer.clone()) },
                                    tool_calls_buffer.clone()
                                )
                            } else {
                                Message::assistant(content_buffer.clone())
                            };

                            let usage = chunk.get("usage").map(|u| Usage {
                                prompt_tokens: u.get("prompt_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32,
                                completion_tokens: u.get("completion_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32,
                                total_tokens: u.get("total_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32,
                            }).unwrap_or_default();

                            yield CompletionResponse { message, usage };
                            return;
                        }
                    }
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
