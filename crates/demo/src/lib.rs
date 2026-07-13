//! Demo crate: shows how to stand up a multiplexed HTTP + gRPC server using
//! [`tinkr_framework`].
//!
//! The gRPC code is generated from `proto/hello.proto` with
//! [`buf generate`](https://buf.build/docs/generate/) and checked in under
//! `src/gen/`; run `just gen` after editing the proto. This crate provides
//! the generated [`pb`] module and a trivial [`MyGreeter`] service
//! implementation; see the runnable examples for full server setups:
//!
//! - `examples/quickstart.rs` — the minimal configuration to get started.
//! - `examples/kitchen_sink.rs` — every optional knob (router merging,
//!   shutdown hook, serve targets, ...).

use tonic::{Request, Response, Status};

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
