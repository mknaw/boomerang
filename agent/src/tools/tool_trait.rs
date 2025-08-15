use std::{pin::Pin, sync::Arc};

use anyhow::Result;
use serde::Serialize;

pub struct ToolOutput {
    pub content: String,
    pub metadata: Option<serde_json::Value>,
}

impl ToolOutput {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            metadata: None,
        }
    }

    pub fn with_metadata(mut self, metadata: impl Serialize) -> Result<Self> {
        self.metadata = Some(serde_json::to_value(metadata)?);
        Ok(self)
    }
}

pub struct ToolError {
    pub message: String,
    pub is_retryable: bool,
}

impl ToolError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            is_retryable: false,
        }
    }

    pub fn retryable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            is_retryable: true,
        }
    }
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::fmt::Debug for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolError")
            .field("message", &self.message)
            .field("is_retryable", &self.is_retryable)
            .finish()
    }
}

pub type ToolFuture =
    Pin<Box<dyn std::future::Future<Output = Result<ToolOutput, ToolError>> + Send>>;

pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;

    fn execute(&self, args: serde_json::Value) -> ToolFuture;

    fn is_read_only(&self) -> bool {
        false
    }
}

pub type ToolRef = Arc<dyn Tool>;
