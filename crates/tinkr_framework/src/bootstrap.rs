//! Bootstraps common resources used when running a service.

use std::env;

use dotenvy::dotenv;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Bootstraps common resources used when running a service.
///
/// Initializes:
/// - Environment variables (from a `.env` file, if present)
/// - Logging (filtered by `RUST_LOG` via [`EnvFilter`], defaulting to `info`
///   when `RUST_LOG` is unset)
///
/// The log output format is selected based on where the process is running:
///
/// - **Local** (default): human-readable output.
/// - **Deployed** (Kubernetes or Cloud Run detected via
///   `KUBERNETES_SERVICE_HOST`, `K_SERVICE`, or `CLOUD_RUN_JOB`): structured
///   JSON output, or the Google Cloud Logging format with the `gcp` feature
///   enabled.
///
/// # Panics
///
/// Panics if a global tracing subscriber is already set. Call this exactly
/// once, at the start of the application.
///
/// Panics if `RUST_LOG` contains an invalid filter directive, so
/// misconfiguration is surfaced at startup instead of being silently
/// ignored.
pub fn init() {
    // Load .env first so RUST_LOG from .env is picked up by the filter.
    dotenv().ok();

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
        return;
    }

    #[cfg(feature = "gcp")]
    registry.with(tracing_stackdriver::layer()).init();

    #[cfg(not(feature = "gcp"))]
    registry.with(fmt::layer().json()).init();
}
