//! The deployment environment: the provided [`Env`] type and the
//! [`Environment`] trait for prescribing a custom set of environments.

use std::fmt::{Debug, Display};
use std::str::FromStr;

/// A deployment environment usable as [`Config::env`](crate::Config).
///
/// The provided [`Env`] type covers the common case. Services with
/// additional environments (staging, ...) prescribe their own enum instead:
/// derive [`Environment`](derive@crate::Environment) (plus `Default` with a
/// `#[default]` variant) and select it with `#[config(env = MyEnv)]` on the
/// `Configurable` struct.
///
/// Values parse case-insensitively from the snake_case variant name; an
/// unrecognized name fails configuration loading. The `Default` value is
/// used when no layer provides one.
pub trait Environment:
    FromStr<Err = UnknownEnvironment> + Debug + Display + Default + Send + Sync + 'static
{
    /// The accepted value names, in declaration order.
    fn variants() -> &'static [&'static str];
}

/// The provided deployment environments: `local` (the default),
/// `development`, and `production`.
///
/// ```
/// use tinkr_config::Env;
///
/// assert_eq!("production".parse::<Env>()?, Env::Production);
/// assert_eq!(Env::default(), Env::Local);
/// assert!("staging".parse::<Env>().is_err());
/// # Ok::<(), tinkr_config::UnknownEnvironment>(())
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Env {
    /// A developer's machine.
    #[default]
    Local,
    /// A shared, deployed development environment.
    Development,
    /// The production environment.
    Production,
}

const ENV_VARIANTS: [&str; 3] = ["local", "development", "production"];

impl FromStr for Env {
    type Err = UnknownEnvironment;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("local") {
            Ok(Self::Local)
        } else if s.eq_ignore_ascii_case("development") {
            Ok(Self::Development)
        } else if s.eq_ignore_ascii_case("production") {
            Ok(Self::Production)
        } else {
            Err(UnknownEnvironment::new(s, &ENV_VARIANTS))
        }
    }
}

impl Display for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Local => "local",
            Self::Development => "development",
            Self::Production => "production",
        })
    }
}

impl Environment for Env {
    fn variants() -> &'static [&'static str] {
        &ENV_VARIANTS
    }
}

/// An environment name that is not a variant of the configured
/// [`Environment`] type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownEnvironment {
    /// The rejected name.
    pub value: String,
    /// The accepted names.
    pub expected: &'static [&'static str],
}

impl UnknownEnvironment {
    /// Rejects `value`, naming the `expected` alternatives.
    pub fn new(value: impl Into<String>, expected: &'static [&'static str]) -> Self {
        Self {
            value: value.into(),
            expected,
        }
    }
}

impl Display for UnknownEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unknown environment `{}`: expected one of {}",
            self.value,
            self.expected
                .iter()
                .map(|name| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl std::error::Error for UnknownEnvironment {}
