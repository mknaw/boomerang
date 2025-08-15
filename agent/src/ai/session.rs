use std::sync::Arc;

use anyhow::Result;
use futures::future::join_all;
use tracing::{debug, trace};

use crate::ai::{
    provider::{CompletionResponse, Provider, ProviderError},
    types::{Message, ToolSpec},
};

pub struct Session {
    provider: Arc<dyn Provider>,
    system_prompt: String,
    messages: Vec<Message>,
    tool_specs: Vec<ToolSpec>,
}

impl Session {
    pub fn new(provider: Arc<dyn Provider>) -> Self {
        Self {
            provider,
            system_prompt: "You are a helpful assistant.".to_string(),
            messages: Vec::new(),
            tool_specs: Vec::new(),
        }
    }

    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    pub fn with_tools(mut self, tool_specs: Vec<ToolSpec>) -> Self {
        self.tool_specs = tool_specs;
        self
    }

    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub fn add_user_message(&mut self, content: impl Into<String>) {
        self.messages.push(Message::user(content));
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn clear_messages(&mut self) {
        self.messages.clear();
    }

    pub async fn complete(&mut self) -> Result<CompletionResponse, ProviderError> {
        let tools: Vec<rmcp::model::Tool> = self
            .tool_specs
            .iter()
            .map(|spec| spec.tool.clone())
            .collect();
        let tools_slice = if tools.is_empty() {
            None
        } else {
            Some(tools.as_slice())
        };

        if !tools.is_empty() {
            debug!(
                "Available tools: {:?}",
                tools.iter().map(|t| &t.name).collect::<Vec<_>>()
            );
        }

        loop {
            debug!(
                "Starting AI completion with {} messages",
                self.messages.len()
            );
            trace!("System prompt: {}", self.system_prompt);
            trace!("Messages: {:?}", self.messages);

            let response = self
                .provider
                .complete(&self.system_prompt, &self.messages, tools_slice)
                .await?;

            debug!(
                "Received AI response with usage: {:?}, message: {:?}",
                response.usage, response.message
            );

            if let Some(content) = &response.message.content {
                debug!("AI response content: {:.200}...", content);
            }

            self.messages.push(response.message.clone());

            let Some(tool_calls) = &response.message.tool_calls else {
                debug!("Completion finished, no tool calls requested");
                return Ok(response);
            };

            debug!("AI requested {} tool calls", tool_calls.len());
            for (i, tool_call) in tool_calls.iter().enumerate() {
                debug!(
                    "Tool call {}: {} with id {}",
                    i + 1,
                    tool_call.function.name,
                    tool_call.id
                );
                trace!(
                    "Tool call {} arguments: {}",
                    i + 1,
                    tool_call.function.arguments
                );
            }

            debug!("Executing {} tool calls in parallel", tool_calls.len());

            let tool_futures: Vec<_> = tool_calls
                .iter()
                .enumerate()
                .map(|(i, tool_call)| {
                    let tool_call_id = tool_call.id.clone();
                    let tool_name = tool_call.function.name.clone();
                    let tool_args = tool_call.function.arguments.clone();
                    let tool_calls_len = tool_calls.len();

                    let tool_spec = self
                        .tool_specs
                        .iter()
                        .find(|spec| spec.tool.name == tool_name)
                        .cloned();

                    async move {
                        debug!(
                            "Executing tool call {} of {}: {}",
                            i + 1,
                            tool_calls_len,
                            tool_name
                        );

                        if let Some(tool_spec) = tool_spec {
                            debug!("Found tool spec for {}, executing...", tool_name);
                            match (tool_spec.executor)(&tool_name, &tool_args).await {
                                Ok(result) => {
                                    debug!(
                                        "Tool {} executed successfully, result length: {} chars",
                                        tool_name,
                                        result.len()
                                    );
                                    trace!("Tool {} result: {}", tool_name, result);
                                    Message::tool_result(&tool_call_id, result)
                                }
                                Err(e) => {
                                    debug!("Tool {} execution failed: {}", tool_name, e);
                                    Message::tool_result(
                                        &tool_call_id,
                                        format!("Error executing tool: {}", e),
                                    )
                                }
                            }
                        } else {
                            debug!("Unknown tool requested: {}", tool_name);
                            Message::tool_result(
                                &tool_call_id,
                                format!("Unknown tool: {}", tool_name),
                            )
                        }
                    }
                })
                .collect();

            let tool_results = join_all(tool_futures).await;
            for result in tool_results {
                self.messages.push(result);
            }

            debug!("All tool calls completed, continuing loop for follow-up");
        }
    }

    pub async fn stream(
        &mut self,
    ) -> Result<impl futures::Stream<Item = Result<CompletionResponse, ProviderError>>, ProviderError>
    {
        let tools: Vec<rmcp::model::Tool> = self
            .tool_specs
            .iter()
            .map(|spec| spec.tool.clone())
            .collect();
        let tools_slice = if tools.is_empty() {
            None
        } else {
            Some(tools.as_slice())
        };

        let stream = self
            .provider
            .stream(&self.system_prompt, &self.messages, tools_slice)
            .await?;

        Ok(stream)
    }
}
