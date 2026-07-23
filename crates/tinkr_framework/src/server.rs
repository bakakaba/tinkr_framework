//! HTTP + gRPC server built on multiplexed ports.
//!
//! [`Server`] serves an [`axum::Router`] (HTTP/REST) and any number of tonic
//! gRPC services on one or more listeners, telling the two kinds of traffic
//! apart automatically. Identity, default port, and shutdown grace period
//! come from the loaded configuration (see [`crate::init!`]).
//!
//! Every server exposes a built-in `GET /health` endpoint (see [`health`]);
//! the `/health` path is reserved.

use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::task::JoinSet;

use crate::errors::{Error, Result};
use crate::health::{self, Health, Status};

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
/// tinkr_framework::init!()?;
///
/// let server = Server::new().route("/hello", get(|| async { "hello" }));
/// # Ok::<(), tinkr_framework::errors::Error>(())
/// ```
///
/// Follow with `.serve()` to bind and run until shutdown; see the demo
/// crate (`crates/demo/examples/`) for complete programs.
pub struct Server {
    service: &'static str,
    version: &'static str,
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
    binds: Vec<BindSpec>,
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
        s.field("binds", &self.binds.len());
        s.finish_non_exhaustive()
    }
}

impl Server {
    /// Create a new, empty server. The built-in
    /// [`/health` endpoint](crate::health) reports the `name` and `version`
    /// from the loaded configuration.
    ///
    /// # Panics
    ///
    /// Panics when the configuration is not loaded — call [`crate::init!`]
    /// first.
    // Not `Default`: construction requires the loaded configuration, and a
    // panicking `default()` would be misleading.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let base = tinkr_config::get::<()>();
        Self {
            service: base.name.as_str(),
            version: base.version.as_str(),
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
            binds: Vec::new(),
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
    /// tinkr_framework::init!()?;
    ///
    /// let server = Server::new().health(|| async {
    ///     Health {
    ///         status: Status::OK,
    ///         checks: vec![Check::new("database", Status::OK)],
    ///     }
    /// });
    /// # Ok::<(), tinkr_framework::errors::Error>(())
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
    /// Use this to close database pools, flush buffers, etc. The hook shares
    /// the configured `shutdown_timeout` grace period with connection
    /// draining.
    pub fn on_shutdown(mut self, hook: impl Future<Output = ()> + Send + 'static) -> Self {
        self.shutdown_hook = Some(Box::pin(hook));
        self
    }

    /// Add a bind address. Call repeatedly to serve on multiple addresses.
    ///
    /// Calling `bind` at least once **replaces** [`serve`](Self::serve)'s
    /// implicit configured-port bind. To keep the configured port alongside
    /// extra addresses, bind it explicitly:
    ///
    /// ```
    /// let cfg = tinkr_framework::init!()?;
    ///
    /// let server = tinkr_framework::Server::new()
    ///     // Explicit binds replace the configured default...
    ///     .bind("127.0.0.1:9090")
    ///     // ...so re-add the configured port if you still want it.
    ///     .bind(cfg.port);
    /// # Ok::<(), tinkr_framework::errors::Error>(())
    /// ```
    ///
    /// # Arguments
    ///
    /// - `target` — a port (`8080`; binds both IPv4 and IPv6, best effort),
    ///   an address (`[127, 0, 0, 1]`, `"127.0.0.1"`, `"127.0.0.1:3000"`, a
    ///   [`SocketAddr`]; bare addresses get the configured port), or a
    ///   pre-bound [`tokio::net::TcpListener`] (useful in tests: bind port
    ///   `0` and read `local_addr()` before serving)
    pub fn bind(mut self, target: impl BindTarget) -> Self {
        self.binds.push(target.into_spec());
        self
    }

    /// Bind and serve until the process receives `ctrl-c` (or `SIGTERM` on
    /// unix), then shut down gracefully and run the
    /// [`on_shutdown`](Self::on_shutdown) hook, if any. Draining and the
    /// hook are abandoned if they outlast the configured `shutdown_timeout`.
    ///
    /// Without any [`bind`](Self::bind) calls this listens on the configured
    /// `port` on all IPv4 and IPv6 addresses (best effort per family);
    /// otherwise exactly the bound addresses are served. The resolved
    /// addresses are logged at startup.
    pub async fn serve(mut self) -> Result<()> {
        let base = tinkr_config::get::<()>();
        let hook = self.shutdown_hook.take();
        let mut binds = std::mem::take(&mut self.binds);
        if binds.is_empty() {
            binds.push(BindSpec::DualStackPort(base.port));
        }
        let app = self.into_app()?;

        let listeners = resolve_binds(binds, base.port).await?;
        let addresses = listeners
            .iter()
            .filter_map(|listener| listener.local_addr().ok())
            .map(|addr| addr.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        tracing::info!(service = %base.name, version = %base.version, "serving on {addresses}");

        // Signal receipt is observed separately so the shutdown deadline
        // starts at the signal, not at connection-drain completion, and is
        // fanned out to every listener's serve loop.
        let (signal_tx, signal_rx) = tokio::sync::oneshot::channel::<()>();
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(());
        tokio::spawn(async move {
            shutdown_signal().await;
            let _ = signal_tx.send(());
            let _ = shutdown_tx.send(());
        });

        let mut servers = JoinSet::new();
        for listener in listeners {
            let app = app.clone();
            let mut shutdown = shutdown_rx.clone();
            servers.spawn(async move {
                axum::serve(listener, app)
                    .with_graceful_shutdown(async move {
                        let _ = shutdown.changed().await;
                    })
                    .await
            });
        }
        drop(shutdown_rx);

        enum Event {
            Signal,
            Serving(Option<std::result::Result<std::io::Result<()>, tokio::task::JoinError>>),
        }

        let mut signal_rx = std::pin::pin!(signal_rx);
        loop {
            let event = tokio::select! {
                _ = signal_rx.as_mut() => Event::Signal,
                joined = servers.join_next() => Event::Serving(joined),
            };
            match event {
                // All listeners finished without a shutdown signal.
                Event::Serving(None) => break,
                // A listener ended early: propagate errors, keep draining.
                Event::Serving(Some(joined)) => joined.expect("server task panicked")?,
                Event::Signal => {
                    let drain = async {
                        while let Some(joined) = servers.join_next().await {
                            joined.expect("server task panicked")?;
                        }
                        if let Some(hook) = hook {
                            hook.await;
                        }
                        Ok::<_, Error>(())
                    };
                    match tokio::time::timeout(base.shutdown_timeout, drain).await {
                        Ok(result) => result?,
                        Err(_) => tracing::warn!(
                            timeout_secs = base.shutdown_timeout.as_secs(),
                            "graceful shutdown timed out; exiting with work still pending"
                        ),
                    }
                    return Ok(());
                }
            }
        }

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
                    health::response(service, version, uptime, evaluating.elapsed(), &health)
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

/// A pending bind, resolved into listeners when `serve()` runs.
// `pub` only because it appears in the sealed `BindTarget::into_spec`
// signature; not constructible or nameable through the documented API.
#[doc(hidden)]
#[derive(Debug)]
pub enum BindSpec {
    /// Bind the port on all IPv4 and IPv6 addresses, best effort per family.
    DualStackPort(u16),
    /// Bind this exact socket address.
    Addr(SocketAddr),
    /// Bind this address, using the configured port.
    Ip(IpAddr),
    /// Parse and bind at serve time: `ip:port` or a bare `ip`.
    Str(String),
    /// Use a pre-bound listener as-is.
    Listener(TcpListener),
}

async fn resolve_binds(binds: Vec<BindSpec>, config_port: u16) -> Result<Vec<TcpListener>> {
    let mut listeners = Vec::new();
    for spec in binds {
        match spec {
            BindSpec::Listener(listener) => listeners.push(listener),
            BindSpec::Addr(addr) => listeners.push(TcpListener::bind(addr).await?),
            BindSpec::Ip(ip) => {
                listeners.push(TcpListener::bind(SocketAddr::from((ip, config_port))).await?)
            }
            BindSpec::Str(s) => {
                // Accept both "ip:port" and bare "ip" (which gets the
                // configured port).
                let addr = if let Ok(addr) = s.parse::<SocketAddr>() {
                    addr
                } else if let Ok(ip) = s.parse::<IpAddr>() {
                    SocketAddr::from((ip, config_port))
                } else {
                    return Err(Error::InvalidAddress(s));
                };
                listeners.push(TcpListener::bind(addr).await?);
            }
            BindSpec::DualStackPort(port) => {
                // Best effort per address family: environments without one
                // of the stacks get the other, silently.
                let v4 = TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port))).await;
                let v6 = bind_v6only(port);
                match (v4, v6) {
                    (Err(e), Err(_)) => return Err(e.into()),
                    (v4, v6) => {
                        listeners.extend(v4);
                        listeners.extend(v6);
                    }
                }
            }
        }
    }
    Ok(listeners)
}

/// Binds `[::]:{port}` accepting IPv6 connections only.
// IPV6_V6ONLY is set so this socket can coexist with the separate IPv4
// listener on the same port; without it the two binds conflict on Linux.
fn bind_v6only(port: u16) -> std::io::Result<TcpListener> {
    use socket2::{Domain, Protocol, Socket, Type};

    let socket = Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_only_v6(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&SocketAddr::from((Ipv6Addr::UNSPECIFIED, port)).into())?;
    socket.listen(1024)?;
    TcpListener::from_std(socket.into())
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

/// A bind target accepted by [`Server::bind`].
///
/// Implemented for ports (`u16`; both IPv4 and IPv6), addresses ([`IpAddr`],
/// `[u8; 4]`, `&str`), [`SocketAddr`], and pre-bound
/// [`tokio::net::TcpListener`]s. This trait is sealed and cannot be
/// implemented outside this crate.
pub trait BindTarget: sealed::Sealed {
    /// Convert this target into a pending bind.
    #[doc(hidden)]
    fn into_spec(self) -> BindSpec;
}

impl BindTarget for u16 {
    fn into_spec(self) -> BindSpec {
        BindSpec::DualStackPort(self)
    }
}

impl BindTarget for IpAddr {
    fn into_spec(self) -> BindSpec {
        BindSpec::Ip(self)
    }
}

impl BindTarget for [u8; 4] {
    fn into_spec(self) -> BindSpec {
        BindSpec::Ip(IpAddr::from(self))
    }
}

impl BindTarget for &str {
    fn into_spec(self) -> BindSpec {
        BindSpec::Str(self.to_string())
    }
}

impl BindTarget for SocketAddr {
    fn into_spec(self) -> BindSpec {
        BindSpec::Addr(self)
    }
}

impl BindTarget for TcpListener {
    fn into_spec(self) -> BindSpec {
        BindSpec::Listener(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dual_stack_port_binds_best_effort() {
        let listeners = resolve_binds(vec![BindSpec::DualStackPort(0)], 0)
            .await
            .unwrap();
        assert!(!listeners.is_empty(), "at least one family must bind");
        assert!(listeners.len() <= 2);
        let addrs: Vec<SocketAddr> = listeners.iter().map(|l| l.local_addr().unwrap()).collect();
        assert!(addrs.iter().any(|a| a.is_ipv4()), "IPv4 always available");
        if listeners.len() == 2 {
            assert!(addrs.iter().any(|a| a.is_ipv6()));
        }
    }

    #[tokio::test]
    async fn bare_ip_gets_config_port() {
        let listeners = resolve_binds(vec![BindSpec::Str("127.0.0.1".into())], 0)
            .await
            .unwrap();
        let addr = listeners[0].local_addr().unwrap();
        assert_eq!(addr.ip(), IpAddr::from([127, 0, 0, 1]));
    }

    #[tokio::test]
    async fn invalid_address_string_errors() {
        let err = resolve_binds(vec![BindSpec::Str("not-an-address".into())], 0)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::InvalidAddress(_)));
    }
}
