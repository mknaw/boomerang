use std::sync::Arc;

use anyhow::Result;
use common::config::Config;

use super::{Agent, AgentId, AgentOutput, Context, Task};
use crate::{
    ai::types::Message,
    executor::{AgentExecutor, PlatformContext},
};

pub struct WorkerAgent {
    id: AgentId,
    capabilities: Vec<String>,
    config: Arc<Config>,
    platform_ctx: Option<PlatformContext>,
}

impl WorkerAgent {
    pub fn new(id: impl Into<String>, config: Arc<Config>) -> Self {
        Self {
            id: id.into(),
            capabilities: vec![
                "web_search".to_string(),
                "memory_write".to_string(),
                "memory_read".to_string(),
                "memory_list".to_string(),
                "memory_search".to_string(),
                "memory_delete".to_string(),
            ],
            config,
            platform_ctx: None,
        }
    }

    pub fn with_capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub fn with_platform_context(mut self, ctx: PlatformContext) -> Self {
        self.platform_ctx = Some(ctx);
        self
    }

    fn create_executor(&self, chat_key: &str) -> AgentExecutor {
        let executor = AgentExecutor::new(self.config.clone(), chat_key.to_string());
        if let Some(ref ctx) = self.platform_ctx {
            executor.with_platform_context(PlatformContext {
                platform_type: ctx.platform_type.clone(),
                external_chat_id: ctx.external_chat_id.clone(),
                adapter_key: ctx.adapter_key.clone(),
            })
        } else {
            executor
        }
    }

    fn build_system_prompt(&self, task: &Task, context: &Context) -> String {
        let mut prompt = String::from(
            "You are a worker agent executing a specific task. Complete the task thoroughly and report your findings.\n\n",
        );

        if let Some(ref constraints) = task.constraints
            && let Some(ref tools) = constraints.allowed_tools
        {
            prompt.push_str(&format!(
                "You may only use the following tools: {}\n\n",
                tools.join(", ")
            ));
        }

        if context.depth > 0 {
            prompt.push_str(&format!(
                "This is a subtask (depth: {}). Focus only on your assigned portion.\n\n",
                context.depth
            ));
        }

        prompt.push_str(&format!("Task ID: {}\n", task.id));
        if let Some(ref parent_id) = task.parent_task_id {
            prompt.push_str(&format!("Parent Task ID: {}\n", parent_id));
        }

        prompt
    }
}

impl Agent for WorkerAgent {
    fn id(&self) -> &AgentId {
        &self.id
    }

    fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    fn execute<'a>(
        &'a self,
        task: Task,
        context: &'a Context,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AgentOutput>> + Send + 'a>> {
        Box::pin(async move {
            let chat_key = format!("task:{}", task.id);
            let executor = self.create_executor(&chat_key);

            let system_prompt = self.build_system_prompt(&task, context);
            let messages = vec![Message::user(&task.description)];

            let response = executor.complete(&messages, &system_prompt).await?;

            let result = response.content.unwrap_or_default();
            Ok(AgentOutput::new(result))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_agent_creation() {
        let config = Arc::new(Config::default());
        let agent = WorkerAgent::new("test-worker", config);

        assert_eq!(agent.id(), "test-worker");
        assert!(!agent.capabilities().is_empty());
    }

    #[test]
    fn test_worker_with_custom_capabilities() {
        let config = Arc::new(Config::default());
        let agent = WorkerAgent::new("test-worker", config)
            .with_capabilities(vec!["web_search".to_string()]);

        assert_eq!(agent.capabilities(), &["web_search"]);
    }
}
