//! Demo crate: shows how to stand up a multiplexed HTTP + gRPC server using
//! [`tinkr_framework`].
//!
//! The gRPC code is generated from `proto/hello.proto` at build time.

use tinkr_framework::Server;
use tonic::{Request, Response, Status};

/// Generated protobuf types (client + server) for the `hello` package.
pub mod pb {
    tonic::include_proto!("hello");
}

use pb::greeter_server::{Greeter, GreeterServer};
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

/// Build a [`Server`] wired with an HTTP `GET /health` route and the gRPC
/// [`Greeter`] service. Call `.serve(target)` on the result to run it.
pub fn server() -> Server {
    use axum::routing::get;

    Server::new()
        .route("/health", get(|| async { "ok" }))
        .add_grpc_service(GreeterServer::new(MyGreeter))
}
