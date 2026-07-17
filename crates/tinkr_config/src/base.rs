//! The base configuration fields present on every [`crate::Config`],
//! resolved through the same layering machinery as application fields.

use std::time::Duration;

use crate::__private::{env_value, merge_required};
use crate::errors::Error;
use crate::schema::{Node, Property};
use crate::sources::FieldSource;

/// Top-level keys claimed by the base fields.
pub(crate) const RESERVED: [&str; 5] =
    ["port", "environment", "shutdown_timeout", "name", "version"];

/// One layer's worth of base-field values.
#[derive(Default, serde::Deserialize)]
pub(crate) struct BaseLayer {
    port: Option<u16>,
    environment: Option<String>,
    shutdown_timeout: Option<u64>,
    name: Option<String>,
    version: Option<String>,
}

impl BaseLayer {
    pub(crate) fn from_env() -> Result<Self, Error> {
        Ok(Self {
            port: env_value("PORT")?,
            environment: env_value("ENVIRONMENT")?,
            shutdown_timeout: env_value("SHUTDOWN_TIMEOUT")?,
            name: env_value("SERVICE_NAME")?,
            version: env_value("SERVICE_VERSION")?,
        })
    }

    pub(crate) fn defaults(name: &str, version: &str) -> Self {
        Self {
            port: Some(8080),
            environment: Some("development".to_string()),
            shutdown_timeout: Some(30),
            name: Some(name.to_string()),
            version: Some(version.to_string()),
        }
    }
}

/// The merged base fields.
pub(crate) struct Base {
    pub port: u16,
    pub environment: String,
    pub shutdown_timeout: Duration,
    pub name: String,
    pub version: String,
}

pub(crate) fn merge(
    env: BaseLayer,
    file: BaseLayer,
    defaults: BaseLayer,
    sources: &mut Vec<FieldSource>,
) -> Result<Base, Error> {
    Ok(Base {
        port: merge_required(
            env.port,
            file.port,
            defaults.port,
            "",
            "port",
            Some("PORT"),
            false,
            sources,
        )?,
        environment: merge_required(
            env.environment,
            file.environment,
            defaults.environment,
            "",
            "environment",
            Some("ENVIRONMENT"),
            false,
            sources,
        )?,
        shutdown_timeout: Duration::from_secs(merge_required(
            env.shutdown_timeout,
            file.shutdown_timeout,
            defaults.shutdown_timeout,
            "",
            "shutdown_timeout",
            Some("SHUTDOWN_TIMEOUT"),
            false,
            sources,
        )?),
        name: merge_required(
            env.name,
            file.name,
            defaults.name,
            "",
            "name",
            Some("SERVICE_NAME"),
            false,
            sources,
        )?,
        version: merge_required(
            env.version,
            file.version,
            defaults.version,
            "",
            "version",
            Some("SERVICE_VERSION"),
            false,
            sources,
        )?,
    })
}

/// Schema properties for the base fields.
pub(crate) fn properties() -> Vec<Property> {
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
            name: "environment",
            description: Some(
                "Deployment environment name, e.g. \"development\" or \"production\".",
            ),
            required: false,
            default: Some("development".into()),
            env: Some("ENVIRONMENT"),
            node: Node::String,
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
    ]
}
