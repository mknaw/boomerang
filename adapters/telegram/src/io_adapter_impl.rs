use common::restate::{IoAdapter, SendMessageRequest};
use restate_sdk::prelude::*;
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct IoAdapterImpl {
    tx: mpsc::UnboundedSender<SendMessageRequest>,
}

impl IoAdapterImpl {
    pub fn new(tx: mpsc::UnboundedSender<SendMessageRequest>) -> Self {
        Self { tx }
    }
}

impl IoAdapter for IoAdapterImpl {
    async fn send_message(
        &self,
        _ctx: ObjectContext<'_>,
        request: Json<SendMessageRequest>,
    ) -> HandlerResult<()> {
        let request = request.0;
        info!(
            "IoAdapter: queueing message for {} ({})",
            request.platform_type, request.external_chat_id
        );

        self.tx.send(request).map_err(|e| {
            error!("Failed to queue message: {}", e);
            HandlerError::from(anyhow::anyhow!("Failed to queue message: {}", e))
        })?;

        Ok(())
    }
}
