//! Per-field provenance: which layer supplied each configuration value.

use std::fmt;

/// The layer a configuration value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Source {
    /// The `#[config(default = ...)]` value (or a built-in default for the
    /// base fields).
    Default,
    /// The configuration file.
    File,
    /// The named environment variable.
    Env(&'static str),
    /// No layer provided a value (the field is `Option` and unset).
    Unset,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::Default => write!(f, "default"),
            Source::File => write!(f, "{}", crate::CONFIG_FILE),
            Source::Env(var) => write!(f, "${var}"),
            Source::Unset => write!(f, "unset"),
        }
    }
}

/// Provenance of a single field.
#[derive(Debug, Clone)]
pub struct FieldSource {
    /// Dotted field path, e.g. `cache.ttl`.
    pub path: String,
    /// The resolved value, rendered for display. Fields marked
    /// `#[config(secret)]` show `<redacted>`.
    pub value: String,
    /// The winning layer.
    pub source: Source,
}

/// Provenance of every field of a loaded configuration.
///
/// Displays as an aligned, line-per-field readout suitable for logging at
/// startup; iterate for programmatic access.
#[derive(Debug, Clone)]
pub struct Sources(Vec<FieldSource>);

impl Sources {
    pub(crate) fn new(fields: Vec<FieldSource>) -> Self {
        Self(fields)
    }

    /// Iterates the fields in declaration order (base fields first).
    pub fn iter(&self) -> std::slice::Iter<'_, FieldSource> {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a Sources {
    type Item = &'a FieldSource;
    type IntoIter = std::slice::Iter<'a, FieldSource>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl fmt::Display for Sources {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path_width = self.iter().map(|s| s.path.len()).max().unwrap_or(0);
        let value_width = self.iter().map(|s| s.value.len()).max().unwrap_or(0);
        let mut lines = self.iter().map(|s| {
            format!(
                "{:<path_width$} = {:<value_width$} ({})",
                s.path, s.value, s.source
            )
        });
        if let Some(first) = lines.next() {
            write!(f, "{first}")?;
        }
        for line in lines {
            write!(f, "\n{line}")?;
        }
        Ok(())
    }
}
