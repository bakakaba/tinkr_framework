//! End-to-end telemetry: REST + gRPC served on one port, with traces,
//! metrics, and logs flowing to the local LGTM stack.
//!
//! ```sh
//! just up          # start the observability stack
//! just dev otel    # run this example (ctrl-c flushes telemetry and exits)
//! ```
//!
//! The server generates its own traffic — an HTTP `GET /hello` (plus an
//! occasional `GET /error`) and a gRPC `Greeter/SayHello` call every couple
//! of seconds — so data appears in Grafana (<http://localhost:3000>) without
//! any manual requests. Explore:
//!
//! - **Traces** (Tempo): `GET /hello` HTTP server spans with a
//!   `compose_greeting` child span, and `/hello.Greeter/SayHello` gRPC
//!   server spans — both from the framework's request middleware.
//! - **Logs** (Loki): handler log lines carrying `trace_id`s that match the
//!   spans; `/error` arrives with `ERROR` severity.
//! - **Metrics** (Prometheus): `demo_otel_requests_total` and
//!   `demo_otel_latency_milliseconds_*` (metrics export every 60 s by
//!   default, so allow a minute; override with
//!   `OTEL_METRIC_EXPORT_INTERVAL=5000` to speed it up).
//!
//! The OTLP endpoint comes from `otel_endpoint` in `config.toml` (or
//! `OTEL_EXPORTER_OTLP_ENDPOINT`); with the stack down, export fails
//! quietly in the background and the server keeps serving.
//!
//! The gRPC client below is uninstrumented, so its calls start fresh traces
//! — propagating outgoing context is the caller's concern; the framework
//! instruments the server side.

use std::time::{Duration, Instant};

use demo::MyGreeter;
use demo::pb::HelloRequest;
use demo::pb::greeter_client::GreeterClient;
use demo::pb::greeter_server::GreeterServer;
use tinkr_framework::Server;
use tinkr_framework::opentelemetry::global;
use tinkr_framework::routing::get;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::Instrument;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = tinkr_framework::init!()?;
    let port = cfg.port;

    // Custom metrics record through the OpenTelemetry global meter (via the
    // framework's `opentelemetry` re-export) and export alongside the
    // built-in telemetry.
    let meter = global::meter("demo");
    let requests = meter.u64_counter("demo.otel.requests").build();
    let latency = meter
        .f64_histogram("demo.otel.latency")
        .with_unit("ms")
        .build();

    tokio::spawn(traffic(port));

    Server::new()
        .route(
            "/hello",
            get(move || {
                let requests = requests.clone();
                let latency = latency.clone();
                async move {
                    let started = Instant::now();
                    requests.add(1, &[]);
                    // Child spans nest under the framework's per-request
                    // span, sharing its trace.
                    let greeting = compose_greeting()
                        .instrument(tracing::info_span!("compose_greeting"))
                        .await;
                    latency.record(started.elapsed().as_secs_f64() * 1000.0, &[]);
                    greeting
                }
            }),
        )
        .route(
            "/error",
            get(|| async {
                // ERROR-severity log line, correlated to this request's
                // trace, plus a 500 recorded on the request span.
                tracing::error!(cause = "demonstration", "the teapot is on fire");
                (
                    tinkr_framework::axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "boom",
                )
            }),
        )
        .grpc_service(GreeterServer::new(MyGreeter))
        .grpc_reflection(demo::pb::FILE_DESCRIPTOR_SET)
        .serve()
        .await?;

    Ok(())
}

/// Simulated work inside `GET /hello`, logged and traced.
async fn compose_greeting() -> &'static str {
    tokio::time::sleep(Duration::from_millis(15)).await;
    tracing::info!(recipient = "world", "composed greeting");
    "hello"
}

/// Demo traffic so telemetry flows without manual requests: HTTP and gRPC
/// against our own port, every couple of seconds.
async fn traffic(port: u16) {
    // Let the server bind first.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut n = 0u64;
    loop {
        n += 1;
        let _ = http_get(port, "/hello").await;
        if n.is_multiple_of(5) {
            let _ = http_get(port, "/error").await;
        }
        if let Ok(mut greeter) = GreeterClient::connect(format!("http://127.0.0.1:{port}")).await {
            let _ = greeter
                .say_hello(HelloRequest {
                    name: format!("visitor {n}"),
                })
                .await;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Minimal HTTP GET against the local server (avoids an HTTP client
/// dependency; the response is discarded).
async fn http_get(port: u16, path: &str) -> std::io::Result<()> {
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;
    Ok(())
}
