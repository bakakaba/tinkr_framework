# tinkr_framework

Rust based framework for building API's.

`tinkr_framework` provides a [`Server`] for standing up an HTTP server (via
[`axum`](https://docs.rs/axum)) and a gRPC server (via
[`tonic`](https://docs.rs/tonic)) on a **single, multiplexed port**. Requests are
dispatched by content-type: `application/grpc*` is routed to the registered
tonic services, everything else is routed to the axum router.

`serve()` runs until the process receives `ctrl-c` (or `SIGTERM` on unix),
shuts down gracefully, and runs an optional clean-up hook.

## Features

| Feature | Default | Description                              |
| ------- | ------- | ---------------------------------------- |
| `grpc`  | yes     | gRPC support via `tonic`.                |

HTTP/REST support (via `axum`) is always available. Disable gRPC with
`default-features = false`.

## Usage

```rust,no_run
use tinkr_framework::Server;
use axum::routing::get;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Server::new()
        .route("/health", get(|| async { "ok" }))          // HTTP
        .add_grpc_service(my_grpc_server)                    // gRPC
        .on_shutdown(async { /* close pools, flush, ... */ })
        .serve(8080)
        .await?;
    Ok(())
}
```

`serve()` accepts several bind targets:

| Call                              | Binds                                  |
| --------------------------------- | -------------------------------------- |
| `serve(8080)`                     | `0.0.0.0:8080`                         |
| `serve([127, 0, 0, 1])`           | `127.0.0.1:8080`                       |
| `serve("10.0.0.1")`               | `10.0.0.1:8080`                        |
| `serve("10.0.0.1:3000")`          | `10.0.0.1:3000`                        |
| `serve(socket_addr)`              | the given `SocketAddr`                 |
| `serve(tcp_listener)`             | a pre-bound `tokio::net::TcpListener` (useful in tests to bind port `0` and read `local_addr()` first) |

### gRPC services

`add_grpc_service` accepts the generated `XxxServer<T>` type. You build the
protobuf descriptors yourself and pass the resulting server in. Both toolchains
are supported:

- **tonic-build / tonic-prost-build** — compile `.proto` files in a `build.rs`.
- **buf** — generate with `buf generate`.

Both emit the same concrete `XxxServer<T>`, so registration is identical.

## Demo

The `demo` crate (`crates/demo`, not published) shows a full setup: a
`proto/hello.proto`, a `build.rs` that compiles it, an HTTP `GET /health` route,
and the gRPC `Greeter` service — all on one port.

```sh
# Run the example server
cargo run -p demo --example combined

# Verify both protocols share one port
cargo test -p demo
```
