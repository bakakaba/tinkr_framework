//! Schema description of a configuration shape, used to generate the JSON
//! Schema and the starter config file.
//!
//! The types here are produced by `#[derive(Configurable)]`; render them
//! with [`crate::schema()`](crate::schema()), [`crate::write_schema`], and
//! [`crate::template`].

use serde_json::{Map, Value, json};

use crate::Configurable;
use crate::base;

/// The schema of a single value.
#[derive(Debug, Clone)]
pub enum Node {
    /// A boolean.
    Boolean,
    /// An integer.
    Integer,
    /// A floating-point number.
    Number,
    /// A string (or string-like value such as a path or address).
    String,
    /// An array of homogeneous items.
    Array(Box<Node>),
    /// A table of named properties (a nested configuration struct).
    Object(Vec<Property>),
    /// A value with no schema constraints.
    Any,
}

impl Node {
    /// A table node; used by generated code.
    #[doc(hidden)]
    pub fn object(properties: Vec<Property>) -> Node {
        Node::Object(properties)
    }
}

/// One named field of an [`Node::Object`].
#[derive(Debug, Clone)]
pub struct Property {
    /// The TOML key.
    pub name: &'static str,
    /// Doc comment of the field.
    pub description: Option<&'static str>,
    /// Whether the file must provide the value (no default, not `Option`,
    /// no environment variable).
    pub required: bool,
    /// The declared default value, when representable.
    pub default: Option<Value>,
    /// The environment variable that overrides the field, if any.
    pub env: Option<&'static str>,
    /// The value's schema.
    pub node: Node,
}

/// Types that can describe themselves in a configuration schema.
///
/// Implemented for the primitives, `String`, paths, IP addresses,
/// `Vec<T>`, and `Option<T>`. Implement it for custom field types used in a
/// [`Configurable`] struct.
pub trait ToSchema {
    /// The schema of this type.
    fn node() -> Node;
}

macro_rules! impl_to_schema {
    ($node:ident => $($ty:ty),+ $(,)?) => {
        $(impl ToSchema for $ty {
            fn node() -> Node {
                Node::$node
            }
        })+
    };
}

impl_to_schema!(Boolean => bool);
impl_to_schema!(Integer => u8, u16, u32, u64, usize, i8, i16, i32, i64, isize);
impl_to_schema!(Number => f32, f64);
impl_to_schema!(String =>
    String,
    char,
    std::path::PathBuf,
    std::net::IpAddr,
    std::net::Ipv4Addr,
    std::net::Ipv6Addr,
    std::net::SocketAddr,
);

impl<T: ToSchema> ToSchema for Vec<T> {
    fn node() -> Node {
        Node::Array(Box::new(T::node()))
    }
}

impl<T: ToSchema> ToSchema for Option<T> {
    fn node() -> Node {
        T::node()
    }
}

/// All top-level properties: base fields first, then the application's.
fn all_properties<T: Configurable>() -> Vec<Property> {
    let mut properties = base::properties();
    if let Node::Object(app) = T::schema_node() {
        properties.extend(app);
    }
    properties
}

pub(crate) fn render<T: Configurable>() -> String {
    let mut root = object_json(&all_properties::<T>());
    let map = root.as_object_mut().expect("object_json returns an object");
    map.insert(
        "$schema".to_string(),
        json!("http://json-schema.org/draft-07/schema#"),
    );
    map.insert("title".to_string(), json!(short_type_name::<T>()));
    let doc = T::doc();
    if !doc.is_empty() {
        map.insert("description".to_string(), json!(doc));
    }
    serde_json::to_string_pretty(&root).expect("schema serializes to JSON")
}

fn object_json(properties: &[Property]) -> Value {
    let mut props = Map::new();
    let mut required = Vec::new();
    for property in properties {
        props.insert(property.name.to_string(), property_json(property));
        if property.required {
            required.push(property.name);
        }
    }
    let mut object = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": props,
    });
    if !required.is_empty() {
        object["required"] = json!(required);
    }
    object
}

fn property_json(property: &Property) -> Value {
    let mut value = node_json(&property.node);
    if let Some(description) = describe(property) {
        value["description"] = json!(description);
    }
    if let Some(default) = &property.default {
        value["default"] = default.clone();
    }
    value
}

fn node_json(node: &Node) -> Value {
    match node {
        Node::Boolean => json!({"type": "boolean"}),
        Node::Integer => json!({"type": "integer"}),
        Node::Number => json!({"type": "number"}),
        Node::String => json!({"type": "string"}),
        Node::Array(item) => json!({"type": "array", "items": node_json(item)}),
        Node::Object(properties) => object_json(properties),
        Node::Any => json!({}),
    }
}

/// The property's description with its env override appended.
fn describe(property: &Property) -> Option<String> {
    match (property.description, property.env) {
        (Some(description), Some(var)) => Some(format!("{description} (env: {var})")),
        (Some(description), None) => Some(description.to_string()),
        (None, Some(var)) => Some(format!("(env: {var})")),
        (None, None) => None,
    }
}

fn short_type_name<T>() -> &'static str {
    let name = std::any::type_name::<T>();
    name.rsplit("::").next().unwrap_or(name)
}

pub(crate) fn render_template<T: Configurable>() -> String {
    let mut out = String::from("#:schema ./config.schema.json\n");
    let doc = T::doc();
    if !doc.is_empty() {
        out.push('\n');
        for line in doc.lines() {
            comment_line(&mut out, line);
        }
    }
    emit_table(&mut out, &all_properties::<T>(), "");
    out
}

/// Writes a table's scalar entries, then its sub-tables (TOML requires that
/// order).
fn emit_table(out: &mut String, properties: &[Property], path: &str) {
    for property in properties {
        if matches!(property.node, Node::Object(_)) {
            continue;
        }
        out.push('\n');
        if let Some(description) = describe(property) {
            for line in description.lines() {
                comment_line(out, line);
            }
        }
        let value = property
            .default
            .as_ref()
            .map(toml_value)
            .unwrap_or_else(|| placeholder(&property.node));
        if property.required {
            out.push_str(&format!("{} = {}\n", property.name, value));
        } else {
            out.push_str(&format!("#{} = {}\n", property.name, value));
        }
    }
    for property in properties {
        let Node::Object(children) = &property.node else {
            continue;
        };
        let child_path = if path.is_empty() {
            property.name.to_string()
        } else {
            format!("{path}.{}", property.name)
        };
        out.push('\n');
        if let Some(description) = describe(property) {
            for line in description.lines() {
                comment_line(out, line);
            }
        }
        out.push_str(&format!("[{child_path}]\n"));
        emit_table(out, children, &child_path);
    }
}

fn comment_line(out: &mut String, line: &str) {
    if line.is_empty() {
        out.push_str("#\n");
    } else {
        out.push_str(&format!("# {line}\n"));
    }
}

/// Renders a JSON value as a TOML literal.
fn toml_value(value: &Value) -> String {
    match value {
        Value::String(s) => format!("{s:?}"),
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(toml_value).collect();
            format!("[{}]", items.join(", "))
        }
        other => other.to_string(),
    }
}

/// A syntactically valid TOML value for fields with no default.
fn placeholder(node: &Node) -> String {
    match node {
        Node::Boolean => "false".to_string(),
        Node::Integer => "0".to_string(),
        Node::Number => "0.0".to_string(),
        Node::Array(_) => "[]".to_string(),
        _ => "\"\"".to_string(),
    }
}
