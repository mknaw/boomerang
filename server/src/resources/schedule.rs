use axum::{
    Router,
    extract::Json,
    http::StatusCode,
    response::Json as ResponseJson,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth::AuthUser, config::Config};

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct Schedule {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub schedule: String,
    #[serde(rename = "isActive")]
    pub is_active: bool,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateScheduleRequest {
    pub name: String,
    pub description: String,
    pub schedule: String,
    #[serde(rename = "isActive")]
    pub is_active: Option<bool>,
}

pub async fn get_schedules(_user: AuthUser) -> ResponseJson<Vec<Schedule>> {
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

pub async fn create_schedule(
    _user: AuthUser,
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

pub fn routes() -> Router<Config> {
    Router::new()
        .route("/", get(get_schedules))
        .route("/", post(create_schedule))
}
