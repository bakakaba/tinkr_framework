//! Integration tests verifying that both the HTTP and gRPC routes work on the
//! same multiplexed port produced by [`tinkr_framework::Server`].

use std::net::SocketAddr;

use demo::MyGreeter;
use demo::pb::HelloRequest;
use demo::pb::greeter_client::GreeterClient;
use demo::pb::greeter_server::GreeterServer;

use axum::routing::get;
use http_body_util::BodyExt;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use tinkr_framework::Server;

/// Start the demo server on an OS-assigned port and return its address.
///
/// The listener is pre-bound on port 0 so the address is known before the
/// server task starts; `serve()` accepts the bound listener directly.
async fn spawn_server() -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind listener");
    let addr = listener.local_addr().expect("failed to read local addr");

    tokio::spawn(async move {
        Server::new()
            .route("/health", get(|| async { "ok" }))
            .grpc_service(GreeterServer::new(MyGreeter))
            .serve(listener)
            .await
            .expect("server error");
    });

    // Give the spawned task a moment to start accepting connections.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    addr
}

/// The HTTP `GET /health` route responds with 200 + "ok".
#[tokio::test]
async fn http_route_responds() {
    let addr = spawn_server().await;

    let (status, body) = http_get(addr, "/health").await;
    assert_eq!(status, 200, "unexpected HTTP status");
    assert_eq!(body, "ok", "unexpected HTTP body");
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
    let (status, body) = http_get(addr, "/health").await;
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
