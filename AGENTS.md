# Agent Architecture

This document describes the agent system architecture, including the Restate services and modular adapter pattern.

## Guidelines for AI Agents

- **Do not change model names**: If the codebase references a model (e.g., `gpt-5-mini`), trust that it exists. The human maintainer has a more recent knowledge cutoff and knows which models are available. Do not "correct" model names based on your training data.

## Overview

The agent system uses [Restate](https://restate.dev) as a durable execution engine. Restate provides:
- Durable execution with automatic retries
- Per-conversation state via virtual objects
- Built-in scheduling for delayed/recurring tasks
- Service-to-service RPC via generated clients

External interfaces (Telegram, iOS, etc.) communicate with the agent through **adapters** that call Restate's HTTP API.

## Workspace Structure

```
backend/
├── Cargo.toml              # Workspace root + boomerang binary
├── src/main.rs             # Single unified binary entry point
├── common/                 # Shared types, config, Restate trait definitions
│   └── src/
│       ├── lib.rs
│       ├── config.rs       # Configuration management
│       └── restate.rs      # IoAdapter, ChatSession, ScheduledSession traits
├── agent/                  # Core agent logic (library)
│   └── src/
│       ├── lib.rs
│       ├── chat.rs         # ChatSessionImpl
│       ├── scheduler.rs    # ScheduledSessionImpl
│       ├── executor.rs     # AgentExecutor - LLM execution
│       ├── ai/             # LLM provider abstraction
│       │   ├── mod.rs
│       │   ├── provider.rs # Provider trait
│       │   ├── session.rs  # Session (agentic loop)
│       │   ├── types.rs    # Message, ToolSpec types
│       │   └── providers/
│       │       ├── openai.rs
│       │       └── openrouter.rs
│       └── tools/          # Tool implementations
│           ├── mod.rs
│           ├── web_search.rs
│           └── schedule.rs
├── adapters/telegram/      # Telegram adapter (library)
│   └── src/
│       ├── lib.rs          # Exports create_telegram_adapter(), run_telegram_bot()
│       ├── bot.rs          # Teloxide bot handlers
│       └── io_adapter_impl.rs  # IoAdapterImpl
└── restate-client/         # HTTP client for external callers
    └── src/lib.rs          # RestateClient
```

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                      boomerang binary                           │
│  ┌─────────────────────────┐  ┌──────────────────────────────┐  │
│  │   agent (library)       │  │  telegram-adapter (library)  │  │
│  │   ChatSessionImpl       │  │  create_telegram_adapter()   │  │
│  │   ScheduledSessionImpl  │  │  run_telegram_bot()          │  │
│  └─────────────────────────┘  └──────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
         │                                │
         │ Single Restate HTTP server     │ Telegram bot
         │ (port 9080)                    │ (long-polling)
         ▼                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                       Restate Server                            │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                   Virtual Objects                        │   │
│  │  ┌─────────────────┐ ┌─────────────────┐ ┌────────────┐  │   │
│  │  │  ChatSession    │ │ScheduledSession │ │ IoAdapter  │  │   │
│  │  │  (per chat_id)  │ │ (per schedule)  │ │(per adapter│  │   │
│  │  └─────────────────┘ └─────────────────┘ └────────────┘  │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
         │                                ▲
         │ ctx.object_client              │ RestateClient (HTTP)
         ▼                                │
┌─────────────────┐              ┌─────────────────┐
│ OpenAI / Tools  │              │  Telegram Bot   │
└─────────────────┘              │  (long-polling) │
                                 └─────────────────┘
                                          │
                                          ▼
                                 ┌─────────────────┐
                                 │   Telegram API  │
                                 └─────────────────┘
```

## Message Flow

```
1. User sends message to Telegram
2. Bot receives via long-polling
3. Bot calls RestateClient.send_message() [HTTP POST to Restate]
4. Restate routes to ChatSession.message() handler
5. ChatSession runs LLM via AgentExecutor
6. ChatSession calls ctx.object_client::<IoAdapterClient>().send_message()
7. IoAdapter queues message to mpsc channel
8. Telegram adapter's sender task receives from channel
9. Bot sends message back to user via Telegram API
```

## Common Crate

The `common` crate contains Restate service definitions, configuration, and shared types. The `#[restate_sdk::object]` macro generates both the trait and a `*Client` type for RPC.

```rust
// common/src/restate.rs

#[restate_sdk::object]
pub trait IoAdapter {
    async fn send_message(request: Json<SendMessageRequest>) -> HandlerResult<()>;
}
// Generates: IoAdapterClient

#[restate_sdk::object]
pub trait ChatSession {
    async fn message(request: Json<IncomingMessage>) -> HandlerResult<()>;
    async fn history() -> HandlerResult<String>;
    async fn clear() -> HandlerResult<()>;
}
// Generates: ChatSessionClient

#[restate_sdk::object]
pub trait ScheduledSession {
    async fn run(spec: Json<ScheduleArgs>) -> HandlerResult<()>;
    async fn execute() -> HandlerResult<()>;
    async fn cancel() -> HandlerResult<()>;
}
// Generates: ScheduledSessionClient
```

**Why a separate crate?**
- Trait definitions are shared across `agent`, `telegram-adapter`, and `restate-client`
- Configuration is shared across all crates
- Avoids circular dependencies
- Generated `*Client` types can be imported without pulling in implementations

## Restate Communication Patterns

### Inside Restate handlers: `ctx.object_client`

Service-to-service calls within Restate are durable and journaled:

```rust
// In ChatSession.message() - calling IoAdapter
ctx.object_client::<IoAdapterClient>(&adapter_key)
    .send_message(Json(request))
    .call()
    .await?;

// In ScheduledSession - self-scheduling
ctx.object_client::<ScheduledSessionClient>(ctx.key())
    .execute()
    .send_after(Duration::from_secs(60));
```

### Outside Restate: `RestateClient` (HTTP)

External callers (like the Telegram bot) use HTTP:

```rust
// In telegram bot handler
restate_client.send_message(&chat_key, "telegram", &chat_id, text).await?;
// Internally: POST http://restate:8080/ChatSession/{key}/message
```

## Running the System

### Prerequisites

1. [Restate Server](https://docs.restate.dev/develop/local_dev#running-restate-server-locally)
2. Rust toolchain (edition 2024)

### Steps

```bash
# 1. Start Restate server
restate-server

# 2. Start the unified boomerang binary
cargo run
# Starts Restate services on port 9080 + Telegram bot (long-polling)

# 3. Register services with Restate (first time only)
restate deployments register http://localhost:9080
```

### Configuration

Single unified config (`config/dev.toml`):
```toml
[ai]
provider = "openai"  # or "openrouter"
model = "gpt-5-mini"
api_key = "YOUR_API_KEY"

[tools]
tavily_api_key = "YOUR_TAVILY_API_KEY"

[agent]
host = "0.0.0.0"
port = 9080
context_window_size = 50

[telegram]
bot_token = "YOUR_BOT_TOKEN"
adapter_key = "telegram-adapter"

[restate]
ingress_url = "http://127.0.0.1:8080"
```

Or via environment variables:
```bash
export AI__OPENAI_API_KEY=sk-...
export TELEGRAM__BOT_TOKEN=your_token
```

## Adding a New Adapter

To add a new platform adapter (e.g., iOS, Discord):

1. Create a new crate in `adapters/`:
   ```
   adapters/your-adapter/
   ├── Cargo.toml
   ├── config/dev.toml
   └── src/lib.rs
   ```

2. Add to workspace in root `Cargo.toml`:
   ```toml
   members = ["agent", "common", "...", "adapters/your-adapter"]
   ```

3. Depend on `common` and implement `IoAdapter`:
   ```rust
   use common::restate::{IoAdapter, SendMessageRequest};

   pub struct YourAdapterImpl { /* ... */ }

   impl IoAdapter for YourAdapterImpl {
       async fn send_message(&self, ctx: ObjectContext<'_>, request: Json<SendMessageRequest>) -> HandlerResult<()> {
           // Send to your platform
       }
   }
   ```

4. Create adapter factory + bot runner, then add to `src/main.rs`:
   ```rust
   // Create adapter (spawns message sender task)
   let your_adapter = your_adapter::create_your_adapter();

   // Register IoAdapter with Restate endpoint
   let restate_task = tokio::spawn(async move {
       HttpServer::new(
           Endpoint::builder()
               .bind(agent::scheduler::ScheduledSessionImpl.serve())
               .bind(agent::chat::ChatSessionImpl.serve())
               .bind(telegram.io_adapter.serve())
               .bind(your_adapter.io_adapter.serve())  // Add here
               .build(),
       )
       .listen_and_serve(bind_addr.parse().unwrap())
       .await;
   });

   // Spawn bot/input handler
   let your_bot_task = tokio::spawn(your_adapter::run_your_bot());

   tokio::select! {
       _ = restate_task => { ... }
       _ = telegram_bot_task => { ... }
       _ = your_bot_task => { ... }
   }
   ```

5. Use `restate-client` for inbound messages to call ChatSession.

## State Management

Conversation state is stored in Restate's virtual object state:
- Key: `"messages"` (JSON-serialized `Vec<Message>`)
- Automatically persisted and restored
- Survives service restarts

To inspect state:
```bash
restate kv get ChatSession/{key}
```
