//! Verifies `init!(..., config_file = ...)` end to end: the named file is
//! loaded regardless of the working directory, so one build can read
//! per-environment configuration from wherever the deployment mounts it.
//!
//! Kept in its own integration test binary because the loaded configuration
//! and the global tracing subscriber affect the whole process.

#[test]
fn init_loads_the_named_config_file() {
    let dir = std::env::temp_dir().join(format!(
        "tinkr_framework_config_file_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mounted.toml");
    std::fs::write(&path, "port = 7171\n").unwrap();

    tinkr_framework::init!(config_file = &path).unwrap();
    assert_eq!(tinkr_framework::config::base().port, 7171);

    std::fs::remove_dir_all(&dir).ok();
}
