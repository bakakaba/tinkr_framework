//! `tinkr_framework` — a reusable library for standing up API servers.
//!
//! It provides a [`Server`] for running HTTP (via [`axum`]) and gRPC (via
//! [`tonic`]) on a single, multiplexed port. [`Server::serve`] runs until
//! `ctrl-c` / `SIGTERM`, shuts down gracefully, and runs an optional clean-up
//! hook.
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

pub mod error;
pub mod server;

pub use error::{Error, Result};
pub use server::{ServeTarget, Server};
