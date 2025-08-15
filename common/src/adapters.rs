use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Trait for input adapters that receive messages from external platforms
#[async_trait]
pub trait InputAdapter: Send + Sync {
    /// Unique identifier for this adapter (e.g., "telegram", "discord")
    fn id(&self) -> &str;

    /// Start receiving messages (blocking)
    async fn run(&self, sender: mpsc::Sender<IncomingMessage>) -> anyhow::Result<()>;
}

/// Commands that can be sent through the adapter system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdapterCommand {
    ClearHistory,
}

/// Message received from an external platform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingMessage {
    pub adapter_id: String,
    pub external_chat_id: String,
    pub payload: MessagePayload,
    pub metadata: Option<serde_json::Value>,
}

/// The payload of an incoming message - either text content or a command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessagePayload {
    Text(String),
    Command(AdapterCommand),
}

impl IncomingMessage {
    pub fn text(
        adapter_id: impl Into<String>,
        external_chat_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            external_chat_id: external_chat_id.into(),
            payload: MessagePayload::Text(content.into()),
            metadata: None,
        }
    }

    pub fn command(
        adapter_id: impl Into<String>,
        external_chat_id: impl Into<String>,
        command: AdapterCommand,
    ) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            external_chat_id: external_chat_id.into(),
            payload: MessagePayload::Command(command),
            metadata: None,
        }
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}
