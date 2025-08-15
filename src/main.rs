use std::sync::Arc;

use clap::Parser;
use common::{
    config::Config,
    restate::{ChatSession, IoAdapter, ScheduledSession, TaskExecution},
};
use restate_client::RestateClient;
use restate_sdk::prelude::*;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

mod adapter_manager;
mod telemetry;

use adapter_manager::AdapterManager;

#[derive(Parser)]
#[command(name = "boomerang")]
#[command(about = "Scheduled LLM tool execution with scheduled responses")]
struct Cli {
    #[arg(short, long)]
    config: Option<String>,
}

fn init_tracing(telemetry_enabled: bool, otlp_endpoint: &str, service_name: &str) {
    let filter_layer = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "boomerang=debug,agent=debug,telegram_adapter=debug".into());

    if telemetry_enabled {
        let otel_layer = telemetry::create_otel_layer(otlp_endpoint, service_name);
        tracing_subscriber::registry()
            .with(otel_layer)
            .with(tracing_subscriber::fmt::layer().with_filter(filter_layer))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_filter(filter_layer))
            .init();
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    Config::init(cli.config.as_deref()).expect("Failed to initialize configuration");
    let cfg = Config::global();

    init_tracing(
        cfg.telemetry.enabled,
        &cfg.telemetry.otlp_endpoint,
        &cfg.telemetry.service_name,
    );

    tracing::info!("Starting Boomerang");

    let _telemetry_guard = if cfg.telemetry.enabled {
        Some(telemetry::init_telemetry(
            &cfg.telemetry.otlp_endpoint,
            &cfg.telemetry.service_name,
            cfg.telemetry.export_interval_secs,
        ))
    } else {
        None
    };

    let bind_addr = format!("{}:{}", cfg.agent.host, cfg.agent.port);

    let telegram_output = telegram_adapter::create_output_adapter();
    let telegram_input = telegram_adapter::create_input_adapter();

    let restate_client = Arc::new(RestateClient::new(
        cfg.restate.ingress_url.clone(),
        cfg.telegram.adapter_key.clone(),
    ));

    let adapter_manager = AdapterManager::new(vec![telegram_input], restate_client);

    let task_execution = agent::task_execution::TaskExecutionImpl::new(Config::global());

    let restate_task = tokio::spawn(async move {
        tracing::info!("Starting Restate services on {}...", bind_addr);
        HttpServer::new(
            Endpoint::builder()
                .bind(agent::scheduler::ScheduledSessionImpl.serve())
                .bind(agent::chat::ChatSessionImpl.serve())
                .bind(task_execution.serve())
                .bind(telegram_output.serve())
                .build(),
        )
        .listen_and_serve(bind_addr.parse().unwrap())
        .await;
    });

    let adapter_task = tokio::spawn(async move {
        if let Err(e) = adapter_manager.run().await {
            tracing::error!("Adapter manager failed: {}", e);
        }
    });

    tokio::select! {
        _ = restate_task => {
            tracing::error!("Restate server exited");
        }
        _ = adapter_task => {
            tracing::error!("Adapter manager exited");
        }
    }

    Ok(())
}
