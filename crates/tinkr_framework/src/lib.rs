//! `tinkr_framework` — a reusable library for standing up API servers.
//!
//! [`Server`] serves HTTP and gRPC on a single port.
//!
//! # Features
//!
//! - `grpc` (default): gRPC support. Without it the server is HTTP-only.
//! - `gcp`: format deployed logs for Google Cloud Logging
//!   ([`tracing-stackdriver`](https://docs.rs/tracing-stackdriver)).
//!
//! # gRPC code generation
//!
//! Generated tonic/prost code refers to the `tonic`, `tonic_prost`, and
//! `prost` crates by name, so a crate containing generated services must
//! declare all three as direct dependencies. Declare them on the same major
//! versions as the framework's [`tonic`], [`tonic_prost`], and [`prost`]
//! re-exports (and use the re-exports in hand-written code) — Cargo then
//! unifies each to a single copy, keeping generated code and the framework
//! in lockstep.
//!
#![cfg_attr(docsrs, feature(doc_cfg))]

mod bootstrap;
pub mod errors;
pub mod health;
pub mod server;
pub mod utilities;

pub use server::Server;

#[doc(hidden)]
pub use bootstrap::init_with as __init_with;

#[doc(no_inline)]
pub use tinkr_config as config;

/// Initializes the service: loads the configuration, sets up logging, and
/// returns the frozen [`config::Config`].
///
/// `init!(AppConfig)` loads a [`config::Configurable`] struct on top of the
/// base fields; `init!()` loads only the base fields (as `Config<()>`).
/// Values resolve per field, highest precedence first: environment variables,
/// `config.toml` in the working directory, declared defaults. The base
/// `name` and `version` fields default to the calling crate's Cargo package.
///
/// Logging reads `RUST_LOG` (default `info`; `.env` is loaded first) and
/// picks the log format by deployment detection (`KUBERNETES_SERVICE_HOST`,
/// `K_SERVICE`, `CLOUD_RUN_JOB`; see the `gcp` feature).
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
    ($ty:ty) => {
        $crate::__init_with::<$ty>(
            ::core::env!("CARGO_PKG_NAME"),
            ::core::env!("CARGO_PKG_VERSION"),
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
pub use prost;
#[cfg(feature = "grpc")]
#[cfg_attr(docsrs, doc(cfg(feature = "grpc")))]
#[doc(no_inline)]
pub use tonic;
#[cfg(feature = "grpc")]
#[cfg_attr(docsrs, doc(cfg(feature = "grpc")))]
#[doc(no_inline)]
pub use tonic_prost;
