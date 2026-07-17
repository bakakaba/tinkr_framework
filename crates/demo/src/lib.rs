//! Demo crate: shows how to stand up a multiplexed HTTP + gRPC server using
//! [`tinkr_framework`].
//!
//! Provides the [`pb`] module (generated from `proto/hello.proto`), a
//! trivial [`MyGreeter`] service implementation, and the [`config`] module
//! with the demo's [`AppConfig`](config::AppConfig); see the runnable
//! examples for full server setups:
//!
//! - `examples/quickstart.rs` — the minimal configuration to get started.
//! - `examples/kitchen_sink.rs` — every optional knob (router merging,
//!   shutdown hook, serve targets, ...).
//! - `examples/config.rs` — layered configuration: `config.toml`, env
//!   overrides, provenance readout, and global access.
//! - `examples/gen_schema.rs` — regenerates `config.schema.json` for editor
//!   intellisense on `config.toml`.

use tonic::{Request, Response, Status};

pub mod config;

/// Generated protobuf types (client + server) for the `hello` package.
pub mod pb {
    include!("gen/hello/hello.rs");
}

use pb::greeter_server::Greeter;
use pb::{HelloReply, HelloRequest};

/// A trivial [`Greeter`] implementation.
#[derive(Debug, Default, Clone)]
pub struct MyGreeter;

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        let name = request.into_inner().name;
        Ok(Response::new(HelloReply {
            message: format!("Hello {name}!"),
        }))
    }
}
