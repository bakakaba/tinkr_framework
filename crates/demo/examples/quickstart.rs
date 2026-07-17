//! Quickstart: the minimal configuration to get a multiplexed HTTP + gRPC
//! server running on one port.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p demo --example quickstart
//! ```
//!
//! Then, in another shell:
//!
//! ```sh
//! curl http://127.0.0.1:8080/health           # -> ok
//! grpcurl -plaintext -d '{"name":"world"}' \
//!     127.0.0.1:8080 hello.Greeter/SayHello    # -> {"message":"Hello world!"}
//! ```
//!
//! Press ctrl-c to shut down gracefully.

use demo::MyGreeter;
use demo::pb::greeter_server::GreeterServer;
use tinkr_framework::{Server, routing::get};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("listening on http://0.0.0.0:8080 (HTTP + gRPC)");

    Server::new()
        .route("/health", get(|| async { "ok" }))
        .grpc_service(GreeterServer::new(MyGreeter))
        .serve(8080)
        .await?;

    Ok(())
}
