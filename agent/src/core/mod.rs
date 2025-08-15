use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod orchestrator;
pub mod worker;

pub use orchestrator::OrchestratorAgent;
pub use worker::WorkerAgent;

pub type AgentId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub parent_task_id: Option<String>,
    pub constraints: Option<TaskConstraints>,
}

impl Task {
    pub fn new(id: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            parent_task_id: None,
            constraints: None,
        }
    }

    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_task_id = Some(parent_id.into());
        self
    }

    pub fn with_constraints(mut self, constraints: TaskConstraints) -> Self {
        self.constraints = Some(constraints);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskConstraints {
    pub max_tokens: Option<usize>,
    pub allowed_tools: Option<Vec<String>>,
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentOutput {
    pub result: String,
    pub artifacts: Vec<Artifact>,
    pub subtasks_spawned: Vec<String>,
}

impl AgentOutput {
    pub fn new(result: impl Into<String>) -> Self {
        Self {
            result: result.into(),
            ..Default::default()
        }
    }

    pub fn with_artifacts(mut self, artifacts: Vec<Artifact>) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub fn with_subtasks(mut self, subtasks: Vec<String>) -> Self {
        self.subtasks_spawned = subtasks;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub name: String,
    pub content_type: String,
    pub data: String,
}

impl Artifact {
    pub fn text(name: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            content_type: "text/plain".to_string(),
            data: data.into(),
        }
    }

    pub fn json(name: impl Into<String>, data: impl Serialize) -> Result<Self, serde_json::Error> {
        Ok(Self {
            name: name.into(),
            content_type: "application/json".to_string(),
            data: serde_json::to_string(&data)?,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Context {
    pub parent_task_id: Option<String>,
    pub depth: usize,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Context {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn child(&self, parent_task_id: &str) -> Self {
        Self {
            parent_task_id: Some(parent_task_id.to_string()),
            depth: self.depth + 1,
            metadata: self.metadata.clone(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub task: Task,
    pub status: TaskStatus,
    pub output: Option<AgentOutput>,
    pub error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl TaskState {
    pub fn new(task: Task) -> Self {
        Self {
            task,
            status: TaskStatus::Pending,
            output: None,
            error: None,
            started_at: None,
            completed_at: None,
        }
    }
}

pub trait Agent: Send + Sync {
    fn id(&self) -> &AgentId;
    fn capabilities(&self) -> &[String];

    fn execute<'a>(
        &'a self,
        task: Task,
        context: &'a Context,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<AgentOutput>> + Send + 'a>>;
}

pub type AgentRef = std::sync::Arc<dyn Agent>;
