use async_trait::async_trait;

pub mod ntfy;

#[async_trait]
pub trait NotificationService: Send + Sync {
    async fn send_notification(&self, title: &str, message: &str) -> anyhow::Result<()>;
}
