//! Runtime support for `#[derive(Configurable)]`-generated code. Not part of
//! the public API; may change without notice.

use std::fmt::{Debug, Display};
use std::str::FromStr;

pub use serde;
pub use serde_json;

use crate::errors::Error;
use crate::sources::{FieldSource, Source};

/// Reads and parses an environment variable, `Ok(None)` when unset.
pub fn env_value<T>(var: &'static str) -> Result<Option<T>, Error>
where
    T: FromStr,
    T::Err: Display,
{
    match std::env::var(var) {
        Ok(raw) => raw
            .parse()
            .map(Some)
            .map_err(|e: T::Err| Error::InvalidEnv {
                var,
                message: e.to_string(),
            }),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(e @ std::env::VarError::NotUnicode(_)) => Err(Error::InvalidEnv {
            var,
            message: e.to_string(),
        }),
    }
}

/// Identity and provenance metadata for one field being merged.
pub struct FieldMeta<'a> {
    /// Dotted path prefix of the containing table (empty at the top level).
    pub prefix: &'a str,
    /// The field's name within its table.
    pub name: &'a str,
    /// The environment variable that can override the field, if any.
    pub env_var: Option<&'static str>,
    /// Whether the value must be redacted in provenance output.
    pub secret: bool,
}

/// Picks the highest-precedence value for a required field, recording its
/// provenance.
pub fn merge_required<T: Debug>(
    env: Option<T>,
    file: Option<T>,
    defaults: Option<T>,
    meta: FieldMeta<'_>,
    sources: &mut Vec<FieldSource>,
) -> Result<T, Error> {
    let path = format!("{}{}", meta.prefix, meta.name);
    let (value, source) = if let Some(value) = env {
        let var = meta
            .env_var
            .expect("environment layer produced a value for a field without `env`");
        (value, Source::Env(var))
    } else if let Some(value) = file {
        (value, Source::File)
    } else if let Some(value) = defaults {
        (value, Source::Default)
    } else {
        return Err(Error::MissingValue {
            path,
            env: meta.env_var,
        });
    };
    sources.push(FieldSource {
        path,
        value: display_value(&value, meta.secret),
        source,
    });
    Ok(value)
}

/// Picks the highest-precedence value for an `Option` field, recording its
/// provenance ([`Source::Unset`] when no layer provides one).
pub fn merge_optional<T: Debug>(
    env: Option<T>,
    file: Option<T>,
    defaults: Option<T>,
    meta: FieldMeta<'_>,
    sources: &mut Vec<FieldSource>,
) -> Option<T> {
    let path = format!("{}{}", meta.prefix, meta.name);
    let (value, source) = if let Some(value) = env {
        let var = meta
            .env_var
            .expect("environment layer produced a value for a field without `env`");
        (Some(value), Source::Env(var))
    } else if let Some(value) = file {
        (Some(value), Source::File)
    } else if let Some(value) = defaults {
        (Some(value), Source::Default)
    } else {
        (None, Source::Unset)
    };
    sources.push(FieldSource {
        path,
        value: value
            .as_ref()
            .map(|v| display_value(v, meta.secret))
            .unwrap_or_else(|| "(unset)".to_string()),
        source,
    });
    value
}

fn display_value<T: Debug>(value: &T, secret: bool) -> String {
    if secret {
        "<redacted>".to_string()
    } else {
        format!("{value:?}")
    }
}

/// Extends a dotted path prefix with a nested table's name.
pub fn child_prefix(prefix: &str, name: &str) -> String {
    format!("{prefix}{name}.")
}

/// Serializes a default value for inclusion in the schema.
pub fn default_json<T: serde::Serialize>(value: &T) -> Option<serde_json::Value> {
    serde_json::to_value(value).ok()
}
