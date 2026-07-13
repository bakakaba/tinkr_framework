//! Verifies that calling `bootstrap::init` twice panics.
//!
//! Kept in its own integration test binary because initializing the global
//! tracing subscriber affects the whole process.

#[test]
#[should_panic]
fn double_init_panics() {
    tinkr_framework::init();
    tinkr_framework::init();
}
