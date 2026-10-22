//! Reading the JSON document: lookups where `null` counts as absent.

use serde_json::Value;

/// `null` counts as absent everywhere.
pub(super) fn get<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.get(key).filter(|x| !x.is_null())
}

pub(super) static NULL: Value = Value::Null;

/// `key`'s object, or `null` when it is absent (every lookup in it then finds nothing).
pub(super) fn obj<'a>(v: &'a Value, key: &str) -> &'a Value {
    get(v, key).unwrap_or(&NULL)
}

/// A declared layer's id (1–255; 0 is the base layer).
pub(super) fn layer_id(v: &Value, key: &str) -> Option<i64> {
    int(v, key).filter(|id| (1..=255).contains(id))
}

/// The layer a mapping is in or jumps to: the base layer when absent, clamped to 0–255.
pub(super) fn target_layer(v: &Value, key: &str) -> u8 {
    int(v, key).unwrap_or(0).clamp(0, 255) as u8
}

pub(super) fn int(v: &Value, key: &str) -> Option<i64> {
    let x = get(v, key)?;
    x.as_i64()
        .or_else(|| x.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64))
}

/// Enum strings are case-insensitive on read.
pub(super) fn enum_str(v: &Value, key: &str) -> Option<String> {
    get(v, key)
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
}

pub(super) fn text(v: &Value, key: &str) -> Option<String> {
    get(v, key).and_then(Value::as_str).map(str::to_owned)
}

pub(super) fn boolean(v: &Value, key: &str) -> bool {
    get(v, key).and_then(Value::as_bool).unwrap_or(false)
}

/// An `[r, g, b]` colour as `#rrggbb`.
pub(super) fn rgb(v: &Value) -> Option<String> {
    let c: Vec<u8> = v
        .as_array()?
        .iter()
        .filter_map(|x| x.as_u64().map(|n| n.min(255) as u8))
        .collect();
    (c.len() == 3).then(|| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]))
}

pub(super) fn array<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    get(v, key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}
