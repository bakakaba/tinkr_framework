//! Runnable example: starts a multiplexed HTTP + gRPC server on one port.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p demo --example combined
//! ```
//!
//! Then, in another shell:
//!
//! ```sh
//! curl http://127.0.0.1:8080/health           # -> ok   (HTTP via axum)
//! grpcurl -plaintext -d '{"name":"world"}' \
//!     127.0.0.1:8080 hello.Greeter/SayHello    # -> {"message":"Hello world!"}
//! ```
//!
//! Press ctrl-c to shut down gracefully; the clean-up hook runs before exit.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("listening on http://127.0.0.1:8080 (HTTP + gRPC)");

    demo::server()
        .on_shutdown(async { println!("shutting down, running clean-up") })
        .serve("127.0.0.1:8080")
        .await?;

    Ok(())
}
