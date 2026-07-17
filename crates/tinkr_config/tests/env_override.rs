//! Verifies that environment variables outrank the file and default layers.
//!
//! Kept in its own integration test binary because it mutates the process
//! environment.

use tinkr_config::{Configurable, Source};

/// Test configuration.
#[derive(Debug, Configurable)]
struct TestConfig {
    /// Worker count.
    #[config(env = "TINKR_TEST_WORKERS", default = 4)]
    workers: usize,

    /// Nested settings.
    #[config(nested)]
    cache: Cache,
}

/// Cache settings.
#[derive(Debug, Configurable)]
struct Cache {
    /// Entry time-to-live, in seconds.
    #[config(env = "TINKR_TEST_CACHE_TTL", default = 300)]
    ttl: u64,
}

#[test]
fn env_overrides_file_and_defaults() {
    // SAFETY: this is the only test in this binary, so no other thread is
    // concurrently reading or writing the environment.
    unsafe {
        std::env::set_var("TINKR_TEST_WORKERS", "16");
        std::env::set_var("TINKR_TEST_CACHE_TTL", "60");
        std::env::set_var("PORT", "9090");
    }

    let toml = r#"
        workers = 8
        port = 7070
    "#;
    let config = tinkr_config::parse::<TestConfig>("svc", "1.0.0", Some(toml)).unwrap();

    assert_eq!(config.workers, 16); // env beats file
    assert_eq!(config.cache.ttl, 60); // env beats nested default
    assert_eq!(config.port, 9090); // env beats file for provided fields

    let by_path = |path: &str| {
        config
            .sources()
            .iter()
            .find(|s| s.path == path)
            .unwrap_or_else(|| panic!("no source entry for {path}"))
            .source
    };
    assert_eq!(by_path("workers"), Source::Env("TINKR_TEST_WORKERS"));
    assert_eq!(by_path("cache.ttl"), Source::Env("TINKR_TEST_CACHE_TTL"));
    assert_eq!(by_path("port"), Source::Env("PORT"));

    // An unparseable value reports the variable, not a panic.
    // SAFETY: see above.
    unsafe { std::env::set_var("TINKR_TEST_WORKERS", "many") };
    let err = tinkr_config::parse::<TestConfig>("svc", "1.0.0", None).unwrap_err();
    assert!(
        err.to_string().contains("$TINKR_TEST_WORKERS"),
        "unexpected message: {err}"
    );
}
