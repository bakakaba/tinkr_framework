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
        return Ulid::r#gen().to_string();
    }

    format!("{}_{}", prefix, Ulid::r#gen())
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
}
