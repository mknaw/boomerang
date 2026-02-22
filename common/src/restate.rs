use restate_sdk::prelude::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::turn::Turn;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SendMessageRequest {
    pub platform_type: String,
    pub external_chat_id: String,
    pub content: String,
}

#[restate_sdk::object]
pub trait IoAdapter {
    async fn send_message(request: Json<SendMessageRequest>) -> HandlerResult<()>;
}

#[restate_sdk::object]
pub trait ChatSession {
    async fn message(request: Json<Turn>) -> HandlerResult<()>;
    async fn history() -> HandlerResult<String>;
    async fn clear() -> HandlerResult<()>;
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ScheduleArgs {
    /// The original user request with timing info intact (e.g., "Monday at 8am remind me to check emails")
    pub request: String,
    /// Whether this is a recurring task
    #[serde(default)]
    pub recurring: bool,
    pub chat_key: String,
    pub platform_type: String,
    pub external_chat_id: String,
    pub adapter_key: String,
}

/// Inspection snapshot for a scheduled/cron task.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CronTaskStatus {
    /// Human-readable task description (timing language stripped).
    pub task: String,
    /// True for recurring tasks, false for one-shot.
    pub is_recurring: bool,
    /// RFC 3339 timestamp of when the task was first created.
    pub created_at: String,
    /// RFC 3339 timestamp of the most recent execution, if any.
    pub last_run_at: Option<String>,
    /// RFC 3339 timestamp of the next scheduled execution, if any.
    pub next_run_at: Option<String>,
    /// How many times this task has executed so far.
    pub run_count: u32,
    /// For recurring tasks, the interval between executions in seconds.
    pub interval_seconds: Option<u32>,
}

#[restate_sdk::object]
pub trait ScheduledSession {
    async fn run(spec: Json<ScheduleArgs>) -> HandlerResult<()>;
    async fn execute() -> HandlerResult<()>;
    async fn cancel() -> HandlerResult<()>;
    /// Returns the current inspection state of this scheduled task.
    /// Returns None if the task has not been initialised or has been cleared.
    #[shared]
    async fn status() -> HandlerResult<Json<Option<CronTaskStatus>>>;
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaskArgs {
    pub task_id: String,
    pub description: String,
    #[serde(default)]
    pub parent_task_id: Option<String>,
    #[serde(default)]
    pub constraints: Option<TaskConstraintsArgs>,
    #[serde(default)]
    pub chat_key: Option<String>,
    #[serde(default)]
    pub adapter_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct TaskConstraintsArgs {
    #[serde(default)]
    pub max_tokens: Option<usize>,
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaskResult {
    pub task_id: String,
    pub status: String,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub subtasks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaskStatusResponse {
    pub task_id: String,
    pub status: String,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
}

#[restate_sdk::object]
pub trait TaskExecution {
    async fn run(task: Json<TaskArgs>) -> HandlerResult<Json<TaskResult>>;
    async fn status() -> HandlerResult<Json<TaskStatusResponse>>;
    async fn cancel() -> HandlerResult<()>;
}

#[derive(Debug, Clone, Copy)]
pub enum Service {
    ChatSession(ChatSessionAction),
}

#[derive(Debug, Clone, Copy)]
pub enum ChatSessionAction {
    Message,
    History,
    Clear,
}

impl Service {
    pub fn as_str(&self) -> &'static str {
        match self {
            Service::ChatSession(_) => "ChatSession",
        }
    }

    pub fn action_str(&self) -> &'static str {
        match self {
            Service::ChatSession(action) => action.as_str(),
        }
    }
}

impl ChatSessionAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChatSessionAction::Message => "message",
            ChatSessionAction::History => "history",
            ChatSessionAction::Clear => "clear",
        }
    }
}
