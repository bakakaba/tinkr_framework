//! Verifies the full `load!`/`get` flow: reading `config.toml` from the
//! working directory, storing the frozen global, and reading it back.
//!
//! Kept in its own integration test binary because the loaded configuration
//! and the working directory are process-wide.

use tinkr_config::{Configurable, Source};

/// Test configuration.
#[derive(Debug, Configurable)]
struct TestConfig {
    /// Greeting text.
    #[config(default = "hello")]
    greeting: String,
}

#[test]
fn load_reads_file_and_freezes_globally() {
    // Run from a scratch directory so the test controls config.toml.
    let dir = std::env::temp_dir().join(format!("tinkr_config_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(tinkr_config::CONFIG_FILE),
        "greeting = \"from file\"\nport = 9999\n",
    )
    .unwrap();
    std::env::set_current_dir(&dir).unwrap();

    let config = tinkr_config::load!(TestConfig).unwrap();
    assert_eq!(config.greeting, "from file");
    assert_eq!(config.port, 9999);
    // name/version default to this crate's Cargo package.
    assert_eq!(config.name, "tinkr_config");
    assert_eq!(config.version, env!("CARGO_PKG_VERSION"));

    // The same configuration is reachable from anywhere via get().
    let config = tinkr_config::get::<TestConfig>();
    assert_eq!(config.greeting, "from file");
    assert_eq!(
        config
            .sources()
            .iter()
            .find(|s| s.path == "greeting")
            .unwrap()
            .source,
        Source::File
    );

    std::fs::remove_dir_all(&dir).ok();
}
