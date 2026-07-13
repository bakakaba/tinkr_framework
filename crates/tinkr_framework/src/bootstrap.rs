//! Bootstraps common resources used when running a service.
//!
//! [`init`] loads environment variables from a `.env` file (if present) and
//! initializes logging via [`tracing_subscriber`]. The log output format is
//! selected based on where the process is running:
//!
//! - **Local** (default): human-readable output.
//! - **Deployed** (Kubernetes or Cloud Run detected): structured JSON output.
//!   With the `gcp` feature enabled, logs are formatted for Google Cloud
//!   Logging via [`tracing-stackdriver`](https://docs.rs/tracing-stackdriver).
//!
//! A deployment is detected when any of these environment variables are set:
//! `KUBERNETES_SERVICE_HOST` (Kubernetes), `K_SERVICE` (Cloud Run service),
//! or `CLOUD_RUN_JOB` (Cloud Run job).

use std::env;

use dotenvy::dotenv;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Bootstraps common resources used when running a service.
///
/// Initializes:
/// - Environment variables (from a `.env` file, if present)
/// - Logging (filtered by `RUST_LOG` via [`EnvFilter`])
///
/// # Panics
///
/// Panics if a global tracing subscriber is already set. Call this exactly
/// once, at the start of the application.
pub fn init() {
    // Load .env first so RUST_LOG from .env is picked up by the filter.
    dotenv().ok();

    let filter = EnvFilter::from_default_env();

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
