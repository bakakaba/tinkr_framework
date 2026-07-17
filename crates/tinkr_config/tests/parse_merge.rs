//! Layer precedence, provenance, and error cases via `parse`, without
//! touching environment variables or the file system.

use tinkr_config::{Configurable, Source};

/// Test configuration.
#[derive(Debug, Configurable)]
struct TestConfig {
    /// Greeting text.
    #[config(default = "hello")]
    greeting: String,

    /// Required connection URL.
    #[config(secret)]
    url: String,

    /// Optional verbosity flag.
    verbose: Option<bool>,

    #[config(nested)]
    cache: Cache,
}

/// Cache settings.
#[derive(Debug, Configurable)]
struct Cache {
    /// Entry time-to-live, in seconds.
    #[config(default = 300)]
    ttl: u64,

    /// Maximum entries.
    #[config(default = 1024)]
    capacity: u64,
}

#[test]
fn file_overrides_defaults() {
    let toml = r#"
        url = "postgres://localhost/db"
        greeting = "hi"

        [cache]
        ttl = 60
    "#;
    let config = tinkr_config::parse::<TestConfig>("svc", "1.2.3", Some(toml)).unwrap();

    assert_eq!(config.greeting, "hi"); // file beats default
    assert_eq!(config.url, "postgres://localhost/db"); // file provides required
    assert_eq!(config.verbose, None); // option stays unset
    assert_eq!(config.cache.ttl, 60); // nested file value
    assert_eq!(config.cache.capacity, 1024); // nested default
}

#[test]
fn provided_fields_resolve() {
    let toml = r#"
        url = "u"
        port = 9999
        environment = "production"
        shutdown_timeout = 5
    "#;
    let config = tinkr_config::parse::<TestConfig>("svc", "1.2.3", Some(toml)).unwrap();

    assert_eq!(config.port, 9999);
    assert_eq!(config.environment, "production");
    assert_eq!(config.shutdown_timeout, std::time::Duration::from_secs(5));
    assert_eq!(config.name, "svc"); // default from load!/parse arguments
    assert_eq!(config.version, "1.2.3");

    let config = tinkr_config::parse::<TestConfig>("svc", "1.2.3", Some("url = \"u\"")).unwrap();
    assert_eq!(config.port, 8080);
    assert_eq!(config.environment, "development");
    assert_eq!(config.shutdown_timeout, std::time::Duration::from_secs(30));
}

#[test]
fn missing_required_value_errors() {
    let err = tinkr_config::parse::<TestConfig>("svc", "1.2.3", None).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("`url`"), "unexpected message: {message}");
    assert!(
        message.contains("config.toml"),
        "unexpected message: {message}"
    );
}

#[test]
fn nested_missing_value_reports_dotted_path() {
    /// Wrapper.
    #[derive(Debug, Configurable)]
    #[allow(dead_code)] // only the error path is exercised
    struct Outer {
        #[config(nested)]
        inner: Inner,
    }
    /// Inner.
    #[derive(Debug, Configurable)]
    #[allow(dead_code)] // only the error path is exercised
    struct Inner {
        /// Required.
        key: String,
    }

    let err = tinkr_config::parse::<Outer>("svc", "1.2.3", None).unwrap_err();
    assert!(
        err.to_string().contains("`inner.key`"),
        "unexpected message: {err}"
    );
}

#[test]
fn provenance_tracks_winning_layer() {
    let toml = r#"
        url = "postgres://user:hunter2@localhost/db"

        [cache]
        ttl = 60
    "#;
    let config = tinkr_config::parse::<TestConfig>("svc", "1.2.3", Some(toml)).unwrap();
    let by_path = |path: &str| {
        config
            .sources()
            .iter()
            .find(|s| s.path == path)
            .unwrap_or_else(|| panic!("no source entry for {path}"))
    };

    assert_eq!(by_path("greeting").source, Source::Default);
    assert_eq!(by_path("url").source, Source::File);
    assert_eq!(by_path("verbose").source, Source::Unset);
    assert_eq!(by_path("cache.ttl").source, Source::File);
    assert_eq!(by_path("cache.capacity").source, Source::Default);
    assert_eq!(by_path("port").source, Source::Default);

    // Secrets never leak into the readout.
    assert_eq!(by_path("url").value, "<redacted>");
    let readout = config.sources().to_string();
    assert!(!readout.contains("hunter2"), "secret leaked:\n{readout}");
    assert!(readout.contains("cache.ttl"), "readout:\n{readout}");
}

#[test]
fn reserved_field_rejected() {
    /// Colliding configuration.
    #[derive(Debug, Configurable)]
    #[allow(dead_code)] // only the error path is exercised
    struct Colliding {
        /// Uses a provided field's name.
        #[config(default = 1)]
        port: u16,
    }

    let err = tinkr_config::parse::<Colliding>("svc", "1.2.3", None).unwrap_err();
    assert!(matches!(
        err,
        tinkr_config::Error::ReservedField { name: "port" }
    ));
}

#[test]
fn invalid_toml_errors() {
    let err = tinkr_config::parse::<TestConfig>("svc", "1.2.3", Some("url = [")).unwrap_err();
    assert!(matches!(err, tinkr_config::Error::File(_)));
}
