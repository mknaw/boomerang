use std::sync::Arc;

use futures::StreamExt;
use goose::{
    agents::Agent,
    conversation::{
        Conversation,
        message::{Message, MessageContent},
    },
};

use crate::{ai::providers::openai::OpenAiProvider, config::AIConfig};

/// Placeholder function for establishing an AI chat session
/// This contains the PoC code that was in get_schedules
pub async fn establish_chat_session(ai_config: &AIConfig) {
    let provider =
        Arc::new(OpenAiProvider::with_api_key("gpt-5", ai_config.openai_api_key.clone()).unwrap());
    let agent = Agent::new();
    agent.update_provider(provider).await.unwrap();
    agent
        .override_system_prompt("You are a helpful assistant.".to_string())
        .await;

    let conversation = Conversation::new(vec![
        Message::user().with_content(MessageContent::text("What is the capital of Belgium?")),
    ])
    .unwrap();
    let mut stream = agent.reply(conversation, None, None).await.unwrap();

    // Consume the stream and print each chunk to stdout
    println!("🤖 AI Response Stream:");
    while let Some(result) = stream.next().await {
        match result {
            Ok(event) => {
                // Print the AgentEvent - let's see what it contains
                println!("📤 Agent Event: {:?}", event);
            }
            Err(e) => {
                println!("\n❌ Stream error: {:?}", e);
                break;
            }
        }
    }
    println!("\n✅ Stream completed");
}
