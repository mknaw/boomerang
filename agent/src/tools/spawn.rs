use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::tool_trait::{Tool, ToolError, ToolFuture, ToolOutput};

pub struct SpawnSubtaskTool {
    client: Client,
    restate_ingress_url: String,
    parent_task_id: Option<String>,
    chat_key: Option<String>,
    adapter_key: Option<String>,
}

impl SpawnSubtaskTool {
    pub fn new(restate_ingress_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            restate_ingress_url: restate_ingress_url.into(),
            parent_task_id: None,
            chat_key: None,
            adapter_key: None,
        }
    }

    pub fn with_parent_task(mut self, task_id: impl Into<String>) -> Self {
        self.parent_task_id = Some(task_id.into());
        self
    }

    pub fn with_chat_context(
        mut self,
        chat_key: impl Into<String>,
        adapter_key: impl Into<String>,
    ) -> Self {
        self.chat_key = Some(chat_key.into());
        self.adapter_key = Some(adapter_key.into());
        self
    }
}

#[derive(Debug, Deserialize)]
struct SpawnArgs {
    description: String,
    #[serde(default)]
    allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Serialize)]
struct TaskArgs {
    task_id: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    constraints: Option<TaskConstraints>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    adapter_key: Option<String>,
}

#[derive(Debug, Serialize)]
struct TaskConstraints {
    #[serde(skip_serializing_if = "Option::is_none")]
    allowed_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout_seconds: Option<u64>,
}

impl Tool for SpawnSubtaskTool {
    fn name(&self) -> &str {
        "spawn_subtask"
    }

    fn description(&self) -> &str {
        "Spawn an autonomous subtask that will execute independently. Use this when a task can be \
         broken down into independent pieces that don't require immediate results. The subtask \
         will run asynchronously and its results will be available later. Returns the task ID \
         for tracking."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "description": {
                    "type": "string",
                    "description": "A clear description of what the subtask should accomplish"
                },
                "allowed_tools": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional list of tool names the subtask is allowed to use"
                },
                "timeout_seconds": {
                    "type": "integer",
                    "description": "Optional timeout in seconds for the subtask"
                }
            },
            "required": ["description"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> ToolFuture {
        let client = self.client.clone();
        let ingress_url = self.restate_ingress_url.clone();
        let parent_task_id = self.parent_task_id.clone();
        let chat_key = self.chat_key.clone();
        let adapter_key = self.adapter_key.clone();

        Box::pin(async move {
            let spawn_args: SpawnArgs = serde_json::from_value(args)
                .map_err(|e| ToolError::new(format!("Invalid arguments: {}", e)))?;

            let task_id = format!("task-{}", Uuid::new_v4());

            let constraints =
                if spawn_args.allowed_tools.is_some() || spawn_args.timeout_seconds.is_some() {
                    Some(TaskConstraints {
                        allowed_tools: spawn_args.allowed_tools,
                        timeout_seconds: spawn_args.timeout_seconds,
                    })
                } else {
                    None
                };

            let task_args = TaskArgs {
                task_id: task_id.clone(),
                description: spawn_args.description.clone(),
                parent_task_id,
                constraints,
                chat_key,
                adapter_key,
            };

            let url = format!("{}/TaskExecution/{}/run", ingress_url, task_id);

            let response = client
                .post(&url)
                .json(&task_args)
                .send()
                .await
                .map_err(|e| ToolError::retryable(format!("Failed to spawn subtask: {}", e)))?;

            if response.status().is_success() {
                Ok(ToolOutput::new(format!(
                    "Subtask spawned successfully.\nTask ID: {}\nDescription: {}",
                    task_id, spawn_args.description
                )))
            } else {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                Err(ToolError::retryable(format!(
                    "Failed to spawn subtask (HTTP {}): {}",
                    status, body
                )))
            }
        })
    }

    fn is_read_only(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_tool_schema() {
        let tool = SpawnSubtaskTool::new("http://localhost:8080");
        let schema = tool.parameters_schema();

        assert!(schema.get("properties").is_some());
        assert!(schema["properties"].get("description").is_some());
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("description"))
        );
    }

    #[test]
    fn test_spawn_tool_with_context() {
        let tool = SpawnSubtaskTool::new("http://localhost:8080")
            .with_parent_task("parent-123")
            .with_chat_context("chat-456", "adapter-789");

        assert_eq!(tool.parent_task_id, Some("parent-123".to_string()));
        assert_eq!(tool.chat_key, Some("chat-456".to_string()));
        assert_eq!(tool.adapter_key, Some("adapter-789".to_string()));
    }
}
