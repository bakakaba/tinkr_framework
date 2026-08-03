//! With the `otel` feature compiled but no OTLP endpoint configured,
//! telemetry stays inactive: `init!` needs no collector and `/health`
//! reports every signal as inactive.
//!
//! Kept in its own integration test binary because it initializes the
//! global subscriber and configuration, and depends on the process
//! environment.
#![cfg(feature = "otel")]

use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn otel_is_inactive_without_endpoint() {
    // SAFETY: this is the only test in this binary, so no other thread is
    // concurrently reading or writing the environment.
    unsafe {
        std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        std::env::remove_var("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT");
        std::env::remove_var("OTEL_EXPORTER_OTLP_METRICS_ENDPOINT");
        std::env::remove_var("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT");
    }

    tinkr_framework::init!().expect("init without an OTLP endpoint");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        tinkr_framework::Server::new()
            .route("/hello", tinkr_framework::routing::get(|| async { "hi" }))
            .bind(listener)
            .serve()
            .await
            .unwrap();
    });

    let response = get(addr, "/health").await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(
        response.contains("\"otel\":{\"traces\":false,\"metrics\":false,\"logs\":false}"),
        "{response}"
    );
}

async fn get(addr: std::net::SocketAddr, path: &str) -> String {
    let mut stream = connect(addr).await;
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response
}

async fn connect(addr: std::net::SocketAddr) -> tokio::net::TcpStream {
    for _ in 0..50 {
        if let Ok(stream) = tokio::net::TcpStream::connect(addr).await {
            return stream;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("server did not start listening");
}
