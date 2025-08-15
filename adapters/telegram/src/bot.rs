use std::time::Duration;

use common::adapters::{AdapterCommand, IncomingMessage};
use teloxide::{prelude::*, types::ChatAction, utils::command::BotCommands};
use tokio::sync::mpsc;
use tracing::{debug, error};

const ADAPTER_ID: &str = "telegram";

pub async fn run_bot(token: &str, sender: mpsc::Sender<IncomingMessage>) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .expect("Failed to build reqwest client");
    let bot = Bot::with_client(token, client);

    let handler = Update::filter_message()
        .branch(
            dptree::entry()
                .filter_command::<Command>()
                .endpoint(handle_command),
        )
        .branch(dptree::endpoint(handle_message));

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![sender])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Available commands:")]
enum Command {
    #[command(description = "Start a new conversation")]
    Start,
    #[command(description = "Clear conversation history")]
    Clear,
    #[command(description = "Show help")]
    Help,
}

async fn handle_command(
    bot: Bot,
    msg: Message,
    cmd: Command,
    sender: mpsc::Sender<IncomingMessage>,
) -> ResponseResult<()> {
    let external_chat_id = msg.chat.id.0.to_string();

    match cmd {
        Command::Start => {
            bot.send_message(msg.chat.id, "Hello! Send me a message and I'll respond.")
                .await?;
        }
        Command::Clear => {
            let message = IncomingMessage::command(
                ADAPTER_ID,
                &external_chat_id,
                AdapterCommand::ClearHistory,
            );
            if let Err(e) = sender.send(message).await {
                error!("Failed to send clear command: {}", e);
                bot.send_message(msg.chat.id, "Failed to clear history.")
                    .await?;
            } else {
                bot.send_message(msg.chat.id, "Conversation cleared!")
                    .await?;
            }
        }
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?;
        }
    }

    Ok(())
}

async fn handle_message(
    bot: Bot,
    msg: Message,
    sender: mpsc::Sender<IncomingMessage>,
) -> ResponseResult<()> {
    let Some(text) = msg.text() else {
        return Ok(());
    };

    if text.starts_with('/') {
        return Ok(());
    }

    let external_chat_id = msg.chat.id.0.to_string();
    debug!(
        "Received message from {} ({}): {}",
        ADAPTER_ID, external_chat_id, text
    );

    bot.send_chat_action(msg.chat.id, ChatAction::Typing)
        .await?;

    let message = IncomingMessage::text(ADAPTER_ID, &external_chat_id, text);

    if let Err(e) = sender.send(message).await {
        error!("Failed to send message to adapter manager: {}", e);
        bot.send_message(msg.chat.id, "Sorry, something went wrong.")
            .await?;
    }

    Ok(())
}
