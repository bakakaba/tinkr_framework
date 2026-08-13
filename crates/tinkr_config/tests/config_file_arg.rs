//! Verifies that `load!(T, config_file = ...)` takes full control of the
//! file location: exactly that file is loaded and `$CONFIG_FILE` is ignored,
//! while environment-variable value overrides still apply.
//!
//! Kept in its own integration test binary because it mutates the process
//! environment and loads the process-wide configuration.

use tinkr_config::{Configurable, Source};

/// Test configuration.
#[derive(Debug, Configurable)]
struct TestConfig {
    /// Greeting text.
    #[config(env = "TINKR_TEST_GREETING", default = "hello")]
    greeting: String,

    /// Worker count.
    #[config(default = 4)]
    workers: usize,
}

#[test]
fn config_file_argument_overrides_the_env_var() {
    let dir = std::env::temp_dir().join(format!("tinkr_config_file_arg_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let arg_path = dir.join("arg.toml");
    let env_path = dir.join("env.toml");
    std::fs::write(&arg_path, "greeting = \"from arg\"\nworkers = 8\n").unwrap();
    std::fs::write(&env_path, "greeting = \"from env var\"\n").unwrap();

    // SAFETY: this is the only test in this binary, so no other thread is
    // concurrently reading or writing the environment.
    unsafe {
        std::env::set_var(tinkr_config::CONFIG_FILE_VAR, &env_path);
        std::env::set_var("TINKR_TEST_GREETING", "from process env");
    }

    // A runtime Option works too: Some(path) opts out of $CONFIG_FILE only.
    let config_file = Some(&arg_path);
    let config = tinkr_config::load!(TestConfig, config_file = config_file).unwrap();

    // The argument's file was read, not $CONFIG_FILE's.
    assert_eq!(config.workers, 8);
    let workers = config
        .sources()
        .iter()
        .find(|s| s.path == "workers")
        .unwrap();
    assert_eq!(workers.source, Source::File);
    assert!(
        workers
            .source
            .to_string()
            .contains(arg_path.to_str().unwrap()),
        "provenance should name the argument's path: {}",
        workers.source
    );

    // Value overrides via other environment variables still apply.
    assert_eq!(config.greeting, "from process env");

    std::fs::remove_dir_all(&dir).ok();
}
