use std::sync::{Arc, OnceLock};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Config {
    pub ai: AIConfig,
    pub tools: ToolConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub notifications: NotificationSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AIConfig {
    pub openai_api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolConfig {
    pub tavily_api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    9080
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NotificationSettings {
    pub ntfy_url: Option<String>,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self { ntfy_url: None }
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
    /// - BOOMERANG_AI__OPENAI_API_KEY=sk-...
    /// - BOOMERANG_TOOLS__TAVILY_API_KEY=tvly-...
    /// - BOOMERANG_SERVER__HOST=0.0.0.0 [default]
    /// - BOOMERANG_SERVER__PORT=9080 [default]
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

static CONFIG: OnceLock<Arc<Config>> = OnceLock::new();

impl Config {
    pub fn global() -> Arc<Config> {
        CONFIG
            .get()
            .expect("Config not initialized. Call Config::init() first.")
            .clone()
    }

    pub fn init() -> Result<(), config::ConfigError> {
        let config = Self::load()?;
        CONFIG
            .set(Arc::new(config))
            .map_err(|_| config::ConfigError::Message("Config already initialized".to_string()))?;
        Ok(())
    }
}
