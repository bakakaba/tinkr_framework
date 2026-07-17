//! Verifies that an invalid `RUST_LOG` value makes `bootstrap::init` panic.
//!
//! Kept in its own integration test binary because it mutates the process
//! environment and initializing the global tracing subscriber affects the
//! whole process.

#[test]
#[should_panic(expected = "invalid RUST_LOG")]
fn invalid_rust_log_panics() {
    // SAFETY: this is the only test in this binary, so no other thread is
    // concurrently reading or writing the environment.
    unsafe { std::env::set_var("RUST_LOG", "foo=not_a_level") };
    tinkr_framework::init();
}
