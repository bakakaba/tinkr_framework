//! Verifies that loading the configuration twice panics.
//!
//! Kept in its own integration test binary because the loaded configuration
//! is process-wide.

use tinkr_config::Configurable;

/// Test configuration.
#[derive(Debug, Configurable)]
#[allow(dead_code)] // only the double-load panic is exercised
struct TestConfig {
    /// Greeting text.
    #[config(default = "hello")]
    greeting: String,
}

#[test]
#[should_panic(expected = "configuration already loaded")]
fn double_load_panics() {
    tinkr_config::load!(TestConfig).unwrap();
    tinkr_config::load!(TestConfig).unwrap();
}
