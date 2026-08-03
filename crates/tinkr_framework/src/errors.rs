//! Error types for the `tinkr_framework` server.

/// Errors that can occur while building or running a [`crate::Server`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// An I/O error, typically from binding the listener.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The server was built without any routes (neither HTTP nor gRPC).
    #[error(
        "no routes configured: register at least one HTTP route, gRPC service, or health check"
    )]
    NoRoutes,

    /// The string passed to [`crate::Server::serve`] is neither a valid
    /// `ip:port` pair nor a bare IP address.
    #[error("invalid address: {0}")]
    InvalidAddress(String),

    /// A descriptor set passed to [`crate::Server::grpc_reflection`] is not
    /// a valid encoded `FileDescriptorSet`.
    #[cfg(feature = "grpc")]
    #[cfg_attr(docsrs, doc(cfg(feature = "grpc")))]
    #[error("invalid reflection descriptor set: {0}")]
    InvalidDescriptorSet(String),

    /// The configuration failed to load; see [`crate::config`].
    #[error(transparent)]
    Config(#[from] tinkr_config::Error),

    /// Building the OpenTelemetry exporters failed, typically because the
    /// configured OTLP endpoint is not a valid URL.
    #[cfg(feature = "otel")]
    #[cfg_attr(docsrs, doc(cfg(feature = "otel")))]
    #[error("telemetry setup failed: {0}")]
    Otel(String),
}

/// Convenience alias for results returned by this crate.
pub type Result<T> = std::result::Result<T, Error>;
