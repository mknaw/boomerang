use std::sync::Arc;

use anyhow::Result;
use common::{Turn, config::PruningConfig};
use tracing::{debug, info, warn};

use super::{
    PruneAction, PruneDecision, ToolChain, evaluator::PruningEvaluator, scorer::TurnScorer,
};
use crate::{
    ai::{provider::Provider, summarizer::Summarizer, types::Message},
    memory::{Memory, Metadata},
};

pub struct MessagePruner {
    scorer: TurnScorer,
    evaluator: PruningEvaluator,
    summarizer: Summarizer,
    memory: Option<Arc<dyn Memory>>,
    config: PruningConfig,
}

impl MessagePruner {
    pub fn new(
        provider: Arc<dyn Provider>,
        chat_key: String,
        memory: Option<Arc<dyn Memory>>,
        config: PruningConfig,
    ) -> Self {
        Self {
            scorer: TurnScorer::new(&config),
            evaluator: PruningEvaluator::new(provider.clone(), chat_key),
            summarizer: Summarizer::new(provider),
            memory,
            config,
        }
    }

    pub async fn prune(&self, turns: Vec<Turn>) -> Result<Vec<Turn>> {
        let current_len = turns.len();

        if current_len <= self.config.soft_limit {
            debug!(
                "Turn count {} is within soft limit {}, no pruning needed",
                current_len, self.config.soft_limit
            );
            return Ok(turns);
        }

        info!(
            "Turn count {} exceeds soft limit {}, starting intelligent pruning",
            current_len, self.config.soft_limit
        );

        let target_removals = current_len - self.config.soft_limit + 5;

        let scored = self.scorer.score_all(&turns);
        let candidates = self
            .scorer
            .select_candidates(&scored, target_removals.min(self.config.batch_size));

        if candidates.is_empty() {
            debug!("No suitable pruning candidates found");
            return Ok(turns);
        }

        let decisions = if self.config.enable_summarization || self.config.enable_memory_persist {
            self.evaluator.evaluate_batch(&turns, &candidates).await?
        } else {
            candidates
                .iter()
                .map(|&i| PruneDecision {
                    index: i,
                    action: PruneAction::Drop,
                    memory_key: None,
                    reason: "Evaluation disabled".into(),
                })
                .collect()
        };

        let result = self.apply_decisions(turns, decisions).await?;

        info!(
            "Pruning complete: {} turns -> {} turns",
            current_len,
            result.len()
        );

        Ok(result)
    }

    async fn apply_decisions(
        &self,
        turns: Vec<Turn>,
        decisions: Vec<PruneDecision>,
    ) -> Result<Vec<Turn>> {
        let tool_chains = super::scorer::find_tool_chains(&turns);

        let mut indices_to_remove: std::collections::HashSet<usize> =
            std::collections::HashSet::new();
        let mut turns_to_summarize: Vec<usize> = Vec::new();
        let mut persist_tasks: Vec<(usize, String)> = Vec::new();

        for decision in &decisions {
            let chain_indices = self.get_chain_indices(decision.index, &tool_chains);

            match decision.action {
                PruneAction::Drop => {
                    for idx in chain_indices {
                        indices_to_remove.insert(idx);
                    }
                }
                PruneAction::Summarize => {
                    for idx in &chain_indices {
                        turns_to_summarize.push(*idx);
                        indices_to_remove.insert(*idx);
                    }
                }
                PruneAction::Persist => {
                    if self.config.enable_memory_persist {
                        if let Some(key) = &decision.memory_key {
                            persist_tasks.push((decision.index, key.clone()));
                        }
                    }
                    for idx in chain_indices {
                        indices_to_remove.insert(idx);
                    }
                }
                PruneAction::Both => {
                    if self.config.enable_memory_persist {
                        if let Some(key) = &decision.memory_key {
                            persist_tasks.push((decision.index, key.clone()));
                        }
                    }
                    for idx in &chain_indices {
                        turns_to_summarize.push(*idx);
                        indices_to_remove.insert(*idx);
                    }
                }
            }
        }

        if self.config.enable_memory_persist {
            if let Some(ref memory) = self.memory {
                for (idx, key) in persist_tasks {
                    if let Some(turn) = turns.get(idx) {
                        if let Some(content) = turn.content() {
                            let metadata = Metadata {
                                tags: vec!["auto-pruned".into()],
                                source: Some("pruning".into()),
                            };
                            if let Err(e) = memory.write(&key, content, Some(metadata)).await {
                                warn!("Failed to persist turn to memory: {}", e);
                            } else {
                                debug!("Persisted turn {} to memory key: {}", idx, key);
                            }
                        }
                    }
                }
            }
        }

        let mut result: Vec<Turn> = Vec::new();
        let mut summary_turn: Option<Turn> = None;

        if self.config.enable_summarization && !turns_to_summarize.is_empty() {
            turns_to_summarize.sort();
            turns_to_summarize.dedup();

            let messages_to_summarize: Vec<Message> = turns_to_summarize
                .iter()
                .filter_map(|&i| turns.get(i))
                .map(Message::from)
                .collect();

            if !messages_to_summarize.is_empty() {
                match self.summarizer.summarize(&messages_to_summarize).await {
                    Ok(summary_msg) => {
                        debug!("Created summary for {} turns", turns_to_summarize.len());
                        let first_idx = *turns_to_summarize.first().unwrap_or(&0);
                        let last_idx = *turns_to_summarize.last().unwrap_or(&0);
                        summary_turn = Some(Turn::summary(
                            summary_msg.content.unwrap_or_default(),
                            (first_idx, last_idx),
                        ));
                    }
                    Err(e) => {
                        warn!("Failed to create summary: {}", e);
                    }
                }
            }
        }

        let first_removed = indices_to_remove.iter().min().copied();

        for (i, turn) in turns.into_iter().enumerate() {
            if indices_to_remove.contains(&i) {
                continue;
            }

            if let Some(first) = first_removed {
                if i == first {
                    if let Some(summary) = summary_turn.take() {
                        result.push(summary);
                    }
                }
            }

            result.push(turn);
        }

        if let Some(summary) = summary_turn {
            result.insert(0, summary);
        }

        Ok(result)
    }

    fn get_chain_indices(&self, index: usize, chains: &[ToolChain]) -> Vec<usize> {
        for chain in chains {
            if index >= chain.start_index && index <= chain.end_index {
                return (chain.start_index..=chain.end_index).collect();
            }
        }
        vec![index]
    }

    pub fn needs_hard_prune(turns: &[Turn], hard_limit: usize) -> bool {
        turns.len() > hard_limit
    }

    pub fn hard_prune(turns: Vec<Turn>, hard_limit: usize) -> Vec<Turn> {
        if turns.len() <= hard_limit {
            return turns;
        }

        let excess = turns.len() - hard_limit;
        warn!(
            "Hard pruning {} turns to meet hard limit {}",
            excess, hard_limit
        );

        turns.into_iter().skip(excess).collect()
    }
}

#[cfg(test)]
mod tests {
    use common::TurnKind;

    use super::*;

    #[test]
    fn test_hard_prune() {
        let turns: Vec<Turn> = (0..60)
            .map(|i| Turn::user_message(format!("Message {}", i)))
            .collect();

        let pruned = MessagePruner::hard_prune(turns, 50);
        assert_eq!(pruned.len(), 50);

        if let TurnKind::UserMessage { content } = &pruned[0].kind {
            assert!(content.contains("10"));
        }
    }

    #[test]
    fn test_needs_hard_prune() {
        let turns: Vec<Turn> = (0..60)
            .map(|i| Turn::user_message(format!("Message {}", i)))
            .collect();

        assert!(MessagePruner::needs_hard_prune(&turns, 50));
        assert!(!MessagePruner::needs_hard_prune(&turns, 70));
    }
}
