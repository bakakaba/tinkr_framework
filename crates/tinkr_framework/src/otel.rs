//! OpenTelemetry export over OTLP (`otel` feature): traces, metrics, and
//! logs.
//!
//! Export is configured at [`crate::init!`] time and activates only when an
//! endpoint resolves for at least one signal. Per signal, highest precedence
//! first:
//!
//! 1. `OTEL_EXPORTER_OTLP_{TRACES,METRICS,LOGS}_ENDPOINT`
//! 2. the `otel_endpoint` base configuration field
//!    (`OTEL_EXPORTER_OTLP_ENDPOINT` or `config.toml`)
//!
//! Setting `OTEL_{TRACES,METRICS,LOGS}_EXPORTER=none` disables that signal
//! regardless of endpoints, e.g. to keep logs on stdout where the platform
//! already collects them (Cloud Run, GKE).
//!
//! # Authentication and TLS
//!
//! Export requests carry the headers from the `otel_headers` base
//! configuration field (`OTEL_EXPORTER_OTLP_HEADERS` or `config.toml`) as
//! `key=value` pairs separated by commas, typically an API key for a hosted
//! collector. The per-signal `OTEL_EXPORTER_OTLP_{TRACES,METRICS,LOGS}_HEADERS`
//! variables outrank it, key by key.
//!
//! `https://` endpoints are verified against the system's root certificates
//! by default. The standard OTLP variables override that, each with a
//! per-signal `OTEL_EXPORTER_OTLP_{TRACES,METRICS,LOGS}_*` form outranking
//! the generic one:
//!
//! - `OTEL_EXPORTER_OTLP_CERTIFICATE` — path to a PEM CA bundle that
//!   replaces the system roots when verifying the collector
//! - `OTEL_EXPORTER_OTLP_CLIENT_CERTIFICATE` / `OTEL_EXPORTER_OTLP_CLIENT_KEY`
//!   — paths to a PEM client certificate chain and private key, presented to
//!   the collector (mTLS); both must be set together
//!
//! The certificate variables are ignored for plain `http://` endpoints.

use std::env;
use std::sync::OnceLock;

use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::tonic_types::metadata::MetadataMap;
use opentelemetry_otlp::tonic_types::transport::{Certificate, ClientTlsConfig, Identity};
use opentelemetry_otlp::{WithExportConfig, WithTonicConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing::Subscriber;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::registry::LookupSpan;

use crate::errors::{Error, Result};

/// The installed providers, kept for flushing at shutdown.
static PROVIDERS: OnceLock<Providers> = OnceLock::new();

#[derive(Default)]
struct Providers {
    tracer: Option<SdkTracerProvider>,
    meter: Option<SdkMeterProvider>,
    logger: Option<SdkLoggerProvider>,
}

/// Whether span export is active (gates the request-tracing middleware).
pub(crate) fn traces_active() -> bool {
    PROVIDERS.get().is_some_and(|p| p.tracer.is_some())
}

/// Whether any signal is being exported.
pub(crate) fn active() -> bool {
    let signals = signals();
    signals.traces || signals.metrics || signals.logs
}

/// Which telemetry signals are exporting, reported by `/health`.
#[derive(Clone, Copy, serde::Serialize)]
pub(crate) struct Signals {
    pub(crate) traces: bool,
    pub(crate) metrics: bool,
    pub(crate) logs: bool,
}

/// Snapshot of the active signals.
pub(crate) fn signals() -> Signals {
    let providers = PROVIDERS.get();
    Signals {
        traces: providers.is_some_and(|p| p.tracer.is_some()),
        metrics: providers.is_some_and(|p| p.meter.is_some()),
        logs: providers.is_some_and(|p| p.logger.is_some()),
    }
}

/// Builds the exporters and returns the tracing layers to install: the span
/// layer and the log bridge (each present only when its signal is enabled).
///
/// Called once from `bootstrap::init_with`; must run inside a tokio runtime
/// when any signal is enabled (the OTLP exporters use tonic).
pub(crate) fn init<S>(
    config: &tinkr_config::BaseConfig,
) -> Result<Vec<Box<dyn Layer<S> + Send + Sync>>>
where
    S: Subscriber + for<'a> LookupSpan<'a> + Send + Sync,
{
    let base = config.otel_endpoint.as_deref();
    let (traces, metrics, logs) = (
        endpoint("TRACES", base),
        endpoint("METRICS", base),
        endpoint("LOGS", base),
    );

    let mut providers = Providers::default();
    let mut layers: Vec<Box<dyn Layer<S> + Send + Sync>> = Vec::new();

    if traces.is_some() || metrics.is_some() || logs.is_some() {
        // W3C `traceparent`/`tracestate` propagation, extracted by the
        // request middleware and injected by any client instrumentation.
        global::set_text_map_propagator(TraceContextPropagator::new());
    }

    let resource = Resource::builder()
        .with_service_name(config.name.clone())
        .with_attributes([
            KeyValue::new("service.version", config.version.clone()),
            KeyValue::new("deployment.environment.name", config.env.clone()),
        ])
        .build();

    let metadata = metadata(config.otel_headers.as_deref())?;

    if let Some(endpoint) = traces {
        let mut builder = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint.as_str())
            .with_metadata(metadata.clone());
        if let Some(tls) = tls_config("TRACES", &endpoint)? {
            builder = builder.with_tls_config(tls);
        }
        let exporter = builder.build().map_err(|e| Error::Otel(e.to_string()))?;
        let provider = SdkTracerProvider::builder()
            .with_resource(resource.clone())
            .with_batch_exporter(exporter)
            .build();
        let tracer = provider.tracer("tinkr_framework");
        global::set_tracer_provider(provider.clone());
        layers.push(Box::new(
            tracing_opentelemetry::layer()
                .with_tracer(tracer)
                .with_filter(quiet()),
        ));
        providers.tracer = Some(provider);
    }

    if let Some(endpoint) = metrics {
        let mut builder = opentelemetry_otlp::MetricExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint.as_str())
            .with_metadata(metadata.clone());
        if let Some(tls) = tls_config("METRICS", &endpoint)? {
            builder = builder.with_tls_config(tls);
        }
        let exporter = builder.build().map_err(|e| Error::Otel(e.to_string()))?;
        let provider = SdkMeterProvider::builder()
            .with_resource(resource.clone())
            .with_periodic_exporter(exporter)
            .build();
        global::set_meter_provider(provider.clone());
        providers.meter = Some(provider);
    }

    if let Some(endpoint) = logs {
        let mut builder = opentelemetry_otlp::LogExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint.as_str())
            .with_metadata(metadata);
        if let Some(tls) = tls_config("LOGS", &endpoint)? {
            builder = builder.with_tls_config(tls);
        }
        let exporter = builder.build().map_err(|e| Error::Otel(e.to_string()))?;
        let provider = SdkLoggerProvider::builder()
            .with_resource(resource)
            .with_batch_exporter(exporter)
            .build();
        layers.push(Box::new(
            OpenTelemetryTracingBridge::new(&provider).with_filter(quiet()),
        ));
        providers.logger = Some(provider);
    }

    let _ = PROVIDERS.set(providers);
    Ok(layers)
}

/// Flushes and shuts down the installed providers, exporting whatever is
/// still buffered. Runs during graceful shutdown, inside the configured
/// grace period.
pub(crate) async fn shutdown() {
    let Some(providers) = PROVIDERS.get() else {
        return;
    };
    if !active() {
        return;
    }
    // The SDK shutdowns block on the export channels; keep them off the
    // async workers.
    let _ = tokio::task::spawn_blocking(|| {
        if let Some(provider) = &providers.tracer {
            let _ = provider.shutdown();
        }
        if let Some(provider) = &providers.meter {
            let _ = provider.shutdown();
        }
        if let Some(provider) = &providers.logger {
            let _ = provider.shutdown();
        }
    })
    .await;
}

/// Creates a span for every request, wired to the incoming W3C trace
/// context; applied to the merged router when span export is active.
pub(crate) async fn trace_request(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use tracing::Instrument;
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    let method = req.method().clone();
    let path = req.uri().path().to_owned();
    let route = req
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|matched| matched.as_str().to_owned());

    let span = tracing::info_span!(
        "request",
        otel.name = format!("{method} {}", route.as_deref().unwrap_or(&path)),
        otel.kind = "server",
        { "http.request.method" } = %method,
        { "url.path" } = %path,
        { "http.route" } = tracing::field::Empty,
        { "http.response.status_code" } = tracing::field::Empty,
    );
    if let Some(route) = &route {
        span.record("http.route", route.as_str());
    }
    let parent =
        global::get_text_map_propagator(|propagator| propagator.extract(&Headers(req.headers())));
    // Fails only when no OpenTelemetry layer is installed, and this
    // middleware is applied only when span export is active.
    let _ = span.set_parent(parent);

    let response = next.run(req).instrument(span.clone()).await;
    span.record("http.response.status_code", response.status().as_u16());
    response
}

/// [`opentelemetry::propagation::Extractor`] over the request headers.
struct Headers<'a>(&'a axum::http::HeaderMap);

impl opentelemetry::propagation::Extractor for Headers<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|key| key.as_str()).collect()
    }
}

/// Resolves the OTLP endpoint for a signal; `None` disables the signal.
fn endpoint(signal: &str, base: Option<&str>) -> Option<String> {
    resolve_endpoint(signal, base, |var| env::var(var).ok())
}

/// [`endpoint`] with the environment lookup injected, so resolution is
/// testable without mutating the process environment.
fn resolve_endpoint(
    signal: &str,
    base: Option<&str>,
    var: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    if var(&format!("OTEL_{signal}_EXPORTER"))
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("none"))
    {
        return None;
    }
    var(&format!("OTEL_EXPORTER_OTLP_{signal}_ENDPOINT"))
        .filter(|v| !v.trim().is_empty())
        .or_else(|| base.map(str::to_owned))
}

/// Parses the `otel_headers` value (`key=value` pairs separated by commas)
/// into gRPC metadata attached to every export request. The per-signal
/// header environment variables are read by the exporter itself and outrank
/// these entries key by key.
fn metadata(headers: Option<&str>) -> Result<MetadataMap> {
    let mut map = http::HeaderMap::new();
    let entries = headers
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty());
    for entry in entries {
        // Header values are credentials more often than not: errors name
        // only the key, never the value.
        let (key, value) = entry.split_once('=').ok_or_else(|| {
            Error::Otel(
                "invalid otel_headers: expected `key=value` pairs separated by commas".into(),
            )
        })?;
        let key = key.trim();
        let name = key
            .parse::<http::header::HeaderName>()
            .map_err(|_| Error::Otel(format!("invalid otel_headers key `{key}`")))?;
        let value = value
            .trim()
            .parse::<http::header::HeaderValue>()
            .map_err(|_| Error::Otel(format!("invalid otel_headers value for key `{key}`")))?;
        map.insert(name, value);
    }
    Ok(MetadataMap::from_headers(map))
}

/// PEM file paths for a signal's TLS setup, resolved from the standard
/// OTLP environment variables.
struct TlsFiles {
    /// CA bundle verifying the collector (replaces the system roots).
    ca: Option<String>,
    /// Client certificate chain presented to the collector (mTLS).
    client_cert: Option<String>,
    /// Private key for the client certificate.
    client_key: Option<String>,
}

/// Resolves the TLS settings for a signal's `https://` endpoint; `None`
/// for plain `http://` endpoints, where the certificate variables do not
/// apply.
fn tls_config(signal: &str, endpoint: &str) -> Result<Option<ClientTlsConfig>> {
    build_tls(
        signal,
        endpoint,
        resolve_tls_files(signal, |var| env::var(var).ok()),
    )
}

/// [`tls_config`] with the file paths already resolved, so the construction
/// rules are testable without touching the process environment.
fn build_tls(signal: &str, endpoint: &str, files: TlsFiles) -> Result<Option<ClientTlsConfig>> {
    if !is_https(endpoint) {
        return Ok(None);
    }
    let tls = match &files.ca {
        Some(path) => ClientTlsConfig::new().ca_certificate(Certificate::from_pem(read_pem(path)?)),
        None => ClientTlsConfig::new().with_enabled_roots(),
    };
    let tls = match (&files.client_cert, &files.client_key) {
        (Some(cert), Some(key)) => {
            tls.identity(Identity::from_pem(read_pem(cert)?, read_pem(key)?))
        }
        (None, None) => tls,
        _ => {
            return Err(Error::Otel(format!(
                "mTLS for {signal} needs both the client certificate and key: set \
                 OTEL_EXPORTER_OTLP_CLIENT_CERTIFICATE and OTEL_EXPORTER_OTLP_CLIENT_KEY \
                 (or both {signal}-specific variables) together"
            )));
        }
    };
    Ok(Some(tls))
}

/// [`resolve_endpoint`]'s counterpart for the TLS variables: the signal-
/// specific variable outranks the generic one, per file.
fn resolve_tls_files(signal: &str, var: impl Fn(&str) -> Option<String>) -> TlsFiles {
    let set = |name: String| var(&name).filter(|v| !v.trim().is_empty());
    let file = |suffix: &str| {
        set(format!("OTEL_EXPORTER_OTLP_{signal}_{suffix}"))
            .or_else(|| set(format!("OTEL_EXPORTER_OTLP_{suffix}")))
    };
    TlsFiles {
        ca: file("CERTIFICATE"),
        client_cert: file("CLIENT_CERTIFICATE"),
        client_key: file("CLIENT_KEY"),
    }
}

/// Whether the endpoint uses TLS.
fn is_https(endpoint: &str) -> bool {
    endpoint
        .trim_start()
        .get(..8)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
}

/// Reads a PEM file referenced by one of the TLS variables.
fn read_pem(path: &str) -> Result<String> {
    std::fs::read_to_string(path)
        .map_err(|e| Error::Otel(format!("failed to read PEM file `{path}`: {e}")))
}

/// Filter keeping the export plumbing's own telemetry out of the exporters
/// (telemetry-induced telemetry loops endlessly otherwise).
fn quiet() -> Targets {
    Targets::new()
        .with_default(LevelFilter::TRACE)
        .with_target("h2", LevelFilter::OFF)
        .with_target("hyper", LevelFilter::OFF)
        .with_target("hyper_util", LevelFilter::OFF)
        .with_target("tonic", LevelFilter::OFF)
        .with_target("tower", LevelFilter::OFF)
        .with_target("opentelemetry", LevelFilter::OFF)
        .with_target("opentelemetry_sdk", LevelFilter::OFF)
        .with_target("opentelemetry_otlp", LevelFilter::OFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_prefers_signal_env_and_honors_none() {
        let vars = |var: &str| match var {
            "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT" => Some("http://traces:4317".to_string()),
            "OTEL_METRICS_EXPORTER" => Some("none".to_string()),
            _ => None,
        };

        let base = Some("http://base:4317");
        assert_eq!(
            resolve_endpoint("TRACES", base, vars).as_deref(),
            Some("http://traces:4317") // signal env beats the base endpoint
        );
        assert_eq!(resolve_endpoint("METRICS", base, vars), None); // exporter=none disables
        assert_eq!(
            resolve_endpoint("LOGS", base, vars).as_deref(),
            Some("http://base:4317") // falls back to the base endpoint
        );
        assert_eq!(resolve_endpoint("LOGS", None, vars), None);
    }

    #[test]
    fn metadata_parses_headers_and_names_only_keys_in_errors() {
        let map = metadata(Some("x-api-key=hunter2, x-tenant = acme ")).unwrap();
        assert_eq!(map.get("x-api-key").unwrap(), "hunter2");
        assert_eq!(map.get("x-tenant").unwrap(), "acme");

        assert!(metadata(None).unwrap().is_empty());
        assert!(metadata(Some("")).unwrap().is_empty());

        // Values are credentials: parse errors must never echo them.
        let err = metadata(Some("x-api-key hunter2")).unwrap_err().to_string();
        assert!(!err.contains("hunter2"), "value leaked: {err}");
        let err = metadata(Some("bad key=hunter2")).unwrap_err().to_string();
        assert!(err.contains("bad key"), "unexpected message: {err}");
        assert!(!err.contains("hunter2"), "value leaked: {err}");
    }

    #[test]
    fn tls_files_prefer_signal_vars() {
        let vars = |var: &str| match var {
            "OTEL_EXPORTER_OTLP_TRACES_CERTIFICATE" => Some("/pem/traces-ca".to_string()),
            "OTEL_EXPORTER_OTLP_CERTIFICATE" => Some("/pem/ca".to_string()),
            "OTEL_EXPORTER_OTLP_CLIENT_CERTIFICATE" => Some("/pem/cert".to_string()),
            "OTEL_EXPORTER_OTLP_METRICS_CLIENT_KEY" => Some(" ".to_string()), // blank ignored
            "OTEL_EXPORTER_OTLP_CLIENT_KEY" => Some("/pem/key".to_string()),
            _ => None,
        };

        let traces = resolve_tls_files("TRACES", vars);
        assert_eq!(traces.ca.as_deref(), Some("/pem/traces-ca")); // signal beats generic
        assert_eq!(traces.client_cert.as_deref(), Some("/pem/cert"));

        let metrics = resolve_tls_files("METRICS", vars);
        assert_eq!(metrics.ca.as_deref(), Some("/pem/ca"));
        assert_eq!(metrics.client_key.as_deref(), Some("/pem/key"));

        let logs = resolve_tls_files("LOGS", |_| None);
        assert!(logs.ca.is_none() && logs.client_cert.is_none() && logs.client_key.is_none());
    }

    /// A throwaway PEM file for the TLS construction tests.
    fn pem_file(name: &str) -> String {
        let path =
            std::env::temp_dir().join(format!("tinkr-otel-test-{}-{name}", std::process::id()));
        std::fs::write(
            &path,
            "-----BEGIN CERTIFICATE-----\n-----END CERTIFICATE-----\n",
        )
        .unwrap();
        path.to_str().unwrap().to_owned()
    }

    #[test]
    fn tls_applies_only_to_https_endpoints() {
        let files = TlsFiles {
            ca: None,
            client_cert: None,
            client_key: None,
        };
        assert!(
            build_tls("TRACES", "http://collector:4317", files)
                .unwrap()
                .is_none()
        );

        let files = TlsFiles {
            ca: Some(pem_file("ignored-ca")),
            client_cert: None,
            client_key: None,
        };
        // Certificate variables do not apply to plaintext endpoints.
        assert!(
            build_tls("TRACES", "http://collector:4317", files)
                .unwrap()
                .is_none()
        );

        let files = TlsFiles {
            ca: None,
            client_cert: None,
            client_key: None,
        };
        // System roots when nothing is overridden.
        assert!(
            build_tls("TRACES", "HTTPS://collector:4317", files)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn tls_loads_custom_ca_and_identity() {
        let files = TlsFiles {
            ca: Some(pem_file("ca")),
            client_cert: Some(pem_file("cert")),
            client_key: Some(pem_file("key")),
        };
        assert!(
            build_tls("TRACES", "https://collector:4317", files)
                .unwrap()
                .is_some()
        );

        // A missing file names its path.
        let files = TlsFiles {
            ca: Some("/does/not/exist.pem".to_string()),
            client_cert: None,
            client_key: None,
        };
        let err = build_tls("TRACES", "https://collector:4317", files)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("/does/not/exist.pem"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn tls_rejects_half_configured_mtls() {
        let files = TlsFiles {
            ca: None,
            client_cert: Some(pem_file("lone-cert")),
            client_key: None,
        };
        let err = build_tls("METRICS", "https://collector:4317", files)
            .unwrap_err()
            .to_string();
        assert!(err.contains("METRICS"), "unexpected message: {err}");
        assert!(err.contains("CLIENT_KEY"), "unexpected message: {err}");
    }
}
