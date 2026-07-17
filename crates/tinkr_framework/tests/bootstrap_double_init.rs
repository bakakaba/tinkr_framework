//! Verifies that calling `init!` twice panics.
//!
//! Kept in its own integration test binary because the loaded configuration
//! and the global tracing subscriber affect the whole process.

#[test]
#[should_panic(expected = "configuration already loaded")]
fn double_init_panics() {
    let _ = tinkr_framework::init!();
    let _ = tinkr_framework::init!();
}
