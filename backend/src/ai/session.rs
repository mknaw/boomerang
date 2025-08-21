use std::sync::Arc;

use anyhow::Result;
use tracing::{debug, trace};

use crate::{
    ai::{
        provider::{CompletionResponse, Provider, ProviderError},
        providers::openai::OpenAIProvider,
        types::{Message, ToolSpec},
    },
    config::AIConfig,
    tools::web_search::WebSearchTool,
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

    pub fn complete(
        &mut self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<CompletionResponse, ProviderError>> + Send + '_,
        >,
    > {
        Box::pin(async move {
            debug!("Starting AI completion with {} messages", self.messages.len());
            trace!("System prompt: {}", self.system_prompt);
            trace!("Messages: {:?}", self.messages);

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
                debug!("Available tools: {:?}", tools.iter().map(|t| &t.name).collect::<Vec<_>>());
            }

            debug!("Calling AI provider for completion");
            let response = self
                .provider
                .complete(&self.system_prompt, &self.messages, tools_slice)
                .await?;

            debug!("Received AI response with usage: {:?}", response.usage);
            
            if let Some(content) = &response.message.content {
                debug!("AI response content: {:.200}...", content);
            }

            if let Some(tool_calls) = &response.message.tool_calls {
                debug!("AI requested {} tool calls", tool_calls.len());
                for (i, tool_call) in tool_calls.iter().enumerate() {
                    debug!("Tool call {}: {} with id {}", i + 1, tool_call.function.name, tool_call.id);
                    trace!("Tool call {} arguments: {}", i + 1, tool_call.function.arguments);
                }
            }

            self.messages.push(response.message.clone());

            if let Some(tool_calls) = &response.message.tool_calls {
                for (i, tool_call) in tool_calls.iter().enumerate() {
                    debug!("Executing tool call {} of {}: {}", i + 1, tool_calls.len(), tool_call.function.name);
                    
                    if let Some(tool_spec) = self
                        .tool_specs
                        .iter()
                        .find(|spec| spec.tool.name == tool_call.function.name)
                    {
                        debug!("Found tool spec for {}, executing...", tool_call.function.name);
                        match (tool_spec.executor)(
                            &tool_call.function.name,
                            &tool_call.function.arguments,
                        ).await {
                            Ok(result) => {
                                debug!("Tool {} executed successfully, result length: {} chars", 
                                       tool_call.function.name, result.len());
                                trace!("Tool {} result: {}", tool_call.function.name, result);
                                let tool_result = Message::tool_result(&tool_call.id, result);
                                self.messages.push(tool_result);
                            }
                            Err(e) => {
                                debug!("Tool {} execution failed: {}", tool_call.function.name, e);
                                let error_result = Message::tool_result(
                                    &tool_call.id,
                                    format!("Error executing tool: {}", e),
                                );
                                self.messages.push(error_result);
                            }
                        }
                    } else {
                        debug!("Unknown tool requested: {}", tool_call.function.name);
                        let error_result = Message::tool_result(
                            &tool_call.id,
                            format!("Unknown tool: {}", tool_call.function.name),
                        );
                        self.messages.push(error_result);
                    }
                }

                debug!("All tool calls completed, recursively calling complete() for follow-up");
                return self.complete().await;
            }

            debug!("Completion finished without tool calls");
            Ok(response)
        })
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

pub async fn establish_chat_session(
    ai_config: &AIConfig,
    tool_config: &crate::config::ToolConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    debug!("Establishing chat session with AI provider");
    return Ok(());
    let provider = Arc::new(OpenAIProvider::new("gpt-5", &ai_config.openai_api_key)?);

    // let search_tool = Arc::new(WebSearchTool::new(tool_config.tavily_api_key.clone()));
    // let web_search_tool_spec = search_tool.to_tool_spec();

    let mut session = Session::new(provider)
        .with_system_prompt("You are a helpful assistant.");
        // .with_system_prompt("You are a helpful assistant with access to web search.");
        // .with_tools(vec![web_search_tool_spec]);

    session.add_user_message("What is the difference between south and north indian food?");

    println!("🤖 AI Response:");
    let response = session.complete().await?;

    if let Some(content) = &response.message.content {
        println!("📤 Response: {}", content);
    }

    println!("📊 Usage: {:?}", response.usage);
    println!("✅ Session completed");

    Ok(())
}
