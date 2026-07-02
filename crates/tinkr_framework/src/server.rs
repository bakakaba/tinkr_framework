//! HTTP + gRPC server built on a single, multiplexed port.
//!
//! [`ServerBuilder`] assembles an [`axum::Router`] (HTTP/REST) and any number of
//! tonic gRPC services onto a single listener. Requests are dispatched by
//! content-type: `application/grpc*` is routed to the tonic services and
//! everything else is routed to the axum router.

use std::future::Future;
use std::net::SocketAddr;

use tokio::net::TcpListener;

use crate::error::{Error, Result};

/// The default address the server binds to when none is provided.
const DEFAULT_ADDR: ([u8; 4], u16) = ([0, 0, 0, 0], 8080);

/// Builder for a multiplexed HTTP + gRPC [`Server`].
///
/// # Example
///
/// ```no_run
/// use tinkr_framework::ServerBuilder;
///
/// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
/// use axum::routing::get;
///
/// let server = ServerBuilder::new()
///     .bind(([127, 0, 0, 1], 8080))
///     .route("/health", get(|| async { "ok" }))
///     .build()
///     .await?;
///
/// server.serve().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct ServerBuilder {
    addr: SocketAddr,
    router: axum::Router,
    has_http: bool,
    #[cfg(feature = "grpc")]
    grpc: tonic::service::RoutesBuilder,
    #[cfg(feature = "grpc")]
    has_grpc: bool,
}

impl Default for ServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerBuilder {
    /// Create a new builder bound to the default address (`0.0.0.0:8080`).
    pub fn new() -> Self {
        Self {
            addr: SocketAddr::from(DEFAULT_ADDR),
            router: axum::Router::new(),
            has_http: false,
            #[cfg(feature = "grpc")]
            grpc: tonic::service::RoutesBuilder::default(),
            #[cfg(feature = "grpc")]
            has_grpc: false,
        }
    }

    /// Set the socket address the server will bind to.
    pub fn bind(mut self, addr: impl Into<SocketAddr>) -> Self {
        self.addr = addr.into();
        self
    }

    /// Merge an [`axum::Router`] into the HTTP routes.
    pub fn router(mut self, router: axum::Router) -> Self {
        self.router = self.router.merge(router);
        self.has_http = true;
        self
    }

    /// Add a single HTTP route.
    ///
    /// This is a thin convenience wrapper around [`axum::Router::route`].
    pub fn route(mut self, path: &str, method_router: axum::routing::MethodRouter) -> Self {
        self.router = self.router.route(path, method_router);
        self.has_http = true;
        self
    }

    /// Register a tonic gRPC service.
    ///
    /// `svc` is the generated `XxxServer<T>` type produced by `tonic-build`,
    /// `tonic-prost-build`, or `buf`. All three toolchains emit the same
    /// concrete server type, so registration is identical regardless of how the
    /// code was generated.
    #[cfg(feature = "grpc")]
    #[cfg_attr(docsrs, doc(cfg(feature = "grpc")))]
    pub fn add_grpc_service<S>(mut self, svc: S) -> Self
    where
        S: tower::Service<
                http::Request<tonic::body::Body>,
                Error = std::convert::Infallible,
            > + tonic::server::NamedService
            + Clone
            + Send
            + Sync
            + 'static,
        S::Response: axum::response::IntoResponse,
        S::Future: Send + 'static,
    {
        self.grpc.add_service(svc);
        self.has_grpc = true;
        self
    }

    /// Build the [`Server`], binding the listener.
    ///
    /// Returns [`Error::NoRoutes`] if neither HTTP routes nor gRPC services were
    /// registered.
    pub async fn build(self) -> Result<Server> {
        let listener = TcpListener::bind(self.addr).await?;
        let app = self.into_app()?;
        Ok(Server { listener, app })
    }

    /// Assemble the merged axum router from the configured HTTP + gRPC routes.
    fn into_app(self) -> Result<axum::Router> {
        // Detect an entirely empty configuration up front to catch
        // misconfiguration (a router with no routes 404s everything).
        #[cfg(feature = "grpc")]
        let configured = self.has_http || self.has_grpc;
        #[cfg(not(feature = "grpc"))]
        let configured = self.has_http;

        if !configured {
            return Err(Error::NoRoutes);
        }

        #[cfg_attr(not(feature = "grpc"), allow(unused_mut))]
        let mut app = self.router;

        #[cfg(feature = "grpc")]
        if self.has_grpc {
            let grpc = self.grpc.routes().into_axum_router();
            app = app.merge(grpc);
        }

        Ok(app)
    }
}

/// A bound, ready-to-serve HTTP + gRPC server.
#[derive(Debug)]
pub struct Server {
    listener: TcpListener,
    app: axum::Router,
}

impl Server {
    /// The local address the server is bound to.
    ///
    /// Useful when binding to port `0` in tests to discover the OS-assigned
    /// port.
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Serve until the process is terminated.
    pub async fn serve(self) -> Result<()> {
        axum::serve(self.listener, self.app).await?;
        Ok(())
    }

    /// Serve until the provided `shutdown` future resolves, then shut down
    /// gracefully.
    pub async fn serve_with_shutdown(self, shutdown: impl Future<Output = ()> + Send + 'static) -> Result<()> {
        axum::serve(self.listener, self.app)
            .with_graceful_shutdown(shutdown)
            .await?;
        Ok(())
    }
}
