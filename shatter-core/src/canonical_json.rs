//! Canonical JSON serialization and recipe hashing for generator composite IDs.
//!
//! Generators return a human-readable `id` and a JSON `recipe` for replay.
//! Core combines them into a stable composite key: `"{id}@{recipe_hash_prefix}"`.
//! This module provides deterministic JSON canonicalization so the hash is
//! independent of serialization order or whitespace.

use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

/// Deterministic JSON serialization: sorted keys, normalized numbers and strings.
///
/// Guarantees: identical logical values always produce identical output bytes,
/// regardless of the key order, whitespace, or number formatting in the input.
pub fn canonicalize_json(value: &serde_json::Value) -> String {
    let mut buf = String::new();
    write_canonical(value, &mut buf);
    buf
}

/// SHA-256 of the canonical JSON, truncated to 6 hex characters.
pub fn recipe_hash(value: &serde_json::Value) -> String {
    let canonical = canonicalize_json(value);
    let digest = Sha256::digest(canonical.as_bytes());
    hex::encode(&digest[..3])
}

/// Composite ID: `"{id}@{recipe_hash_prefix}"`.
pub fn composite_id(id: &str, recipe: &serde_json::Value) -> String {
    format!("{}@{}", id, recipe_hash(recipe))
}

fn write_canonical(value: &serde_json::Value, buf: &mut String) {
    match value {
        serde_json::Value::Null => buf.push_str("null"),
        serde_json::Value::Bool(b) => buf.push_str(if *b { "true" } else { "false" }),
        serde_json::Value::Number(n) => write_canonical_number(n, buf),
        serde_json::Value::String(s) => write_canonical_string(s, buf),
        serde_json::Value::Array(arr) => {
            buf.push('[');
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    buf.push(',');
                }
                write_canonical(item, buf);
            }
            buf.push(']');
        }
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            buf.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    buf.push(',');
                }
                write_canonical_string(key, buf);
                buf.push(':');
                write_canonical(&map[*key], buf);
            }
            buf.push('}');
        }
    }
}

fn write_canonical_number(n: &serde_json::Number, buf: &mut String) {
    if let Some(i) = n.as_i64() {
        buf.push_str(&i.to_string());
    } else if let Some(u) = n.as_u64() {
        buf.push_str(&u.to_string());
    } else if let Some(f) = n.as_f64() {
        let mut ryu_buf = ryu::Buffer::new();
        buf.push_str(ryu_buf.format(f));
    }
}

fn write_canonical_string(s: &str, buf: &mut String) {
    let normalized: String = s.nfc().collect();
    buf.push('"');
    for ch in normalized.chars() {
        match ch {
            '"' => buf.push_str("\\\""),
            '\\' => buf.push_str("\\\\"),
            '\n' => buf.push_str("\\n"),
            '\r' => buf.push_str("\\r"),
            '\t' => buf.push_str("\\t"),
            c if c < '\u{0020}' => {
                buf.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => buf.push(c),
        }
    }
    buf.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn null_bool_canonicalize() {
        assert_eq!(canonicalize_json(&json!(null)), "null");
        assert_eq!(canonicalize_json(&json!(true)), "true");
        assert_eq!(canonicalize_json(&json!(false)), "false");
    }

    #[test]
    fn integer_canonicalize() {
        assert_eq!(canonicalize_json(&json!(0)), "0");
        assert_eq!(canonicalize_json(&json!(42)), "42");
        assert_eq!(canonicalize_json(&json!(-1)), "-1");
    }

    #[test]
    fn float_canonicalize() {
        assert_eq!(canonicalize_json(&json!(1.5)), "1.5");
        assert_eq!(canonicalize_json(&json!(0.0)), "0.0");
    }

    #[test]
    fn string_canonicalize_escapes() {
        assert_eq!(canonicalize_json(&json!("hello")), "\"hello\"");
        assert_eq!(canonicalize_json(&json!("a\"b")), "\"a\\\"b\"");
        assert_eq!(canonicalize_json(&json!("a\nb")), "\"a\\nb\"");
    }

    #[test]
    fn object_keys_sorted() {
        let v = json!({"z": 1, "a": 2, "m": 3});
        assert_eq!(canonicalize_json(&v), "{\"a\":2,\"m\":3,\"z\":1}");
    }

    #[test]
    fn nested_object_keys_sorted() {
        let v = json!({"b": {"z": 1, "a": 2}, "a": 0});
        assert_eq!(canonicalize_json(&v), "{\"a\":0,\"b\":{\"a\":2,\"z\":1}}");
    }

    #[test]
    fn array_preserves_order() {
        let v = json!([3, 1, 2]);
        assert_eq!(canonicalize_json(&v), "[3,1,2]");
    }

    #[test]
    fn key_order_does_not_affect_hash() {
        let v1 = json!({"host": "localhost", "port": 5432});
        let v2 = json!({"port": 5432, "host": "localhost"});
        assert_eq!(recipe_hash(&v1), recipe_hash(&v2));
    }

    #[test]
    fn different_values_produce_different_hashes() {
        let v1 = json!({"host": "localhost"});
        let v2 = json!({"host": "remote"});
        assert_ne!(recipe_hash(&v1), recipe_hash(&v2));
    }

    #[test]
    fn hash_is_six_hex_chars() {
        let h = recipe_hash(&json!({"x": 1}));
        assert_eq!(h.len(), 6);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn composite_id_format() {
        let recipe = json!({"host": "localhost", "port": 5432});
        let id = composite_id("postgres-test-db", &recipe);
        assert!(id.starts_with("postgres-test-db@"));
        assert_eq!(id.len(), "postgres-test-db@".len() + 6);
    }

    #[test]
    fn composite_id_stable_across_calls() {
        let recipe = json!({"a": 1, "b": 2});
        let id1 = composite_id("test", &recipe);
        let id2 = composite_id("test", &recipe);
        assert_eq!(id1, id2);
    }

    #[test]
    fn unicode_nfc_normalization() {
        let decomposed = "e\u{0301}";
        let precomposed = "\u{00e9}";
        let v1 = serde_json::Value::String(decomposed.to_string());
        let v2 = serde_json::Value::String(precomposed.to_string());
        assert_eq!(canonicalize_json(&v1), canonicalize_json(&v2));
    }

    #[test]
    fn empty_object_and_array() {
        assert_eq!(canonicalize_json(&json!({})), "{}");
        assert_eq!(canonicalize_json(&json!([])), "[]");
    }

    #[test]
    fn deeply_nested_structure() {
        let v = json!({"a": [{"c": 3, "b": 2}, {"e": 5, "d": 4}]});
        assert_eq!(
            canonicalize_json(&v),
            "{\"a\":[{\"b\":2,\"c\":3},{\"d\":4,\"e\":5}]}"
        );
    }
}
