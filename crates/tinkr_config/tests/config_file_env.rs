//! Verifies that `$CONFIG_FILE` selects the configuration file: a missing or
//! invalid file at that path fails the load (with the real path in the
//! message), and a valid one is read in place of `config.toml`.
//!
//! Kept in its own integration test binary because it mutates the process
//! environment and loads the process-wide configuration.

use tinkr_config::{Configurable, Source};

/// Test configuration.
#[derive(Debug, Configurable)]
struct TestConfig {
    /// Greeting text.
    #[config(default = "hello")]
    greeting: String,
}

#[test]
fn config_file_env_var_selects_the_file() {
    let dir = std::env::temp_dir().join(format!("tinkr_config_file_env_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mounted.toml");

    // SAFETY: this is the only test in this binary, so no other thread is
    // concurrently reading or writing the environment.
    unsafe {
        std::env::set_var(tinkr_config::CONFIG_FILE_VAR, &path);
    }

    // An explicitly named file must exist: no silent fall-through to
    // defaults when a deployment's mount is misconfigured.
    let error = tinkr_config::load!(TestConfig).unwrap_err().to_string();
    assert!(
        error.contains(path.to_str().unwrap()),
        "error should name the real path: {error}"
    );

    // Parse failures also report the real path.
    std::fs::write(&path, "greeting = ").unwrap();
    let error = tinkr_config::load!(TestConfig).unwrap_err().to_string();
    assert!(
        error.contains(path.to_str().unwrap()),
        "error should name the real path: {error}"
    );

    // A valid file at the path loads in place of config.toml.
    std::fs::write(&path, "greeting = \"from mounted file\"\n").unwrap();
    let config = tinkr_config::load!(TestConfig).unwrap();
    assert_eq!(config.greeting, "from mounted file");

    // Provenance reports the resolved path, not the default name.
    let greeting = config
        .sources()
        .iter()
        .find(|s| s.path == "greeting")
        .unwrap();
    assert_eq!(greeting.source, Source::File);
    assert!(
        greeting.source.to_string().contains(path.to_str().unwrap()),
        "provenance should name the real path: {}",
        greeting.source
    );

    std::fs::remove_dir_all(&dir).ok();
}
