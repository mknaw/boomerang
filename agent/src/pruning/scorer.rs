use chrono::Utc;
use common::{Turn, TurnKind, config::PruningConfig};

use super::{ScoredMessage, ToolChain};

pub struct TurnScorer {
    max_age_hours: f64,
}

impl TurnScorer {
    pub fn new(config: &PruningConfig) -> Self {
        Self {
            max_age_hours: config.max_age_hours,
        }
    }

    pub fn score(&self, turn: &Turn, index: usize) -> ScoredMessage {
        if turn.is_summary() {
            return ScoredMessage {
                index,
                score: 0.0,
                tool_chain_start: None,
            };
        }

        let age_factor = self.compute_age_factor(turn);
        let length_factor = self.compute_length_factor(turn);
        let role_factor = self.compute_role_factor(turn);

        let score = age_factor * 0.5 + length_factor * 0.3 + role_factor * 0.2;

        ScoredMessage {
            index,
            score,
            tool_chain_start: None,
        }
    }

    fn compute_age_factor(&self, turn: &Turn) -> f64 {
        let now = Utc::now();
        let age_hours = (now - turn.timestamp).num_minutes() as f64 / 60.0;
        (age_hours / self.max_age_hours).min(1.0).max(0.0)
    }

    fn compute_length_factor(&self, turn: &Turn) -> f64 {
        let content_len = turn.content().map(|c| c.len()).unwrap_or(0);
        (content_len as f64 / 2000.0).min(1.0)
    }

    fn compute_role_factor(&self, turn: &Turn) -> f64 {
        match &turn.kind {
            TurnKind::ToolResult { .. } => 0.8,
            TurnKind::AssistantResponse { .. } | TurnKind::ToolInvocation { .. } => 0.5,
            TurnKind::UserMessage { .. } => 0.3,
            TurnKind::SystemPrompt { .. } => 0.1,
            TurnKind::ScheduledCompletion { .. } => 0.4,
            TurnKind::InjectedContext { .. } => 0.2,
        }
    }

    pub fn score_all(&self, turns: &[Turn]) -> Vec<ScoredMessage> {
        let tool_chains = find_tool_chains(turns);

        let mut scored: Vec<ScoredMessage> = turns
            .iter()
            .enumerate()
            .map(|(i, turn)| {
                let mut scored = self.score(turn, i);
                for chain in &tool_chains {
                    if i >= chain.start_index && i <= chain.end_index {
                        scored.tool_chain_start = Some(chain.start_index);
                    }
                }
                scored
            })
            .collect();

        for chain in &tool_chains {
            let chain_max_score = scored[chain.start_index..=chain.end_index]
                .iter()
                .map(|s| s.score)
                .fold(0.0, f64::max);

            for scored_msg in &mut scored[chain.start_index..=chain.end_index] {
                scored_msg.score = chain_max_score;
            }
        }

        scored
    }

    pub fn select_candidates(&self, scored: &[ScoredMessage], count: usize) -> Vec<usize> {
        let mut sorted: Vec<_> = scored.iter().filter(|s| s.score > 0.0).collect();

        sorted.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut candidates = Vec::new();
        let mut seen_chains: std::collections::HashSet<usize> = std::collections::HashSet::new();

        for scored_msg in sorted {
            if candidates.len() >= count {
                break;
            }

            if let Some(chain_start) = scored_msg.tool_chain_start {
                if seen_chains.contains(&chain_start) {
                    continue;
                }
                seen_chains.insert(chain_start);
            }

            candidates.push(scored_msg.index);
        }

        candidates
    }
}

pub fn find_tool_chains(turns: &[Turn]) -> Vec<ToolChain> {
    let mut chains = Vec::new();

    for (i, turn) in turns.iter().enumerate() {
        if let Some(tool_calls) = turn.tool_calls() {
            if tool_calls.is_empty() {
                continue;
            }

            let tool_call_ids: Vec<String> = tool_calls.iter().map(|tc| tc.id.clone()).collect();

            let mut end_index = i;
            for (j, future_turn) in turns.iter().enumerate().skip(i + 1) {
                if let Some(tool_call_id) = future_turn.tool_call_id() {
                    if tool_call_ids.contains(&tool_call_id.to_string()) {
                        end_index = j;
                    }
                }
                if future_turn.tool_calls().is_some() {
                    break;
                }
            }

            if end_index > i {
                chains.push(ToolChain {
                    start_index: i,
                    end_index,
                    tool_call_ids,
                });
            }
        }
    }

    chains
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use common::{FunctionCall, ToolCall, ToolCategory};

    use super::*;

    fn make_user_turn(content: &str, hours_ago: i64) -> Turn {
        Turn::user_message(content).with_timestamp(Utc::now() - Duration::hours(hours_ago))
    }

    fn make_assistant_turn(content: &str, hours_ago: i64) -> Turn {
        Turn::assistant_response(content).with_timestamp(Utc::now() - Duration::hours(hours_ago))
    }

    fn make_tool_call_turn(hours_ago: i64) -> Turn {
        Turn::tool_invocation(
            Some("Let me search for that.".into()),
            vec![ToolCall {
                id: "call_123".into(),
                tool_type: "function".into(),
                function: FunctionCall {
                    name: "web_search".into(),
                    arguments: "{}".into(),
                },
            }],
        )
        .with_timestamp(Utc::now() - Duration::hours(hours_ago))
    }

    fn make_tool_result_turn(hours_ago: i64) -> Turn {
        Turn::tool_result("call_123", "Search results here", ToolCategory::WebSearch)
            .with_timestamp(Utc::now() - Duration::hours(hours_ago))
    }

    #[test]
    fn test_age_factor() {
        let config = PruningConfig::default();
        let scorer = TurnScorer::new(&config);

        let recent_turn = make_user_turn("hello", 0);
        let old_turn = make_user_turn("hello", 100);
        let very_old_turn = make_user_turn("hello", 200);

        let recent_score = scorer.score(&recent_turn, 0);
        let old_score = scorer.score(&old_turn, 0);
        let very_old_score = scorer.score(&very_old_turn, 0);

        assert!(recent_score.score < old_score.score);
        assert!(old_score.score < very_old_score.score);
    }

    #[test]
    fn test_length_factor() {
        let config = PruningConfig::default();
        let scorer = TurnScorer::new(&config);

        let short_turn = make_user_turn("hi", 1);
        let long_turn = make_user_turn(&"a".repeat(2000), 1);

        let short_score = scorer.score(&short_turn, 0);
        let long_score = scorer.score(&long_turn, 0);

        assert!(short_score.score < long_score.score);
    }

    #[test]
    fn test_role_factor() {
        let config = PruningConfig::default();
        let scorer = TurnScorer::new(&config);

        let user_turn = make_user_turn("hello", 1);
        let assistant_turn = make_assistant_turn("hello", 1);
        let tool_turn = Turn::tool_result("id", "result", ToolCategory::Unknown);

        let user_score = scorer.score(&user_turn, 0);
        let assistant_score = scorer.score(&assistant_turn, 0);
        let tool_score = scorer.score(&tool_turn, 0);

        assert!(user_score.score < assistant_score.score);
        assert!(assistant_score.score < tool_score.score);
    }

    #[test]
    fn test_summary_turns_score_zero() {
        let config = PruningConfig::default();
        let scorer = TurnScorer::new(&config);

        let summary = Turn::summary("This is a summary", (0, 5));
        let scored = scorer.score(&summary, 0);

        assert_eq!(scored.score, 0.0);
    }

    #[test]
    fn test_find_tool_chains() {
        let turns = vec![
            make_user_turn("search for X", 2),
            make_tool_call_turn(2),
            make_tool_result_turn(2),
            make_assistant_turn("Here's what I found", 2),
        ];

        let chains = find_tool_chains(&turns);
        assert_eq!(chains.len(), 1);
        assert_eq!(chains[0].start_index, 1);
        assert_eq!(chains[0].end_index, 2);
        assert_eq!(chains[0].tool_call_ids, vec!["call_123"]);
    }

    #[test]
    fn test_tool_chain_unified_scoring() {
        let config = PruningConfig::default();
        let scorer = TurnScorer::new(&config);

        let turns = vec![
            make_user_turn("search for X", 2),
            make_tool_call_turn(2),
            make_tool_result_turn(2),
            make_assistant_turn("Here's what I found", 2),
        ];

        let scored = scorer.score_all(&turns);

        assert_eq!(scored[1].tool_chain_start, Some(1));
        assert_eq!(scored[2].tool_chain_start, Some(1));

        assert_eq!(scored[1].score, scored[2].score);
    }
}
