//! HTTP + gRPC server built on a single, multiplexed port.
//!
//! [`Server`] assembles an [`axum::Router`] (HTTP/REST) and any number of
//! tonic gRPC services onto a single listener. Requests are dispatched by
//! content-type: `application/grpc*` is routed to the tonic services and
//! everything else is routed to the axum router.
//!
//! [`Server::serve`] runs until the process receives `ctrl-c` (or `SIGTERM` on
//! unix), then shuts down gracefully and runs the optional
//! [`Server::on_shutdown`] clean-up hook.

use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::pin::Pin;

use tokio::net::TcpListener;

use crate::error::{Error, Result};

/// The port used when a [`ServeTarget`] specifies only an address.
const DEFAULT_PORT: u16 = 8080;

type ShutdownHook = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// A multiplexed HTTP + gRPC server.
///
/// # Example
///
/// ```no_run
/// use tinkr_framework::Server;
///
/// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
/// use axum::routing::get;
///
/// Server::new()
///     .route("/health", get(|| async { "ok" }))
///     .serve(8080)
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct Server {
    router: axum::Router,
    has_http: bool,
    /// gRPC routes accumulated *without* tonic's `unimplemented` fallback, so
    /// unmatched paths fall through to axum's default 404 and so a
    /// user-supplied [`grpc_routes`](Self::grpc_routes) (which carries the one
    /// allowed custom fallback) can still be merged in.
    #[cfg(feature = "grpc")]
    grpc: tonic::service::Routes,
    #[cfg(feature = "grpc")]
    has_grpc: bool,
    #[cfg(feature = "grpc")]
    has_grpc_routes: bool,
    shutdown_hook: Option<ShutdownHook>,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("Server");
        s.field("has_http", &self.has_http);
        #[cfg(feature = "grpc")]
        s.field("has_grpc", &self.has_grpc);
        s.field("has_shutdown_hook", &self.shutdown_hook.is_some());
        s.finish_non_exhaustive()
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

impl Server {
    /// Create a new, empty server.
    pub fn new() -> Self {
        Self {
            router: axum::Router::new(),
            has_http: false,
            // `Routes::from(axum::Router)` does NOT attach tonic's
            // `unimplemented` fallback (unlike `Routes::default()`), keeping
            // this router mergeable and 404-friendly.
            #[cfg(feature = "grpc")]
            grpc: tonic::service::Routes::from(axum::Router::new()),
            #[cfg(feature = "grpc")]
            has_grpc: false,
            #[cfg(feature = "grpc")]
            has_grpc_routes: false,
            shutdown_hook: None,
        }
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
    /// code was generated. Call repeatedly to register multiple services.
    #[cfg(feature = "grpc")]
    #[cfg_attr(docsrs, doc(cfg(feature = "grpc")))]
    pub fn grpc_service<S>(mut self, svc: S) -> Self
    where
        S: tower::Service<http::Request<tonic::body::Body>, Error = std::convert::Infallible>
            + tonic::server::NamedService
            + Clone
            + Send
            + Sync
            + 'static,
        S::Response: axum::response::IntoResponse,
        S::Future: Send + 'static,
    {
        self.grpc = self.grpc.add_service(svc);
        self.has_grpc = true;
        self
    }

    /// Merge a pre-built [`tonic::service::Routes`] into the gRPC routes,
    /// mirroring [`router`](Self::router) for the HTTP side.
    ///
    /// Build the routes anywhere (e.g. with `tonic::service::Routes::builder()`)
    /// and pass them in whole.
    ///
    /// # Panics
    ///
    /// Panics if called more than once. A tonic [`Routes`] always carries its
    /// own `unimplemented` fallback and axum cannot merge two routers that
    /// both have one. Register additional services with
    /// [`grpc_service`](Self::grpc_service) instead.
    ///
    /// Note: the fallback that comes with `routes` applies to the whole
    /// server, so unmatched requests will receive a gRPC `Unimplemented`
    /// response instead of an HTTP 404.
    ///
    /// [`Routes`]: tonic::service::Routes
    #[cfg(feature = "grpc")]
    #[cfg_attr(docsrs, doc(cfg(feature = "grpc")))]
    pub fn grpc_routes(mut self, routes: tonic::service::Routes) -> Self {
        assert!(
            !self.has_grpc_routes,
            "`grpc_routes` may only be called once: tonic `Routes` carry a \
             fallback and axum cannot merge two routers that both have one. \
             Register additional services with `grpc_service`."
        );
        self.has_grpc_routes = true;
        self.grpc = tonic::service::Routes::from(
            self.grpc
                .into_axum_router()
                .merge(routes.into_axum_router()),
        );
        self.has_grpc = true;
        self
    }

    /// Register a clean-up hook that runs after graceful shutdown completes,
    /// just before [`serve`](Self::serve) returns.
    ///
    /// Use this to close database pools, flush buffers, etc.
    pub fn on_shutdown(mut self, hook: impl Future<Output = ()> + Send + 'static) -> Self {
        self.shutdown_hook = Some(Box::pin(hook));
        self
    }

    /// Bind to `target` and serve until the process receives `ctrl-c` (or
    /// `SIGTERM` on unix), then shut down gracefully and run the
    /// [`on_shutdown`](Self::on_shutdown) hook, if any.
    ///
    /// `target` may be:
    ///
    /// - a port: `serve(8080)` binds `0.0.0.0:8080`
    /// - an address: `serve([127, 0, 0, 1])` or `serve(IpAddr::...)` binds port
    ///   `8080`
    /// - a string: `serve("127.0.0.1")` or `serve("127.0.0.1:3000")`
    /// - a [`SocketAddr`]
    /// - a pre-bound [`tokio::net::TcpListener`] (useful in tests: bind port
    ///   `0` yourself and read `local_addr()` before serving)
    ///
    /// Returns [`Error::NoRoutes`] if neither HTTP routes nor gRPC services
    /// were registered.
    pub async fn serve(mut self, target: impl ServeTarget) -> Result<()> {
        let hook = self.shutdown_hook.take();
        let app = self.into_app()?;
        let listener = target.into_listener().await?;

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await?;

        if let Some(hook) = hook {
            hook.await;
        }

        Ok(())
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
            let grpc = self.grpc.into_axum_router();
            app = app.merge(grpc);
        }

        Ok(app)
    }
}

/// Resolves when the process receives `ctrl-c`, or `SIGTERM` on unix.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install ctrl-c handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

mod sealed {
    pub trait Sealed {}

    impl Sealed for u16 {}
    impl Sealed for std::net::IpAddr {}
    impl Sealed for [u8; 4] {}
    impl Sealed for &str {}
    impl Sealed for std::net::SocketAddr {}
    impl Sealed for tokio::net::TcpListener {}
}

/// A bind target accepted by [`Server::serve`].
///
/// Implemented for ports (`u16`), addresses ([`IpAddr`], `[u8; 4]`, `&str`),
/// [`SocketAddr`], and pre-bound [`tokio::net::TcpListener`]s. This trait is
/// sealed and cannot be implemented outside this crate.
#[allow(async_fn_in_trait)]
pub trait ServeTarget: sealed::Sealed {
    /// Resolve this target into a bound listener.
    #[doc(hidden)]
    async fn into_listener(self) -> Result<TcpListener>;
}

impl ServeTarget for u16 {
    async fn into_listener(self) -> Result<TcpListener> {
        let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, self));
        Ok(TcpListener::bind(addr).await?)
    }
}

impl ServeTarget for IpAddr {
    async fn into_listener(self) -> Result<TcpListener> {
        Ok(TcpListener::bind(SocketAddr::from((self, DEFAULT_PORT))).await?)
    }
}

impl ServeTarget for [u8; 4] {
    async fn into_listener(self) -> Result<TcpListener> {
        IpAddr::from(self).into_listener().await
    }
}

impl ServeTarget for &str {
    async fn into_listener(self) -> Result<TcpListener> {
        // Accept both "ip:port" and bare "ip" (which gets the default port).
        let addr = if let Ok(addr) = self.parse::<SocketAddr>() {
            addr
        } else if let Ok(ip) = self.parse::<IpAddr>() {
            SocketAddr::from((ip, DEFAULT_PORT))
        } else {
            return Err(Error::InvalidAddress(self.to_string()));
        };
        Ok(TcpListener::bind(addr).await?)
    }
}

impl ServeTarget for SocketAddr {
    async fn into_listener(self) -> Result<TcpListener> {
        Ok(TcpListener::bind(self).await?)
    }
}

impl ServeTarget for TcpListener {
    async fn into_listener(self) -> Result<TcpListener> {
        Ok(self)
    }
}
