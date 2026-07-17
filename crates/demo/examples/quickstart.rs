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
//! curl http://127.0.0.1:8080/health           # -> {"service":"demo",...,"status":"ok"}
//! grpcurl -plaintext -d '{"name":"world"}' \
//!     127.0.0.1:8080 hello.Greeter/SayHello    # -> {"message":"Hello world!"}
//! ```
//!
//! Press ctrl-c to shut down gracefully.

use demo::MyGreeter;
use demo::pb::greeter_server::GreeterServer;
use tinkr_framework::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env + configuration (env vars > config.toml > defaults) and set
    // up RUST_LOG-filtered logging. Call exactly once.
    tinkr_framework::init!()?;

    // `/health` is built in; nothing to register for it. serve() listens on
    // the configured port (default 8080) on both IPv4 and IPv6.
    Server::new()
        .grpc_service(GreeterServer::new(MyGreeter))
        .serve()
        .await?;

    Ok(())
}
