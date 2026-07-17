//! Bootstraps common resources used when running a service.

use std::env;

use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::errors::Result;

/// Implementation of [`crate::init!`]; call the macro instead, which fills
/// in `name` and `version` from the calling crate's Cargo package.
#[doc(hidden)]
pub fn init_with<T>(name: &str, version: &str) -> Result<&'static tinkr_config::Config<T>>
where
    T: tinkr_config::Configurable + Send + Sync + 'static,
{
    // Load the configuration first: it loads .env, so RUST_LOG set there is
    // picked up by the filter below. Errors are returned before logging is
    // available.
    let config = tinkr_config::load_with::<T>(name, version)?;

    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env()
        .expect("invalid RUST_LOG environment variable");

    let is_deployed = env::var("KUBERNETES_SERVICE_HOST").is_ok() // Kubernetes
        || env::var("K_SERVICE").is_ok() // Cloud Run service
        || env::var("CLOUD_RUN_JOB").is_ok(); // Cloud Run job

    let registry = tracing_subscriber::registry().with(filter);

    if !is_deployed {
        registry.with(fmt::layer()).init();
    } else {
        #[cfg(feature = "gcp")]
        registry.with(tracing_stackdriver::layer()).init();

        #[cfg(not(feature = "gcp"))]
        registry.with(fmt::layer().json()).init();
    }

    tracing::debug!("configuration sources:\n{}", config.sources());
    Ok(config)
}
