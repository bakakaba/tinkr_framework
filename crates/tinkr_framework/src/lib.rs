//! `tinkr_framework` — a reusable library for standing up API servers.
//!
//! [`Server`] serves HTTP and gRPC on a single port.
//!
//! # Features
//!
//! - `grpc` (default): gRPC support. Without it the server is HTTP-only.
//! - `otel` (default): OpenTelemetry export — traces, metrics, and logs
//!   over OTLP/gRPC, plus a span and request metrics for every request.
//!   Compiled in by default but inert until an endpoint is configured; see
//!   [`init!`].
//!
//! # gRPC code generation
//!
//! Generated tonic/prost code refers to the `tonic`, `tonic_prost`, and
//! `prost` crates by name, so a crate containing generated services must
//! declare all three as direct dependencies, on the same major version as
//! the framework's [`tonic`] re-export — Cargo then unifies each to a
//! single copy, keeping generated code and the framework in lockstep.
//!
#![cfg_attr(docsrs, feature(doc_cfg))]

mod bootstrap;
pub mod errors;
pub mod health;
mod logging;
#[cfg(feature = "otel")]
mod otel;
pub mod server;
pub mod utilities;

pub use server::Server;

#[doc(hidden)]
pub use bootstrap::init_with as __init_with;

#[doc(no_inline)]
pub use tinkr_config as config;

/// Initializes the service: loads the configuration, sets up logging and
/// telemetry, and returns the frozen [`config::Config`].
///
/// `init!(AppConfig)` loads a [`config::Configurable`] struct on top of the
/// base fields; `init!()` loads only the base fields (as `Config<()>`).
/// Values resolve per field, highest precedence first: environment variables,
/// the configuration file, declared defaults. The base `name` and `version`
/// fields default to the calling crate's Cargo package.
///
/// The configuration file is the path in the `CONFIG_FILE` environment
/// variable when set — letting one build read per-environment configuration
/// from wherever the deployment mounts it — otherwise `config.toml` in the
/// working directory. An explicitly named file must exist; the default
/// `config.toml` may be absent. The optional `config_file = ...` parameter
/// takes full control of the location instead: exactly that file is loaded
/// and `$CONFIG_FILE` is ignored (value overrides via other environment
/// variables still apply). It accepts a path or an `Option` of one
/// ([`config::IntoConfigFile`]), so the decision can be made at runtime —
/// `None` preserves the default resolution:
///
/// ```no_run
/// # /// Empty.
/// # #[derive(Debug, tinkr_framework::config::Configurable)]
/// # struct AppConfig {}
/// # fn stub() -> tinkr_framework::errors::Result<()> {
/// let config = tinkr_framework::init!(AppConfig, config_file = "/etc/config/config.toml")?;
/// # Ok(())
/// # }
/// ```
///
/// Logging reads `RUST_LOG` (default `info`; `.env` is loaded first) and
/// picks the log format by deployment detection (`KUBERNETES_SERVICE_HOST`,
/// `K_SERVICE`, `CLOUD_RUN_JOB`): human-readable locally, one JSON object
/// per line when deployed (with trace-correlation fields understood by
/// Google Cloud Logging and Grafana Loki alike).
///
/// # Telemetry (`otel` feature, default)
///
/// Traces, metrics, and logs are exported over OTLP when an endpoint is
/// configured — typically an [OpenTelemetry Collector] at
/// `http://localhost:4317`, which fans out to backends (Google Cloud,
/// Grafana, ...) as an operational concern. Without an endpoint, export is
/// disabled and costs nothing at runtime.
///
/// Per signal, the endpoint resolves from
/// `OTEL_EXPORTER_OTLP_{TRACES,METRICS,LOGS}_ENDPOINT` first, then the
/// `otel_endpoint` base field (`OTEL_EXPORTER_OTLP_ENDPOINT` or
/// `config.toml`). `OTEL_{TRACES,METRICS,LOGS}_EXPORTER=none` disables a
/// signal — e.g. set `OTEL_LOGS_EXPORTER=none` on platforms that already
/// collect stdout logs (Cloud Run, GKE), which keeps logs single-sourced
/// while traces and metrics flow through the collector.
///
/// Hosted collectors work too: export requests carry the headers from the
/// `otel_headers` base field (`OTEL_EXPORTER_OTLP_HEADERS` or `config.toml`;
/// `key=value` pairs separated by commas, typically an API key), and
/// `https://` endpoints are verified against the system roots. The standard
/// `OTEL_EXPORTER_OTLP_CERTIFICATE`, `_CLIENT_CERTIFICATE`, and `_CLIENT_KEY`
/// variables (paths to PEM files, per-signal variants included) swap in a
/// custom CA or enable mTLS.
///
/// When span export is active every [`Server`] request gets a span,
/// continuing the incoming W3C `traceparent` context, and deployed log
/// lines carry the matching trace IDs. When metric export is active every
/// request records the semantic-convention `http.server.request.duration`
/// and `http.server.active_requests` metrics — gRPC requests included,
/// with `rpc.*` attributes and the final `grpc-status` read from the
/// response trailers. Additional application metrics record through
/// `opentelemetry::global::meter`. Buffered telemetry is flushed during
/// graceful shutdown.
///
/// [OpenTelemetry Collector]: https://opentelemetry.io/docs/collector/
///
/// Call exactly once, at the top of `main`. Afterwards the configuration is
/// readable anywhere with [`config::get`], and [`Server`]s can be built.
///
/// ```
/// let cfg = tinkr_framework::init!()?;
/// assert_eq!(cfg.port, 8080); // base field default
/// # Ok::<(), tinkr_framework::errors::Error>(())
/// ```
///
/// # Panics
///
/// Panics when called more than once, or when `RUST_LOG` is invalid.
#[macro_export]
macro_rules! init {
    () => {
        $crate::init!(())
    };
    (config_file = $path:expr) => {
        $crate::init!((), config_file = $path)
    };
    ($ty:ty) => {
        $crate::__init_with::<$ty>(
            ::core::env!("CARGO_PKG_NAME"),
            ::core::env!("CARGO_PKG_VERSION"),
            ::core::option::Option::None,
        )
    };
    ($ty:ty, config_file = $path:expr) => {
        $crate::__init_with::<$ty>(
            ::core::env!("CARGO_PKG_NAME"),
            ::core::env!("CARGO_PKG_VERSION"),
            $crate::config::IntoConfigFile::into_config_file($path).as_deref(),
        )
    };
}

#[doc(no_inline)]
pub use axum;
#[doc(no_inline)]
pub use axum::{Router, routing};

#[cfg(feature = "grpc")]
#[cfg_attr(docsrs, doc(cfg(feature = "grpc")))]
#[doc(no_inline)]
pub use tonic;

#[cfg(feature = "otel")]
#[cfg_attr(docsrs, doc(cfg(feature = "otel")))]
#[doc(no_inline)]
pub use opentelemetry;
