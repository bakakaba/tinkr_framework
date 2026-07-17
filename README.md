# tinkr_framework

Rust based framework for building API's.

`tinkr_framework` provides a [`Server`] for standing up an HTTP server (via
[`axum`](https://docs.rs/axum)) and a gRPC server (via
[`tonic`](https://docs.rs/tonic)) on a **single, multiplexed port**. Requests are
dispatched by content-type: `application/grpc*` is routed to the registered
tonic services, everything else is routed to the axum router.

`serve()` listens on the configured port (IPv4 + IPv6) and runs until the
process receives `ctrl-c` (or `SIGTERM` on unix), then shuts down gracefully —
within the configured grace period — and runs an optional clean-up hook.

## Features

| Feature | Default | When to enable                                                          |
| ------- | ------- | ----------------------------------------------------------------------- |
| `grpc`  | yes     | Serving gRPC via `tonic`. Disable with `default-features = false`.      |
| `gcp`   | no      | Deploying to Google Cloud — deployed logs use the Cloud Logging format. |

HTTP/REST support (via `axum`) is always available.

## Usage

Call `tinkr_framework::init!` first, then register HTTP routes and gRPC
services on a `Server` and call `serve()`. See the [demo](#demo) for
complete, runnable programs, and the `Server::bind` rustdoc for serving
extra addresses. Optionally, register a clean-up hook with
`.on_shutdown(async { ... })` — it runs after graceful shutdown completes,
right before `serve()` returns.

### gRPC services

`grpc_service` accepts the generated `XxxServer<T>` type. You build the
protobuf descriptors yourself and pass the resulting server in. Both toolchains
are supported:

- **tonic-build / tonic-prost-build** — compile `.proto` files in a `build.rs`.
- **buf** — generate with `buf generate`.

Both emit the same concrete `XxxServer<T>`, so registration is identical.

## Bootstrap & configuration

Call `tinkr_framework::init!` first thing in `main`: it loads `.env` (if
present) and the configuration, initializes `RUST_LOG`-filtered logging
(defaulting to `info` when `RUST_LOG` is unset, with the output format picked
for the environment), and returns the frozen config. Call it **exactly
once** — a second call panics, as does an invalid `RUST_LOG` value, so
misconfiguration is caught at startup.

Configuration comes from [`tinkr_config`](crates/tinkr_config) (re-exported
as `tinkr_framework::config`): derive `Configurable` on a struct describing
your settings and pass it to `init!`. Each field resolves from its
environment variable, then `config.toml` in the working directory, then the
declared default; every service also gets the base fields `port`,
`environment`, `shutdown_timeout`, `name`, and `version`, which drive the
`Server`. Read the loaded config anywhere with `config::get::<AppConfig>()` and
inspect per-field provenance with `.sources()`. For editor intellisense on
`config.toml`, generate a JSON Schema with a small `config::write_schema`
target and check the file in, guarded by a generate-and-diff CI step (see
`crates/demo/examples/gen_schema.rs` and the `schema` job step; run
`just schema` after changing config structs). See
`crates/demo/examples/config.rs` for the full tour.

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

# Every optional knob: router merging, shutdown hook, multiple binds, ...
cargo run -p demo --example kitchen_sink

# Layered configuration: config.toml, env overrides, provenance, schema
cargo run -p demo --example config    # run from crates/demo

# Verify both protocols share one port
cargo test -p demo
```
