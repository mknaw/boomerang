use std::sync::Arc;

use chrono::Utc;
use common::{
    config::Config,
    restate::{IoAdapterClient, TaskArgs, TaskExecution, TaskResult, TaskStatusResponse},
};
use restate_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

use crate::core::{Agent, Context, Task, TaskConstraints, WorkerAgent};

pub struct TaskExecutionImpl {
    config: Arc<Config>,
}

impl TaskExecutionImpl {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskState {
    task_id: String,
    status: String,
    result: Option<String>,
    error: Option<String>,
    subtasks: Vec<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
}

impl TaskExecution for TaskExecutionImpl {
    async fn run(
        &self,
        ctx: ObjectContext<'_>,
        task_args: Json<TaskArgs>,
    ) -> HandlerResult<Json<TaskResult>> {
        let args = task_args.0;
        let task_id = args.task_id.clone();
        info!("Starting task execution: {}", task_id);

        let state = TaskState {
            task_id: task_id.clone(),
            status: "running".to_string(),
            result: None,
            error: None,
            subtasks: Vec::new(),
            started_at: Some(Utc::now().to_rfc3339()),
            completed_at: None,
        };
        ctx.set("state", serde_json::to_string(&state)?);

        let task = Task {
            id: args.task_id.clone(),
            description: args.description.clone(),
            parent_task_id: args.parent_task_id.clone(),
            constraints: args.constraints.as_ref().map(|c| TaskConstraints {
                max_tokens: c.max_tokens,
                allowed_tools: c.allowed_tools.clone(),
                timeout_seconds: c.timeout_seconds,
            }),
        };

        let worker = WorkerAgent::new(format!("worker-{}", task.id), self.config.clone());

        let context = match &task.parent_task_id {
            Some(parent_id) => Context::new().child(parent_id),
            None => Context::new(),
        };

        let result = worker.execute(task, &context).await;

        let (final_status, result_str, error_str, subtasks) = match result {
            Ok(output) => (
                "completed".to_string(),
                Some(output.result),
                None,
                output.subtasks_spawned,
            ),
            Err(e) => {
                error!("Task {} failed: {}", task_id, e);
                ("failed".to_string(), None, Some(e.to_string()), Vec::new())
            }
        };

        let final_state = TaskState {
            task_id: task_id.clone(),
            status: final_status.clone(),
            result: result_str.clone(),
            error: error_str.clone(),
            subtasks: subtasks.clone(),
            started_at: state.started_at,
            completed_at: Some(Utc::now().to_rfc3339()),
        };
        ctx.set("state", serde_json::to_string(&final_state)?);

        if let Some(adapter_key) = &args.adapter_key
            && let (Some(result), Some(chat_key)) = (&result_str, &args.chat_key)
        {
            debug!("Sending task result to adapter: {}", adapter_key);
            ctx.object_client::<IoAdapterClient>(adapter_key)
                .send_message(Json(common::restate::SendMessageRequest {
                    platform_type: "task".to_string(),
                    external_chat_id: chat_key.clone(),
                    content: result.clone(),
                }))
                .call()
                .await?;
        }

        Ok(Json(TaskResult {
            task_id,
            status: final_status,
            result: result_str,
            error: error_str,
            subtasks,
        }))
    }

    async fn status(&self, ctx: ObjectContext<'_>) -> HandlerResult<Json<TaskStatusResponse>> {
        let state_json: Option<String> = ctx.get("state").await?;

        let response = match state_json {
            Some(json) => {
                let state: TaskState = serde_json::from_str(&json)?;
                TaskStatusResponse {
                    task_id: state.task_id,
                    status: state.status,
                    started_at: state.started_at,
                    completed_at: state.completed_at,
                }
            }
            None => TaskStatusResponse {
                task_id: ctx.key().to_string(),
                status: "not_found".to_string(),
                started_at: None,
                completed_at: None,
            },
        };

        Ok(Json(response))
    }

    async fn cancel(&self, ctx: ObjectContext<'_>) -> HandlerResult<()> {
        let state_json: Option<String> = ctx.get("state").await?;

        if let Some(json) = state_json {
            let mut state: TaskState = serde_json::from_str(&json)?;

            if state.status == "running" || state.status == "pending" {
                state.status = "cancelled".to_string();
                state.completed_at = Some(Utc::now().to_rfc3339());
                ctx.set("state", serde_json::to_string(&state)?);
                info!("Task {} cancelled", state.task_id);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_state_serialization() {
        let state = TaskState {
            task_id: "test-1".to_string(),
            status: "running".to_string(),
            result: None,
            error: None,
            subtasks: vec!["sub-1".to_string()],
            started_at: Some("2024-01-01T00:00:00Z".to_string()),
            completed_at: None,
        };

        let json = serde_json::to_string(&state).unwrap();
        let parsed: TaskState = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.task_id, "test-1");
        assert_eq!(parsed.status, "running");
    }
}
