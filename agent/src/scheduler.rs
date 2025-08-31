use std::{sync::Arc, time::Duration};

use restate_sdk::prelude::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use crate::{
    ai::{providers::openai::OpenAIProvider, session::Session},
    config::Config,
    tools::web_search::WebSearchTool,
};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ScheduleArgs {
    seconds: u32,
    query: String,
}

#[restate_sdk::object]
pub trait ScheduledSession {
    async fn run(spec: Json<ScheduleArgs>) -> HandlerResult<()>;
}

pub struct ScheduledSessionImpl;

impl ScheduledSession for ScheduledSessionImpl {
    async fn run(&self, ctx: ObjectContext<'_>, args: Json<ScheduleArgs>) -> HandlerResult<()> {
        debug!(
            "Starting scheduled session: sleeping for {} seconds before executing query: '{}'",
            args.0.seconds, args.0.query
        );

        ctx.sleep(Duration::from_secs(args.0.seconds as u64))
            .await?;

        debug!(
            "Awoke from sleep, now executing LLM session with query: {}",
            args.0.query
        );

        let config = Config::global();
        match execute_llm_session(&args.0.query, &config).await {
            Ok(response) => {
                debug!("LLM session completed successfully");
                println!("🤖 Scheduled query result: {}", response);
            }
            Err(e) => {
                error!("LLM session failed: {}", e);
                return Err(HandlerError::from(anyhow::anyhow!(
                    "LLM session failed: {}",
                    e
                )));
            }
        }

        Ok(())
    }
}

async fn execute_llm_session(query: &str, config: &Config) -> anyhow::Result<String> {
    debug!("Setting up AI provider and session");

    let provider = Arc::new(OpenAIProvider::new("gpt-5", &config.ai.openai_api_key)?);

    let search_tool = Arc::new(WebSearchTool::new(config.tools.tavily_api_key.clone()));
    let web_search_tool_spec = search_tool.to_tool_spec();

    let mut session = Session::new(provider)
        .with_system_prompt("You are a helpful assistant with access to web search. Provide concise, relevant information for scheduled queries.")
        .with_tools(vec![web_search_tool_spec]);

    session.add_user_message(query);

    let response = session.complete().await?;

    let content = response
        .message
        .content
        .unwrap_or_else(|| "No response content".to_string());

    debug!("LLM session usage: {:?}", response.usage);

    Ok(content)
}
