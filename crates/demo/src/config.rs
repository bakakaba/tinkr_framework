//! Demo application configuration, shared by `examples/config.rs` and the
//! integration tests.

use tinkr_framework::config::Configurable;

/// Configuration for the demo service.
///
/// Application fields defined here compose with the framework-provided ones
/// (`port`, `environment`, `shutdown_timeout`, `name`, `version`); all of
/// them live at the top level of `config.toml` and can be overridden by
/// their environment variables.
#[derive(Debug, Configurable)]
pub struct AppConfig {
    /// Greeting returned by `GET /greeting`.
    #[config(env = "GREETING", default = "Hello from tinkr_config!")]
    pub greeting: String,

    /// Number of worker tasks.
    #[config(env = "WORKERS", default = 4)]
    pub workers: usize,

    /// Database connection URL; redacted in the startup readout.
    #[config(env = "DATABASE_URL", secret)]
    pub database_url: Option<String>,

    /// Cache tuning.
    #[config(nested)]
    pub cache: CacheConfig,
}

/// Cache tuning knobs.
#[derive(Debug, Configurable)]
pub struct CacheConfig {
    /// Entry time-to-live, in seconds.
    #[config(env = "CACHE_TTL", default = 300)]
    pub ttl: u64,

    /// Maximum number of cached entries.
    #[config(default = 1024)]
    pub capacity: u64,
}
