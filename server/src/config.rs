use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
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
