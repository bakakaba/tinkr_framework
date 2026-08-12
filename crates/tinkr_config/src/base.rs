//! The base configuration fields present on every [`crate::Config`],
//! resolved through the same layering machinery as application fields.

use std::time::Duration;

use crate::__private::{FieldMeta, env_value, merge_optional, merge_required};
use crate::env::Environment;
use crate::errors::Error;
use crate::schema::{Node, Property};
use crate::sources::{FieldSource, Source};

/// Top-level keys claimed by the base fields.
pub(crate) const RESERVED: [&str; 7] = [
    "port",
    "env",
    "shutdown_timeout",
    "name",
    "version",
    "otel_endpoint",
    "otel_headers",
];

/// One layer's worth of base-field values.
#[derive(Default, serde::Deserialize)]
pub(crate) struct BaseLayer {
    port: Option<u16>,
    env: Option<String>,
    shutdown_timeout: Option<u64>,
    name: Option<String>,
    version: Option<String>,
    otel_endpoint: Option<String>,
    otel_headers: Option<String>,
}

impl BaseLayer {
    pub(crate) fn from_env() -> Result<Self, Error> {
        Ok(Self {
            port: env_value("PORT")?,
            env: env_value("ENV")?,
            shutdown_timeout: env_value("SHUTDOWN_TIMEOUT")?,
            name: env_value("SERVICE_NAME")?,
            version: env_value("SERVICE_VERSION")?,
            otel_endpoint: env_value("OTEL_EXPORTER_OTLP_ENDPOINT")?,
            otel_headers: env_value("OTEL_EXPORTER_OTLP_HEADERS")?,
        })
    }

    pub(crate) fn defaults(name: &str, version: &str) -> Self {
        Self {
            port: Some(8080),
            // `env` has no string default: the environment type's `Default`
            // value applies when no layer provides one (see `merge_env`).
            env: None,
            shutdown_timeout: Some(30),
            name: Some(name.to_string()),
            version: Some(version.to_string()),
            otel_endpoint: None,
            otel_headers: None,
        }
    }
}

/// The merged base fields.
pub(crate) struct Base<E> {
    pub port: u16,
    pub env: E,
    /// The resolved environment name, kept for the type-agnostic
    /// [`crate::BaseConfig`] view.
    pub env_name: String,
    pub shutdown_timeout: Duration,
    pub name: String,
    pub version: String,
    pub otel_endpoint: Option<String>,
    pub otel_headers: Option<String>,
}

pub(crate) fn merge<E: Environment>(
    env: BaseLayer,
    file: BaseLayer,
    defaults: BaseLayer,
    sources: &mut Vec<FieldSource>,
) -> Result<Base<E>, Error> {
    let (environment, env_name) = merge_env::<E>(env.env, file.env, sources)?;
    Ok(Base {
        port: merge_required(
            env.port,
            file.port,
            defaults.port,
            base_meta("port", "PORT", false),
            sources,
        )?,
        env: environment,
        env_name,
        shutdown_timeout: Duration::from_secs(merge_required(
            env.shutdown_timeout,
            file.shutdown_timeout,
            defaults.shutdown_timeout,
            base_meta("shutdown_timeout", "SHUTDOWN_TIMEOUT", false),
            sources,
        )?),
        name: merge_required(
            env.name,
            file.name,
            defaults.name,
            base_meta("name", "SERVICE_NAME", false),
            sources,
        )?,
        version: merge_required(
            env.version,
            file.version,
            defaults.version,
            base_meta("version", "SERVICE_VERSION", false),
            sources,
        )?,
        otel_endpoint: merge_optional(
            env.otel_endpoint,
            file.otel_endpoint,
            defaults.otel_endpoint,
            base_meta("otel_endpoint", "OTEL_EXPORTER_OTLP_ENDPOINT", false),
            sources,
        ),
        otel_headers: merge_optional(
            env.otel_headers,
            file.otel_headers,
            defaults.otel_headers,
            base_meta("otel_headers", "OTEL_EXPORTER_OTLP_HEADERS", true),
            sources,
        ),
    })
}

/// Builds the [`FieldMeta`] for a top-level provided field.
fn base_meta(name: &'static str, env_var: &'static str, secret: bool) -> FieldMeta<'static> {
    FieldMeta {
        prefix: "",
        name,
        env_var: Some(env_var),
        secret,
    }
}

/// Resolves the `env` field: `ENV` over the file over the environment
/// type's `Default`, parsing the name into `E` and recording provenance.
fn merge_env<E: Environment>(
    env: Option<String>,
    file: Option<String>,
    sources: &mut Vec<FieldSource>,
) -> Result<(E, String), Error> {
    let (value, name, source) = match (env, file) {
        (Some(raw), _) => {
            let value = raw.parse::<E>().map_err(|e| Error::InvalidEnv {
                var: "ENV",
                message: e.to_string(),
            })?;
            (value, raw, Source::Env("ENV"))
        }
        (None, Some(raw)) => {
            let value = raw.parse::<E>()?;
            (value, raw, Source::File)
        }
        (None, None) => {
            let value = E::default();
            let name = value.to_string();
            (value, name, Source::Default)
        }
    };
    sources.push(FieldSource {
        path: "env".to_string(),
        value: format!("{value:?}"),
        source,
    });
    Ok((value, name))
}

/// Schema properties for the base fields.
pub(crate) fn properties<E: Environment>() -> Vec<Property> {
    vec![
        Property {
            name: "port",
            description: Some("TCP port the server listens on."),
            required: false,
            default: Some(8080.into()),
            env: Some("PORT"),
            node: Node::Integer,
        },
        Property {
            name: "env",
            description: Some("Deployment environment."),
            required: false,
            default: Some(E::default().to_string().into()),
            env: Some("ENV"),
            node: Node::Enum(E::variants()),
        },
        Property {
            name: "shutdown_timeout",
            description: Some("Graceful shutdown grace period, in seconds."),
            required: false,
            default: Some(30.into()),
            env: Some("SHUTDOWN_TIMEOUT"),
            node: Node::Integer,
        },
        Property {
            name: "name",
            description: Some("Service name. Defaults to the Cargo package name."),
            required: false,
            default: None,
            env: Some("SERVICE_NAME"),
            node: Node::String,
        },
        Property {
            name: "version",
            description: Some("Service version. Defaults to the Cargo package version."),
            required: false,
            default: None,
            env: Some("SERVICE_VERSION"),
            node: Node::String,
        },
        Property {
            name: "otel_endpoint",
            description: Some(
                "OTLP gRPC endpoint telemetry is exported to, e.g. \
                 http://localhost:4317 (an OpenTelemetry Collector). \
                 Telemetry export is disabled when unset.",
            ),
            required: false,
            default: None,
            env: Some("OTEL_EXPORTER_OTLP_ENDPOINT"),
            node: Node::String,
        },
        Property {
            name: "otel_headers",
            description: Some(
                "Headers attached to every OTLP export request as \
                 key=value pairs separated by commas, e.g. \
                 x-api-key=secret (typically authentication for a \
                 hosted collector). Prefer the environment variable \
                 for secret values.",
            ),
            required: false,
            default: None,
            env: Some("OTEL_EXPORTER_OTLP_HEADERS"),
            node: Node::String,
        },
    ]
}
