//! Request metrics following the OpenTelemetry HTTP semantic conventions:
//! `http.server.request.duration` and `http.server.active_requests`,
//! recorded for HTTP and gRPC requests alike.
//!
//! gRPC responses report their real outcome: `grpc-status` arrives in the
//! response trailers (or in the headers, for trailers-only error responses),
//! so recording waits for the response body to finish instead of trusting
//! the HTTP status line — streaming calls and late failures are not
//! misreported as successes.

use std::pin::Pin;
use std::sync::OnceLock;
use std::task::{Context, Poll};
use std::time::Instant;

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, Method, header};
use opentelemetry::KeyValue;
use opentelemetry::metrics::{Histogram, Meter, UpDownCounter};

/// Semconv bucket boundaries for `http.server.request.duration`, in seconds.
const DURATION_BOUNDARIES: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0,
];

/// `grpc-status` code for UNIMPLEMENTED.
const GRPC_UNIMPLEMENTED: i64 = 12;

/// The request instruments, created once when metric export activates.
static INSTRUMENTS: OnceLock<Instruments> = OnceLock::new();

struct Instruments {
    duration: Histogram<f64>,
    active: UpDownCounter<i64>,
}

/// Creates the request instruments on `meter`. Called once from
/// [`super::init`] when metric export is active.
pub(crate) fn init(meter: &Meter) {
    let _ = INSTRUMENTS.set(Instruments {
        duration: meter
            .f64_histogram("http.server.request.duration")
            .with_unit("s")
            .with_description("Duration of HTTP server requests.")
            .with_boundaries(DURATION_BOUNDARIES.to_vec())
            .build(),
        active: meter
            .i64_up_down_counter("http.server.active_requests")
            .with_unit("{request}")
            .with_description("Number of active HTTP server requests.")
            .build(),
    });
}

/// Whether the request instruments are installed (gates the middleware).
pub(crate) fn active() -> bool {
    INSTRUMENTS.get().is_some()
}

/// Records duration and in-flight metrics for every request; applied to the
/// merged router when metric export is active.
pub(crate) async fn track_request(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let Some(instruments) = INSTRUMENTS.get() else {
        return next.run(req).await;
    };

    let start = Instant::now();
    let method = method_label(req.method());
    let scheme = req
        .uri()
        .scheme_str()
        .unwrap_or("http") // origin-form request targets carry no scheme
        .to_owned();
    let grpc = is_grpc(req.headers());
    let path = req.uri().path().to_owned();
    let route = req
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|matched| matched.as_str().to_owned());

    let active_attrs = [
        KeyValue::new("http.request.method", method),
        KeyValue::new("url.scheme", scheme.clone()),
    ];
    instruments.active.add(1, &active_attrs);

    let response = next.run(req).await;

    let state = State {
        start,
        method,
        scheme,
        grpc,
        path,
        route,
        status: response.status().as_u16(),
        // Trailers-only gRPC responses put grpc-status in the headers;
        // regular ones override this from the trailers below.
        grpc_status: grpc_status(response.headers()),
        active_attrs,
    };
    let (parts, body) = response.into_parts();
    axum::response::Response::from_parts(
        parts,
        Body::new(Tracked {
            inner: body,
            state: Some(state),
        }),
    )
}

/// Everything needed to record a finished request.
struct State {
    start: Instant,
    method: &'static str,
    scheme: String,
    grpc: bool,
    path: String,
    route: Option<String>,
    status: u16,
    grpc_status: Option<i64>,
    active_attrs: [KeyValue; 2],
}

/// Records the request once its response has fully completed.
fn record(state: State) {
    // Installed whenever the middleware is: `track_request` only builds a
    // `State` after finding the instruments.
    let Some(instruments) = INSTRUMENTS.get() else {
        return;
    };
    let elapsed = state.start.elapsed().as_secs_f64();
    let attrs = attributes(&state);
    instruments.duration.record(elapsed, &attrs);
    instruments.active.add(-1, &state.active_attrs);
}

/// Assembles the semconv attribute set for the duration histogram.
fn attributes(state: &State) -> Vec<KeyValue> {
    let mut attrs = vec![
        KeyValue::new("http.request.method", state.method),
        KeyValue::new("url.scheme", state.scheme.clone()),
        KeyValue::new("http.response.status_code", i64::from(state.status)),
    ];
    let mut route = state.route.clone();

    if state.grpc {
        attrs.push(KeyValue::new("rpc.system", "grpc"));
        // Name the RPC only for requests that matched a registered service
        // (tonic routes are `/{Service}/{*rest}`), and keep the wildcard
        // route for UNIMPLEMENTED responses: unknown-method probes collapse
        // into one series instead of minting unbounded attribute sets.
        if state.route.is_some()
            && let Some((service, rpc_method)) = rpc_names(&state.path)
        {
            attrs.push(KeyValue::new("rpc.service", service.to_owned()));
            if state.grpc_status != Some(GRPC_UNIMPLEMENTED) {
                attrs.push(KeyValue::new("rpc.method", rpc_method.to_owned()));
                route = Some(state.path.clone());
            }
        }
    }

    match (state.grpc, state.grpc_status) {
        (true, Some(code)) => {
            attrs.push(KeyValue::new("rpc.grpc.status_code", code));
            if code != 0 {
                attrs.push(KeyValue::new("error.type", code.to_string()));
            }
        }
        _ if state.status >= 500 => {
            attrs.push(KeyValue::new("error.type", state.status.to_string()));
        }
        _ => {}
    }

    if let Some(route) = route {
        attrs.push(KeyValue::new("http.route", route));
    }
    attrs
}

/// Splits a gRPC request path (`/{package.Service}/{Method}`) into the
/// `rpc.service` and `rpc.method` values.
fn rpc_names(path: &str) -> Option<(&str, &str)> {
    let (service, method) = path.strip_prefix('/')?.split_once('/')?;
    (!service.is_empty() && !method.is_empty() && !method.contains('/'))
        .then_some((service, method))
}

/// The semconv `http.request.method` value: a known method, or `_OTHER`.
fn method_label(method: &Method) -> &'static str {
    match *method {
        Method::GET => "GET",
        Method::HEAD => "HEAD",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::DELETE => "DELETE",
        Method::CONNECT => "CONNECT",
        Method::OPTIONS => "OPTIONS",
        Method::TRACE => "TRACE",
        Method::PATCH => "PATCH",
        _ => "_OTHER",
    }
}

/// Whether the request speaks gRPC, per its content type.
fn is_grpc(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/grpc"))
}

/// Parses a `grpc-status` code out of headers or trailers.
fn grpc_status(headers: &HeaderMap) -> Option<i64> {
    headers.get("grpc-status")?.to_str().ok()?.parse().ok()
}

/// A response body that records the request metrics exactly once, when the
/// body finishes — where gRPC trailers carry the final `grpc-status`.
struct Tracked {
    inner: Body,
    /// Present until recorded; dropped-early bodies (client disconnects)
    /// record through `Drop`.
    state: Option<State>,
}

impl http_body::Body for Tracked {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        let this = self.get_mut();
        match std::task::ready!(Pin::new(&mut this.inner).poll_frame(cx)) {
            Some(Ok(frame)) => {
                if let Some(trailers) = frame.trailers_ref()
                    && let Some(code) = grpc_status(trailers)
                    && let Some(state) = this.state.as_mut()
                {
                    state.grpc_status = Some(code);
                }
                Poll::Ready(Some(Ok(frame)))
            }
            Some(Err(error)) => {
                if let Some(state) = this.state.take() {
                    record(state);
                }
                Poll::Ready(Some(Err(error)))
            }
            None => {
                if let Some(state) = this.state.take() {
                    record(state);
                }
                Poll::Ready(None)
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        http_body::Body::size_hint(&self.inner)
    }
}

impl Drop for Tracked {
    fn drop(&mut self) {
        if let Some(state) = self.state.take() {
            record(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use axum::http::StatusCode;
    use axum::routing::{get, post};
    use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData};
    use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};
    use tower::ServiceExt;

    use super::*;

    fn state(grpc: bool, path: &str, route: Option<&str>, status: u16) -> State {
        State {
            start: Instant::now(),
            method: "POST",
            scheme: "http".into(),
            grpc,
            path: path.into(),
            route: route.map(str::to_owned),
            status,
            grpc_status: None,
            active_attrs: [
                KeyValue::new("http.request.method", "POST"),
                KeyValue::new("url.scheme", "http"),
            ],
        }
    }

    fn attr(attrs: &[KeyValue], key: &str) -> Option<String> {
        attrs
            .iter()
            .find(|kv| kv.key.as_str() == key)
            .map(|kv| kv.value.to_string())
    }

    #[test]
    fn rpc_names_splits_grpc_paths() {
        assert_eq!(
            rpc_names("/hello.Greeter/SayHello"),
            Some(("hello.Greeter", "SayHello"))
        );
        assert_eq!(rpc_names("/no-method"), None);
        assert_eq!(rpc_names("/svc//"), None);
        assert_eq!(rpc_names("//method"), None);
        assert_eq!(rpc_names("/a/b/c"), None);
    }

    #[test]
    fn method_label_normalizes_unknown_methods() {
        assert_eq!(method_label(&Method::GET), "GET");
        let unusual = Method::from_bytes(b"PROPFIND").unwrap();
        assert_eq!(method_label(&unusual), "_OTHER");
    }

    #[test]
    fn grpc_status_parses_headers() {
        let mut headers = HeaderMap::new();
        assert_eq!(grpc_status(&headers), None);
        headers.insert("grpc-status", "12".parse().unwrap());
        assert_eq!(grpc_status(&headers), Some(12));
    }

    #[test]
    fn http_attributes_carry_route_and_errors() {
        let mut ok = state(false, "/things/42", Some("/things/{id}"), 200);
        ok.method = "GET";
        let attrs = attributes(&ok);
        assert_eq!(attr(&attrs, "http.route").as_deref(), Some("/things/{id}"));
        assert_eq!(
            attr(&attrs, "http.response.status_code").as_deref(),
            Some("200")
        );
        assert_eq!(attr(&attrs, "error.type"), None);
        assert_eq!(attr(&attrs, "rpc.system"), None);

        let attrs = attributes(&state(false, "/things/42", Some("/things/{id}"), 503));
        assert_eq!(attr(&attrs, "error.type").as_deref(), Some("503"));

        // Unmatched paths must not become attributes (unbounded cardinality).
        let attrs = attributes(&state(false, "/anything-goes", None, 404));
        assert_eq!(attr(&attrs, "http.route"), None);
    }

    #[test]
    fn grpc_attributes_use_real_method_and_status() {
        let mut call = state(
            true,
            "/hello.Greeter/SayHello",
            Some("/hello.Greeter/{*rest}"),
            200,
        );
        call.grpc_status = Some(0);
        let attrs = attributes(&call);
        assert_eq!(attr(&attrs, "rpc.system").as_deref(), Some("grpc"));
        assert_eq!(
            attr(&attrs, "rpc.service").as_deref(),
            Some("hello.Greeter")
        );
        assert_eq!(attr(&attrs, "rpc.method").as_deref(), Some("SayHello"));
        assert_eq!(attr(&attrs, "rpc.grpc.status_code").as_deref(), Some("0"));
        assert_eq!(
            attr(&attrs, "http.route").as_deref(),
            Some("/hello.Greeter/SayHello")
        );
        assert_eq!(attr(&attrs, "error.type"), None);

        // Failed calls surface the grpc status, not the HTTP 200.
        let mut failed = state(
            true,
            "/hello.Greeter/SayHello",
            Some("/hello.Greeter/{*rest}"),
            200,
        );
        failed.grpc_status = Some(3);
        let attrs = attributes(&failed);
        assert_eq!(attr(&attrs, "error.type").as_deref(), Some("3"));

        // Unknown-method probes collapse into the wildcard route.
        let mut probe = state(
            true,
            "/hello.Greeter/Fuzzed",
            Some("/hello.Greeter/{*rest}"),
            200,
        );
        probe.grpc_status = Some(GRPC_UNIMPLEMENTED);
        let attrs = attributes(&probe);
        assert_eq!(attr(&attrs, "rpc.method"), None);
        assert_eq!(
            attr(&attrs, "http.route").as_deref(),
            Some("/hello.Greeter/{*rest}")
        );
        assert_eq!(attr(&attrs, "error.type").as_deref(), Some("12"));

        // gRPC to an unmatched path: no rpc naming at all.
        let attrs = attributes(&state(true, "/not.Registered/Method", None, 404));
        assert_eq!(attr(&attrs, "rpc.service"), None);
        assert_eq!(attr(&attrs, "http.route"), None);
    }

    /// A body emitting one data frame and then gRPC trailers, like tonic.
    struct TrailerBody {
        data: Option<Bytes>,
        trailers: Option<HeaderMap>,
    }

    impl http_body::Body for TrailerBody {
        type Data = Bytes;
        type Error = std::convert::Infallible;

        fn poll_frame(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
            let this = self.get_mut();
            if let Some(data) = this.data.take() {
                return Poll::Ready(Some(Ok(http_body::Frame::data(data))));
            }
            if let Some(trailers) = this.trailers.take() {
                return Poll::Ready(Some(Ok(http_body::Frame::trailers(trailers))));
            }
            Poll::Ready(None)
        }
    }

    fn grpc_response(status: i64) -> axum::response::Response {
        let mut trailers = HeaderMap::new();
        trailers.insert("grpc-status", status.to_string().parse().unwrap());
        axum::response::Response::builder()
            .header(header::CONTENT_TYPE, "application/grpc")
            .body(Body::new(TrailerBody {
                data: Some(Bytes::from_static(b"payload")),
                trailers: Some(trailers),
            }))
            .unwrap()
    }

    /// One end-to-end test: the instruments are process-global (`OnceLock`),
    /// so all middleware assertions share this function.
    #[tokio::test]
    async fn middleware_records_http_and_grpc_requests() {
        let exporter = InMemoryMetricExporter::default();
        let provider = SdkMeterProvider::builder()
            .with_reader(PeriodicReader::builder(exporter.clone()).build())
            .build();
        use opentelemetry::metrics::MeterProvider as _;
        init(&provider.meter("test"));
        assert!(active());

        let app = axum::Router::new()
            .route("/ok", get(|| async { "ok" }))
            .route(
                "/fail",
                get(|| async { (StatusCode::INTERNAL_SERVER_ERROR, "boom") }),
            )
            .route(
                "/test.Svc/{*rest}",
                post(|req: axum::extract::Request| async move {
                    match req.uri().path() {
                        "/test.Svc/Ok" => grpc_response(0),
                        "/test.Svc/Fuzzed" => grpc_response(12),
                        _ => grpc_response(3),
                    }
                }),
            )
            .layer(axum::middleware::from_fn(track_request));

        let http = |method: Method, path: &str, grpc: bool| {
            let mut request = axum::http::Request::builder().method(method).uri(path);
            if grpc {
                request = request.header(header::CONTENT_TYPE, "application/grpc");
            }
            request.body(Body::empty()).unwrap()
        };
        for request in [
            http(Method::GET, "/ok", false),
            http(Method::GET, "/fail", false),
            http(Method::POST, "/test.Svc/Ok", true),
            http(Method::POST, "/test.Svc/Err", true),
            http(Method::POST, "/test.Svc/Fuzzed", true),
        ] {
            let response = app.clone().oneshot(request).await.unwrap();
            // Reading the body to completion consumes the trailers and
            // triggers the recording.
            axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
        }

        provider.force_flush().unwrap();
        let (durations, active_sum) = collect(&exporter);

        let by_route = |route: &str| {
            durations
                .iter()
                .find(|attrs| attrs.get("http.route").map(String::as_str) == Some(route))
                .unwrap_or_else(|| panic!("no data point for route {route}: {durations:?}"))
        };

        let ok = by_route("/ok");
        assert_eq!(ok.get("http.response.status_code").unwrap(), "200");
        assert!(!ok.contains_key("error.type"));

        let fail = by_route("/fail");
        assert_eq!(fail.get("error.type").unwrap(), "500");

        let grpc_ok = by_route("/test.Svc/Ok");
        assert_eq!(grpc_ok.get("rpc.service").unwrap(), "test.Svc");
        assert_eq!(grpc_ok.get("rpc.method").unwrap(), "Ok");
        assert_eq!(grpc_ok.get("rpc.grpc.status_code").unwrap(), "0");
        assert!(!grpc_ok.contains_key("error.type"));

        let grpc_err = by_route("/test.Svc/Err");
        assert_eq!(grpc_err.get("error.type").unwrap(), "3");
        assert_eq!(grpc_err.get("http.response.status_code").unwrap(), "200");

        let probe = by_route("/test.Svc/{*rest}");
        assert_eq!(probe.get("error.type").unwrap(), "12");
        assert!(!probe.contains_key("rpc.method"));

        // Every request finished: in-flight counts cancel out.
        assert_eq!(active_sum, 0);
    }

    /// Flattens the exported metrics into duration data point attribute maps
    /// and the summed `active_requests` value.
    fn collect(exporter: &InMemoryMetricExporter) -> (Vec<HashMap<String, String>>, i64) {
        let mut durations = Vec::new();
        let mut active_sum = 0;
        for resource_metrics in exporter.get_finished_metrics().unwrap() {
            for scope in resource_metrics.scope_metrics() {
                for metric in scope.metrics() {
                    match (metric.name(), metric.data()) {
                        (
                            "http.server.request.duration",
                            AggregatedMetrics::F64(MetricData::Histogram(histogram)),
                        ) => durations.extend(histogram.data_points().map(|point| {
                            point
                                .attributes()
                                .map(|kv| (kv.key.to_string(), kv.value.to_string()))
                                .collect::<HashMap<_, _>>()
                        })),
                        (
                            "http.server.active_requests",
                            AggregatedMetrics::I64(MetricData::Sum(sum)),
                        ) => {
                            active_sum += sum.data_points().map(|point| point.value()).sum::<i64>()
                        }
                        _ => {}
                    }
                }
            }
        }
        (durations, active_sum)
    }
}
