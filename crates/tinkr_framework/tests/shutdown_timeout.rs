//! Verifies that the configured shutdown grace period cuts off a hung
//! shutdown hook.
//!
//! Kept in its own integration test binary because it mutates the process
//! environment, loads the global configuration, and raises `SIGTERM` for
//! the whole process.

#![cfg(unix)]

use std::time::{Duration, Instant};

use tinkr_framework::{Server, routing::get};

#[tokio::test]
async fn hung_shutdown_hook_is_cut_off() {
    // SAFETY: this is the only test in this binary, so no other thread is
    // concurrently reading or writing the environment.
    unsafe { std::env::set_var("SHUTDOWN_TIMEOUT", "1") };
    tinkr_framework::config::load!(()).unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server = Server::new()
        .route("/", get(|| async { "ok" }))
        .on_shutdown(std::future::pending()) // never completes
        .bind(listener)
        .serve();
    let handle = tokio::spawn(server);

    // Give the server time to install its signal handler before signalling.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let signalled = Instant::now();
    // SAFETY: raising a handled signal for the current process is safe; the
    // server's SIGTERM handler consumes it.
    unsafe { libc::raise(libc::SIGTERM) };

    handle.await.unwrap().unwrap();
    let elapsed = signalled.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "serve did not respect the shutdown timeout: {elapsed:?}"
    );
}
