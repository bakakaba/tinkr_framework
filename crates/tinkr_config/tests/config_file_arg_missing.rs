//! Verifies `config_file = ...` edge cases: a missing explicit file fails
//! the load, and passing `None` preserves the default resolution (where a
//! missing `config.toml` falls through to env vars and defaults).
//!
//! Kept in its own integration test binary because it changes the working
//! directory and loads the process-wide configuration.

use std::path::PathBuf;

use tinkr_config::{Configurable, Source};

/// Test configuration.
#[derive(Debug, Configurable)]
struct TestConfig {
    /// Greeting text.
    #[config(default = "hello")]
    greeting: String,
}

#[test]
fn missing_explicit_file_fails_and_none_falls_through() {
    // Run from an empty scratch directory so no config.toml is present.
    let dir = std::env::temp_dir().join(format!(
        "tinkr_config_file_arg_missing_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_current_dir(&dir).unwrap();

    let missing = dir.join("nope.toml");
    let error = tinkr_config::load!(TestConfig, config_file = &missing)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains(missing.to_str().unwrap()),
        "error should name the real path: {error}"
    );

    // None opts back into the default resolution: the absent config.toml
    // falls through to env vars and defaults.
    let config = tinkr_config::load!(TestConfig, config_file = None::<PathBuf>).unwrap();
    assert_eq!(config.greeting, "hello");
    let greeting = config
        .sources()
        .iter()
        .find(|s| s.path == "greeting")
        .unwrap();
    assert_eq!(greeting.source, Source::Default);

    std::fs::remove_dir_all(&dir).ok();
}
