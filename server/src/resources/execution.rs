use axum::{
    Router,
    extract::{Json, Path, State},
    http::StatusCode,
    response::Json as ResponseJson,
    routing::post,
};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::{auth::AuthUser, config::Config};

#[derive(Debug, Deserialize, Serialize)]
pub struct ExecutionRequest {
    pub seconds: u32,
    pub query: String,
}

#[derive(Debug, Serialize)]
pub struct ExecutionResponse {
    pub success: bool,
    pub message: String,
}

pub async fn run_execution(
    _user: AuthUser,
    State(config): State<Config>,
    Path(key): Path<String>,
    Json(request): Json<ExecutionRequest>,
) -> Result<ResponseJson<ExecutionResponse>, StatusCode> {
    let url = format!(
        "{}/ScheduledSession/{}/run/send/",
        config.restate.server, key
    );

    debug!("Sending execution request {:?} to URL: {}", request, url);

    let client = reqwest::Client::new();
    let response = client.post(&url).json(&request).send().await.map_err(|e| {
        debug!("HTTP request failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if response.status().is_success() {
        Ok(ResponseJson(ExecutionResponse {
            success: true,
            message: "Execution request sent successfully".to_string(),
        }))
    } else {
        debug!(
            "Restate server error - Status: {}, Body: {:?}",
            response.status(),
            response.text().await
        );
        Err(StatusCode::BAD_GATEWAY)
    }
}

pub fn routes() -> Router<Config> {
    Router::new().route("/:key", post(run_execution))
}
