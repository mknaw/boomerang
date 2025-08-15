mod evaluator;
mod pruner;
mod scorer;

pub use evaluator::PruningEvaluator;
pub use pruner::MessagePruner;
pub use scorer::TurnScorer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PruneAction {
    Persist,
    Summarize,
    Both,
    Drop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruneDecision {
    pub index: usize,
    pub action: PruneAction,
    pub memory_key: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct ScoredMessage {
    pub index: usize,
    pub score: f64,
    pub tool_chain_start: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ToolChain {
    pub start_index: usize,
    pub end_index: usize,
    pub tool_call_ids: Vec<String>,
}
