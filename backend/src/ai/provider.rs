use async_trait::async_trait;
use anyhow::Result;
use futures::Stream;
use std::pin::Pin;

use crate::ai::types::{Message, Usage};

#[derive(Debug)]
pub enum ProviderError {
    RequestFailed(String),
    Authentication(String),
    RateLimited(String),
    InvalidRequest(String),
    Unknown(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::RequestFailed(msg) => write!(f, "Request failed: {}", msg),
            ProviderError::Authentication(msg) => write!(f, "Authentication error: {}", msg),
            ProviderError::RateLimited(msg) => write!(f, "Rate limited: {}", msg),
            ProviderError::InvalidRequest(msg) => write!(f, "Invalid request: {}", msg),
            ProviderError::Unknown(msg) => write!(f, "Unknown error: {}", msg),
        }
    }
}

impl std::error::Error for ProviderError {}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub message: Message,
    pub usage: Usage,
}

pub type MessageStream = Pin<Box<dyn Stream<Item = Result<CompletionResponse, ProviderError>> + Send>>;

#[async_trait]
pub trait Provider: Send + Sync {
    async fn complete(
        &self,
        system_prompt: &str,
        messages: &[Message],
        tools: Option<&[rmcp::model::Tool]>,
    ) -> Result<CompletionResponse, ProviderError>;

    async fn stream(
        &self,
        system_prompt: &str,
        messages: &[Message],
        tools: Option<&[rmcp::model::Tool]>,
    ) -> Result<MessageStream, ProviderError>;

    fn supports_streaming(&self) -> bool {
        false
    }

    fn model_name(&self) -> &str;
}