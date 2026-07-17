//! HTTP + gRPC server built on a single, multiplexed port.
//!
//! [`Server`] serves an [`axum::Router`] (HTTP/REST) and any number of tonic
//! gRPC services on one listener, telling the two kinds of traffic apart
//! automatically.
//!
//! Every server exposes a built-in `GET /health` endpoint (see
//! [`health`]); the `/health` path is reserved.
//!
//! [`Server::serve`] runs until the process receives `ctrl-c` (or `SIGTERM` on
//! unix), then shuts down gracefully and runs the optional
//! [`Server::on_shutdown`] clean-up hook.

use std::borrow::Cow;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;

use tokio::net::TcpListener;

use crate::errors::{Error, Result};
use crate::health::{self, Health, Status};

/// The port used when a [`ServeTarget`] specifies only an address.
const DEFAULT_PORT: u16 = 8080;

type ShutdownHook = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

type HealthCheck =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Health> + Send + 'static>> + Send + Sync>;

/// A multiplexed HTTP + gRPC server.
///
/// # Example
///
/// ```
/// use tinkr_framework::{Server, routing::get};
///
/// let server = Server::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
///     .route("/hello", get(|| async { "hello" }));
/// ```
///
/// Follow with `.serve(...)` to bind and run until shutdown; see the demo
/// crate (`crates/demo/examples/`) for complete programs.
pub struct Server {
    service: Cow<'static, str>,
    version: Cow<'static, str>,
    router: axum::Router,
    has_http: bool,
    // gRPC routes accumulated *without* tonic's `unimplemented` fallback, so
    // unmatched paths fall through to axum's default 404.
    #[cfg(feature = "grpc")]
    grpc: tonic::service::Routes,
    #[cfg(feature = "grpc")]
    has_grpc: bool,
    health_check: Option<HealthCheck>,
    shutdown_hook: Option<ShutdownHook>,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("Server");
        s.field("service", &self.service);
        s.field("version", &self.version);
        s.field("has_http", &self.has_http);
        #[cfg(feature = "grpc")]
        s.field("has_grpc", &self.has_grpc);
        s.field("has_health_check", &self.health_check.is_some());
        s.field("has_shutdown_hook", &self.shutdown_hook.is_some());
        s.finish_non_exhaustive()
    }
}

impl Server {
    /// Create a new, empty server identified by service name and version,
    /// which the built-in [`/health` endpoint](crate::health) reports.
    ///
    /// # Example
    ///
    /// ```
    /// use tinkr_framework::Server;
    ///
    /// let server = Server::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    /// ```
    pub fn new(name: impl Into<Cow<'static, str>>, version: impl Into<Cow<'static, str>>) -> Self {
        Self {
            service: name.into(),
            version: version.into(),
            router: axum::Router::new(),
            has_http: false,
            // `Routes::from(axum::Router)` does NOT attach tonic's
            // `unimplemented` fallback (unlike `Routes::default()`), keeping
            // this router mergeable and 404-friendly.
            #[cfg(feature = "grpc")]
            grpc: tonic::service::Routes::from(axum::Router::new()),
            #[cfg(feature = "grpc")]
            has_grpc: false,
            health_check: None,
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

    /// Register a tonic gRPC service. Call repeatedly to register multiple
    /// services.
    ///
    /// # Arguments
    ///
    /// - `svc` — the generated server type (e.g. `GreeterServer<MyGreeter>`)
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

    /// Register a custom health evaluation for the built-in `/health`
    /// endpoint.
    ///
    /// `check` runs on every `GET /health` request and returns the overall
    /// [`Status`] together with the individual [`Check`](crate::health::Check)s
    /// it was derived from; see [`health`] for the response
    /// format. Without a custom evaluation the endpoint reports
    /// [`Status::OK`] with no checks.
    ///
    /// # Example
    ///
    /// ```
    /// use tinkr_framework::Server;
    /// use tinkr_framework::health::{Check, Health, Status};
    ///
    /// let server = Server::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
    ///     .health(|| async {
    ///         Health {
    ///             status: Status::OK,
    ///             checks: vec![Check::new("database", Status::OK)],
    ///         }
    ///     });
    /// ```
    pub fn health<F, Fut>(mut self, check: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Health> + Send + 'static,
    {
        self.health_check = Some(Arc::new(move || Box::pin(check())));
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
    /// # Arguments
    ///
    /// - `target` — a port (`8080`), an address (`[127, 0, 0, 1]`,
    ///   `"127.0.0.1"`, `"127.0.0.1:3000"`, a [`SocketAddr`]), or a pre-bound
    ///   [`tokio::net::TcpListener`] (useful in tests: bind port `0` and read
    ///   `local_addr()` before serving)
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
        // misconfiguration. The always-on default `/health` route doesn't
        // count, but an explicitly registered health check does.
        #[cfg(feature = "grpc")]
        let configured = self.has_http || self.has_grpc || self.health_check.is_some();
        #[cfg(not(feature = "grpc"))]
        let configured = self.has_http || self.health_check.is_some();

        if !configured {
            return Err(Error::NoRoutes);
        }

        let mut app = self.router;

        let service = self.service;
        let version = self.version;
        let check = self.health_check;
        let started = std::time::Instant::now();
        app = app.route(
            "/health",
            axum::routing::get(move || {
                let service = service.clone();
                let version = version.clone();
                let check = check.clone();
                async move {
                    let uptime = started.elapsed();
                    // Wall time around the whole evaluation, so it stays
                    // accurate when the check fn runs its probes in parallel.
                    let evaluating = std::time::Instant::now();
                    let health = match &check {
                        Some(check) => check().await,
                        None => Health::new(Status::OK),
                    };
                    health::response(&service, &version, uptime, evaluating.elapsed(), &health)
                }
            }),
        );

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
