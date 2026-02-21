use std::sync::{Arc, OnceLock};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Config {
    pub ai: AIConfig,
    pub tools: ToolConfig,
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub restate: RestateConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    #[default]
    OpenAI,
    OpenRouter,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AIProviderConfig {
    #[serde(default)]
    pub provider: ProviderType,
    #[serde(default = "default_ai_model")]
    pub model: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AIConfig {
    pub workhorse: AIProviderConfig,
    #[serde(default)]
    pub summarization: Option<AIProviderConfig>,
}

fn default_ai_model() -> String {
    "gpt-5-mini".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolConfig {
    pub tavily_api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PruningConfig {
    #[serde(default = "default_pruning_soft_limit")]
    pub soft_limit: usize,
    #[serde(default = "default_pruning_hard_limit")]
    pub hard_limit: usize,
    #[serde(default = "default_pruning_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_pruning_max_age_hours")]
    pub max_age_hours: f64,
    #[serde(default = "default_true")]
    pub enable_memory_persist: bool,
    #[serde(default = "default_true")]
    pub enable_summarization: bool,
}

fn default_pruning_soft_limit() -> usize {
    40
}

fn default_pruning_hard_limit() -> usize {
    50
}

fn default_pruning_batch_size() -> usize {
    10
}

fn default_pruning_max_age_hours() -> f64 {
    168.0
}

fn default_true() -> bool {
    true
}

impl Default for PruningConfig {
    fn default() -> Self {
        Self {
            soft_limit: default_pruning_soft_limit(),
            hard_limit: default_pruning_hard_limit(),
            batch_size: default_pruning_batch_size(),
            max_age_hours: default_pruning_max_age_hours(),
            enable_memory_persist: true,
            enable_summarization: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentConfig {
    #[serde(default = "default_agent_host")]
    pub host: String,
    #[serde(default = "default_agent_port")]
    pub port: u16,
    #[serde(default = "default_context_window_size")]
    pub context_window_size: usize,
    #[serde(default)]
    pub pruning: PruningConfig,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            host: default_agent_host(),
            port: default_agent_port(),
            context_window_size: default_context_window_size(),
            pruning: PruningConfig::default(),
        }
    }
}

fn default_agent_host() -> String {
    "0.0.0.0".to_string()
}

fn default_agent_port() -> u16 {
    9080
}

fn default_context_window_size() -> usize {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct TelegramConfig {
    #[serde(default)]
    pub bot_token: String,
    #[serde(default = "default_telegram_adapter_key")]
    pub adapter_key: String,
    #[serde(default = "default_telegram_bind_address")]
    pub bind_address: String,
}

fn default_telegram_adapter_key() -> String {
    "telegram-adapter".to_string()
}

fn default_telegram_bind_address() -> String {
    "127.0.0.1:9081".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RestateConfig {
    #[serde(default = "default_restate_ingress_url")]
    pub ingress_url: String,
    #[serde(default = "default_restate_introspection_host")]
    pub introspection_host: String,
    #[serde(default = "default_restate_introspection_port")]
    pub introspection_port: u16,
}

impl Default for RestateConfig {
    fn default() -> Self {
        Self {
            ingress_url: default_restate_ingress_url(),
            introspection_host: default_restate_introspection_host(),
            introspection_port: default_restate_introspection_port(),
        }
    }
}

fn default_restate_ingress_url() -> String {
    "http://localhost:8080".to_string()
}

fn default_restate_introspection_host() -> String {
    "localhost".to_string()
}

fn default_restate_introspection_port() -> u16 {
    5432
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemoryConfig {
    #[serde(default = "default_scratch_space_path")]
    pub scratch_space_path: String,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            scratch_space_path: default_scratch_space_path(),
        }
    }
}

fn default_scratch_space_path() -> String {
    dirs::home_dir()
        .map(|p| p.join(".boomerang/memory"))
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "/tmp/boomerang/memory".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TelemetryConfig {
    #[serde(default = "default_telemetry_enabled")]
    pub enabled: bool,
    #[serde(default = "default_otlp_endpoint")]
    pub otlp_endpoint: String,
    #[serde(default = "default_service_name")]
    pub service_name: String,
    #[serde(default = "default_export_interval_secs")]
    pub export_interval_secs: u64,
}

fn default_telemetry_enabled() -> bool {
    true
}

fn default_otlp_endpoint() -> String {
    "http://localhost:4318".to_string()
}

fn default_service_name() -> String {
    "boomerang".to_string()
}

fn default_export_interval_secs() -> u64 {
    60
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: default_telemetry_enabled(),
            otlp_endpoint: default_otlp_endpoint(),
            service_name: default_service_name(),
            export_interval_secs: default_export_interval_secs(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ai: AIConfig {
                workhorse: AIProviderConfig {
                    provider: ProviderType::default(),
                    model: default_ai_model(),
                    api_key: String::new(),
                },
                summarization: None,
            },
            tools: ToolConfig {
                tavily_api_key: String::new(),
            },
            agent: AgentConfig::default(),
            telegram: TelegramConfig::default(),
            restate: RestateConfig::default(),
            memory: MemoryConfig::default(),
            telemetry: TelemetryConfig::default(),
        }
    }
}

impl RestateConfig {
    pub fn introspection_connection_string(&self) -> String {
        format!(
            "host={} port={} user=restate password=restate dbname=restate",
            self.introspection_host, self.introspection_port
        )
    }
}

impl Config {
    /// Load configuration from a config file and environment variables
    ///
    /// If `path` is provided, loads from that file.
    /// Otherwise, loads from config/{ENV}.toml where ENV defaults to "dev".
    ///
    /// Environment variables can override config values using `__` as separator:
    /// - AI__WORKHORSE__PROVIDER=openai|openrouter [default: openai]
    /// - AI__WORKHORSE__MODEL=gpt-4o-mini [default]
    /// - AI__WORKHORSE__API_KEY=sk-...
    /// - AI__SUMMARIZATION__PROVIDER=openai|openrouter (optional, falls back to workhorse)
    /// - AI__SUMMARIZATION__MODEL=gpt-4o-mini (optional)
    /// - AI__SUMMARIZATION__API_KEY=sk-... (optional)
    /// - TOOLS__TAVILY_API_KEY=tvly-...
    /// - AGENT__HOST=0.0.0.0 [default]
    /// - AGENT__PORT=9080 [default]
    /// - AGENT__PRUNING__SOFT_LIMIT=40 [default]
    /// - AGENT__PRUNING__HARD_LIMIT=50 [default]
    /// - TELEGRAM__BOT_TOKEN=...
    /// - RESTATE__INGRESS_URL=http://localhost:8080 [default]
    /// - TELEMETRY__ENABLED=true [default]
    /// - TELEMETRY__OTLP_ENDPOINT=http://localhost:4318 [default]
    /// - TELEMETRY__SERVICE_NAME=boomerang [default]
    /// - TELEMETRY__EXPORT_INTERVAL_SECS=60 [default]
    pub fn load(path: Option<&str>) -> Result<Self, config::ConfigError> {
        let config_file = match path {
            Some(p) => p.to_string(),
            None => {
                let env = std::env::var("ENV").unwrap_or_else(|_| "dev".to_string());
                format!("config/{}", env)
            }
        };

        let cfg = config::Config::builder()
            .add_source(config::File::with_name(&config_file).required(path.is_some()))
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

    pub fn init(path: Option<&str>) -> Result<(), config::ConfigError> {
        let config = Self::load(path)?;
        CONFIG
            .set(Arc::new(config))
            .map_err(|_| config::ConfigError::Message("Config already initialized".to_string()))?;
        Ok(())
    }
}
