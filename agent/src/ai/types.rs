use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageMetadata {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_summary: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summarized_range: Option<(usize, usize)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persisted_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageRole {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "tool")]
    Tool,
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
pub struct Message {
    pub role: MessageRole,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MessageMetadata>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            timestamp: Utc::now(),
            metadata: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::user_at(content, Utc::now())
    }

    pub fn user_at(content: impl Into<String>, timestamp: DateTime<Utc>) -> Self {
        Self {
            role: MessageRole::User,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            timestamp,
            metadata: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            timestamp: Utc::now(),
            metadata: None,
        }
    }

    pub fn assistant_with_tools(content: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content,
            tool_calls: Some(tool_calls),
            tool_call_id: None,
            timestamp: Utc::now(),
            metadata: None,
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Tool,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
            timestamp: Utc::now(),
            metadata: None,
        }
    }

    pub fn summary(content: impl Into<String>, summarized_range: (usize, usize)) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            timestamp: Utc::now(),
            metadata: Some(MessageMetadata {
                is_summary: true,
                summarized_range: Some(summarized_range),
                persisted_key: None,
            }),
        }
    }

    pub fn is_summary(&self) -> bool {
        self.metadata.as_ref().map_or(false, |m| m.is_summary)
    }
}

// Re-export rmcp::model::Tool for convenience
pub use rmcp::model::Tool;

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

pub type ToolExecutor = std::sync::Arc<
    dyn Fn(
            &str,
            &str,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<String>> + Send>>
        + Send
        + Sync,
>;

#[derive(Clone)]
pub struct ToolSpec {
    pub tool: Tool,
    pub executor: ToolExecutor,
}

impl ToolSpec {
    pub fn new(tool: Tool, executor: ToolExecutor) -> Self {
        Self { tool, executor }
    }
}

impl From<&common::Turn> for Message {
    fn from(turn: &common::Turn) -> Self {
        use common::TurnKind;

        match &turn.kind {
            TurnKind::UserMessage { content } => Message {
                role: MessageRole::User,
                content: Some(content.clone()),
                tool_calls: None,
                tool_call_id: None,
                timestamp: turn.timestamp,
                metadata: None,
            },
            TurnKind::AssistantResponse { content } => Message {
                role: MessageRole::Assistant,
                content: Some(content.clone()),
                tool_calls: None,
                tool_call_id: None,
                timestamp: turn.timestamp,
                metadata: None,
            },
            TurnKind::ToolInvocation {
                content,
                tool_calls,
            } => {
                let converted_calls: Vec<ToolCall> = tool_calls
                    .iter()
                    .map(|tc| ToolCall {
                        id: tc.id.clone(),
                        tool_type: tc.tool_type.clone(),
                        function: FunctionCall {
                            name: tc.function.name.clone(),
                            arguments: tc.function.arguments.clone(),
                        },
                    })
                    .collect();
                Message {
                    role: MessageRole::Assistant,
                    content: content.clone(),
                    tool_calls: Some(converted_calls),
                    tool_call_id: None,
                    timestamp: turn.timestamp,
                    metadata: None,
                }
            }
            TurnKind::ToolResult {
                tool_call_id,
                content,
                ..
            } => Message {
                role: MessageRole::Tool,
                content: Some(content.clone()),
                tool_calls: None,
                tool_call_id: Some(tool_call_id.clone()),
                timestamp: turn.timestamp,
                metadata: None,
            },
            TurnKind::ScheduledCompletion { result, .. } => Message {
                role: MessageRole::Assistant,
                content: Some(result.clone()),
                tool_calls: None,
                tool_call_id: None,
                timestamp: turn.timestamp,
                metadata: None,
            },
            TurnKind::InjectedContext {
                content,
                context_type,
            } => {
                let metadata = match context_type {
                    common::ContextType::Summary { summarized_range } => Some(MessageMetadata {
                        is_summary: true,
                        summarized_range: Some(*summarized_range),
                        persisted_key: None,
                    }),
                    common::ContextType::MemoryRetrieval { memory_key } => Some(MessageMetadata {
                        is_summary: false,
                        summarized_range: None,
                        persisted_key: Some(memory_key.clone()),
                    }),
                };
                Message {
                    role: MessageRole::Assistant,
                    content: Some(content.clone()),
                    tool_calls: None,
                    tool_call_id: None,
                    timestamp: turn.timestamp,
                    metadata,
                }
            }
            TurnKind::SystemPrompt { content } => Message {
                role: MessageRole::System,
                content: Some(content.clone()),
                tool_calls: None,
                tool_call_id: None,
                timestamp: turn.timestamp,
                metadata: None,
            },
        }
    }
}

impl From<&Message> for common::Turn {
    fn from(msg: &Message) -> Self {
        use common::{ContextType, TurnKind};

        let kind = match &msg.role {
            MessageRole::System => TurnKind::SystemPrompt {
                content: msg.content.clone().unwrap_or_default(),
            },
            MessageRole::User => TurnKind::UserMessage {
                content: msg.content.clone().unwrap_or_default(),
            },
            MessageRole::Assistant => {
                if let Some(ref tool_calls) = msg.tool_calls {
                    let converted_calls: Vec<common::ToolCall> = tool_calls
                        .iter()
                        .map(|tc| common::ToolCall {
                            id: tc.id.clone(),
                            tool_type: tc.tool_type.clone(),
                            function: common::FunctionCall {
                                name: tc.function.name.clone(),
                                arguments: tc.function.arguments.clone(),
                            },
                        })
                        .collect();
                    TurnKind::ToolInvocation {
                        content: msg.content.clone(),
                        tool_calls: converted_calls,
                    }
                } else if msg.is_summary() {
                    let summarized_range = msg
                        .metadata
                        .as_ref()
                        .and_then(|m| m.summarized_range)
                        .unwrap_or((0, 0));
                    TurnKind::InjectedContext {
                        content: msg.content.clone().unwrap_or_default(),
                        context_type: ContextType::Summary { summarized_range },
                    }
                } else {
                    TurnKind::AssistantResponse {
                        content: msg.content.clone().unwrap_or_default(),
                    }
                }
            }
            MessageRole::Tool => TurnKind::ToolResult {
                tool_call_id: msg.tool_call_id.clone().unwrap_or_default(),
                tool_category: common::ToolCategory::Unknown,
                content: msg.content.clone().unwrap_or_default(),
            },
        };

        common::Turn {
            id: common::TurnId::new(),
            kind,
            timestamp: msg.timestamp,
            platform_origin: None,
        }
    }
}
