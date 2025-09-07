use axum::{
    Router,
    routing::{get, post},
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod resources;

use config::Config;
use resources::schedule::{create_schedule, get_schedules};

#[tokio::main]
async fn main() -> Result<(), String> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "server=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::load().map_err(|e| format!("Failed to load configuration: {}", e))?;

    let app = Router::new()
        .route("/schedules", get(get_schedules))
        .route("/schedules", post(create_schedule))
        .layer(config.cors.to_cors_layer());

    let bind_address = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&bind_address)
        .await
        .map_err(|e| format!("Failed to bind to {}: {}", bind_address, e))?;

    println!(
        "Server running on http://{}:{}",
        config.server.host, config.server.port
    );

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("Server error: {}", e))
}
