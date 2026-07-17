//! Standardized health reporting for the built-in `/health` endpoint.
//!
//! Every [`Server`](crate::Server) serves `GET /health`, responding with JSON
//! that identifies the service and its current status:
//!
//! ```json
//! {
//!   "service": "demo",
//!   "version": "0.4.2",
//!   "status": "degraded",
//!   "uptime": "P2DT3H4M5.123S",
//!   "durationMs": 12,
//!   "checks": [
//!     {"name": "database", "status": "error", "message": "connection refused", "durationMs": 4}
//!   ]
//! }
//! ```
//!
//! The HTTP status code is `200` when the overall [`Status`] is ok and `503`
//! otherwise, so load balancers and orchestrator probes work without body
//! inspection.
//!
//! `uptime` is an ISO 8601 duration (see [`DurationExt`]) measured from when
//! [`Server::serve`](crate::Server::serve) started; the top-level
//! `durationMs` is the wall time the whole evaluation took, so it stays
//! accurate when individual checks run in parallel.
//!
//! By default the endpoint reports [`Status::OK`] with no checks. Register a
//! custom evaluation with [`Server::health`](crate::Server::health) to report
//! dependency [`Check`]s and derive the overall status yourself.

use std::borrow::Cow;
use std::time::Duration;

use serde::{Serialize, Serializer};

use crate::utilities::DurationExt;

/// The outcome of a health evaluation, serialized as its name (e.g. `"ok"`).
///
/// The standard set is [`Status::OK`], [`Status::DEGRADED`], and
/// [`Status::ERROR`]. Consumers can extend it with [`Status::new`]:
///
/// ```
/// use tinkr_framework::health::Status;
///
/// // Not serving yet (503), but distinct from an error.
/// const WARMING_UP: Status = Status::new("warming_up", false);
///
/// assert_eq!(WARMING_UP.name(), "warming_up");
/// assert!(!WARMING_UP.is_ok());
/// assert_ne!(WARMING_UP, Status::ERROR);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    name: Cow<'static, str>,
    ok: bool,
}

impl Status {
    /// Serving normally.
    pub const OK: Status = Status::new("ok", true);
    /// Serving, but impaired.
    pub const DEGRADED: Status = Status::new("degraded", true);
    /// Not serving.
    pub const ERROR: Status = Status::new("error", false);

    /// Define a status.
    ///
    /// # Arguments
    ///
    /// - `ok` — whether the endpoint should report `200` (serving) rather
    ///   than `503` when this is the overall status
    pub const fn new(name: &'static str, ok: bool) -> Self {
        Status {
            name: Cow::Borrowed(name),
            ok,
        }
    }

    /// Whether this status counts as serving (`200` rather than `503`).
    pub fn is_ok(&self) -> bool {
        self.ok
    }

    /// The name this status serializes as.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

impl Serialize for Status {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.name)
    }
}

/// The result of checking a single dependency, reported in the `checks`
/// array of the `/health` response.
///
/// `duration` is the time the check took to run and serializes as
/// `durationMs` (whole milliseconds); `message` is omitted when `None`.
///
/// ```
/// use std::time::Duration;
/// use tinkr_framework::health::{Check, Status};
///
/// let check = Check {
///     message: Some("connection refused".into()),
///     duration: Duration::from_millis(4),
///     ..Check::new("database", Status::ERROR)
/// };
///
/// assert_eq!(
///     serde_json::to_value(&check).unwrap(),
///     serde_json::json!({
///         "name": "database",
///         "status": "error",
///         "message": "connection refused",
///         "durationMs": 4,
///     }),
/// );
/// ```
#[derive(Clone, Debug, Serialize)]
pub struct Check {
    pub name: String,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(rename = "durationMs", serialize_with = "duration_as_millis")]
    pub duration: Duration,
}

impl Check {
    /// Create a check with no message and zero duration.
    pub fn new(name: impl Into<String>, status: Status) -> Self {
        Check {
            name: name.into(),
            status,
            message: None,
            duration: Duration::ZERO,
        }
    }
}

fn duration_as_millis<S: Serializer>(d: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_u64(u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// A health report: the overall [`Status`] plus the individual [`Check`]s it
/// was derived from.
///
/// Returned by the function registered with
/// [`Server::health`](crate::Server::health); the overall `status` decides
/// the endpoint's HTTP status code.
#[derive(Clone, Debug)]
pub struct Health {
    pub status: Status,
    pub checks: Vec<Check>,
}

impl Health {
    /// Create a report with no checks.
    pub fn new(status: Status) -> Self {
        Health {
            status,
            checks: Vec::new(),
        }
    }
}

/// Render the full `/health` response for a report, stamping in the service
/// identity given to `Server::new`, the uptime since serve start, and the
/// wall time the evaluation took.
pub(crate) fn response(
    service: &str,
    version: &str,
    uptime: Duration,
    duration: Duration,
    health: &Health,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    #[derive(Serialize)]
    struct Body<'a> {
        service: &'a str,
        version: &'a str,
        status: &'a Status,
        uptime: String,
        #[serde(rename = "durationMs", serialize_with = "duration_as_millis")]
        duration: Duration,
        #[serde(skip_serializing_if = "<[Check]>::is_empty")]
        checks: &'a [Check],
    }

    let code = if health.status.is_ok() {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    };

    (
        code,
        axum::Json(Body {
            service,
            version,
            status: &health.status,
            uptime: uptime.to_iso8601(),
            duration,
            checks: &health.checks,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_statuses_serialize_as_names() {
        for (status, name) in [
            (Status::OK, "ok"),
            (Status::DEGRADED, "degraded"),
            (Status::ERROR, "error"),
        ] {
            assert_eq!(serde_json::to_value(&status).unwrap(), name);
        }
    }

    #[test]
    fn degraded_still_serves() {
        assert!(Status::OK.is_ok());
        assert!(Status::DEGRADED.is_ok());
        assert!(!Status::ERROR.is_ok());
    }

    #[test]
    fn check_omits_empty_message() {
        let value = serde_json::to_value(Check::new("cache", Status::OK)).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"name": "cache", "status": "ok", "durationMs": 0}),
        );
    }
}
