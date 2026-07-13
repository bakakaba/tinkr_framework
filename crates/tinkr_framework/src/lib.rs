//! `tinkr_framework` — a reusable library for standing up API servers.
//!
//! It provides a [`Server`] for running HTTP (via [`axum`]) and gRPC (via
//! [`tonic`]) on a single, multiplexed port. [`Server::serve`] runs until
//! `ctrl-c` / `SIGTERM`, shuts down gracefully, and runs an optional clean-up
//! hook.
//!
//! It also ships service essentials: [`bootstrap::init`] loads `.env` and
//! initializes logging (human-readable locally, structured JSON when
//! deployed), and [`new_id`] generates prefixed ULID identifiers.
//!
//! # Features
//!
//! - `grpc` (default): gRPC support via [`tonic`].
//! - `gcp`: format deployed logs for Google Cloud Logging
//!   ([`tracing-stackdriver`](https://docs.rs/tracing-stackdriver)).
//!
//! HTTP/REST support (via [`axum`]) is always available.
//!
//! # Example
//!
//! ```no_run
//! use tinkr_framework::Server;
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! use axum::routing::get;
//!
//! Server::new()
//!     .route("/health", get(|| async { "ok" }))
//!     .serve(8080)
//!     .await?;
//! # Ok(())
//! # }
//! ```
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod bootstrap;
pub mod error;
pub mod server;
pub mod utilities;

pub use error::{Error, Result};
pub use server::{ServeTarget, Server};
pub use utilities::new_id;
