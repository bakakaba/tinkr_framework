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

| Feature | Default | What it provides                                                              |
| ------- | ------- | ----------------------------------------------------------------------------- |
| `grpc`  | yes     | Serving gRPC via `tonic`. Disable with `default-features = false`.             |
| `otel`  | yes     | OpenTelemetry export: traces, metrics, and logs over OTLP, plus request spans. |

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

`grpc_reflection` enables [gRPC server reflection](https://grpc.io/docs/guides/reflection/)
(both the `v1` and `v1alpha` protocols), letting clients like `grpcurl`
discover your services at runtime. Pass an encoded `FileDescriptorSet` —
from `tonic_prost_build`'s `file_descriptor_set_path` or buf's
`file_descriptor_set` plugin option (see `crates/demo/buf.gen.yaml`).

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
`env`, `shutdown_timeout`, `name`, and `version` (all but `env` drive the
`Server`). The `env` field is the typed deployment environment: `local`
(the default), `development`, or `production` (`ENV` env var); unknown
names fail at startup. Services with more environments derive
`config::Environment` on their own enum (plus `Default` with a `#[default]`
variant) and select it with `#[config(env = MyEnv)]` on the config struct.
Read the loaded config anywhere with `config::get::<AppConfig>()` and
inspect per-field provenance with `.sources()`. For editor intellisense on
`config.toml`, generate a JSON Schema with a small `config::write_schema`
target and check the file in, guarded by a generate-and-diff CI step (see
`crates/demo/examples/gen_schema.rs` and the `schema` job step; run
`just schema` after changing config structs). See
`crates/demo/examples/config.rs` for the full tour.

## Observability

Logs always go to stdout: human-readable locally, one JSON object per line
when deployed (detected via `KUBERNETES_SERVICE_HOST`, `K_SERVICE`, or
`CLOUD_RUN_JOB`). The JSON format is understood natively by Google Cloud
Logging (`severity`, `logging.googleapis.com/*` fields) and parses cleanly in
Loki/Elastic/Datadog; when tracing is active each line carries the matching
trace IDs, so logs correlate with traces without any pipeline configuration.

With the `otel` feature (default), setting `OTEL_EXPORTER_OTLP_ENDPOINT`
(or `otel_endpoint` in `config.toml`) exports traces, metrics, and logs over
OTLP/gRPC and wraps every request in a server span that continues the
incoming W3C `traceparent`. Unset, telemetry export is disabled entirely.

Point the endpoint at an [OpenTelemetry Collector](https://opentelemetry.io/docs/collector/)
(`http://localhost:4317`) rather than a backend: the collector owns fan-out,
authentication, and retries.

Per-signal overrides follow the OpenTelemetry conventions:
`OTEL_EXPORTER_OTLP_{TRACES,METRICS,LOGS}_ENDPOINT` routes one signal
elsewhere, and `OTEL_{TRACES,METRICS,LOGS}_EXPORTER=none` turns one off.
On platforms that already collect stdout (Cloud Run, GKE) set
`OTEL_LOGS_EXPORTER=none` to keep logs single-sourced — stdout lines carry
the trace IDs regardless.

Buffered telemetry is flushed during graceful shutdown, and `/health`
reports whether export is active (`"otel": true|false`).

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

# A consumer-prescribed deployment-environment enum
ENV=staging cargo run -p demo --example custom_env

# Verify both protocols share one port
cargo test -p demo
```
