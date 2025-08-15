use std::sync::Arc;

use anyhow::Result;
use common::config::{AIConfig, AIProviderConfig, ProviderType};
use tracing::warn;

pub mod metrics;
pub mod provider;
pub mod providers {
    pub mod openai;
    pub mod openrouter;
}
pub mod session;
pub mod summarizer;
pub mod types;

pub fn create_provider_from_config(
    config: &AIProviderConfig,
) -> Result<Arc<dyn provider::Provider>> {
    match config.provider {
        ProviderType::OpenAI => Ok(Arc::new(providers::openai::OpenAIProvider::new(
            &config.model,
            &config.api_key,
        )?)),
        ProviderType::OpenRouter => Ok(Arc::new(providers::openrouter::OpenRouterProvider::new(
            &config.model,
            &config.api_key,
        )?)),
    }
}

pub fn create_workhorse_provider(config: &AIConfig) -> Result<Arc<dyn provider::Provider>> {
    create_provider_from_config(&config.workhorse)
}

pub fn create_summarization_provider(config: &AIConfig) -> Result<Arc<dyn provider::Provider>> {
    match &config.summarization {
        Some(summarization_config) => create_provider_from_config(summarization_config),
        None => {
            warn!(
                "No summarization provider configured, using workhorse model for pruning/summarization"
            );
            create_provider_from_config(&config.workhorse)
        }
    }
}
