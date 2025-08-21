use std::sync::Arc;

use anyhow::Result;
use serde_json::json;

use crate::{
    ai::{
        provider::{CompletionResponse, Provider, ProviderError},
        providers::openai::OpenAIProvider,
        types::{Message, ToolSpec},
    },
    config::AIConfig,
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
            let tools: Vec<rmcp::model::Tool> = self.tool_specs.iter().map(|spec| spec.tool.clone()).collect();
            let tools_slice = if tools.is_empty() { None } else { Some(tools.as_slice()) };
            
            let response = self
                .provider
                .complete(&self.system_prompt, &self.messages, tools_slice)
                .await?;

            self.messages.push(response.message.clone());

            if let Some(tool_calls) = &response.message.tool_calls {
                for tool_call in tool_calls {
                    if let Some(tool_spec) = self.tool_specs.iter().find(|spec| spec.tool.name == tool_call.function.name) {
                        match (tool_spec.executor)(&tool_call.function.name, &tool_call.function.arguments) {
                            Ok(result) => {
                                let tool_result = Message::tool_result(&tool_call.id, result);
                                self.messages.push(tool_result);
                            }
                            Err(e) => {
                                let error_result = Message::tool_result(
                                    &tool_call.id,
                                    format!("Error executing tool: {}", e),
                                );
                                self.messages.push(error_result);
                            }
                        }
                    } else {
                        let error_result = Message::tool_result(
                            &tool_call.id,
                            format!("Unknown tool: {}", tool_call.function.name),
                        );
                        self.messages.push(error_result);
                    }
                }

                return self.complete().await;
            }

            Ok(response)
        })
    }

    pub async fn stream(
        &mut self,
    ) -> Result<impl futures::Stream<Item = Result<CompletionResponse, ProviderError>>, ProviderError>
    {
        let tools: Vec<rmcp::model::Tool> = self.tool_specs.iter().map(|spec| spec.tool.clone()).collect();
        let tools_slice = if tools.is_empty() { None } else { Some(tools.as_slice()) };
        
        let stream = self
            .provider
            .stream(&self.system_prompt, &self.messages, tools_slice)
            .await?;

        Ok(stream)
    }
}

pub async fn establish_chat_session(
    ai_config: &AIConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let provider = Arc::new(OpenAIProvider::new("gpt-4", &ai_config.openai_api_key)?);

    // Example tool definition using rmcp::model::Tool
    let weather_tool = rmcp::model::Tool {
        name: "get_weather".into(),
        description: Some("Get current weather for a location".into()),
        input_schema: {
            let mut schema = serde_json::Map::new();
            schema.insert("type".to_string(), json!("object"));
            schema.insert(
                "properties".to_string(),
                json!({
                    "location": {
                        "type": "string",
                        "description": "The city and state, e.g. San Francisco, CA"
                    }
                }),
            );
            schema.insert("required".to_string(), json!(["location"]));
            std::sync::Arc::new(schema)
        },
        output_schema: None,
        annotations: None,
    };

    let weather_tool_spec = ToolSpec::new(
        weather_tool,
        Arc::new(|_name, args| {
            let args: serde_json::Value = serde_json::from_str(args)?;
            let location = args["location"].as_str().unwrap_or("Unknown");
            Ok(format!("The weather in {} is sunny with 72°F", location))
        }),
    );

    let mut session = Session::new(provider)
        .with_system_prompt("You are a helpful assistant.")
        .with_tools(vec![weather_tool_spec]);

    session.add_user_message("What is the weather like in San Diego?");

    println!("🤖 AI Response:");
    let response = session.complete().await?;

    if let Some(content) = &response.message.content {
        println!("📤 Response: {}", content);
    }

    println!("📊 Usage: {:?}", response.usage);
    println!("✅ Session completed");

    Ok(())
}
