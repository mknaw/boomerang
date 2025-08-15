use std::sync::Arc;

use anyhow::Result;
use common::{Turn, TurnKind};
use serde::Deserialize;
use tracing::{debug, warn};

use super::{PruneAction, PruneDecision};
use crate::ai::{provider::Provider, session::Session};

const PRUNING_EVAL_PROMPT: &str = include_str!("../prompts/pruning_eval.txt");

pub struct PruningEvaluator {
    provider: Arc<dyn Provider>,
    chat_key: String,
}

impl PruningEvaluator {
    pub fn new(provider: Arc<dyn Provider>, chat_key: String) -> Self {
        Self { provider, chat_key }
    }

    pub async fn evaluate_batch(
        &self,
        turns: &[Turn],
        candidate_indices: &[usize],
    ) -> Result<Vec<PruneDecision>> {
        if candidate_indices.is_empty() {
            return Ok(Vec::new());
        }

        let candidates_text: Vec<String> = candidate_indices
            .iter()
            .filter_map(|&i| {
                turns.get(i).map(|turn| {
                    let role = turn_role_str(turn);
                    let content = turn.content().unwrap_or("[no content]");
                    let truncated = if content.len() > 500 {
                        format!("{}...", &content[..500])
                    } else {
                        content.to_string()
                    };
                    format!("[Index {}] {}: {}", i, role, truncated)
                })
            })
            .collect();

        let prompt = format!(
            "{}\n\nChat context key: {}\n\n---\nMessages to evaluate:\n{}",
            PRUNING_EVAL_PROMPT,
            self.chat_key,
            candidates_text.join("\n\n")
        );

        let mut session = Session::new(self.provider.clone()).with_system_prompt(
            "You are a message pruning assistant. Always respond with valid JSON array.",
        );

        session.add_user_message(&prompt);

        let response = session.complete().await?;
        let content = response.message.content.unwrap_or_default();

        self.parse_decisions(&content, candidate_indices)
    }

    fn parse_decisions(
        &self,
        content: &str,
        candidate_indices: &[usize],
    ) -> Result<Vec<PruneDecision>> {
        let start = content.find('[').unwrap_or(0);
        let end = content.rfind(']').map(|i| i + 1).unwrap_or(content.len());
        let json_str = &content[start..end];

        let parsed: Vec<RawDecision> = match serde_json::from_str(json_str) {
            Ok(d) => d,
            Err(e) => {
                warn!(
                    "Failed to parse pruning decisions: {}, defaulting to drop",
                    e
                );
                return Ok(candidate_indices
                    .iter()
                    .map(|&i| PruneDecision {
                        index: i,
                        action: PruneAction::Drop,
                        memory_key: None,
                        reason: "Parse failure, defaulting to drop".into(),
                    })
                    .collect());
            }
        };

        let decisions = parsed
            .into_iter()
            .filter_map(|raw| {
                if !candidate_indices.contains(&raw.index) {
                    debug!(
                        "Ignoring decision for index {} not in candidates",
                        raw.index
                    );
                    return None;
                }

                let action = match raw.action.to_lowercase().as_str() {
                    "persist" => PruneAction::Persist,
                    "summarize" => PruneAction::Summarize,
                    "both" => PruneAction::Both,
                    "drop" => PruneAction::Drop,
                    _ => {
                        warn!(
                            "Unknown action '{}' for index {}, defaulting to drop",
                            raw.action, raw.index
                        );
                        PruneAction::Drop
                    }
                };

                let memory_key = raw.memory_key.map(|key| self.normalize_memory_key(&key));

                Some(PruneDecision {
                    index: raw.index,
                    action,
                    memory_key,
                    reason: raw.reason,
                })
            })
            .collect();

        Ok(decisions)
    }

    fn normalize_memory_key(&self, suggested_key: &str) -> String {
        let timestamp = chrono::Utc::now().timestamp();
        let slug: String = suggested_key
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .take(50)
            .collect();

        let slug = if slug.is_empty() { "context" } else { &slug };

        format!("{}/context/{}_{}", self.chat_key, timestamp, slug)
    }
}

fn turn_role_str(turn: &Turn) -> &'static str {
    match &turn.kind {
        TurnKind::UserMessage { .. } => "User",
        TurnKind::AssistantResponse { .. } => "Assistant",
        TurnKind::ToolInvocation { .. } => "Assistant",
        TurnKind::ToolResult { .. } => "Tool",
        TurnKind::SystemPrompt { .. } => "System",
        TurnKind::ScheduledCompletion { .. } => "Scheduled",
        TurnKind::InjectedContext { .. } => "Context",
    }
}

#[derive(Debug, Deserialize)]
struct RawDecision {
    index: usize,
    action: String,
    memory_key: Option<String>,
    reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::types::Message;

    #[test]
    fn test_normalize_memory_key() {
        let evaluator = PruningEvaluator {
            provider: Arc::new(MockProvider),
            chat_key: "user123".into(),
        };

        let key = evaluator.normalize_memory_key("important user preferences");
        assert!(key.starts_with("user123/context/"));
        assert!(
            key.contains("important_user_preferences") || key.contains("importantuserpreferences")
        );
    }

    struct MockProvider;

    #[async_trait::async_trait]
    impl Provider for MockProvider {
        async fn complete(
            &self,
            _system_prompt: &str,
            _messages: &[Message],
            _tools: Option<&[rmcp::model::Tool]>,
        ) -> Result<crate::ai::provider::CompletionResponse, crate::ai::provider::ProviderError>
        {
            Ok(crate::ai::provider::CompletionResponse {
                message: Message::assistant("[]"),
                usage: Default::default(),
            })
        }

        async fn stream(
            &self,
            _system_prompt: &str,
            _messages: &[Message],
            _tools: Option<&[rmcp::model::Tool]>,
        ) -> Result<crate::ai::provider::MessageStream, crate::ai::provider::ProviderError>
        {
            unimplemented!()
        }

        fn model_name(&self) -> &str {
            "mock"
        }
    }
}
