use std::sync::Arc;

use anyhow::Result;
use common::config::Config;
use tracing::{debug, instrument};

use crate::{
    ai::{
        create_workhorse_provider,
        provider::Provider,
        session::Session,
        types::{Message, ToolSpec},
    },
    memory::{FileMemory, MemoryRef},
    tools::{ToolOutput, ToolRegistry, create_memory_tools},
};

pub struct PlatformContext {
    pub platform_type: String,
    pub external_chat_id: String,
    pub adapter_key: String,
}

pub struct AgentConfig {
    pub provider: Arc<dyn Provider>,
    pub tools: ToolRegistry,
    pub system_prompt: String,
    pub context_window_size: usize,
}

impl AgentConfig {
    pub fn new(
        provider: Arc<dyn Provider>,
        tools: ToolRegistry,
        system_prompt: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            tools,
            system_prompt: system_prompt.into(),
            context_window_size: 50,
        }
    }

    pub fn with_context_window_size(mut self, size: usize) -> Self {
        self.context_window_size = size;
        self
    }
}

pub struct AgentExecutor {
    config: Arc<Config>,
    chat_key: String,
    platform_ctx: Option<PlatformContext>,
    no_tools: bool,
}

const DEFAULT_SYSTEM_PROMPT_BASE: &str = include_str!("prompts/default_system_base.txt");
const SCHEDULED_SYSTEM_PROMPT: &str = include_str!("prompts/scheduled_system.txt");

impl AgentExecutor {
    pub fn new(config: Arc<Config>, chat_key: String) -> Self {
        Self {
            config,
            chat_key,
            platform_ctx: None,
            no_tools: false,
        }
    }

    pub fn new_without_tools(config: Arc<Config>) -> Self {
        Self {
            config,
            chat_key: String::new(),
            platform_ctx: None,
            no_tools: true,
        }
    }

    pub fn with_platform_context(mut self, ctx: PlatformContext) -> Self {
        self.platform_ctx = Some(ctx);
        self
    }

    fn build_agent_config(&self, system_prompt: &str) -> Result<AgentConfig> {
        let provider = create_workhorse_provider(&self.config.ai)?;

        let mut registry = ToolRegistry::new();

        if !self.no_tools {
            // Add web search tool
            let web_search = Arc::new(crate::tools::web_search::WebSearchTool::new(
                self.config.tools.tavily_api_key.clone(),
            ));
            registry.register(web_search);

            // Add memory tools
            let memory: MemoryRef =
                Arc::new(FileMemory::new(&self.config.memory.scratch_space_path));
            for tool in create_memory_tools(memory) {
                registry.register(tool);
            }

            // Add schedule tool if platform context is available
            if let Some(ref ctx) = self.platform_ctx {
                let schedule = Arc::new(crate::tools::schedule::ScheduleTool::new(
                    self.config.restate.ingress_url.clone(),
                    self.chat_key.clone(),
                    ctx.platform_type.clone(),
                    ctx.external_chat_id.clone(),
                    ctx.adapter_key.clone(),
                ));
                registry.register(schedule);
            }
        }

        Ok(AgentConfig::new(
            provider,
            registry,
            system_prompt.to_string(),
        ))
    }

    #[instrument(skip(self, messages, system_prompt), fields(message_count = messages.len()))]
    pub async fn complete(&self, messages: &[Message], system_prompt: &str) -> Result<Message> {
        debug!("Setting up AI provider and session");

        let agent_config = self.build_agent_config(system_prompt)?;

        let mut session = Session::new(agent_config.provider)
            .with_system_prompt(&agent_config.system_prompt)
            .with_tools(self.convert_tools(&agent_config.tools));

        for msg in messages {
            session.add_message(msg.clone());
        }

        let response = session.complete().await?;
        debug!("LLM session usage: {:?}", response.usage);

        Ok(response.message)
    }

    fn convert_tools(&self, registry: &ToolRegistry) -> Vec<ToolSpec> {
        registry
            .tools()
            .into_iter()
            .map(|tool| {
                let name = tool.name().to_string();
                let description = tool.description().to_string();
                let schema = tool.parameters_schema();

                let mut tool_map = serde_json::Map::new();
                tool_map.insert("type".to_string(), serde_json::json!("object"));
                if let Some(props) = schema.get("properties") {
                    tool_map.insert("properties".to_string(), props.clone());
                }
                if let Some(required) = schema.get("required") {
                    tool_map.insert("required".to_string(), required.clone());
                }

                ToolSpec::new(
                    rmcp::model::Tool {
                        name: name.into(),
                        description: Some(description.into()),
                        input_schema: Arc::new(tool_map),
                        output_schema: None,
                        annotations: None,
                    },
                    Arc::new(move |_name, args| {
                        let tool = tool.clone();
                        let args: serde_json::Value = match serde_json::from_str(args) {
                            Ok(v) => v,
                            Err(e) => {
                                return Box::pin(async move {
                                    Err(anyhow::anyhow!("Failed to parse tool arguments: {}", e))
                                });
                            }
                        };
                        Box::pin(async move {
                            match tool.execute(args).await {
                                Ok(ToolOutput { content, .. }) => Ok(content),
                                Err(e) => Err(anyhow::anyhow!("{}", e)),
                            }
                        })
                    }),
                )
            })
            .collect()
    }

    pub fn default_system_prompt(platform_type: Option<&str>) -> String {
        let now = chrono::Local::now();

        let platform_context = match platform_type {
            Some("telegram") => "\n\nYou are communicating with the user via Telegram. \
                When they ask you to message or text them, you are already doing so - \
                just respond directly in this chat."
                .to_string(),
            Some(other) => format!("\n\nYou are communicating with the user via {}.", other),
            None => String::new(),
        };

        format!(
            "{}{}\n\nCurrent time: {} (UTC{})",
            DEFAULT_SYSTEM_PROMPT_BASE,
            platform_context,
            now.format("%A, %B %d, %Y %H:%M:%S"),
            now.format("%:z")
        )
    }

    pub fn scheduled_system_prompt() -> &'static str {
        SCHEDULED_SYSTEM_PROMPT
    }
}
