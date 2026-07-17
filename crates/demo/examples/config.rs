//! Configuration demo: layered loading, provenance readout, and global
//! access without passing the config around.
//!
//! Run from `crates/demo` so `config.toml` is found in the working
//! directory:
//!
//! ```sh
//! cargo run --example config
//! curl localhost:8081/greeting
//!
//! # Environment variables outrank the file:
//! GREETING="Hi from the environment" PORT=3000 cargo run --example config
//! ```

use demo::config::AppConfig;
use tinkr_framework::{Server, config, routing::get};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Resolve every field (env > config.toml > defaults), freeze the result,
    // and set up logging. Secrets are redacted in the readout.
    let cfg = tinkr_framework::init!(AppConfig)?;
    tracing::info!("resolved configuration:\n{}", cfg.sources());

    // The server takes its identity, port, and shutdown grace period from
    // the loaded configuration; application fields deref directly.
    Server::new()
        .route("/greeting", get(greeting))
        .serve()
        .await?;
    Ok(())
}

/// Handlers read the loaded configuration from anywhere with `config::get`.
async fn greeting() -> String {
    let cfg = config::get::<AppConfig>();
    format!(
        "{} (workers: {}, cache ttl: {}s)\n",
        cfg.greeting, cfg.workers, cfg.cache.ttl
    )
}
