//! `tinkr_framework` — a reusable library for standing up API servers.
//!
//! It provides a builder for running HTTP (via [`axum`]) and gRPC (via
//! [`tonic`]) on a single, multiplexed port.
//!
//! # Features
//!
//! - `grpc` (default): gRPC support via [`tonic`].
//!
//! HTTP/REST support (via [`axum`]) is always available.
//!
//! # Example
//!
//! ```no_run
//! use tinkr_framework::ServerBuilder;
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! use axum::routing::get;
//!
//! let server = ServerBuilder::new()
//!     .bind(([0, 0, 0, 0], 8080))
//!     .route("/health", get(|| async { "ok" }))
//!     .build()
//!     .await?;
//!
//! server.serve().await?;
//! # Ok(())
//! # }
//! ```
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod error;
pub mod server;

pub use error::{Error, Result};
pub use server::{Server, ServerBuilder};
