use axum::{
    async_trait,
    extract::{FromRequestParts, Json, State},
    http::{StatusCode, request::Parts},
    response::Json as ResponseJson,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LoginRequest {
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub exp: i64,
    pub iat: i64,
}

pub async fn login(
    State(config): State<Config>,
    Json(request): Json<LoginRequest>,
) -> Result<ResponseJson<LoginResponse>, StatusCode> {
    if request.password != config.password {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let now = Utc::now();
    let claims = Claims {
        iat: now.timestamp(),
        exp: (now + Duration::hours(24)).timestamp(),
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_ref()),
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(ResponseJson(LoginResponse { token }))
}

pub struct AuthUser;

#[async_trait]
impl FromRequestParts<Config> for AuthUser {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Config,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get("Authorization")
            .ok_or(StatusCode::UNAUTHORIZED)?
            .to_str()
            .map_err(|_| StatusCode::UNAUTHORIZED)?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or(StatusCode::UNAUTHORIZED)?;

        let _claims = decode::<Claims>(
            token,
            &DecodingKey::from_secret(state.jwt_secret.as_ref()),
            &Validation::default(),
        )
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

        Ok(AuthUser)
    }
}
