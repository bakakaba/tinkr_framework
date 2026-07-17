//! Verifies that reading the configuration before loading it panics.
//!
//! Kept in its own integration test binary because the loaded configuration
//! is process-wide.

use tinkr_config::Configurable;

/// Test configuration.
#[derive(Debug, Configurable)]
#[allow(dead_code)] // never constructed; get() panics first
struct TestConfig {
    /// Greeting text.
    #[config(default = "hello")]
    greeting: String,
}

#[test]
#[should_panic(expected = "configuration not loaded")]
fn get_before_load_panics() {
    tinkr_config::get::<TestConfig>();
}
