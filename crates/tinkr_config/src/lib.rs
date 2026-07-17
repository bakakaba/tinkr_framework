//! Layered, attribute-driven configuration for services.
//!
//! Derive [`Configurable`] on a struct describing your settings, then load it
//! once at startup with [`load!`]. Values are resolved per field, highest
//! precedence first:
//!
//! 1. environment variables (declared per field with `#[config(env = "...")]`)
//! 2. `config.toml` in the working directory
//! 3. `#[config(default = ...)]` values
//!
//! The loaded configuration is frozen and globally accessible through
//! [`get`], and every field remembers which layer supplied its value
//! (see [`Config::sources`]).
//!
//! ```
//! use tinkr_config::Configurable;
//!
//! /// Example application configuration.
//! #[derive(Debug, Configurable)]
//! struct AppConfig {
//!     /// Number of worker tasks.
//!     #[config(env = "TINKR_DOC_WORKERS", default = 4)]
//!     workers: usize,
//! }
//!
//! // `parse` is the test-friendly variant of `load!`: same layering, but no
//! // global state or file system access.
//! let config = tinkr_config::parse::<AppConfig>("demo", "1.0.0", Some("workers = 8"))?;
//! assert_eq!(config.workers, 8);   // application field, from the TOML document
//! assert_eq!(config.port, 8080);   // base field, from its default
//! # Ok::<(), tinkr_config::Error>(())
//! ```

use std::any::Any;
use std::ops::Deref;
use std::sync::OnceLock;
use std::time::Duration;

mod base;
mod errors;
mod sources;

pub mod schema;

#[doc(hidden)]
pub mod __private;

pub use errors::Error;
pub use sources::{FieldSource, Source, Sources};
pub use tinkr_config_macros::Configurable;

/// The configuration file read by [`load!`], relative to the working
/// directory.
pub const CONFIG_FILE: &str = "config.toml";

/// A configuration shape that can be loaded from layered sources.
///
/// Implement by deriving: see [`Configurable`](macro@Configurable). The
/// methods are machinery for the derive and not meant to be called or
/// implemented by hand.
pub trait Configurable: Sized {
    /// Option-typed mirror of the struct, holding the values one layer
    /// provides.
    #[doc(hidden)]
    type Layer: Layer;

    /// The struct's doc comment, used as the schema description.
    #[doc(hidden)]
    fn doc() -> &'static str;

    /// Schema tree describing the struct's fields.
    #[doc(hidden)]
    fn schema_node() -> schema::Node;

    /// Merges the three layers into a value, recording per-field provenance.
    #[doc(hidden)]
    fn from_layers(
        env: Self::Layer,
        file: Self::Layer,
        defaults: Self::Layer,
        prefix: &str,
        sources: &mut Vec<FieldSource>,
    ) -> Result<Self, Error>;
}

/// One source's worth of values for a [`Configurable`] struct.
#[doc(hidden)]
pub trait Layer: Default + serde::de::DeserializeOwned {
    /// Reads the fields that declare `#[config(env = "...")]` from the
    /// process environment.
    fn from_env() -> Result<Self, Error>;

    /// The `#[config(default = ...)]` values.
    fn defaults() -> Self;
}

/// Layer of a configuration with no fields of its own.
// A braced struct (not `()`) so it deserializes from a TOML table.
#[doc(hidden)]
#[derive(Default, serde::Deserialize)]
pub struct EmptyLayer {}

impl Layer for EmptyLayer {
    fn from_env() -> Result<Self, Error> {
        Ok(Self {})
    }

    fn defaults() -> Self {
        Self {}
    }
}

/// The empty configuration shape: a `Config<()>` carries only the base
/// fields. Load one when a service has no settings of its own, or read it
/// with [`get::<()>()`](get) to access the base fields without knowing the
/// loaded type.
impl Configurable for () {
    type Layer = EmptyLayer;

    fn doc() -> &'static str {
        ""
    }

    fn schema_node() -> schema::Node {
        schema::Node::Object(Vec::new())
    }

    fn from_layers(
        _env: Self::Layer,
        _file: Self::Layer,
        _defaults: Self::Layer,
        _prefix: &str,
        _sources: &mut Vec<FieldSource>,
    ) -> Result<Self, Error> {
        Ok(())
    }
}

/// A loaded configuration: the base fields plus the application's
/// own [`Configurable`] struct `T`, reachable directly through deref.
///
/// ```
/// use tinkr_config::Configurable;
///
/// #[derive(Debug, Configurable)]
/// struct AppConfig {
///     /// Greeting text.
///     #[config(default = "hello")]
///     greeting: String,
/// }
///
/// let config = tinkr_config::parse::<AppConfig>("demo", "1.0.0", None)?;
/// assert_eq!(config.greeting, "hello");            // application field
/// assert_eq!(config.environment, "development");   // base field
/// # Ok::<(), tinkr_config::Error>(())
/// ```
#[derive(Debug)]
pub struct Config<T> {
    /// TCP port the server listens on (`PORT`, default `8080`).
    pub port: u16,
    /// Deployment environment name (`ENVIRONMENT`, default `"development"`).
    pub environment: String,
    /// Graceful shutdown grace period (`SHUTDOWN_TIMEOUT` in seconds,
    /// default 30).
    pub shutdown_timeout: Duration,
    /// Service name (`SERVICE_NAME`, defaults to the Cargo package name).
    pub name: String,
    /// Service version (`SERVICE_VERSION`, defaults to the Cargo package
    /// version).
    pub version: String,
    app: T,
    sources: Sources,
}

impl<T> Deref for Config<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.app
    }
}

impl<T> Config<T> {
    /// Where each field's value came from, as a printable readout.
    ///
    /// Fields marked `#[config(secret)]` are redacted.
    pub fn sources(&self) -> &Sources {
        &self.sources
    }

    /// A copy of the base fields as a `Config<()>`, served by
    /// [`get::<()>()`](get).
    fn base_view(&self) -> Config<()> {
        Config {
            port: self.port,
            environment: self.environment.clone(),
            shutdown_timeout: self.shutdown_timeout,
            name: self.name.clone(),
            version: self.version.clone(),
            app: (),
            sources: self.sources.clone(),
        }
    }
}

/// The single loaded configuration for this process.
static GLOBAL: OnceLock<Stored> = OnceLock::new();

struct Stored {
    type_name: &'static str,
    value: Box<dyn Any + Send + Sync>,
    /// Base-fields view of `value`, so [`get::<()>()`](get) works without
    /// knowing the loaded type.
    base: Config<()>,
}

/// Loads the configuration and freezes it for the lifetime of the process.
///
/// `load!(AppConfig)` resolves every field from the environment,
/// [`CONFIG_FILE`], and declared defaults, stores the result globally, and
/// returns `Result<&'static Config<AppConfig>, Error>`. Call it once during
/// startup; afterwards any part of the program can read the configuration
/// with [`get`]. (When using `tinkr_framework`, its `init!` macro does this
/// for you.)
///
/// The base `name` and `version` fields default to the calling
/// crate's Cargo package name and version.
///
/// # Panics
///
/// Panics when called more than once.
#[macro_export]
macro_rules! load {
    ($ty:ty) => {
        $crate::load_with::<$ty>(
            ::core::env!("CARGO_PKG_NAME"),
            ::core::env!("CARGO_PKG_VERSION"),
        )
    };
}

/// Non-macro form of [`load!`] taking explicit `name`/`version` defaults.
///
/// Prefer [`load!`], which fills these in from the calling crate's Cargo
/// package.
///
/// # Panics
///
/// Panics when a configuration was already loaded.
pub fn load_with<T>(name: &str, version: &str) -> Result<&'static Config<T>, Error>
where
    T: Configurable + Send + Sync + 'static,
{
    // Make .env values visible to the environment layer even when the host
    // application didn't load them itself.
    dotenvy::dotenv().ok();

    if GLOBAL.get().is_some() {
        panic!("configuration already loaded: load the configuration exactly once at startup");
    }

    let file = match std::fs::read_to_string(CONFIG_FILE) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let config = parse::<T>(name, version, file.as_deref())?;

    let stored = Stored {
        type_name: std::any::type_name::<T>(),
        base: config.base_view(),
        value: Box::new(config),
    };
    if GLOBAL.set(stored).is_err() {
        panic!("configuration already loaded: load the configuration exactly once at startup");
    }
    Ok(get::<T>())
}

/// Returns the configuration loaded by [`load!`], from anywhere in the
/// program.
///
/// `get::<()>()` returns just the base fields — regardless of the loaded
/// type — for code that doesn't know the application's configuration struct.
///
/// ```
/// use tinkr_config::Configurable;
///
/// /// Doctest configuration.
/// #[derive(Debug, Configurable)]
/// struct AppConfig {
///     /// Greeting text.
///     #[config(default = "hello")]
///     greeting: String,
/// }
///
/// tinkr_config::load!(AppConfig)?;
///
/// let config = tinkr_config::get::<AppConfig>();
/// assert_eq!(config.greeting, "hello");
///
/// let base = tinkr_config::get::<()>(); // base fields only, type-agnostic
/// assert_eq!(base.port, config.port);
/// # Ok::<(), tinkr_config::Error>(())
/// ```
///
/// # Panics
///
/// Panics when no configuration has been loaded, or when `T` is neither the
/// loaded type nor `()`.
pub fn get<T: 'static>() -> &'static Config<T> {
    let stored = GLOBAL
        .get()
        .expect("configuration not loaded: load the configuration at startup first");
    if let Some(config) = stored.value.downcast_ref::<Config<T>>() {
        return config;
    }
    if let Some(base) = (&stored.base as &dyn Any).downcast_ref::<Config<T>>() {
        return base;
    }
    panic!(
        "configuration type mismatch: loaded `{}`, requested `{}`",
        stored.type_name,
        std::any::type_name::<T>()
    );
}

/// Resolves a configuration without touching the global slot or the file
/// system: the same layering as [`load!`], with `file` standing in for
/// `config.toml`.
///
/// Useful in tests and tooling. Environment variables still apply.
pub fn parse<T: Configurable>(
    name: &str,
    version: &str,
    file: Option<&str>,
) -> Result<Config<T>, Error> {
    // Application fields must not shadow the base ones: both are read
    // from the top level of the same file.
    if let schema::Node::Object(props) = T::schema_node() {
        for prop in &props {
            if base::RESERVED.contains(&prop.name) {
                return Err(Error::ReservedField { name: prop.name });
            }
        }
    }

    let (base_file, app_file) = match file {
        Some(text) => (
            toml::from_str::<base::BaseLayer>(text)?,
            toml::from_str::<T::Layer>(text)?,
        ),
        None => (base::BaseLayer::default(), T::Layer::default()),
    };

    let mut sources = Vec::new();
    let base = base::merge(
        base::BaseLayer::from_env()?,
        base_file,
        base::BaseLayer::defaults(name, version),
        &mut sources,
    )?;
    let app = T::from_layers(
        T::Layer::from_env()?,
        app_file,
        T::Layer::defaults(),
        "",
        &mut sources,
    )?;

    Ok(Config {
        port: base.port,
        environment: base.environment,
        shutdown_timeout: base.shutdown_timeout,
        name: base.name,
        version: base.version,
        app,
        sources: Sources::new(sources),
    })
}

/// Renders the JSON Schema (draft-07) describing `T` plus the base
/// fields, for editor validation and completion of [`CONFIG_FILE`].
///
/// Point your TOML editor tooling at it by making the first line of the
/// config file `#:schema ./config.schema.json` (see [`template`]).
pub fn schema<T: Configurable>() -> String {
    schema::render::<T>()
}

/// Writes [`schema()`]`::<T>()` to `path`, typically `config.schema.json`
/// next to [`CONFIG_FILE`].
///
/// Call this from a small generator target (e.g. a `cargo` example) and
/// commit the output; keep it fresh by regenerating and diffing in CI.
pub fn write_schema<T: Configurable>(path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
    std::fs::write(path, schema::<T>() + "\n")
}

/// Renders a commented starter [`CONFIG_FILE`] for `T`: every field with its
/// description, default, and environment variable, plus the `#:schema` line
/// that enables editor intellisense.
pub fn template<T: Configurable>() -> String {
    schema::render_template::<T>()
}
