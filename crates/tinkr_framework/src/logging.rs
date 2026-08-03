//! Structured JSON log output for deployed services.
//!
//! One line per event, shaped so common log backends parse it natively:
//! `severity`/`time`/`message` plus the event's fields, the active span
//! chain, and — when the `otel` feature exports traces — trace-correlation
//! fields in both the Google Cloud Logging form
//! (`logging.googleapis.com/trace`) and the generic form (`trace_id`).

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value, json};
use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields, FormattedFields};
use tracing_subscriber::registry::LookupSpan;

/// The deployed log layer: single-line JSON events (see [`CloudJson`]),
/// with span fields captured as JSON for the `spans` readout.
pub(crate) fn layer<S>() -> impl tracing_subscriber::Layer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    tracing_subscriber::fmt::layer()
        .fmt_fields(tracing_subscriber::fmt::format::JsonFields::new())
        .event_format(CloudJson)
}

/// Event formatter producing one JSON object per line.
struct CloudJson;

impl<S, N> FormatEvent<S, N> for CloudJson
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();

        let mut visitor = Visitor::default();
        event.record(&mut visitor);

        // Event fields first: the fixed keys below win on collision.
        let mut entry = visitor.fields;

        // The span chain, root first, each with its captured fields.
        let mut spans = Vec::new();
        if let Some(scope) = ctx.event_scope() {
            for span in scope.from_root() {
                let mut fields = Map::new();
                if let Some(formatted) = span.extensions().get::<FormattedFields<N>>()
                    && let Ok(Value::Object(map)) = serde_json::from_str(formatted.fields.as_str())
                {
                    fields = map;
                }
                fields.insert("name".into(), span.name().into());
                spans.push(Value::Object(fields));
            }
        }
        if !spans.is_empty() {
            entry.insert("spans".into(), spans.into());
        }

        entry.insert("severity".into(), severity(meta.level()).into());
        entry.insert("time".into(), rfc3339(SystemTime::now()).into());
        entry.insert("message".into(), visitor.message.unwrap_or_default().into());
        entry.insert("target".into(), meta.target().into());
        if let (Some(file), Some(line)) = (meta.file(), meta.line()) {
            entry.insert(
                "logging.googleapis.com/sourceLocation".into(),
                json!({ "file": file, "line": line.to_string() }),
            );
        }

        #[cfg(feature = "otel")]
        correlate_trace(&mut entry);

        writer.write_str(&Value::Object(entry).to_string())?;
        writeln!(writer)
    }
}

/// Adds trace-correlation fields from the current span's OpenTelemetry
/// context, in both the Cloud Logging and generic forms. No-op when the
/// event is outside a span or trace export is not active.
#[cfg(feature = "otel")]
fn correlate_trace(entry: &mut Map<String, Value>) {
    use opentelemetry::trace::TraceContextExt;
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    let context = tracing::Span::current().context();
    let span = context.span();
    let span_context = span.span_context();
    if !span_context.is_valid() {
        return;
    }

    let trace_id = span_context.trace_id().to_string();
    let span_id = span_context.span_id().to_string();
    entry.insert(
        "logging.googleapis.com/trace".into(),
        trace_id.clone().into(),
    );
    entry.insert(
        "logging.googleapis.com/spanId".into(),
        span_id.clone().into(),
    );
    entry.insert(
        "logging.googleapis.com/trace_sampled".into(),
        span_context.is_sampled().into(),
    );
    entry.insert("trace_id".into(), trace_id.into());
    entry.insert("span_id".into(), span_id.into());
}

/// Maps a tracing level onto a Cloud Logging `severity` name (`WARNING`
/// rather than `WARN`; `TRACE` folds into `DEBUG`).
fn severity(level: &tracing::Level) -> &'static str {
    match *level {
        tracing::Level::TRACE | tracing::Level::DEBUG => "DEBUG",
        tracing::Level::INFO => "INFO",
        tracing::Level::WARN => "WARNING",
        tracing::Level::ERROR => "ERROR",
    }
}

/// Collects an event's fields into a JSON map, separating out `message`.
#[derive(Default)]
struct Visitor {
    message: Option<String>,
    fields: Map<String, Value>,
}

impl tracing::field::Visit for Visitor {
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.fields.insert(field.name().into(), value.into());
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields.insert(field.name().into(), value.into());
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields.insert(field.name().into(), value.into());
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.fields.insert(field.name().into(), value.into());
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.fields.insert(field.name().into(), value.into());
        }
    }

    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        self.fields
            .insert(field.name().into(), value.to_string().into());
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        } else {
            self.fields
                .insert(field.name().into(), format!("{value:?}").into());
        }
    }
}

/// Formats a timestamp as RFC 3339 in UTC with microsecond precision.
fn rfc3339(now: SystemTime) -> String {
    let elapsed = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = elapsed.as_secs();
    let micros = elapsed.subsec_micros();
    let (hour, minute, second) = (seconds / 3600 % 24, seconds / 60 % 60, seconds % 60);
    let (year, month, day) = civil_from_days((seconds / 86400) as i64);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{micros:06}Z")
}

/// Converts days since the Unix epoch to a calendar date.
// Howard Hinnant's `civil_from_days` algorithm:
// https://howardhinnant.github.io/date_algorithms.html#civil_from_days
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    /// A `MakeWriter` capturing output for assertions.
    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Capture {
        fn lines(&self) -> Vec<Value> {
            let bytes = self.0.lock().unwrap();
            String::from_utf8(bytes.clone())
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).expect("invalid JSON log line"))
                .collect()
        }
    }

    impl std::io::Write for Capture {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
        type Writer = Capture;

        fn make_writer(&'a self) -> Capture {
            self.clone()
        }
    }

    fn subscriber(capture: Capture) -> impl Subscriber {
        use tracing_subscriber::layer::SubscriberExt;

        tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .fmt_fields(tracing_subscriber::fmt::format::JsonFields::new())
                .event_format(CloudJson)
                .with_writer(capture),
        )
    }

    #[test]
    fn events_render_as_single_line_json() {
        let capture = Capture::default();
        tracing::subscriber::with_default(subscriber(capture.clone()), || {
            tracing::info!(user = "ada", attempts = 3, "logged in");
        });

        let lines = capture.lines();
        assert_eq!(lines.len(), 1);
        let entry = &lines[0];
        assert_eq!(entry["severity"], "INFO");
        assert_eq!(entry["message"], "logged in");
        assert_eq!(entry["user"], "ada");
        assert_eq!(entry["attempts"], 3);
        assert!(entry["logging.googleapis.com/sourceLocation"]["file"].is_string());
        assert!(entry["time"].as_str().unwrap().ends_with('Z'));
    }

    #[test]
    fn severities_map_to_cloud_logging_names() {
        let capture = Capture::default();
        tracing::subscriber::with_default(subscriber(capture.clone()), || {
            tracing::trace!("t");
            tracing::debug!("d");
            tracing::info!("i");
            tracing::warn!("w");
            tracing::error!("e");
        });

        let severities: Vec<String> = capture
            .lines()
            .iter()
            .map(|entry| entry["severity"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(severities, ["DEBUG", "DEBUG", "INFO", "WARNING", "ERROR"]);
    }

    #[test]
    fn span_chain_and_fields_are_captured() {
        let capture = Capture::default();
        tracing::subscriber::with_default(subscriber(capture.clone()), || {
            let outer = tracing::info_span!("request", method = "GET");
            let _outer = outer.enter();
            let inner = tracing::info_span!("query", table = "users");
            let _inner = inner.enter();
            tracing::info!("fetching");
        });

        let lines = capture.lines();
        let spans = lines[0]["spans"].as_array().unwrap();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0]["name"], "request");
        assert_eq!(spans[0]["method"], "GET");
        assert_eq!(spans[1]["name"], "query");
        assert_eq!(spans[1]["table"], "users");
    }

    #[test]
    fn events_outside_spans_have_no_trace_fields() {
        let capture = Capture::default();
        tracing::subscriber::with_default(subscriber(capture.clone()), || {
            tracing::info!("hello");
        });

        let entry = &capture.lines()[0];
        assert!(entry.get("trace_id").is_none());
        assert!(entry.get("logging.googleapis.com/trace").is_none());
    }

    #[test]
    fn timestamps_render_rfc3339() {
        let time = rfc3339(UNIX_EPOCH + std::time::Duration::new(1_755_000_000, 123_456_000));
        assert_eq!(time, "2025-08-12T12:00:00.123456Z");

        let epoch = rfc3339(UNIX_EPOCH);
        assert_eq!(epoch, "1970-01-01T00:00:00.000000Z");

        // Leap-year day.
        let leap = rfc3339(UNIX_EPOCH + std::time::Duration::from_secs(1_709_164_800));
        assert!(leap.starts_with("2024-02-29T"), "got {leap}");
    }
}
