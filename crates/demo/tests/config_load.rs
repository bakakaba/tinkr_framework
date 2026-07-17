//! Loads the demo configuration end-to-end: an env override on top of the
//! committed `config.toml` on top of declared defaults, then global access
//! through `config::get`.
//!
//! Kept in its own integration test binary because it mutates the process
//! environment and the loaded configuration is process-wide.

use demo::config::AppConfig;
use tinkr_framework::config::{self, Source};

#[test]
fn loads_layers_and_freezes_globally() {
    // SAFETY: this is the only test in this binary, so no other thread is
    // concurrently reading or writing the environment.
    unsafe { std::env::set_var("WORKERS", "16") };

    // cargo runs tests with the package as working directory, so this reads
    // crates/demo/config.toml.
    let cfg = config::load!(AppConfig).unwrap();

    assert_eq!(cfg.workers, 16); // environment beats the file's default
    assert_eq!(cfg.port, 8081); // config.toml
    assert_eq!(cfg.greeting, "Hello from config.toml!"); // config.toml
    assert_eq!(cfg.cache.ttl, 600); // config.toml, nested table
    assert_eq!(cfg.cache.capacity, 1024); // #[config(default = ...)]
    assert_eq!(cfg.database_url, None); // unset Option
    assert_eq!(cfg.name, "demo"); // Cargo package name via load!
    assert_eq!(cfg.version, env!("CARGO_PKG_VERSION"));

    // The frozen configuration is reachable from anywhere in the process.
    let cfg = config::get::<AppConfig>();
    let source = |path: &str| {
        cfg.sources()
            .iter()
            .find(|s| s.path == path)
            .unwrap_or_else(|| panic!("no source entry for {path}"))
            .source
    };
    assert_eq!(source("workers"), Source::Env("WORKERS"));
    assert_eq!(source("greeting"), Source::File);
    assert_eq!(source("cache.capacity"), Source::Default);
    assert_eq!(source("database_url"), Source::Unset);
}
