//! `tinkr_framework` — a reusable library for standing up API servers.
//!
//! It provides a [`Server`] for running HTTP (via [`axum`]) and gRPC (via
//! [`tonic`]) on a single, multiplexed port. [`Server::serve`] runs until
//! `ctrl-c` / `SIGTERM`, shuts down gracefully, and runs an optional clean-up
//! hook.
//!
//! It also ships service essentials: [`bootstrap::init`] loads `.env` and
//! initializes logging (human-readable locally, structured JSON when
//! deployed), and [`utilities::new_id`] generates prefixed ULID identifiers.
//!
//! # Features
//!
//! - `grpc` (default): gRPC support via [`tonic`].
//! - `gcp`: format deployed logs for Google Cloud Logging
//!   ([`tracing-stackdriver`](https://docs.rs/tracing-stackdriver)).
//!
//! HTTP/REST support (via [`axum`]) is always available.
//!
#![cfg_attr(docsrs, feature(doc_cfg))]

mod bootstrap;
pub mod errors;
pub mod server;
pub mod utilities;

pub use bootstrap::init;
pub use server::Server;
