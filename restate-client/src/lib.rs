use anyhow::Result;
pub use common::{ChatSessionAction, Service};
use common::{PlatformOrigin, Turn};
use reqwest::Client;
use serde::Serialize;
use tracing::debug;

pub struct RestateClient {
    client: Client,
    base_url: String,
    adapter_key: String,
}

impl RestateClient {
    pub fn new(base_url: String, adapter_key: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            adapter_key,
        }
    }

    pub async fn invoke<T: Serialize>(
        &self,
        service: Service,
        key: &str,
        body: Option<T>,
    ) -> Result<()> {
        let url = format!(
            "{}/{}/{}/{}",
            self.base_url,
            service.as_str(),
            key,
            service.action_str()
        );
        debug!("Calling Restate (fire-and-forget): POST {}", url);

        let mut request = self.client.post(&url).header("Accept", "application/json");

        if let Some(body) = body {
            request = request
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&body)?);
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Restate error {}: {}", status, body);
        }

        debug!(
            "Request to Restate successful (service: {}, key: {})",
            service.as_str(),
            key
        );
        Ok(())
    }

    pub async fn send_message(
        &self,
        chat_key: &str,
        platform_type: &str,
        external_chat_id: &str,
        content: &str,
    ) -> Result<()> {
        let turn = Turn::user_message(content).with_platform_origin(PlatformOrigin {
            platform_type: platform_type.to_string(),
            external_chat_id: external_chat_id.to_string(),
            adapter_key: self.adapter_key.clone(),
        });

        self.invoke(
            Service::ChatSession(ChatSessionAction::Message),
            chat_key,
            Some(turn),
        )
        .await
    }

    pub async fn clear_history(&self, chat_key: &str) -> Result<()> {
        self.invoke(
            Service::ChatSession(ChatSessionAction::Clear),
            chat_key,
            None::<()>,
        )
        .await
    }
}
