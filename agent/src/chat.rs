use std::sync::Arc;

use common::{
    Turn, TurnKind,
    config::Config,
    restate::{ChatSession, IoAdapterClient, SendMessageRequest},
};
use restate_sdk::prelude::*;
use tracing::{debug, error, info, instrument, warn};

use crate::{
    ai::{create_summarization_provider, types::Message},
    executor::{AgentExecutor, PlatformContext},
    memory::FileMemory,
    pruning::MessagePruner,
};

pub struct ChatSessionImpl;

fn serialize_turns(turns: &[Turn]) -> serde_json::Result<String> {
    serde_json::to_string(turns)
}

impl ChatSession for ChatSessionImpl {
    #[instrument(skip(self, ctx, request), fields(chat_key = %ctx.key()))]
    async fn message(&self, ctx: ObjectContext<'_>, request: Json<Turn>) -> HandlerResult<()> {
        let turn = request.0;
        let chat_key = ctx.key().to_string();

        let platform_origin = turn.platform_origin.clone();
        let (platform_type, external_chat_id, adapter_key) = match &platform_origin {
            Some(origin) => (
                origin.platform_type.clone(),
                origin.external_chat_id.clone(),
                origin.adapter_key.clone(),
            ),
            None => {
                error!(
                    "ChatSession[{}]: Turn missing platform_origin, cannot respond",
                    chat_key
                );
                return Err(HandlerError::from(anyhow::anyhow!(
                    "Turn missing platform_origin"
                )));
            }
        };

        info!(
            "ChatSession[{}]: received turn from {} ({}): {:?}",
            chat_key, platform_type, external_chat_id, turn.kind
        );

        let state_json: Option<String> = ctx.get("messages").await?;
        let mut turns: Vec<Turn> = state_json
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        info!(
            "ChatSession[{}]: current turns ({} total)",
            chat_key,
            turns.len()
        );

        match &turn.kind {
            TurnKind::ScheduledCompletion {
                result, is_final, ..
            } => {
                info!(
                    "ChatSession[{}]: forwarding scheduled result directly (final: {})",
                    chat_key, is_final
                );
                turns.push(turn.clone());

                let updated_json = serialize_turns(&turns).map_err(|e| {
                    HandlerError::from(anyhow::anyhow!("Serialization failed: {}", e))
                })?;
                ctx.set("messages", updated_json);

                let send_request = SendMessageRequest {
                    platform_type,
                    external_chat_id,
                    content: format_scheduled_response(result, *is_final),
                };

                if let Err(e) = ctx
                    .object_client::<IoAdapterClient>(&adapter_key)
                    .send_message(Json(send_request))
                    .call()
                    .await
                {
                    error!(
                        "ChatSession[{}]: failed to send scheduled result to adapter {}: {}",
                        chat_key, adapter_key, e
                    );
                }

                return Ok(());
            }
            TurnKind::UserMessage { .. } => {
                turns.push(turn.clone());
                self.handle_user_message(
                    ctx,
                    &chat_key,
                    &platform_type,
                    &external_chat_id,
                    &adapter_key,
                    turns,
                )
                .await
            }
            other => {
                warn!(
                    "ChatSession[{}]: unexpected turn kind received: {:?}",
                    chat_key, other
                );
                Ok(())
            }
        }
    }

    async fn history(&self, ctx: ObjectContext<'_>) -> HandlerResult<String> {
        let state_json: Option<String> = ctx.get("messages").await?;
        Ok(state_json.unwrap_or_else(|| "[]".to_string()))
    }

    async fn clear(&self, ctx: ObjectContext<'_>) -> HandlerResult<()> {
        let chat_key = ctx.key();
        debug!("ChatSession[{}]: clearing conversation history", chat_key);

        ctx.set("messages", "[]".to_string());
        Ok(())
    }
}

impl ChatSessionImpl {
    #[instrument(skip(self, ctx), fields(chat_key))]
    async fn handle_user_message(
        &self,
        ctx: ObjectContext<'_>,
        chat_key: &str,
        platform_type: &str,
        external_chat_id: &str,
        adapter_key: &str,
        mut turns: Vec<Turn>,
    ) -> HandlerResult<()> {
        let config = Config::global();
        let window_size = config.agent.context_window_size;
        let platform_ctx = PlatformContext {
            platform_type: platform_type.to_string(),
            external_chat_id: external_chat_id.to_string(),
            adapter_key: adapter_key.to_string(),
        };
        let executor = AgentExecutor::new(config.clone(), chat_key.to_string())
            .with_platform_context(platform_ctx);

        let messages: Vec<Message> = turns.iter().map(Message::from).collect();

        let system_prompt = AgentExecutor::default_system_prompt(Some(platform_type));
        let llm_result_json = ctx
            .run(|| async {
                let msg = executor
                    .complete(&messages, &system_prompt)
                    .await
                    .map_err(|e| HandlerError::from(anyhow::anyhow!("LLM error: {}", e)))?;
                serde_json::to_string(&msg)
                    .map_err(|e| HandlerError::from(anyhow::anyhow!("Serialization error: {}", e)))
            })
            .await;

        let llm_result = llm_result_json.and_then(|json| {
            serde_json::from_str::<Message>(&json)
                .map_err(|e| TerminalError::new(format!("Deserialization error: {}", e)))
        });

        match llm_result {
            Ok(response_message) => {
                let response_content = response_message
                    .content
                    .clone()
                    .unwrap_or_else(|| "No response content".to_string());

                info!(
                    "ChatSession[{}]: assistant response: {}",
                    chat_key, response_content
                );

                let response_turn = Turn::from(&response_message);
                turns.push(response_turn);

                let pruning_config = &config.agent.pruning;

                if MessagePruner::needs_hard_prune(&turns, pruning_config.hard_limit) {
                    warn!(
                        "ChatSession[{}]: hard limit {} exceeded, performing emergency prune",
                        chat_key, pruning_config.hard_limit
                    );
                    turns = MessagePruner::hard_prune(turns, pruning_config.hard_limit);
                } else if turns.len() > pruning_config.soft_limit {
                    match create_summarization_provider(&config.ai) {
                        Ok(provider) => {
                            let memory: Option<Arc<dyn crate::memory::Memory>> = if pruning_config
                                .enable_memory_persist
                            {
                                Some(Arc::new(FileMemory::new(&config.memory.scratch_space_path)))
                            } else {
                                None
                            };

                            let pruner = MessagePruner::new(
                                provider,
                                chat_key.to_string(),
                                memory,
                                pruning_config.clone(),
                            );

                            match pruner.prune(turns.clone()).await {
                                Ok(pruned) => {
                                    info!(
                                        "ChatSession[{}]: pruned {} -> {} turns",
                                        chat_key,
                                        turns.len(),
                                        pruned.len()
                                    );
                                    turns = pruned;
                                }
                                Err(e) => {
                                    warn!(
                                        "ChatSession[{}]: intelligent pruning failed: {}, falling back to FIFO",
                                        chat_key, e
                                    );
                                    if turns.len() > window_size {
                                        turns.drain(..turns.len() - window_size);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!(
                                "ChatSession[{}]: failed to create summarization provider: {}, using FIFO",
                                chat_key, e
                            );
                            if turns.len() > window_size {
                                turns.drain(..turns.len() - window_size);
                            }
                        }
                    }
                }

                let updated_json = serialize_turns(&turns).map_err(|e| {
                    HandlerError::from(anyhow::anyhow!("Serialization failed: {}", e))
                })?;
                ctx.set("messages", updated_json);

                let send_request = SendMessageRequest {
                    platform_type: platform_type.to_string(),
                    external_chat_id: external_chat_id.to_string(),
                    content: response_content,
                };

                if let Err(e) = ctx
                    .object_client::<IoAdapterClient>(adapter_key)
                    .send_message(Json(send_request))
                    .call()
                    .await
                {
                    error!(
                        "ChatSession[{}]: failed to send response to adapter {}: {}",
                        chat_key, adapter_key, e
                    );
                }

                Ok(())
            }
            Err(e) => {
                error!("ChatSession[{}]: LLM session failed: {}", chat_key, e);

                let error_request = SendMessageRequest {
                    platform_type: platform_type.to_string(),
                    external_chat_id: external_chat_id.to_string(),
                    content: "Sorry, I encountered an error processing your message.".to_string(),
                };

                if let Err(send_err) = ctx
                    .object_client::<IoAdapterClient>(adapter_key)
                    .send_message(Json(error_request))
                    .call()
                    .await
                {
                    error!(
                        "ChatSession[{}]: failed to send error to adapter {}: {}",
                        chat_key, adapter_key, send_err
                    );
                }

                Err(HandlerError::from(anyhow::anyhow!(
                    "LLM session failed: {}",
                    e
                )))
            }
        }
    }
}

fn format_scheduled_response(result: &str, is_final: bool) -> String {
    let prefix = if is_final {
        "📅 Scheduled task completed:\n\n"
    } else {
        "📅 Scheduled task result:\n\n"
    };
    format!("{}{}", prefix, result)
}
