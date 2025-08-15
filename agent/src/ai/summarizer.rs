use std::sync::Arc;

use anyhow::Result;

use super::{
    provider::Provider,
    types::{Message, MessageMetadata},
};

const SUMMARIZE_PROMPT: &str = include_str!("../prompts/summarize.txt");

pub struct Summarizer {
    provider: Arc<dyn Provider>,
}

impl Summarizer {
    pub fn new(provider: Arc<dyn Provider>) -> Self {
        Self { provider }
    }

    pub async fn summarize(&self, messages: &[Message]) -> Result<Message> {
        if messages.is_empty() {
            return Ok(Message::summary("No messages to summarize.", (0, 0)));
        }

        let messages_text: Vec<String> = messages
            .iter()
            .enumerate()
            .filter_map(|(i, msg)| {
                let role = match msg.role {
                    super::types::MessageRole::User => "User",
                    super::types::MessageRole::Assistant => "Assistant",
                    super::types::MessageRole::Tool => "Tool",
                    super::types::MessageRole::System => return None,
                };
                msg.content
                    .as_ref()
                    .map(|c| format!("[{}] {}: {}", i, role, c))
            })
            .collect();

        let prompt = format!(
            "{}\n\n---\nMessages to summarize:\n{}",
            SUMMARIZE_PROMPT,
            messages_text.join("\n\n")
        );

        use super::session::Session;

        let mut session = Session::new(self.provider.clone()).with_system_prompt(
            "You are a conversation summarizer. Be concise but preserve key information.",
        );

        session.add_user_message(&prompt);

        let response = session.complete().await?;
        let summary_content = response.message.content.unwrap_or_default();

        Ok(Message {
            role: super::types::MessageRole::Assistant,
            content: Some(summary_content),
            tool_calls: None,
            tool_call_id: None,
            timestamp: chrono::Utc::now(),
            metadata: Some(MessageMetadata {
                is_summary: true,
                summarized_range: Some((0, messages.len().saturating_sub(1))),
                persisted_key: None,
            }),
        })
    }
}
