use std::sync::Arc;

use async_trait::async_trait;
use common::{
    adapters::{IncomingMessage, InputAdapter},
    config::Config,
    restate::SendMessageRequest,
};
use teloxide::{prelude::*, types::ChatId};
use tokio::sync::mpsc;

mod bot;
mod io_adapter_impl;

pub use io_adapter_impl::IoAdapterImpl;

/// Create the Telegram output adapter (IoAdapter for Restate)
pub fn create_output_adapter() -> IoAdapterImpl {
    let (tx, mut rx) = mpsc::unbounded_channel::<SendMessageRequest>();

    tokio::spawn(async move {
        let cfg = Config::global();
        let bot = Bot::new(&cfg.telegram.bot_token);

        while let Some(req) = rx.recv().await {
            let chat_id: i64 = match req.external_chat_id.parse() {
                Ok(id) => id,
                Err(e) => {
                    tracing::error!("Invalid chat ID {}: {}", req.external_chat_id, e);
                    continue;
                }
            };
            if let Err(e) = bot.send_message(ChatId(chat_id), req.content).await {
                tracing::error!("Failed to send message: {}", e);
            }
        }
    });

    IoAdapterImpl::new(tx)
}

/// Create the Telegram input adapter
pub fn create_input_adapter() -> Arc<dyn InputAdapter> {
    Arc::new(TelegramInputAdapter)
}

/// Telegram-specific input adapter implementation
#[derive(Clone)]
pub struct TelegramInputAdapter;

#[async_trait]
impl InputAdapter for TelegramInputAdapter {
    fn id(&self) -> &str {
        "telegram"
    }

    async fn run(&self, sender: mpsc::Sender<IncomingMessage>) -> anyhow::Result<()> {
        let cfg = Config::global();
        tracing::info!("Starting Telegram input adapter...");
        bot::run_bot(&cfg.telegram.bot_token, sender).await;
        Ok(())
    }
}
