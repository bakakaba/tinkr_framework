//! Kitchen sink: every optional knob `tinkr_framework` offers.
//!
//! Shows, on top of what `quickstart` covers:
//!
//! - merging a pre-built [`Router`] with `.router(...)`
//! - mixing in single routes with `.route(...)`
//! - registering gRPC services with `.grpc_service(...)`
//! - customizing the built-in `/health` endpoint with `.health(...)`,
//!   including consumer-defined statuses
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
//! curl http://127.0.0.1:8080/health            # -> {"service":"demo",...,"checks":[...]}
//! curl http://127.0.0.1:8080/api/hello         # -> hello from the merged router
//! curl http://127.0.0.1:8080/api/version       # -> demo 0.0.0
//! grpcurl -plaintext -d '{"name":"world"}' \
//!     127.0.0.1:8080 hello.Greeter/SayHello     # -> {"message":"Hello world!"}
//! ```
//!
//! Press ctrl-c (or send SIGTERM) to shut down gracefully: in-flight requests
//! drain first, then the `.on_shutdown(...)` hook runs before the process
//! exits.

use std::time::Instant;

use demo::MyGreeter;
use demo::pb::greeter_server::GreeterServer;
use tinkr_framework::health::{Check, Health, Status};
use tinkr_framework::routing::get;
use tinkr_framework::{Router, Server};

// The standard statuses (`Status::OK`, `DEGRADED`, `ERROR`) can be extended
// with your own. The bool says whether `/health` should still answer 200.
const READ_ONLY: Status = Status::new("read_only", true);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env and set up RUST_LOG-filtered logging. Call exactly once.
    tinkr_framework::init();

    // A pre-built Router. Build these anywhere (other modules, other
    // crates) and merge them in whole with `.router(...)`.
    let api = Router::new()
        .route(
            "/api/hello",
            get(|| async { "hello from the merged router" }),
        )
        .route("/api/version", get(|| async { "demo 0.0.0" }));

    tracing::info!("listening on http://127.0.0.1:8080 (HTTP + gRPC)");

    Server::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
        // Merge a whole router...
        .router(api)
        // ...and/or add individual routes; both styles compose freely.
        .route("/api/echo", get(|| async { "echo" }))
        // Repeat `.grpc_service(...)` for each gRPC service you have.
        .grpc_service(GreeterServer::new(MyGreeter))
        // Optional: customize the built-in `/health` endpoint. The function
        // returns the overall status plus the checks it was derived from.
        .health(|| async {
            let start = Instant::now();
            let database = ping_database().await;

            let db_check = Check {
                name: "database".into(),
                status: if database.is_ok() {
                    Status::OK
                } else {
                    Status::ERROR
                },
                message: database.err(),
                duration: start.elapsed(),
            };

            // Derive the overall status however makes sense for the service;
            // here we can still serve reads from cache without the database.
            let overall = if db_check.status == Status::ERROR {
                READ_ONLY
            } else {
                Status::OK
            };

            Health {
                status: overall,
                checks: vec![db_check],
            }
        })
        // Optional: runs after graceful shutdown completes, right before
        // `serve()` returns. Close database pools, flush buffers, etc. here.
        .on_shutdown(async { tracing::info!("shutting down, running clean-up") })
        // `serve()` accepts several bind targets (port, IP, string, socket
        // address, pre-bound listener); see the `Server::serve` docs.
        .serve("127.0.0.1:8080")
        .await?;

    Ok(())
}

/// Stand-in for a real dependency probe.
async fn ping_database() -> Result<(), String> {
    Ok(())
}
