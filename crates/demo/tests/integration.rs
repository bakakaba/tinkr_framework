//! Integration tests verifying that both the HTTP and gRPC routes work on the
//! same multiplexed port produced by [`tinkr_framework::Server`].

use std::net::SocketAddr;

use demo::MyGreeter;
use demo::pb::HelloRequest;
use demo::pb::greeter_client::GreeterClient;
use demo::pb::greeter_server::GreeterServer;

use http_body_util::BodyExt;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use tinkr_framework::Server;
use tinkr_framework::health::{Check, Health, Status};
use tinkr_framework::routing::get;

/// Start a demo server on an OS-assigned port and return its address.
///
/// The listener is pre-bound on port 0 so the address is known before the
/// server task starts; `serve()` accepts the bound listener directly.
async fn spawn(configure: impl FnOnce(Server) -> Server + Send + 'static) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind listener");
    let addr = listener.local_addr().expect("failed to read local addr");

    tokio::spawn(async move {
        let server = Server::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        configure(server)
            .serve(listener)
            .await
            .expect("server error");
    });

    // Give the spawned task a moment to start accepting connections.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    addr
}

/// The default demo server: one HTTP route plus the gRPC greeter.
async fn spawn_server() -> SocketAddr {
    spawn(|server| {
        server
            .route("/hello", get(|| async { "ok" }))
            .grpc_service(GreeterServer::new(MyGreeter))
    })
    .await
}

/// The HTTP `GET /hello` route responds with 200 + "ok".
#[tokio::test]
async fn http_route_responds() {
    let addr = spawn_server().await;

    let (status, body) = http_get(addr, "/hello").await;
    assert_eq!(status, 200, "unexpected HTTP status");
    assert_eq!(body, "ok", "unexpected HTTP body");
}

/// Without a custom evaluation, `/health` reports the service identity from
/// `Server::new`, an overall "ok" status, uptime, and evaluation duration.
#[tokio::test]
async fn health_default_reports_ok() {
    let addr = spawn_server().await;

    let (status, body) = http_get(addr, "/health").await;
    assert_eq!(status, 200, "unexpected HTTP status");
    let json: serde_json::Value = serde_json::from_str(&body).expect("body is not JSON");
    assert_eq!(json["service"], env!("CARGO_PKG_NAME"));
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(json["status"], "ok");
    assert!(json["durationMs"].is_u64(), "durationMs missing: {json}");

    // Uptime is an ISO 8601 duration, e.g. "PT0.050S".
    let uptime = json["uptime"].as_str().expect("uptime missing");
    assert!(
        uptime.starts_with("P") && uptime.ends_with("S"),
        "uptime is not an ISO 8601 duration: {uptime}"
    );

    assert!(
        json.get("checks").is_none(),
        "empty checks should be omitted: {json}"
    );
}

/// A custom health evaluation's checks appear in the `/health` response with
/// name, status, message, and duration, alongside the overall evaluation
/// duration.
#[tokio::test]
async fn health_custom_checks_reported() {
    let addr = spawn(|server| {
        server.health(|| async {
            let mut db = Check::new("database", Status::ERROR);
            db.message = Some("connection refused".to_string());
            db.duration = std::time::Duration::from_millis(7);

            Health {
                status: Status::DEGRADED,
                checks: vec![db, Check::new("cache", Status::OK)],
            }
        })
    })
    .await;

    let (status, body) = http_get(addr, "/health").await;
    // DEGRADED still counts as serving.
    assert_eq!(status, 200, "unexpected HTTP status");
    let json: serde_json::Value = serde_json::from_str(&body).expect("body is not JSON");
    assert_eq!(json["status"], "degraded");
    assert!(json["durationMs"].is_u64(), "durationMs missing: {json}");
    assert_eq!(
        json["checks"],
        serde_json::json!([
            {
                "name": "database",
                "status": "error",
                "message": "connection refused",
                "durationMs": 7,
            },
            {"name": "cache", "status": "ok", "durationMs": 0},
        ]),
    );
}

/// A non-serving overall status — including a consumer-defined one — turns
/// `/health` into a 503.
#[tokio::test]
async fn health_not_serving_is_503() {
    const WARMING_UP: Status = Status::new("warming_up", false);

    let addr = spawn(|server| server.health(|| async { Health::new(WARMING_UP) })).await;

    let (status, body) = http_get(addr, "/health").await;
    assert_eq!(status, 503, "non-serving status should map to 503");
    let json: serde_json::Value = serde_json::from_str(&body).expect("body is not JSON");
    assert_eq!(json["status"], "warming_up");
}

/// The gRPC `Greeter/SayHello` route returns the expected reply.
#[tokio::test]
async fn grpc_route_responds() {
    let addr = spawn_server().await;

    let mut client = GreeterClient::connect(format!("http://{addr}"))
        .await
        .expect("failed to connect gRPC client");

    let reply = client
        .say_hello(HelloRequest {
            name: "world".to_string(),
        })
        .await
        .expect("gRPC call failed")
        .into_inner();

    assert_eq!(reply.message, "Hello world!");
}

/// Both HTTP and gRPC work against the *same* server instance / port.
#[tokio::test]
async fn http_and_grpc_share_one_port() {
    let addr = spawn_server().await;

    // HTTP
    let (status, body) = http_get(addr, "/hello").await;
    assert_eq!(status, 200);
    assert_eq!(body, "ok");

    // gRPC — same addr
    let mut client = GreeterClient::connect(format!("http://{addr}"))
        .await
        .expect("failed to connect gRPC client");
    let reply = client
        .say_hello(HelloRequest {
            name: "tinkr".to_string(),
        })
        .await
        .expect("gRPC call failed")
        .into_inner();
    assert_eq!(reply.message, "Hello tinkr!");
}

/// Unmatched HTTP paths get a plain 404, not tonic's `unimplemented` fallback.
#[tokio::test]
async fn unmatched_path_is_http_404() {
    let addr = spawn_server().await;

    let (status, _) = http_get(addr, "/nonexistent").await;
    assert_eq!(status, 404, "unmatched paths should fall through to a 404");
}

/// Minimal HTTP/1.1 GET helper returning `(status, body)`.
async fn http_get(addr: SocketAddr, path: &str) -> (u16, String) {
    let client: Client<_, http_body_util::Empty<bytes_compat::Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();

    let uri: hyper::Uri = format!("http://{addr}{path}").parse().unwrap();
    let resp = client.get(uri).await.expect("http request failed");
    let status = resp.status().as_u16();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("failed to read body")
        .to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

// `hyper` re-exports `bytes`; alias it so the test doesn't need a direct dep.
mod bytes_compat {
    pub use hyper::body::Bytes;
}
