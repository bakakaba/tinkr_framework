//! Kitchen sink: every optional knob `tinkr_framework` offers.
//!
//! Shows, on top of what `quickstart` covers:
//!
//! - merging a pre-built [`axum::Router`] with `.router(...)`
//! - mixing in single routes with `.route(...)`
//! - registering gRPC services with `.grpc_service(...)`
//! - merging pre-built tonic routes with `.grpc_routes(...)`
//! - a graceful-shutdown clean-up hook with `.on_shutdown(...)`
//! - the flexible bind targets accepted by `.serve(...)`
//!
//! Run with:
//!
//! ```sh
//! cargo run -p demo --example kitchen_sink
//! ```
//!
//! Then, in another shell:
//!
//! ```sh
//! curl http://127.0.0.1:8080/health            # -> ok
//! curl http://127.0.0.1:8080/api/hello         # -> hello from the merged router
//! curl http://127.0.0.1:8080/api/version       # -> demo 0.0.0
//! grpcurl -plaintext -d '{"name":"world"}' \
//!     127.0.0.1:8080 hello.Greeter/SayHello     # -> {"message":"Hello world!"}
//! ```
//!
//! Press ctrl-c (or send SIGTERM) to shut down gracefully: in-flight requests
//! drain first, then the `.on_shutdown(...)` hook runs before the process
//! exits.

use axum::Router;
use axum::routing::get;
use demo::MyGreeter;
use demo::pb::greeter_server::GreeterServer;
use tinkr_framework::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A pre-built axum Router. Build these anywhere (other modules, other
    // crates) and merge them in whole with `.router(...)`.
    let api = Router::new()
        .route(
            "/api/hello",
            get(|| async { "hello from the merged router" }),
        )
        .route("/api/version", get(|| async { "demo 0.0.0" }));

    println!("listening on http://127.0.0.1:8080 (HTTP + gRPC)");

    Server::new()
        // Merge a whole router...
        .router(api)
        // ...and/or add individual routes; both styles compose freely.
        .route("/health", get(|| async { "ok" }))
        // Repeat `.grpc_service(...)` for each gRPC service you have.
        .grpc_service(GreeterServer::new(MyGreeter))
        // Pre-built `tonic::service::Routes` can be merged in whole with
        // `.grpc_routes(routes)` — the gRPC counterpart of `.router(...)`.
        // May only be called once; see the `Server::grpc_routes` docs.
        //
        // Optional: runs after graceful shutdown completes, right before
        // `serve()` returns. Close database pools, flush buffers, etc. here.
        .on_shutdown(async { println!("shutting down, running clean-up") })
        // `serve()` accepts several bind targets (port, IP, string, socket
        // address, pre-bound listener); see the `Server::serve` docs.
        .serve("127.0.0.1:8080")
        .await?;

    Ok(())
}
