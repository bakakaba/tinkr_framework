//! A consumer-prescribed [`tinkr_config::Environment`] type, selected with
//! the struct-level `#[config(env = ...)]` attribute.

use tinkr_config::{Configurable, Environment};

/// Deployment environments including staging.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Environment)]
enum MyEnv {
    #[default]
    Local,
    Development,
    Staging,
    PreProd,
    Production,
}

/// Test configuration.
#[derive(Debug, Configurable)]
#[config(env = MyEnv)]
struct TestConfig {
    /// Greeting text.
    #[config(default = "hello")]
    greeting: String,
}

#[test]
fn custom_env_parses_and_defaults() {
    let config =
        tinkr_config::parse::<TestConfig>("svc", "1.0.0", Some("env = \"staging\"")).unwrap();
    assert_eq!(config.env, MyEnv::Staging);
    assert_eq!(config.greeting, "hello");

    // Unset everywhere: the enum's `Default` variant.
    let config = tinkr_config::parse::<TestConfig>("svc", "1.0.0", None).unwrap();
    assert_eq!(config.env, MyEnv::Local);
}

#[test]
fn custom_env_rejects_unknown_names() {
    let err = tinkr_config::parse::<TestConfig>("svc", "1.0.0", Some("env = \"qa\"")).unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("`qa`") && message.contains("`staging`"),
        "unexpected message: {message}"
    );
}

#[test]
fn derive_maps_variant_names_to_snake_case() {
    assert_eq!(
        MyEnv::variants(),
        ["local", "development", "staging", "pre_prod", "production"]
    );
    assert_eq!("pre_prod".parse::<MyEnv>().unwrap(), MyEnv::PreProd);
    assert_eq!("Staging".parse::<MyEnv>().unwrap(), MyEnv::Staging); // case-insensitive
    assert_eq!(MyEnv::PreProd.to_string(), "pre_prod");
}

#[test]
fn schema_lists_custom_variants() {
    let schema = tinkr_config::schema::<TestConfig>();
    let json: serde_json::Value = serde_json::from_str(&schema).unwrap();
    assert_eq!(
        json["properties"]["env"]["enum"],
        serde_json::json!(["local", "development", "staging", "pre_prod", "production"]),
    );
    assert_eq!(json["properties"]["env"]["default"], "local");
}
