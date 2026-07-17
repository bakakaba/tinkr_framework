//! Verifies the generated JSON Schema and starter config template.

use tinkr_config::Configurable;

/// Settings for the schema test service.
///
/// Second line of the description.
#[derive(Debug, Configurable)]
#[allow(dead_code)] // only the schema output is exercised
struct TestConfig {
    /// Connection URL.
    #[config(env = "TINKR_SCHEMA_URL", secret)]
    url: String,

    /// Worker count.
    #[config(default = 4)]
    workers: usize,

    /// Optional label.
    label: Option<String>,

    /// Feature flags.
    #[config(default = Vec::new())]
    flags: Vec<String>,

    /// Nested settings.
    #[config(nested)]
    cache: Cache,
}

/// Cache settings.
#[derive(Debug, Configurable)]
#[allow(dead_code)] // only the schema output is exercised
struct Cache {
    /// Entry time-to-live, in seconds.
    #[config(default = 300)]
    ttl: u64,
}

#[test]
fn schema_describes_provided_and_app_fields() {
    let schema: serde_json::Value =
        serde_json::from_str(&tinkr_config::schema::<TestConfig>()).unwrap();

    assert_eq!(schema["$schema"], "http://json-schema.org/draft-07/schema#");
    assert_eq!(schema["title"], "TestConfig");
    assert_eq!(
        schema["description"],
        "Settings for the schema test service.\n\nSecond line of the description."
    );
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["additionalProperties"], false);

    let props = schema["properties"].as_object().unwrap();

    // Provided fields are present.
    assert_eq!(props["port"]["type"], "integer");
    assert_eq!(props["port"]["default"], 8080);
    assert_eq!(
        props["port"]["description"],
        "TCP port the server listens on. (env: PORT)"
    );

    // Application fields, with descriptions, env notes, and defaults.
    assert_eq!(props["url"]["type"], "string");
    assert_eq!(
        props["url"]["description"],
        "Connection URL. (env: TINKR_SCHEMA_URL)"
    );
    assert_eq!(props["workers"]["default"], 4);
    assert_eq!(props["label"]["type"], "string");
    assert_eq!(props["flags"]["type"], "array");
    assert_eq!(props["flags"]["items"]["type"], "string");

    // Nested tables render as full object schemas.
    assert_eq!(props["cache"]["type"], "object");
    assert_eq!(props["cache"]["additionalProperties"], false);
    assert_eq!(props["cache"]["properties"]["ttl"]["default"], 300);

    // `url` has an env override, so the file alone doesn't have to provide
    // it; nothing else is required.
    assert_eq!(schema["required"], serde_json::json!(null));
}

#[test]
fn required_lists_file_only_fields() {
    /// Requires a value with no env override.
    #[derive(Debug, Configurable)]
    #[allow(dead_code)] // only the schema output is exercised
    struct Strict {
        /// Must come from the file.
        key: String,
    }

    let schema: serde_json::Value =
        serde_json::from_str(&tinkr_config::schema::<Strict>()).unwrap();
    assert_eq!(schema["required"], serde_json::json!(["key"]));
}

#[test]
fn template_starts_with_schema_directive() {
    let template = tinkr_config::template::<TestConfig>();

    assert!(
        template.starts_with("#:schema ./config.schema.json\n"),
        "template:\n{template}"
    );
    // Struct description is included.
    assert!(template.contains("# Settings for the schema test service."));
    // Defaulted fields are commented out with their default.
    assert!(template.contains("#workers = 4"), "template:\n{template}");
    assert!(template.contains("#port = 8080"), "template:\n{template}");
    // Nested tables come after scalars.
    assert!(template.contains("[cache]"), "template:\n{template}");
    assert!(template.contains("#ttl = 300"), "template:\n{template}");
    // The template itself is valid TOML.
    toml::from_str::<toml::Table>(&template).expect("template parses as TOML");
}
