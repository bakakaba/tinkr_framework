//! Verifies that building a `Server` before `init!` panics.
//!
//! Kept in its own integration test binary because the loaded configuration
//! is process-wide: no other test here may load it.

#[test]
#[should_panic(expected = "configuration not loaded")]
fn server_new_without_init_panics() {
    let _ = tinkr_framework::Server::new();
}
