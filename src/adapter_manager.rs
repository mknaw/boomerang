use std::{sync::Arc, time::Duration};

use anyhow::Result;
use common::adapters::{AdapterCommand, IncomingMessage, InputAdapter, MessagePayload};
use restate_client::RestateClient;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

pub struct AdapterManager {
    input_adapters: Vec<Arc<dyn InputAdapter>>,
    restate_client: Arc<RestateClient>,
}

impl AdapterManager {
    pub fn new(
        input_adapters: Vec<Arc<dyn InputAdapter>>,
        restate_client: Arc<RestateClient>,
    ) -> Self {
        Self {
            input_adapters,
            restate_client,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let (tx, mut rx) = mpsc::channel(100);

        for adapter in &self.input_adapters {
            let tx = tx.clone();
            let adapter = adapter.clone();
            tokio::spawn(async move {
                Self::run_adapter_with_restart(adapter, tx).await;
            });
        }

        while let Some(msg) = rx.recv().await {
            if let Err(e) = self.route_message(&msg).await {
                error!("Failed to route message: {}", e);
            }
        }

        Ok(())
    }

    async fn run_adapter_with_restart(
        adapter: Arc<dyn InputAdapter>,
        tx: mpsc::Sender<IncomingMessage>,
    ) {
        let adapter_id = adapter.id().to_string();
        let mut backoff = Duration::from_secs(1);
        const MAX_BACKOFF: Duration = Duration::from_secs(60);

        loop {
            info!("Starting input adapter '{}'", adapter_id);

            match adapter.run(tx.clone()).await {
                Ok(()) => {
                    info!("Input adapter '{}' shut down cleanly", adapter_id);
                    break;
                }
                Err(e) => {
                    error!(
                        "Input adapter '{}' failed: {}. Restarting in {:?}...",
                        adapter_id, e, backoff
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                }
            }
        }
    }

    async fn route_message(&self, msg: &IncomingMessage) -> Result<()> {
        let chat_key = &msg.external_chat_id;

        match &msg.payload {
            MessagePayload::Text(content) => {
                debug!(
                    "Routing text message from '{}' chat '{}' to Restate",
                    msg.adapter_id, chat_key
                );
                self.restate_client
                    .send_message(chat_key, &msg.adapter_id, &msg.external_chat_id, content)
                    .await
            }
            MessagePayload::Command(cmd) => {
                debug!(
                    "Routing command {:?} from '{}' chat '{}' to Restate",
                    cmd, msg.adapter_id, chat_key
                );
                match cmd {
                    AdapterCommand::ClearHistory => {
                        self.restate_client.clear_history(chat_key).await
                    }
                }
            }
        }
    }
}
