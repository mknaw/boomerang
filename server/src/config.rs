use http::Method;
use serde::{Deserialize, Serialize};
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    #[serde(default)]
    pub cors: CorsConfig,
    pub password: String,
    pub jwt_secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    pub allowed_origins: Vec<String>,
    pub allow_all_localhost: Option<bool>,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: vec![],
            allow_all_localhost: Some(true),
        }
    }
}

impl CorsConfig {
    pub fn to_cors_layer(&self) -> CorsLayer {
        let allowed_origins = self.allowed_origins.clone();
        let allow_all_localhost = self.allow_all_localhost.unwrap_or(false);

        CorsLayer::new()
            .allow_origin(AllowOrigin::predicate(move |origin, _request_parts| {
                let origin_str = origin.to_str().unwrap_or("");

                if allow_all_localhost
                    && (origin_str.starts_with("http://localhost:")
                        || origin_str.starts_with("http://127.0.0.1:"))
                {
                    return true;
                }

                allowed_origins.iter().any(|allowed| origin_str == allowed)
            }))
            .allow_methods(AllowMethods::list([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::PATCH,
                Method::OPTIONS,
            ]))
            .allow_headers(AllowHeaders::list([
                http::header::CONTENT_TYPE,
                http::header::AUTHORIZATION,
                http::header::ACCEPT,
            ]))
    }
}

impl Config {
    /// Load configuration from environment-specific config files and environment variables
    ///
    /// Loading order:
    /// 1. config/{ENV}.toml where ENV comes from environment variable (defaults to "dev")
    /// 2. Environment variables with BOOMERANG_ prefix (optional)
    ///
    /// Environment variables:
    /// - ENV=prod (loads config/prod.toml)
    /// - ENV=dev (loads config/dev.toml) [default]
    /// - BOOMERANG_SERVER__HOST=0.0.0.0
    /// - BOOMERANG_SERVER__PORT=8080
    pub fn load() -> Result<Self, config::ConfigError> {
        let env = std::env::var("ENV").unwrap_or_else(|_| "dev".to_string());
        let config_file = format!("config/{}", env);

        let cfg = config::Config::builder()
            .add_source(config::File::with_name(&config_file).required(false))
            .add_source(
                config::Environment::default()
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;

        cfg.try_deserialize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, http::Request, routing::get};
    use tower::ServiceExt;

    async fn dummy_handler() -> &'static str {
        "test"
    }

    #[tokio::test]
    async fn test_default_cors_allows_localhost() {
        let config = CorsConfig::default();
        let cors_layer = config.to_cors_layer();

        let app = Router::new()
            .route("/test", get(dummy_handler))
            .layer(cors_layer);

        let request = Request::builder()
            .uri("/test")
            .header("Origin", "http://localhost:3000")
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        let cors_header = response.headers().get("access-control-allow-origin");
        assert!(cors_header.is_some());
        assert_eq!(cors_header.unwrap(), "http://localhost:3000");
    }

    #[tokio::test]
    async fn test_default_cors_allows_127_0_0_1() {
        let config = CorsConfig::default();
        let cors_layer = config.to_cors_layer();

        let app = Router::new()
            .route("/test", get(dummy_handler))
            .layer(cors_layer);

        let request = Request::builder()
            .uri("/test")
            .header("Origin", "http://127.0.0.1:8080")
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        let cors_header = response.headers().get("access-control-allow-origin");
        assert!(cors_header.is_some());
        assert_eq!(cors_header.unwrap(), "http://127.0.0.1:8080");
    }

    #[tokio::test]
    async fn test_default_cors_rejects_external_origin() {
        let config = CorsConfig::default();
        let cors_layer = config.to_cors_layer();

        let app = Router::new()
            .route("/test", get(dummy_handler))
            .layer(cors_layer);

        let request = Request::builder()
            .uri("/test")
            .header("Origin", "https://evil.com")
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        let cors_header = response.headers().get("access-control-allow-origin");
        assert!(cors_header.is_none());
    }

    #[tokio::test]
    async fn test_custom_allowed_origins() {
        let config = CorsConfig {
            allowed_origins: vec!["https://example.com".to_string()],
            allow_all_localhost: Some(false),
        };
        let cors_layer = config.to_cors_layer();

        let app = Router::new()
            .route("/test", get(dummy_handler))
            .layer(cors_layer);

        let request = Request::builder()
            .uri("/test")
            .header("Origin", "https://example.com")
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        let cors_header = response.headers().get("access-control-allow-origin");
        assert!(cors_header.is_some());
        assert_eq!(cors_header.unwrap(), "https://example.com");
    }

    #[tokio::test]
    async fn test_custom_config_rejects_localhost_when_disabled() {
        let config = CorsConfig {
            allowed_origins: vec!["https://example.com".to_string()],
            allow_all_localhost: Some(false),
        };
        let cors_layer = config.to_cors_layer();

        let app = Router::new()
            .route("/test", get(dummy_handler))
            .layer(cors_layer);

        let request = Request::builder()
            .uri("/test")
            .header("Origin", "http://localhost:3000")
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        let cors_header = response.headers().get("access-control-allow-origin");
        assert!(cors_header.is_none());
    }

    #[tokio::test]
    async fn test_mixed_config_allows_both_localhost_and_custom() {
        let config = CorsConfig {
            allowed_origins: vec!["https://example.com".to_string()],
            allow_all_localhost: Some(true),
        };
        let cors_layer = config.to_cors_layer();

        let app1 = Router::new()
            .route("/test", get(dummy_handler))
            .layer(cors_layer.clone());

        let app2 = Router::new()
            .route("/test", get(dummy_handler))
            .layer(cors_layer);

        // Test localhost
        let request1 = Request::builder()
            .uri("/test")
            .header("Origin", "http://localhost:3000")
            .body(axum::body::Body::empty())
            .unwrap();

        let response1 = app1.oneshot(request1).await.unwrap();
        let cors_header1 = response1.headers().get("access-control-allow-origin");
        assert!(cors_header1.is_some());
        assert_eq!(cors_header1.unwrap(), "http://localhost:3000");

        // Test custom origin
        let request2 = Request::builder()
            .uri("/test")
            .header("Origin", "https://example.com")
            .body(axum::body::Body::empty())
            .unwrap();

        let response2 = app2.oneshot(request2).await.unwrap();
        let cors_header2 = response2.headers().get("access-control-allow-origin");
        assert!(cors_header2.is_some());
        assert_eq!(cors_header2.unwrap(), "https://example.com");
    }

    #[tokio::test]
    async fn test_options_preflight_request() {
        let config = CorsConfig::default();
        let cors_layer = config.to_cors_layer();

        let app = Router::new()
            .route("/test", get(dummy_handler))
            .layer(cors_layer);

        let request = Request::builder()
            .method("OPTIONS")
            .uri("/test")
            .header("Origin", "http://localhost:3000")
            .header("Access-Control-Request-Method", "POST")
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert!(
            response
                .headers()
                .get("access-control-allow-origin")
                .is_some()
        );
        assert!(
            response
                .headers()
                .get("access-control-allow-methods")
                .is_some()
        );
        assert!(
            response
                .headers()
                .get("access-control-allow-headers")
                .is_some()
        );
    }
}
