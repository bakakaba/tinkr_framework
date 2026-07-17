//! Error type for configuration loading.

/// Errors that can occur while resolving a configuration.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// No layer provided a value for a required field (one without a
    /// `#[config(default = ...)]` that is not `Option`).
    #[error("missing required config value `{path}`: {}", missing_hint(.env))]
    MissingValue {
        /// Dotted field path, e.g. `cache.ttl`.
        path: String,
        /// The field's environment variable, when it declares one.
        env: Option<&'static str>,
    },

    /// An environment variable was set but its value failed to parse into
    /// the field's type.
    #[error("invalid value in ${var}: {message}")]
    InvalidEnv {
        /// The environment variable name.
        var: &'static str,
        /// Why the value was rejected.
        message: String,
    },

    /// The configuration file is not valid TOML, or a value in it has the
    /// wrong type.
    #[error("invalid config.toml: {0}")]
    File(#[from] toml::de::Error),

    /// The configuration file exists but could not be read.
    #[error("failed to read config.toml: {0}")]
    Io(#[from] std::io::Error),

    /// An application field uses a name reserved by the base
    /// fields (`port`, `environment`, `shutdown_timeout`, `name`,
    /// `version`).
    #[error("config field `{name}` collides with a base field; rename it")]
    ReservedField {
        /// The colliding field name.
        name: &'static str,
    },
}

fn missing_hint(env: &Option<&'static str>) -> String {
    match env {
        Some(var) => format!("set ${var} or add it to {}", crate::CONFIG_FILE),
        None => format!("add it to {}", crate::CONFIG_FILE),
    }
}
