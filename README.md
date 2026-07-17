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

| Feature | Default | When to enable                                                    |
| ------- | ------- | ----------------------------------------------------------------- |
| `grpc`  | yes     | Serving gRPC via `tonic`. Disable with `default-features = false`. |
| `gcp`   | no      | Deploying to Google Cloud — deployed logs use the Cloud Logging format. |

HTTP/REST support (via `axum`) is always available.

## Usage

Register HTTP routes and gRPC services on a `Server`, then call `serve()`.
See the [demo](#demo) for complete, runnable programs, and the `Server`
rustdoc for the accepted bind targets. Optionally, register a clean-up hook
with `.on_shutdown(async { ... })` — it runs after graceful shutdown
completes, right before `serve()` returns.

### gRPC services

`grpc_service` accepts the generated `XxxServer<T>` type. You build the
protobuf descriptors yourself and pass the resulting server in. Both toolchains
are supported:

- **tonic-build / tonic-prost-build** — compile `.proto` files in a `build.rs`.
- **buf** — generate with `buf generate`.

Both emit the same concrete `XxxServer<T>`, so registration is identical.

## Bootstrap

Call `bootstrap::init()` first thing in `main`: it loads `.env` (if present)
and initializes `RUST_LOG`-filtered logging (defaulting to `info` when
`RUST_LOG` is unset), picking the output format for the environment
(human-readable locally, structured for deployments). Call it
**exactly once** — a second call panics, as does an invalid `RUST_LOG`
value so misconfiguration is caught at startup.

## Utilities

`utilities::new_id(prefix)` generates a prefixed
[ULID](https://github.com/ulid/spec) identifier, e.g. `user_01JGWXYZ...`.
Persisted identifiers should always include a prefix.

## Demo

The `demo` crate (`crates/demo`, not published) shows a full setup: a
`proto/hello.proto` compiled with `buf generate` (generated code is checked in
under `crates/demo/src/gen/`; run `just gen` after editing the proto), an HTTP
`GET /health` route, and the gRPC `Greeter` service — all on one port.

```sh
# Minimal configuration to get started
cargo run -p demo --example quickstart

# Every optional knob: router merging, shutdown hook, serve targets, ...
cargo run -p demo --example kitchen_sink

# Verify both protocols share one port
cargo test -p demo
```
