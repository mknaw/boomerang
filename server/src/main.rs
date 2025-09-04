use axum::{
    Router,
    extract::Json,
    http::StatusCode,
    response::Json as ResponseJson,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use http::Method;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

mod config;
use config::Config;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
struct Schedule {
    id: Uuid,
    name: String,
    description: String,
    schedule: String,
    #[serde(rename = "isActive")]
    is_active: bool,
    #[serde(rename = "createdAt")]
    created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CreateScheduleRequest {
    name: String,
    description: String,
    schedule: String,
    #[serde(rename = "isActive")]
    is_active: Option<bool>,
}

async fn get_schedules() -> ResponseJson<Vec<Schedule>> {
    let dummy_schedules = vec![
        Schedule {
            id: Uuid::new_v4(),
            name: "Morning Email Check".to_string(),
            description: "Check emails every morning and notify if important".to_string(),
            schedule: "0 8 * * 1-5".to_string(),
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        Schedule {
            id: Uuid::new_v4(),
            name: "Weather Update".to_string(),
            description: "Get weather forecast for the day".to_string(),
            schedule: "0 7 * * *".to_string(),
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
    ];

    ResponseJson(dummy_schedules)
}

async fn create_schedule(
    Json(request): Json<CreateScheduleRequest>,
) -> (StatusCode, ResponseJson<Schedule>) {
    let now = Utc::now();

    let schedule = Schedule {
        id: Uuid::new_v4(),
        name: request.name,
        description: request.description,
        schedule: request.schedule,
        is_active: request.is_active.unwrap_or(true),
        created_at: now,
        updated_at: now,
    };

    (StatusCode::CREATED, ResponseJson(schedule))
}

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

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::exact("http://air.local:3000".parse().unwrap()))
        .allow_methods(AllowMethods::list([
            Method::GET,
            Method::POST,
            Method::OPTIONS,
        ]))
        .allow_headers(AllowHeaders::list([http::header::CONTENT_TYPE]));

    let app = Router::new()
        .route("/schedules", get(get_schedules))
        .route("/schedules", post(create_schedule))
        .layer(cors);

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
