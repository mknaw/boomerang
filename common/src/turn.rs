use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TurnId(pub String);

impl TurnId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for TurnId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TurnId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformOrigin {
    pub platform_type: String,
    pub external_chat_id: String,
    pub adapter_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    WebSearch,
    Memory,
    Schedule,
    Unknown,
}

impl Default for ToolCategory {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContextType {
    Summary { summarized_range: (usize, usize) },
    MemoryRetrieval { memory_key: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TurnKind {
    UserMessage {
        content: String,
    },
    AssistantResponse {
        content: String,
    },
    ToolInvocation {
        content: Option<String>,
        tool_calls: Vec<ToolCall>,
    },
    ToolResult {
        tool_call_id: String,
        #[serde(default)]
        tool_category: ToolCategory,
        content: String,
    },
    ScheduledCompletion {
        original_query: String,
        result: String,
        #[serde(default)]
        is_final: bool,
    },
    InjectedContext {
        content: String,
        context_type: ContextType,
    },
    SystemPrompt {
        content: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    #[serde(default = "TurnId::new")]
    pub id: TurnId,
    pub kind: TurnKind,
    #[serde(default = "Utc::now")]
    pub timestamp: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_origin: Option<PlatformOrigin>,
}

impl Turn {
    pub fn new(kind: TurnKind) -> Self {
        Self {
            id: TurnId::new(),
            kind,
            timestamp: Utc::now(),
            platform_origin: None,
        }
    }

    pub fn with_platform_origin(mut self, origin: PlatformOrigin) -> Self {
        self.platform_origin = Some(origin);
        self
    }

    pub fn with_timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = timestamp;
        self
    }

    pub fn user_message(content: impl Into<String>) -> Self {
        Self::new(TurnKind::UserMessage {
            content: content.into(),
        })
    }

    pub fn assistant_response(content: impl Into<String>) -> Self {
        Self::new(TurnKind::AssistantResponse {
            content: content.into(),
        })
    }

    pub fn tool_invocation(content: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self::new(TurnKind::ToolInvocation {
            content,
            tool_calls,
        })
    }

    pub fn tool_result(
        tool_call_id: impl Into<String>,
        content: impl Into<String>,
        tool_category: ToolCategory,
    ) -> Self {
        Self::new(TurnKind::ToolResult {
            tool_call_id: tool_call_id.into(),
            tool_category,
            content: content.into(),
        })
    }

    pub fn scheduled_completion(
        original_query: impl Into<String>,
        result: impl Into<String>,
        is_final: bool,
    ) -> Self {
        Self::new(TurnKind::ScheduledCompletion {
            original_query: original_query.into(),
            result: result.into(),
            is_final,
        })
    }

    pub fn summary(content: impl Into<String>, summarized_range: (usize, usize)) -> Self {
        Self::new(TurnKind::InjectedContext {
            content: content.into(),
            context_type: ContextType::Summary { summarized_range },
        })
    }

    pub fn system_prompt(content: impl Into<String>) -> Self {
        Self::new(TurnKind::SystemPrompt {
            content: content.into(),
        })
    }

    pub fn is_summary(&self) -> bool {
        matches!(
            &self.kind,
            TurnKind::InjectedContext {
                context_type: ContextType::Summary { .. },
                ..
            }
        )
    }

    pub fn summarized_range(&self) -> Option<(usize, usize)> {
        match &self.kind {
            TurnKind::InjectedContext {
                context_type: ContextType::Summary { summarized_range },
                ..
            } => Some(*summarized_range),
            _ => None,
        }
    }

    pub fn tool_calls(&self) -> Option<&Vec<ToolCall>> {
        match &self.kind {
            TurnKind::ToolInvocation { tool_calls, .. } => Some(tool_calls),
            _ => None,
        }
    }

    pub fn tool_call_id(&self) -> Option<&str> {
        match &self.kind {
            TurnKind::ToolResult { tool_call_id, .. } => Some(tool_call_id),
            _ => None,
        }
    }

    pub fn content(&self) -> Option<&str> {
        match &self.kind {
            TurnKind::UserMessage { content } => Some(content),
            TurnKind::AssistantResponse { content } => Some(content),
            TurnKind::ToolInvocation { content, .. } => content.as_deref(),
            TurnKind::ToolResult { content, .. } => Some(content),
            TurnKind::ScheduledCompletion { result, .. } => Some(result),
            TurnKind::InjectedContext { content, .. } => Some(content),
            TurnKind::SystemPrompt { content } => Some(content),
        }
    }

    pub fn is_user(&self) -> bool {
        matches!(self.kind, TurnKind::UserMessage { .. })
    }

    pub fn is_assistant(&self) -> bool {
        matches!(
            self.kind,
            TurnKind::AssistantResponse { .. } | TurnKind::ToolInvocation { .. }
        )
    }

    pub fn is_tool_result(&self) -> bool {
        matches!(self.kind, TurnKind::ToolResult { .. })
    }

    pub fn is_system(&self) -> bool {
        matches!(self.kind, TurnKind::SystemPrompt { .. })
    }

    pub fn is_scheduled(&self) -> bool {
        matches!(self.kind, TurnKind::ScheduledCompletion { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turn_serialization() {
        let turn = Turn::user_message("Hello");
        let json = serde_json::to_string(&turn).unwrap();
        let deserialized: Turn = serde_json::from_str(&json).unwrap();
        assert_eq!(turn.id, deserialized.id);
        assert!(matches!(
            deserialized.kind,
            TurnKind::UserMessage { content } if content == "Hello"
        ));
    }

    #[test]
    fn test_turn_id_generated_on_deserialize() {
        let json = r#"{"kind":{"type":"user_message","content":"Hello"}}"#;
        let turn: Turn = serde_json::from_str(json).unwrap();
        assert!(!turn.id.0.is_empty());
    }

    #[test]
    fn test_scheduled_completion() {
        let turn = Turn::scheduled_completion("check weather", "It's sunny", false);
        assert!(turn.is_scheduled());
        assert!(!turn.is_user());
    }

    #[test]
    fn test_summary_methods() {
        let turn = Turn::summary("Summary of conversation", (0, 5));
        assert!(turn.is_summary());
        assert_eq!(turn.summarized_range(), Some((0, 5)));
    }

    #[test]
    fn test_tool_calls() {
        let tool_call = ToolCall {
            id: "call_123".to_string(),
            tool_type: "function".to_string(),
            function: FunctionCall {
                name: "web_search".to_string(),
                arguments: "{}".to_string(),
            },
        };
        let turn = Turn::tool_invocation(Some("Searching...".to_string()), vec![tool_call]);
        assert!(turn.tool_calls().is_some());
        assert_eq!(turn.tool_calls().unwrap().len(), 1);
    }
}
