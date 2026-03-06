//! Data model for nondeterminism detection.
//!
//! Presence in the nondeterministic field list means "we have evidence
//! this is nondeterministic." Absence does NOT assert determinism.

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::LazyLock;

/// How nondeterminism was detected for a field or parameter.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum NondeterminismEvidence {
    /// Explicitly declared by the user (e.g., config or annotation).
    UserDeclared,
    /// Different outputs observed for the same input within a single run.
    ObservedWithinRun,
    /// Different outputs observed for the same input across separate runs.
    ObservedAcrossRuns,
    /// Matched a known nondeterministic API pattern (e.g., `Date.now()`, `Math.random()`).
    PatternMatch { pattern: String },
    /// Name heuristic suggests nondeterminism (e.g., "timestamp", "random", "uuid").
    NameHeuristic { matched_name: String },
}

/// Confidence that a field is nondeterministic, based on accumulated evidence.
///
/// Ordered low-to-high so that [`Ord`] gives natural confidence comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

/// A parameter or field identified as potentially nondeterministic.
///
/// The `evidence` vector accumulates over time — multiple detection methods
/// may independently flag the same field, increasing confidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NondeterministicField {
    /// Path to the field (e.g., "param0", "param1.timestamp", "return.id").
    pub field_path: String,
    /// Evidence supporting the nondeterminism classification.
    pub evidence: Vec<NondeterminismEvidence>,
    /// Overall confidence derived from the evidence.
    pub confidence: Confidence,
}

/// Similarity score above which two values likely differ only by nondeterministic fields.
pub const SIMILARITY_HIGH_THRESHOLD: f64 = 0.9;

/// Similarity score below which two values likely represent genuinely different behavior.
pub const SIMILARITY_LOW_THRESHOLD: f64 = 0.5;

/// Result of comparing two JSON values structurally.
#[derive(Debug, Clone, PartialEq)]
pub struct SimilarityResult {
    /// Fraction of matching leaves (0.0–1.0).
    pub score: f64,
    /// Dot-separated paths of leaves that differ between the two values.
    pub changed_paths: Vec<String>,
    /// Total number of leaf comparisons performed.
    pub total_leaves: usize,
}

/// Compare two JSON values leaf-by-leaf, returning similarity as a fraction.
///
/// Objects are compared by union of keys (missing keys count as mismatches).
/// Arrays are compared element-by-element up to the longer length.
/// Primitives use exact equality. Type mismatches at any node count as
/// a single leaf mismatch.
pub fn structural_similarity(a: &Value, b: &Value) -> SimilarityResult {
    let mut matched: usize = 0;
    let mut total: usize = 0;
    let mut changed_paths: Vec<String> = Vec::new();

    collect_diff(a, b, "", &mut matched, &mut total, &mut changed_paths);

    let score = if total == 0 { 1.0 } else { matched as f64 / total as f64 };

    SimilarityResult {
        score,
        changed_paths,
        total_leaves: total,
    }
}

fn collect_diff(
    a: &Value,
    b: &Value,
    prefix: &str,
    matched: &mut usize,
    total: &mut usize,
    changed: &mut Vec<String>,
) {
    match (a, b) {
        (Value::Object(ma), Value::Object(mb)) => {
            if ma.is_empty() && mb.is_empty() {
                // Two empty objects are identical — one matching leaf.
                *total += 1;
                *matched += 1;
                return;
            }
            let mut keys: Vec<&String> = ma.keys().chain(mb.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                let child_path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                match (ma.get(key), mb.get(key)) {
                    (Some(va), Some(vb)) => {
                        collect_diff(va, vb, &child_path, matched, total, changed);
                    }
                    _ => {
                        // Key missing on one side — count leaves from present side.
                        let present = ma.get(key).or_else(|| mb.get(key)).unwrap();
                        let leaf_count = count_leaves(present);
                        *total += leaf_count;
                        changed.push(child_path);
                    }
                }
            }
        }
        (Value::Array(aa), Value::Array(ab)) => {
            if aa.is_empty() && ab.is_empty() {
                *total += 1;
                *matched += 1;
                return;
            }
            let max_len = aa.len().max(ab.len());
            for i in 0..max_len {
                let child_path = if prefix.is_empty() {
                    format!("[{i}]")
                } else {
                    format!("{prefix}[{i}]")
                };
                match (aa.get(i), ab.get(i)) {
                    (Some(va), Some(vb)) => {
                        collect_diff(va, vb, &child_path, matched, total, changed);
                    }
                    (Some(present), None) | (None, Some(present)) => {
                        let leaf_count = count_leaves(present);
                        *total += leaf_count;
                        changed.push(child_path);
                    }
                    (None, None) => unreachable!(),
                }
            }
        }
        // Both are leaves (or type mismatch at this level).
        _ => {
            *total += 1;
            if a == b {
                *matched += 1;
            } else {
                changed.push(prefix.to_string());
            }
        }
    }
}

/// Count the number of leaf values in a JSON tree.
fn count_leaves(v: &Value) -> usize {
    match v {
        Value::Object(m) if !m.is_empty() => m.values().map(count_leaves).sum(),
        Value::Array(a) if !a.is_empty() => a.iter().map(count_leaves).sum(),
        _ => 1,
    }
}

// --- Value-pattern heuristics ---

/// A matched value pattern indicating likely nondeterminism.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValuePatternMatch {
    /// Human-readable pattern name (e.g., "uuid_v4", "iso8601_datetime").
    pub pattern_name: String,
    /// Confidence that this pattern indicates nondeterminism.
    pub confidence: Confidence,
}

/// Pattern name constants — used in both production code and tests.
pub const PATTERN_UUID_V4: &str = "uuid_v4";
pub const PATTERN_ISO8601_DATETIME: &str = "iso8601_datetime";
pub const PATTERN_UNIX_TIMESTAMP_S: &str = "unix_timestamp_seconds";
pub const PATTERN_UNIX_TIMESTAMP_MS: &str = "unix_timestamp_millis";
pub const PATTERN_JWT: &str = "jwt_token";
pub const PATTERN_SHA256_HEX: &str = "sha256_hex";
pub const PATTERN_RANDOM_HEX: &str = "random_hex";

/// Epoch boundaries for unix timestamp detection (2020-01-01 to 2030-01-01).
const UNIX_TS_MIN_S: i64 = 1_577_836_800;
const UNIX_TS_MAX_S: i64 = 1_893_456_000;
const UNIX_TS_MIN_MS: i64 = UNIX_TS_MIN_S * 1_000;
const UNIX_TS_MAX_MS: i64 = UNIX_TS_MAX_S * 1_000;

/// Minimum length for random hex string detection.
const RANDOM_HEX_MIN_LEN: usize = 32;

/// Exact length of a SHA-256 hex digest.
const SHA256_HEX_LEN: usize = 64;

static RE_UUID_V4: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
        .expect("uuid_v4 regex")
});

static RE_ISO8601: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}").expect("iso8601 regex")
});

static RE_JWT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$").expect("jwt regex")
});

static RE_HEX_LOWER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^[0-9a-f]+$").expect("hex regex")
});

/// Check a JSON value against known nondeterministic value patterns.
///
/// For strings: tests UUID v4, ISO 8601 datetime, JWT, SHA-256 hex, and random hex.
/// For numbers: tests unix timestamp ranges (seconds and milliseconds).
/// SHA-256 (64 hex chars) is preferred over generic random hex when both match.
pub fn check_value_patterns(value: &Value) -> Vec<ValuePatternMatch> {
    match value {
        Value::String(s) => check_string_patterns(s),
        Value::Number(n) => check_number_patterns(n),
        _ => Vec::new(),
    }
}

fn check_string_patterns(s: &str) -> Vec<ValuePatternMatch> {
    let mut matches = Vec::new();

    if RE_UUID_V4.is_match(s) {
        matches.push(ValuePatternMatch {
            pattern_name: PATTERN_UUID_V4.into(),
            confidence: Confidence::High,
        });
    }

    if RE_ISO8601.is_match(s) {
        matches.push(ValuePatternMatch {
            pattern_name: PATTERN_ISO8601_DATETIME.into(),
            confidence: Confidence::High,
        });
    }

    if RE_JWT.is_match(s) {
        matches.push(ValuePatternMatch {
            pattern_name: PATTERN_JWT.into(),
            confidence: Confidence::High,
        });
    }

    // Hex string patterns: prefer SHA-256 (specific) over generic random hex.
    if s.len() >= RANDOM_HEX_MIN_LEN && RE_HEX_LOWER.is_match(s) {
        if s.len() == SHA256_HEX_LEN {
            matches.push(ValuePatternMatch {
                pattern_name: PATTERN_SHA256_HEX.into(),
                confidence: Confidence::Medium,
            });
        } else {
            matches.push(ValuePatternMatch {
                pattern_name: PATTERN_RANDOM_HEX.into(),
                confidence: Confidence::Medium,
            });
        }
    }

    matches
}

fn check_number_patterns(n: &serde_json::Number) -> Vec<ValuePatternMatch> {
    let mut matches = Vec::new();

    if let Some(v) = n.as_i64() {
        if (UNIX_TS_MIN_S..=UNIX_TS_MAX_S).contains(&v) {
            matches.push(ValuePatternMatch {
                pattern_name: PATTERN_UNIX_TIMESTAMP_S.into(),
                confidence: Confidence::Medium,
            });
        } else if (UNIX_TS_MIN_MS..=UNIX_TS_MAX_MS).contains(&v) {
            matches.push(ValuePatternMatch {
                pattern_name: PATTERN_UNIX_TIMESTAMP_MS.into(),
                confidence: Confidence::Medium,
            });
        }
    }

    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn construct_nondeterministic_field() {
        let field = NondeterministicField {
            field_path: "param0.timestamp".into(),
            evidence: vec![NondeterminismEvidence::ObservedAcrossRuns],
            confidence: Confidence::Medium,
        };
        assert_eq!(field.field_path, "param0.timestamp");
        assert_eq!(field.evidence.len(), 1);
        assert_eq!(field.confidence, Confidence::Medium);
    }

    #[test]
    fn serialize_deserialize_round_trip() {
        let field = NondeterministicField {
            field_path: "return.id".into(),
            evidence: vec![
                NondeterminismEvidence::PatternMatch {
                    pattern: "Math.random()".into(),
                },
                NondeterminismEvidence::NameHeuristic {
                    matched_name: "random".into(),
                },
            ],
            confidence: Confidence::High,
        };

        let json = serde_json::to_string(&field).expect("serialize");
        let restored: NondeterministicField =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(field, restored);
    }

    #[test]
    fn evidence_variants_round_trip() {
        let variants = vec![
            NondeterminismEvidence::UserDeclared,
            NondeterminismEvidence::ObservedWithinRun,
            NondeterminismEvidence::ObservedAcrossRuns,
            NondeterminismEvidence::PatternMatch {
                pattern: "Date.now()".into(),
            },
            NondeterminismEvidence::NameHeuristic {
                matched_name: "uuid".into(),
            },
        ];

        for variant in &variants {
            let json = serde_json::to_string(variant).expect("serialize");
            let restored: NondeterminismEvidence =
                serde_json::from_str(&json).expect("deserialize");
            assert_eq!(*variant, restored);
        }
    }

    #[test]
    fn confidence_ordering() {
        assert!(Confidence::Low < Confidence::Medium);
        assert!(Confidence::Medium < Confidence::High);
    }

    // --- structural_similarity tests ---

    #[test]
    fn similarity_identical_primitives() {
        let r = structural_similarity(&json!(42), &json!(42));
        assert_eq!(r.score, 1.0);
        assert!(r.changed_paths.is_empty());
        assert_eq!(r.total_leaves, 1);
    }

    #[test]
    fn similarity_different_primitives() {
        let r = structural_similarity(&json!(42), &json!("hello"));
        assert_eq!(r.score, 0.0);
        assert_eq!(r.changed_paths, vec![""]);
    }

    #[test]
    fn similarity_identical_objects() {
        let a = json!({"a": 1, "b": 2});
        let r = structural_similarity(&a, &a.clone());
        assert_eq!(r.score, 1.0);
        assert!(r.changed_paths.is_empty());
    }

    #[test]
    fn similarity_one_field_changed() {
        let a = json!({"a": 1, "b": 2, "c": 3});
        let b = json!({"a": 1, "b": 99, "c": 3});
        let r = structural_similarity(&a, &b);
        let expected = 2.0 / 3.0;
        assert!((r.score - expected).abs() < 1e-10);
        assert_eq!(r.changed_paths, vec!["b"]);
        assert_eq!(r.total_leaves, 3);
    }

    #[test]
    fn similarity_nested_objects() {
        let a = json!({"a": {"x": 1, "y": 2}});
        let b = json!({"a": {"x": 1, "y": 99}});
        let r = structural_similarity(&a, &b);
        assert_eq!(r.score, 0.5);
        assert_eq!(r.changed_paths, vec!["a.y"]);
    }

    #[test]
    fn similarity_completely_different_objects() {
        let a = json!({"a": 1});
        let b = json!({"b": 2});
        let r = structural_similarity(&a, &b);
        assert_eq!(r.score, 0.0);
        assert_eq!(r.total_leaves, 2);
    }

    #[test]
    fn similarity_array_same_length() {
        let a = json!([1, 2, 3]);
        let b = json!([1, 2, 4]);
        let r = structural_similarity(&a, &b);
        let expected = 2.0 / 3.0;
        assert!((r.score - expected).abs() < 1e-10);
        assert_eq!(r.changed_paths, vec!["[2]"]);
    }

    #[test]
    fn similarity_array_different_length() {
        let a = json!([1, 2]);
        let b = json!([1, 2, 3]);
        let r = structural_similarity(&a, &b);
        let expected = 2.0 / 3.0;
        assert!((r.score - expected).abs() < 1e-10);
        assert_eq!(r.changed_paths, vec!["[2]"]);
    }

    #[test]
    fn similarity_type_mismatch() {
        let r = structural_similarity(&json!("hello"), &json!(42));
        assert_eq!(r.score, 0.0);
        assert_eq!(r.total_leaves, 1);
    }

    #[test]
    fn similarity_null_handling() {
        let r = structural_similarity(&json!(null), &json!(null));
        assert_eq!(r.score, 1.0);

        let r2 = structural_similarity(&json!(null), &json!(42));
        assert_eq!(r2.score, 0.0);
    }

    #[test]
    fn similarity_empty_objects() {
        let r = structural_similarity(&json!({}), &json!({}));
        assert_eq!(r.score, 1.0);
        assert_eq!(r.total_leaves, 1);
    }

    #[test]
    fn similarity_empty_arrays() {
        let r = structural_similarity(&json!([]), &json!([]));
        assert_eq!(r.score, 1.0);
        assert_eq!(r.total_leaves, 1);
    }

    #[test]
    fn similarity_high_threshold_large_object() {
        // 10 fields, 1 changed → 0.9, which meets the threshold.
        let a = json!({"a":1,"b":2,"c":3,"d":4,"e":5,"f":6,"g":7,"h":8,"i":9,"j":10});
        let b = json!({"a":1,"b":2,"c":3,"d":4,"e":5,"f":6,"g":7,"h":8,"i":9,"j":99});
        let r = structural_similarity(&a, &b);
        assert!(r.score >= SIMILARITY_HIGH_THRESHOLD);
    }

    #[test]
    fn similarity_low_threshold_mostly_different() {
        let a = json!({"a":1,"b":2,"c":3,"d":4});
        let b = json!({"a":99,"b":98,"c":97,"d":4});
        let r = structural_similarity(&a, &b);
        assert!(r.score < SIMILARITY_LOW_THRESHOLD);
    }

    #[test]
    fn similarity_deeply_nested() {
        let a = json!({"l1": {"l2": {"l3": {"val": 1, "stable": true}}}});
        let b = json!({"l1": {"l2": {"l3": {"val": 2, "stable": true}}}});
        let r = structural_similarity(&a, &b);
        assert_eq!(r.score, 0.5);
        assert_eq!(r.changed_paths, vec!["l1.l2.l3.val"]);
    }

    #[test]
    fn similarity_mixed_array_and_object() {
        let a = json!({"items": [1, 2], "name": "test"});
        let b = json!({"items": [1, 3], "name": "test"});
        let r = structural_similarity(&a, &b);
        // 3 leaves: items[0]=match, items[1]=diff, name=match → 2/3
        let expected = 2.0 / 3.0;
        assert!((r.score - expected).abs() < 1e-10);
        assert_eq!(r.changed_paths, vec!["items[1]"]);
    }

    #[test]
    fn similarity_object_vs_primitive() {
        let a = json!({"a": 1});
        let b = json!(42);
        let r = structural_similarity(&a, &b);
        assert_eq!(r.score, 0.0);
        assert_eq!(r.total_leaves, 1);
    }

    #[test]
    fn similarity_missing_key_with_nested_value() {
        // Missing key has a nested object with 2 leaves — both count as mismatches.
        let a = json!({"x": 1, "y": {"p": 1, "q": 2}});
        let b = json!({"x": 1});
        let r = structural_similarity(&a, &b);
        // total: x(1) + y.p(1) + y.q(1) = 3, matched: 1
        assert!((r.score - 1.0 / 3.0).abs() < 1e-10);
        assert_eq!(r.changed_paths, vec!["y"]);
    }

    #[test]
    fn multiple_evidence_accumulates() {
        let mut field = NondeterministicField {
            field_path: "param0".into(),
            evidence: vec![NondeterminismEvidence::NameHeuristic {
                matched_name: "timestamp".into(),
            }],
            confidence: Confidence::Low,
        };

        field
            .evidence
            .push(NondeterminismEvidence::ObservedAcrossRuns);
        field.confidence = Confidence::High;

        assert_eq!(field.evidence.len(), 2);
        assert_eq!(field.confidence, Confidence::High);
    }

    // --- value pattern heuristic tests ---

    #[test]
    fn pattern_uuid_v4() {
        let v = json!("550e8400-e29b-41d4-a716-446655440000");
        let matches = check_value_patterns(&v);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_UUID_V4);
        assert_eq!(matches[0].confidence, Confidence::High);
    }

    #[test]
    fn pattern_uuid_v4_uppercase() {
        let v = json!("550E8400-E29B-41D4-A716-446655440000");
        let matches = check_value_patterns(&v);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_UUID_V4);
    }

    #[test]
    fn pattern_uuid_v4_rejects_v1() {
        // Version nibble is 1, not 4.
        let v = json!("550e8400-e29b-11d4-a716-446655440000");
        let matches = check_value_patterns(&v);
        assert!(
            matches.iter().all(|m| m.pattern_name != PATTERN_UUID_V4),
            "v1 UUID should not match uuid_v4 pattern"
        );
    }

    #[test]
    fn pattern_iso8601_datetime() {
        let v = json!("2026-03-05T14:30:00Z");
        let matches = check_value_patterns(&v);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_ISO8601_DATETIME);
        assert_eq!(matches[0].confidence, Confidence::High);
    }

    #[test]
    fn pattern_iso8601_with_offset() {
        let v = json!("2026-03-05T14:30:00+05:30");
        let matches = check_value_patterns(&v);
        assert!(matches.iter().any(|m| m.pattern_name == PATTERN_ISO8601_DATETIME));
    }

    #[test]
    fn pattern_iso8601_rejects_date_only() {
        let v = json!("2026-03-05");
        let matches = check_value_patterns(&v);
        assert!(matches.iter().all(|m| m.pattern_name != PATTERN_ISO8601_DATETIME));
    }

    #[test]
    fn pattern_jwt_token() {
        // Real JWT structure: header.payload.signature (all base64url).
        let v = json!("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U");
        let matches = check_value_patterns(&v);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_JWT);
        assert_eq!(matches[0].confidence, Confidence::High);
    }

    #[test]
    fn pattern_jwt_rejects_non_jwt() {
        let v = json!("not.a.jwt");
        let matches = check_value_patterns(&v);
        assert!(matches.iter().all(|m| m.pattern_name != PATTERN_JWT));
    }

    #[test]
    fn pattern_sha256_hex() {
        let v = json!("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        let matches = check_value_patterns(&v);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_SHA256_HEX);
        assert_eq!(matches[0].confidence, Confidence::Medium);
    }

    #[test]
    fn pattern_random_hex_long() {
        // 48 hex chars — not SHA-256 length, so classified as random hex.
        let v = json!("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6");
        let matches = check_value_patterns(&v);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_RANDOM_HEX);
        assert_eq!(matches[0].confidence, Confidence::Medium);
    }

    #[test]
    fn pattern_hex_too_short() {
        // 16 hex chars — below RANDOM_HEX_MIN_LEN threshold.
        let v = json!("a1b2c3d4e5f6a1b2");
        let matches = check_value_patterns(&v);
        assert!(matches.is_empty());
    }

    #[test]
    fn pattern_unix_timestamp_seconds() {
        // 2026-01-01 ~ 1767225600
        let v = json!(1_767_225_600_i64);
        let matches = check_value_patterns(&v);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_UNIX_TIMESTAMP_S);
        assert_eq!(matches[0].confidence, Confidence::Medium);
    }

    #[test]
    fn pattern_unix_timestamp_millis() {
        let v = json!(1_767_225_600_000_i64);
        let matches = check_value_patterns(&v);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pattern_name, PATTERN_UNIX_TIMESTAMP_MS);
    }

    #[test]
    fn pattern_number_outside_timestamp_range() {
        let v = json!(42);
        let matches = check_value_patterns(&v);
        assert!(matches.is_empty());
    }

    #[test]
    fn pattern_null_and_bool() {
        assert!(check_value_patterns(&json!(null)).is_empty());
        assert!(check_value_patterns(&json!(true)).is_empty());
    }

    #[test]
    fn pattern_empty_string() {
        assert!(check_value_patterns(&json!("")).is_empty());
    }

    #[test]
    fn pattern_non_hex_string() {
        // Contains 'g' which is not hex.
        let v = json!("g1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4");
        let matches = check_value_patterns(&v);
        assert!(matches.iter().all(|m| m.pattern_name != PATTERN_RANDOM_HEX));
    }

    #[test]
    fn pattern_match_serialization_round_trip() {
        let m = ValuePatternMatch {
            pattern_name: PATTERN_UUID_V4.into(),
            confidence: Confidence::High,
        };
        let json_str = serde_json::to_string(&m).expect("serialize");
        let restored: ValuePatternMatch = serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(m, restored);
    }

    #[test]
    fn pattern_object_and_array_return_empty() {
        assert!(check_value_patterns(&json!({"id": "abc"})).is_empty());
        assert!(check_value_patterns(&json!([1, 2, 3])).is_empty());
    }
}
