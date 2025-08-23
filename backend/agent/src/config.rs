use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub ai: AIConfig,
    pub tools: ToolConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIConfig {
    pub openai_api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    pub tavily_api_key: String,
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
    pub fn load() -> Result<Self, config::ConfigError> {
        let env = std::env::var("ENV").unwrap_or_else(|_| "dev".to_string());
        let config_file = format!("config/{}", env);

        let cfg = config::Config::builder()
            .add_source(config::File::with_name(&config_file))
            .add_source(
                config::Environment::with_prefix("BOOMERANG")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;

        cfg.try_deserialize()
    }
}