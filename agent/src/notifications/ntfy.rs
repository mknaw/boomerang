use async_trait::async_trait;
use reqwest::Client;
use tracing::{debug, error};

use super::NotificationService;

pub struct NtfyService {
    client: Client,
    url: String,
}

impl NtfyService {
    pub fn new(url: String) -> Self {
        Self {
            client: Client::new(),
            url,
        }
    }
}

#[async_trait]
impl NotificationService for NtfyService {
    async fn send_notification(&self, title: &str, message: &str) -> anyhow::Result<()> {
        debug!("Sending notification to ntfy: {}", title);

        let response = self
            .client
            .post(&self.url)
            .header("Title", title)
            .body(message.to_string())
            .send()
            .await?;

        if response.status().is_success() {
            debug!("Notification sent successfully");
            Ok(())
        } else {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read response".to_string());
            error!("Failed to send notification: {} - {}", status, body);
            Err(anyhow::anyhow!(
                "Failed to send notification: {} - {}",
                status,
                body
            ))
        }
    }
}
