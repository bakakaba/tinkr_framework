use std::time::Duration;

use ulid::Ulid;

/// Generates a consistent prefix style unique identifier.
///
/// Returns `{prefix}_{ULID}`, or a bare [ULID](https://github.com/ulid/spec)
/// when `prefix` is empty. Persisted identifiers should always include a
/// prefix.
///
/// # Example
///
/// ```
/// let id = tinkr_framework::utilities::new_id("user");
/// assert!(id.starts_with("user_"));
/// ```
pub fn new_id(prefix: &str) -> String {
    if prefix.is_empty() {
        return Ulid::generate().to_string();
    }

    format!("{}_{}", prefix, Ulid::generate())
}

mod sealed {
    pub trait Sealed {}

    impl Sealed for std::time::Duration {}
}

/// Extension methods for [`std::time::Duration`].
///
/// This trait is sealed and cannot be implemented outside this crate.
pub trait DurationExt: sealed::Sealed {
    /// Format as an [ISO 8601 duration](https://en.wikipedia.org/wiki/ISO_8601#Durations)
    /// string with millisecond precision, e.g. `P2DT3H4M5.123S`.
    ///
    /// Zero components are omitted, matching how `java.time` and JavaScript's
    /// `Temporal.Duration` serialize: `PT5S`, `PT4M`, `P2D`. A zero duration
    /// is `PT0S`.
    ///
    /// # Example
    ///
    /// ```
    /// use std::time::Duration;
    /// use tinkr_framework::utilities::DurationExt;
    ///
    /// assert_eq!(Duration::from_secs(65).to_iso8601(), "PT1M5S");
    /// ```
    fn to_iso8601(&self) -> String;
}

impl DurationExt for Duration {
    fn to_iso8601(&self) -> String {
        let millis = self.subsec_millis();
        let secs = self.as_secs();
        let (days, hours, mins, secs) = (
            secs / 86_400,
            secs % 86_400 / 3_600,
            secs % 3_600 / 60,
            secs % 60,
        );

        let mut out = String::from("P");
        if days > 0 {
            out.push_str(&format!("{days}D"));
        }
        if hours > 0 || mins > 0 || secs > 0 || millis > 0 || days == 0 {
            out.push('T');
            if hours > 0 {
                out.push_str(&format!("{hours}H"));
            }
            if mins > 0 {
                out.push_str(&format!("{mins}M"));
            }
            if millis > 0 {
                out.push_str(&format!("{secs}.{millis:03}S"));
            } else if secs > 0 || (hours == 0 && mins == 0) {
                out.push_str(&format!("{secs}S"));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_prefix() {
        let id = new_id("user");
        assert!(id.starts_with("user_"));
        // "user_" + 26-char ULID
        assert_eq!(id.len(), 5 + 26);
    }

    #[test]
    fn empty_prefix_returns_bare_ulid() {
        let id = new_id("");
        assert_eq!(id.len(), 26);
        assert!(!id.contains('_'));
    }

    #[test]
    fn ids_are_unique() {
        let a = new_id("t");
        let b = new_id("t");
        assert_ne!(a, b);
    }

    #[test]
    fn to_iso8601_formats_durations() {
        for (duration, expected) in [
            (Duration::ZERO, "PT0S"),
            (Duration::from_millis(123), "PT0.123S"),
            (Duration::from_secs(5), "PT5S"),
            (Duration::from_secs(4 * 60), "PT4M"),
            (Duration::from_secs(4 * 60 + 5), "PT4M5S"),
            (Duration::from_secs(3600), "PT1H"),
            (
                Duration::from_millis((3 * 3600 + 4 * 60 + 5) * 1000 + 123),
                "PT3H4M5.123S",
            ),
            (Duration::from_secs(2 * 86_400), "P2D"),
            (
                Duration::from_millis((2 * 86_400 + 3 * 3600 + 4 * 60 + 5) * 1000 + 123),
                "P2DT3H4M5.123S",
            ),
        ] {
            assert_eq!(duration.to_iso8601(), expected, "for {duration:?}");
        }
    }
}
