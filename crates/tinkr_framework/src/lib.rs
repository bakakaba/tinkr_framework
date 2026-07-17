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
#![cfg_attr(docsrs, feature(doc_cfg))]

mod bootstrap;
pub mod errors;
pub mod health;
pub mod server;
pub mod utilities;

pub use bootstrap::init;
pub use server::Server;

#[doc(no_inline)]
pub use axum;
#[doc(no_inline)]
pub use axum::{Router, routing};

#[cfg(feature = "grpc")]
#[cfg_attr(docsrs, doc(cfg(feature = "grpc")))]
#[doc(no_inline)]
pub use tonic;
