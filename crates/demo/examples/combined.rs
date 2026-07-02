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

use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));

    let server = demo::builder(addr).build().await?;

    println!("listening on http://{} (HTTP + gRPC)", server.local_addr()?);
    server.serve().await?;

    Ok(())
}
