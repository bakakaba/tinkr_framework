//! Verifies the committed `config.toml` stays valid for the demo's
//! configuration structs.

#[test]
fn committed_config_toml_resolves() {
    // cargo runs tests with the package as working directory.
    let toml = std::fs::read_to_string("config.toml").expect("config.toml exists");
    let config =
        tinkr_framework::config::parse::<demo::config::AppConfig>("demo", "0.0.0", Some(&toml))
            .expect("committed config.toml resolves");
    assert_eq!(config.port, 8081);
    assert_eq!(config.greeting, "Hello from config.toml!");
    assert_eq!(config.cache.ttl, 600);
}
