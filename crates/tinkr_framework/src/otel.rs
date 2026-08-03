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

use std::env;
use std::sync::OnceLock;

use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::WithExportConfig;
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

    if let Some(endpoint) = traces {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()
            .map_err(|e| Error::Otel(e.to_string()))?;
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
        let exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()
            .map_err(|e| Error::Otel(e.to_string()))?;
        let provider = SdkMeterProvider::builder()
            .with_resource(resource.clone())
            .with_periodic_exporter(exporter)
            .build();
        global::set_meter_provider(provider.clone());
        providers.meter = Some(provider);
    }

    if let Some(endpoint) = logs {
        let exporter = opentelemetry_otlp::LogExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()
            .map_err(|e| Error::Otel(e.to_string()))?;
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
}
