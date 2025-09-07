use axum::{Router, routing::post};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod auth;
mod config;
mod resources;

use auth::login;
use config::Config;
use resources::schedule;

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
        .route("/login", post(login))
        .nest("/schedules", schedule::routes())
        .with_state(config.clone())
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
