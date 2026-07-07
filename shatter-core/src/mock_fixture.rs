//! User-defined mock fixtures with scoped resolution and type validation.
//!
//! Mock fixtures let users declare exactly how external dependencies should
//! behave during exploration. Fixtures are defined in `.shatter/config.yaml`
//! at three scope levels (global → file → function), with innermost scope
//! winning on conflicts.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::TypeInfo;

// ---------------------------------------------------------------------------
// MockValueSpace — strategy for generating mock return values
// ---------------------------------------------------------------------------
// NOTE: This duplicates the enum from mock_value_space.rs (str-3ky9.10).
// It will be reconciled once both branches merge — one definition will be
// removed and re-exported from the surviving module.

/// Strategy for selecting mock return values during exploration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum MockValueSpace {
    /// Try the real service first; fall back to synthetic mocks on connection
    /// failure. This is the default for network/database dependencies.
    LiveFirst,
    /// Use a fixed set of user-provided return values (round-robin).
    FixedSet { values: Vec<Value> },
    /// Engine generates values autonomously based on type information.
    Autonomous,
    /// Replay a recorded seed corpus (e.g. from a prior live session).
    Seeded { seed_file: String },
}

// ---------------------------------------------------------------------------
// Expectation types — post-execution verification
// ---------------------------------------------------------------------------

/// A single argument matcher for `called_with` expectations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "match")]
pub enum ArgMatcher {
    /// Exact JSON equality.
    Exact { value: Value },
    /// Matches any value (wildcard).
    Any,
    /// Matches any value of the given type.
    TypeOf { expected: TypeInfo },
}

/// Post-execution expectations for a mock fixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MockExpectations {
    /// Expected argument patterns for each call. If present, every call must
    /// match at least one entry (order-independent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub called_with: Option<Vec<Vec<ArgMatcher>>>,

    /// Expected number of calls. `None` means no assertion on call count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_count: Option<CallCountExpectation>,
}

/// How many times a mock is expected to be called.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CallCountExpectation {
    /// Exactly N times.
    Exact { n: u32 },
    /// At least N times.
    AtLeast { n: u32 },
    /// At most N times.
    AtMost { n: u32 },
    /// Between min and max (inclusive).
    Between { min: u32, max: u32 },
}

// ---------------------------------------------------------------------------
// MockFixture — a single user-declared fixture
// ---------------------------------------------------------------------------

/// A user-defined mock fixture for a specific external dependency symbol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MockFixture {
    /// The dependency symbol this fixture targets (e.g. `"db.query"`, `"fetch"`).
    pub symbol: String,

    /// Strategy for selecting return values.
    pub value_space: MockValueSpace,

    /// Explicit return values (used with `FixedSet` value space, or as
    /// overrides for other strategies).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_values: Option<Vec<Value>>,

    /// Expected return type. Used for load-time validation and to guide
    /// autonomous value generation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_type: Option<TypeInfo>,

    /// Post-execution expectations (call count, argument patterns).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expectations: Option<MockExpectations>,
}

// ---------------------------------------------------------------------------
// MockFixtureConfig — three-level scoped configuration
// ---------------------------------------------------------------------------

/// Three-level scoped mock fixture configuration.
///
/// Resolution order (innermost wins):
/// 1. `functions["file:func"].fixtures["symbol"]`
/// 2. `files["file"].fixtures["symbol"]`
/// 3. `global.fixtures["symbol"]`
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MockFixtureConfig {
    /// Global fixtures applied to all functions unless overridden.
    #[serde(default)]
    pub global: MockFixtureScope,

    /// Per-file fixtures, keyed by file path or glob pattern.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub files: HashMap<String, MockFixtureScope>,

    /// Per-function fixtures, keyed by `"file:function"` identifier.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub functions: HashMap<String, MockFixtureScope>,
}

/// A collection of fixtures at a single scope level.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MockFixtureScope {
    /// Fixtures keyed by dependency symbol.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub fixtures: HashMap<String, MockFixture>,
}

// ---------------------------------------------------------------------------
// Scope resolution — innermost wins
// ---------------------------------------------------------------------------

/// Resolve the effective fixture for a given symbol, applying innermost-wins
/// precedence: function → file → global.
pub fn resolve_fixture<'a>(
    config: &'a MockFixtureConfig,
    file: &str,
    function: &str,
    symbol: &str,
) -> Option<&'a MockFixture> {
    let func_key = format!("{file}:{function}");

    // 1. Function-level (most specific)
    if let Some(fixture) = config
        .functions
        .get(&func_key)
        .and_then(|scope| scope.fixtures.get(symbol))
    {
        return Some(fixture);
    }

    // 2. File-level
    if let Some(fixture) = config
        .files
        .get(file)
        .and_then(|scope| scope.fixtures.get(symbol))
    {
        return Some(fixture);
    }

    // 3. Global (least specific)
    config.global.fixtures.get(symbol)
}

// ---------------------------------------------------------------------------
// Type validation
// ---------------------------------------------------------------------------

/// Errors from fixture validation.
#[derive(Debug, Clone, PartialEq)]
pub enum FixtureValidationError {
    /// A return value doesn't match the declared return type.
    TypeMismatch {
        symbol: String,
        index: usize,
        expected: TypeInfo,
        actual_value: Value,
    },
    /// FixedSet value space requires `return_values` to be non-empty.
    EmptyFixedSet { symbol: String },
}

impl std::fmt::Display for FixtureValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TypeMismatch {
                symbol,
                index,
                expected,
                actual_value,
            } => write!(
                f,
                "fixture '{symbol}': return_values[{index}] doesn't match declared type {expected:?}: got {actual_value}"
            ),
            Self::EmptyFixedSet { symbol } => write!(
                f,
                "fixture '{symbol}': value_space is fixed_set but return_values is empty"
            ),
        }
    }
}

impl std::error::Error for FixtureValidationError {}

/// Check that a JSON value is compatible with a TypeInfo.
fn json_matches_type(value: &Value, ty: &TypeInfo) -> bool {
    match (value, ty) {
        (Value::Null, TypeInfo::Nullable { .. }) => true,
        (_, TypeInfo::Nullable { inner }) => json_matches_type(value, inner),
        (Value::Number(n), TypeInfo::Int { .. }) => n.as_i64().is_some(),
        (Value::Number(n), TypeInfo::Float) => n.as_f64().is_some(),
        (Value::String(_), TypeInfo::Str) => true,
        (Value::Bool(_), TypeInfo::Bool) => true,
        (Value::Array(items), TypeInfo::Array { element }) => {
            items.iter().all(|item| json_matches_type(item, element))
        }
        (Value::Object(map), TypeInfo::Object { fields }) => {
            fields.iter().all(|(name, field_ty)| {
                map.get(name)
                    .is_some_and(|v| json_matches_type(v, field_ty))
            })
        }
        (_, TypeInfo::Union { variants, .. }) => variants.iter().any(|v| json_matches_type(value, v)),
        // Unknown/Complex/Opaque — can't validate statically, accept anything.
        (_, TypeInfo::Unknown | TypeInfo::Complex { .. } | TypeInfo::Opaque { .. }) => true,
        _ => false,
    }
}

/// Validate all fixtures in a config at load time.
///
/// Checks:
/// - `FixedSet` value spaces have non-empty `return_values`
/// - Each return value matches the declared `return_type` (when both are present)
pub fn validate_fixture_types(config: &MockFixtureConfig) -> Vec<FixtureValidationError> {
    let mut errors = Vec::new();
    let all_scopes = std::iter::once(&config.global)
        .chain(config.files.values())
        .chain(config.functions.values());

    for scope in all_scopes {
        for fixture in scope.fixtures.values() {
            validate_single_fixture(fixture, &mut errors);
        }
    }
    errors
}

fn validate_single_fixture(fixture: &MockFixture, errors: &mut Vec<FixtureValidationError>) {
    // FixedSet requires non-empty return_values.
    if matches!(fixture.value_space, MockValueSpace::FixedSet { .. }) {
        let has_values = fixture
            .return_values
            .as_ref()
            .is_some_and(|v| !v.is_empty());
        if !has_values {
            errors.push(FixtureValidationError::EmptyFixedSet {
                symbol: fixture.symbol.clone(),
            });
        }
    }

    // Type-check return_values against return_type when both are present.
    if let (Some(values), Some(ty)) = (&fixture.return_values, &fixture.return_type) {
        for (i, val) in values.iter().enumerate() {
            if !json_matches_type(val, ty) {
                errors.push(FixtureValidationError::TypeMismatch {
                    symbol: fixture.symbol.clone(),
                    index: i,
                    expected: ty.clone(),
                    actual_value: val.clone(),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Expectation validation — post-execution
// ---------------------------------------------------------------------------

/// A record of a single call made to a mocked dependency.
#[derive(Debug, Clone, PartialEq)]
pub struct MockCallRecord {
    /// Arguments passed to the mock.
    pub args: Vec<Value>,
}

/// Errors from post-execution expectation validation.
#[derive(Debug, Clone, PartialEq)]
pub enum ExpectationError {
    /// Call count didn't match the expectation.
    CallCountMismatch {
        symbol: String,
        expected: CallCountExpectation,
        actual: u32,
    },
    /// A call's arguments didn't match any `called_with` pattern.
    ArgMismatch {
        symbol: String,
        call_index: usize,
        args: Vec<Value>,
    },
}

impl std::fmt::Display for ExpectationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CallCountMismatch {
                symbol,
                expected,
                actual,
            } => write!(
                f,
                "fixture '{symbol}': expected {expected:?} calls, got {actual}"
            ),
            Self::ArgMismatch {
                symbol,
                call_index,
                args,
            } => write!(
                f,
                "fixture '{symbol}': call #{call_index} args {args:?} didn't match any called_with pattern"
            ),
        }
    }
}

impl std::error::Error for ExpectationError {}

/// Validate post-execution expectations against observed call records.
pub fn validate_expectations(
    fixture: &MockFixture,
    calls: &[MockCallRecord],
) -> Vec<ExpectationError> {
    let mut errors = Vec::new();
    let Some(expectations) = &fixture.expectations else {
        return errors;
    };

    // Check call count.
    if let Some(count_exp) = &expectations.call_count {
        let actual = calls.len() as u32;
        let ok = match count_exp {
            CallCountExpectation::Exact { n } => actual == *n,
            CallCountExpectation::AtLeast { n } => actual >= *n,
            CallCountExpectation::AtMost { n } => actual <= *n,
            CallCountExpectation::Between { min, max } => actual >= *min && actual <= *max,
        };
        if !ok {
            errors.push(ExpectationError::CallCountMismatch {
                symbol: fixture.symbol.clone(),
                expected: count_exp.clone(),
                actual,
            });
        }
    }

    // Check called_with patterns.
    if let Some(patterns) = &expectations.called_with {
        for (i, call) in calls.iter().enumerate() {
            let matches_any = patterns
                .iter()
                .any(|pattern| args_match(pattern, &call.args));
            if !matches_any {
                errors.push(ExpectationError::ArgMismatch {
                    symbol: fixture.symbol.clone(),
                    call_index: i,
                    args: call.args.clone(),
                });
            }
        }
    }

    errors
}

/// Check whether a call's arguments match a pattern of ArgMatchers.
fn args_match(pattern: &[ArgMatcher], args: &[Value]) -> bool {
    if pattern.len() != args.len() {
        return false;
    }
    pattern
        .iter()
        .zip(args.iter())
        .all(|(matcher, arg)| match matcher {
            ArgMatcher::Exact { value } => arg == value,
            ArgMatcher::Any => true,
            ArgMatcher::TypeOf { expected } => json_matches_type(arg, expected),
        })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- Helper builders --

    fn global_fixture(symbol: &str, value_space: MockValueSpace) -> MockFixture {
        MockFixture {
            symbol: symbol.to_string(),
            value_space,
            return_values: None,
            return_type: None,
            expectations: None,
        }
    }

    fn fixture_with_values(symbol: &str, values: Vec<Value>, ty: TypeInfo) -> MockFixture {
        MockFixture {
            symbol: symbol.to_string(),
            value_space: MockValueSpace::FixedSet {
                values: values.clone(),
            },
            return_values: Some(values),
            return_type: Some(ty),
            expectations: None,
        }
    }

    fn config_with_scopes(
        global: Vec<MockFixture>,
        files: Vec<(&str, Vec<MockFixture>)>,
        functions: Vec<(&str, Vec<MockFixture>)>,
    ) -> MockFixtureConfig {
        let to_scope = |fixtures: Vec<MockFixture>| MockFixtureScope {
            fixtures: fixtures
                .into_iter()
                .map(|f| (f.symbol.clone(), f))
                .collect(),
        };

        MockFixtureConfig {
            global: to_scope(global),
            files: files
                .into_iter()
                .map(|(k, v)| (k.to_string(), to_scope(v)))
                .collect(),
            functions: functions
                .into_iter()
                .map(|(k, v)| (k.to_string(), to_scope(v)))
                .collect(),
        }
    }

    // -- Serde roundtrip tests --

    #[test]
    fn mock_fixture_serde_roundtrip() {
        let fixture = MockFixture {
            symbol: "db.query".into(),
            value_space: MockValueSpace::FixedSet {
                values: vec![json!({"id": 1}), json!({"id": 2})],
            },
            return_values: Some(vec![json!({"id": 1}), json!({"id": 2})]),
            return_type: Some(TypeInfo::Object {
                fields: vec![("id".into(), TypeInfo::Int { int_width: None, int_signed: None })],
            }),
            expectations: Some(MockExpectations {
                called_with: Some(vec![vec![ArgMatcher::Exact {
                    value: json!("SELECT * FROM users"),
                }]]),
                call_count: Some(CallCountExpectation::AtLeast { n: 1 }),
            }),
        };

        let yaml = serde_yaml::to_string(&fixture).unwrap();
        let back: MockFixture = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back, fixture);
    }

    #[test]
    fn mock_fixture_config_serde_roundtrip() {
        let config = config_with_scopes(
            vec![global_fixture("fetch", MockValueSpace::Autonomous)],
            vec![(
                "src/api.ts",
                vec![global_fixture("db.query", MockValueSpace::LiveFirst)],
            )],
            vec![(
                "src/api.ts:getUser",
                vec![fixture_with_values(
                    "db.query",
                    vec![json!({"id": 1, "name": "Alice"})],
                    TypeInfo::Object {
                        fields: vec![("id".into(), TypeInfo::Int { int_width: None, int_signed: None }), ("name".into(), TypeInfo::Str)],
                    },
                )],
            )],
        );

        let yaml = serde_yaml::to_string(&config).unwrap();
        let back: MockFixtureConfig = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back, config);
    }

    #[test]
    fn arg_matcher_serde_roundtrip() {
        let matchers = vec![
            ArgMatcher::Exact { value: json!(42) },
            ArgMatcher::Any,
            ArgMatcher::TypeOf {
                expected: TypeInfo::Str,
            },
        ];

        for m in &matchers {
            let json_str = serde_json::to_string(m).unwrap();
            let back: ArgMatcher = serde_json::from_str(&json_str).unwrap();
            assert_eq!(&back, m);
        }
    }

    #[test]
    fn call_count_expectation_serde_roundtrip() {
        let expectations = vec![
            CallCountExpectation::Exact { n: 3 },
            CallCountExpectation::AtLeast { n: 1 },
            CallCountExpectation::AtMost { n: 5 },
            CallCountExpectation::Between { min: 2, max: 4 },
        ];

        for exp in &expectations {
            let json_str = serde_json::to_string(exp).unwrap();
            let back: CallCountExpectation = serde_json::from_str(&json_str).unwrap();
            assert_eq!(&back, exp);
        }
    }

    // -- Scope resolution tests --

    #[test]
    fn resolve_global_only() {
        let config = config_with_scopes(
            vec![global_fixture("fetch", MockValueSpace::Autonomous)],
            vec![],
            vec![],
        );

        let result = resolve_fixture(&config, "src/api.ts", "getUser", "fetch");
        assert!(result.is_some());
        assert_eq!(result.unwrap().value_space, MockValueSpace::Autonomous);
    }

    #[test]
    fn resolve_file_overrides_global() {
        let config = config_with_scopes(
            vec![global_fixture("fetch", MockValueSpace::Autonomous)],
            vec![(
                "src/api.ts",
                vec![global_fixture("fetch", MockValueSpace::LiveFirst)],
            )],
            vec![],
        );

        let result = resolve_fixture(&config, "src/api.ts", "getUser", "fetch");
        assert_eq!(result.unwrap().value_space, MockValueSpace::LiveFirst);
    }

    #[test]
    fn resolve_function_overrides_file() {
        let config = config_with_scopes(
            vec![global_fixture("fetch", MockValueSpace::Autonomous)],
            vec![(
                "src/api.ts",
                vec![global_fixture("fetch", MockValueSpace::LiveFirst)],
            )],
            vec![(
                "src/api.ts:getUser",
                vec![global_fixture(
                    "fetch",
                    MockValueSpace::Seeded {
                        seed_file: "seeds/fetch.jsonl".into(),
                    },
                )],
            )],
        );

        let result = resolve_fixture(&config, "src/api.ts", "getUser", "fetch");
        assert!(matches!(
            result.unwrap().value_space,
            MockValueSpace::Seeded { .. }
        ));
    }

    #[test]
    fn resolve_different_symbols_at_different_scopes() {
        let config = config_with_scopes(
            vec![global_fixture("fetch", MockValueSpace::Autonomous)],
            vec![(
                "src/api.ts",
                vec![global_fixture("db.query", MockValueSpace::LiveFirst)],
            )],
            vec![],
        );

        // fetch from global
        let fetch = resolve_fixture(&config, "src/api.ts", "getUser", "fetch");
        assert_eq!(fetch.unwrap().value_space, MockValueSpace::Autonomous);

        // db.query from file
        let db = resolve_fixture(&config, "src/api.ts", "getUser", "db.query");
        assert_eq!(db.unwrap().value_space, MockValueSpace::LiveFirst);
    }

    #[test]
    fn resolve_unknown_symbol_returns_none() {
        let config = config_with_scopes(
            vec![global_fixture("fetch", MockValueSpace::Autonomous)],
            vec![],
            vec![],
        );

        assert!(resolve_fixture(&config, "src/api.ts", "getUser", "unknown").is_none());
    }

    #[test]
    fn resolve_wrong_file_falls_to_global() {
        let config = config_with_scopes(
            vec![global_fixture("fetch", MockValueSpace::Autonomous)],
            vec![(
                "src/api.ts",
                vec![global_fixture("fetch", MockValueSpace::LiveFirst)],
            )],
            vec![],
        );

        // Different file — should get global, not file-level.
        let result = resolve_fixture(&config, "src/other.ts", "getUser", "fetch");
        assert_eq!(result.unwrap().value_space, MockValueSpace::Autonomous);
    }

    // -- Type validation tests --

    #[test]
    fn validate_matching_types_passes() {
        let config = config_with_scopes(
            vec![fixture_with_values(
                "db.query",
                vec![json!(42), json!(-1)],
                TypeInfo::Int { int_width: None, int_signed: None },
            )],
            vec![],
            vec![],
        );

        let errors = validate_fixture_types(&config);
        assert!(errors.is_empty());
    }

    #[test]
    fn validate_type_mismatch_detected() {
        let config = config_with_scopes(
            vec![fixture_with_values(
                "db.query",
                vec![json!(42), json!("not_an_int")],
                TypeInfo::Int { int_width: None, int_signed: None },
            )],
            vec![],
            vec![],
        );

        let errors = validate_fixture_types(&config);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            FixtureValidationError::TypeMismatch { symbol, index: 1, .. } if symbol == "db.query"
        ));
    }

    #[test]
    fn validate_empty_fixed_set_detected() {
        let fixture = MockFixture {
            symbol: "fetch".into(),
            value_space: MockValueSpace::FixedSet {
                values: vec![json!(1)],
            },
            return_values: None, // Missing!
            return_type: None,
            expectations: None,
        };

        let config = MockFixtureConfig {
            global: MockFixtureScope {
                fixtures: [("fetch".into(), fixture)].into_iter().collect(),
            },
            ..Default::default()
        };

        let errors = validate_fixture_types(&config);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            FixtureValidationError::EmptyFixedSet { symbol } if symbol == "fetch"
        ));
    }

    #[test]
    fn validate_nullable_type_accepts_null() {
        let config = config_with_scopes(
            vec![fixture_with_values(
                "db.findOne",
                vec![json!(null), json!("found")],
                TypeInfo::Nullable {
                    inner: Box::new(TypeInfo::Str),
                },
            )],
            vec![],
            vec![],
        );

        let errors = validate_fixture_types(&config);
        assert!(errors.is_empty());
    }

    #[test]
    fn validate_union_type_accepts_any_variant() {
        let config = config_with_scopes(
            vec![fixture_with_values(
                "parse",
                vec![json!(42), json!("hello")],
                TypeInfo::Union {
                    variants: vec![TypeInfo::Int { int_width: None, int_signed: None }, TypeInfo::Str],
                    enum_values: Vec::new(),
                },
            )],
            vec![],
            vec![],
        );

        let errors = validate_fixture_types(&config);
        assert!(errors.is_empty());
    }

    #[test]
    fn validate_array_type_checks_elements() {
        let config = config_with_scopes(
            vec![fixture_with_values(
                "getIds",
                vec![json!([1, 2, 3]), json!([1, "oops"])],
                TypeInfo::Array {
                    element: Box::new(TypeInfo::Int { int_width: None, int_signed: None }),
                },
            )],
            vec![],
            vec![],
        );

        let errors = validate_fixture_types(&config);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            FixtureValidationError::TypeMismatch { index: 1, .. }
        ));
    }

    #[test]
    fn validate_object_type_checks_fields() {
        let config = config_with_scopes(
            vec![fixture_with_values(
                "getUser",
                vec![json!({"id": 1, "name": "Alice"}), json!({"id": "bad"})],
                TypeInfo::Object {
                    fields: vec![("id".into(), TypeInfo::Int { int_width: None, int_signed: None })],
                },
            )],
            vec![],
            vec![],
        );

        let errors = validate_fixture_types(&config);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn validate_unknown_type_accepts_anything() {
        let config = config_with_scopes(
            vec![fixture_with_values(
                "mystery",
                vec![json!(42), json!("hi"), json!(null)],
                TypeInfo::Unknown,
            )],
            vec![],
            vec![],
        );

        assert!(validate_fixture_types(&config).is_empty());
    }

    // -- Expectation validation tests --

    #[test]
    fn validate_expectations_exact_count_pass() {
        let fixture = MockFixture {
            symbol: "db.query".into(),
            value_space: MockValueSpace::Autonomous,
            return_values: None,
            return_type: None,
            expectations: Some(MockExpectations {
                called_with: None,
                call_count: Some(CallCountExpectation::Exact { n: 2 }),
            }),
        };

        let calls = vec![
            MockCallRecord {
                args: vec![json!("a")],
            },
            MockCallRecord {
                args: vec![json!("b")],
            },
        ];

        assert!(validate_expectations(&fixture, &calls).is_empty());
    }

    #[test]
    fn validate_expectations_exact_count_fail() {
        let fixture = MockFixture {
            symbol: "db.query".into(),
            value_space: MockValueSpace::Autonomous,
            return_values: None,
            return_type: None,
            expectations: Some(MockExpectations {
                called_with: None,
                call_count: Some(CallCountExpectation::Exact { n: 3 }),
            }),
        };

        let calls = vec![MockCallRecord {
            args: vec![json!("a")],
        }];
        let errors = validate_expectations(&fixture, &calls);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            ExpectationError::CallCountMismatch { actual: 1, .. }
        ));
    }

    #[test]
    fn validate_expectations_at_least() {
        let fixture = MockFixture {
            symbol: "fetch".into(),
            value_space: MockValueSpace::Autonomous,
            return_values: None,
            return_type: None,
            expectations: Some(MockExpectations {
                called_with: None,
                call_count: Some(CallCountExpectation::AtLeast { n: 2 }),
            }),
        };

        // 1 call — fails
        let errors = validate_expectations(&fixture, &[MockCallRecord { args: vec![] }]);
        assert_eq!(errors.len(), 1);

        // 3 calls — passes
        let calls: Vec<_> = (0..3).map(|_| MockCallRecord { args: vec![] }).collect();
        assert!(validate_expectations(&fixture, &calls).is_empty());
    }

    #[test]
    fn validate_expectations_between() {
        let fixture = MockFixture {
            symbol: "fetch".into(),
            value_space: MockValueSpace::Autonomous,
            return_values: None,
            return_type: None,
            expectations: Some(MockExpectations {
                called_with: None,
                call_count: Some(CallCountExpectation::Between { min: 2, max: 4 }),
            }),
        };

        // 1 call — too few
        let errors = validate_expectations(&fixture, &[MockCallRecord { args: vec![] }]);
        assert_eq!(errors.len(), 1);

        // 3 calls — in range
        let calls: Vec<_> = (0..3).map(|_| MockCallRecord { args: vec![] }).collect();
        assert!(validate_expectations(&fixture, &calls).is_empty());

        // 5 calls — too many
        let calls: Vec<_> = (0..5).map(|_| MockCallRecord { args: vec![] }).collect();
        assert_eq!(validate_expectations(&fixture, &calls).len(), 1);
    }

    #[test]
    fn validate_expectations_called_with_match() {
        let fixture = MockFixture {
            symbol: "db.query".into(),
            value_space: MockValueSpace::Autonomous,
            return_values: None,
            return_type: None,
            expectations: Some(MockExpectations {
                called_with: Some(vec![
                    vec![
                        ArgMatcher::Exact {
                            value: json!("SELECT"),
                        },
                        ArgMatcher::Any,
                    ],
                    vec![
                        ArgMatcher::Exact {
                            value: json!("INSERT"),
                        },
                        ArgMatcher::Any,
                    ],
                ]),
                call_count: None,
            }),
        };

        let calls = vec![
            MockCallRecord {
                args: vec![json!("SELECT"), json!(1)],
            },
            MockCallRecord {
                args: vec![json!("INSERT"), json!({"name": "Bob"})],
            },
        ];

        assert!(validate_expectations(&fixture, &calls).is_empty());
    }

    #[test]
    fn validate_expectations_called_with_mismatch() {
        let fixture = MockFixture {
            symbol: "db.query".into(),
            value_space: MockValueSpace::Autonomous,
            return_values: None,
            return_type: None,
            expectations: Some(MockExpectations {
                called_with: Some(vec![vec![ArgMatcher::Exact {
                    value: json!("SELECT"),
                }]]),
                call_count: None,
            }),
        };

        let calls = vec![MockCallRecord {
            args: vec![json!("DELETE")],
        }];

        let errors = validate_expectations(&fixture, &calls);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            ExpectationError::ArgMismatch { call_index: 0, .. }
        ));
    }

    #[test]
    fn validate_expectations_typeof_matcher() {
        let fixture = MockFixture {
            symbol: "api.call".into(),
            value_space: MockValueSpace::Autonomous,
            return_values: None,
            return_type: None,
            expectations: Some(MockExpectations {
                called_with: Some(vec![vec![ArgMatcher::TypeOf {
                    expected: TypeInfo::Str,
                }]]),
                call_count: None,
            }),
        };

        // String arg — matches
        let calls = vec![MockCallRecord {
            args: vec![json!("hello")],
        }];
        assert!(validate_expectations(&fixture, &calls).is_empty());

        // Int arg — doesn't match
        let calls = vec![MockCallRecord {
            args: vec![json!(42)],
        }];
        assert_eq!(validate_expectations(&fixture, &calls).len(), 1);
    }

    #[test]
    fn validate_expectations_none_always_passes() {
        let fixture = MockFixture {
            symbol: "fetch".into(),
            value_space: MockValueSpace::Autonomous,
            return_values: None,
            return_type: None,
            expectations: None,
        };

        let calls = vec![MockCallRecord {
            args: vec![json!(1)],
        }];
        assert!(validate_expectations(&fixture, &calls).is_empty());
    }

    #[test]
    fn validate_expectations_arity_mismatch() {
        let fixture = MockFixture {
            symbol: "fn".into(),
            value_space: MockValueSpace::Autonomous,
            return_values: None,
            return_type: None,
            expectations: Some(MockExpectations {
                called_with: Some(vec![vec![ArgMatcher::Any, ArgMatcher::Any]]),
                call_count: None,
            }),
        };

        // Call with 1 arg vs pattern expecting 2 → mismatch
        let calls = vec![MockCallRecord {
            args: vec![json!(1)],
        }];
        assert_eq!(validate_expectations(&fixture, &calls).len(), 1);
    }

    // -- json_matches_type unit tests --

    #[test]
    fn json_matches_int() {
        assert!(json_matches_type(&json!(42), &TypeInfo::Int { int_width: None, int_signed: None }));
        assert!(!json_matches_type(&json!(2.5), &TypeInfo::Int { int_width: None, int_signed: None }));
        assert!(!json_matches_type(&json!("hi"), &TypeInfo::Int { int_width: None, int_signed: None }));
    }

    #[test]
    fn json_matches_float() {
        assert!(json_matches_type(&json!(2.5), &TypeInfo::Float));
        assert!(json_matches_type(&json!(42), &TypeInfo::Float)); // ints coerce to f64
        assert!(!json_matches_type(&json!("hi"), &TypeInfo::Float));
    }

    #[test]
    fn json_matches_nested_array() {
        let ty = TypeInfo::Array {
            element: Box::new(TypeInfo::Array {
                element: Box::new(TypeInfo::Int { int_width: None, int_signed: None }),
            }),
        };
        assert!(json_matches_type(&json!([[1, 2], [3]]), &ty));
        assert!(!json_matches_type(&json!([[1, "x"]]), &ty));
    }

    // -- Proptest --

    mod proptests {
        use super::super::*;
        use proptest::prelude::*;
        use serde_json::json;

        fn arb_mock_value_space() -> impl Strategy<Value = MockValueSpace> {
            prop_oneof![
                Just(MockValueSpace::LiveFirst),
                prop::collection::vec(
                    prop_oneof![
                        Just(json!(null)),
                        Just(json!(42)),
                        Just(json!("test")),
                        Just(json!(true)),
                    ],
                    0..5,
                )
                .prop_map(|values| MockValueSpace::FixedSet { values }),
                Just(MockValueSpace::Autonomous),
                "[a-z/_]{1,20}".prop_map(|seed_file| MockValueSpace::Seeded { seed_file }),
            ]
        }

        fn arb_type_info_leaf() -> impl Strategy<Value = TypeInfo> {
            prop_oneof![
                Just(TypeInfo::Int { int_width: None, int_signed: None }),
                Just(TypeInfo::Float),
                Just(TypeInfo::Str),
                Just(TypeInfo::Bool),
                Just(TypeInfo::Unknown),
            ]
        }

        fn arb_arg_matcher() -> impl Strategy<Value = ArgMatcher> {
            prop_oneof![
                prop_oneof![
                    Just(json!(null)),
                    Just(json!(42)),
                    Just(json!("test")),
                    Just(json!(true)),
                ]
                .prop_map(|value| ArgMatcher::Exact { value }),
                Just(ArgMatcher::Any),
                arb_type_info_leaf().prop_map(|expected| ArgMatcher::TypeOf { expected }),
            ]
        }

        fn arb_call_count() -> impl Strategy<Value = CallCountExpectation> {
            prop_oneof![
                (0u32..100).prop_map(|n| CallCountExpectation::Exact { n }),
                (0u32..100).prop_map(|n| CallCountExpectation::AtLeast { n }),
                (0u32..100).prop_map(|n| CallCountExpectation::AtMost { n }),
                (0u32..50, 0u32..50).prop_map(|(a, b)| CallCountExpectation::Between {
                    min: a.min(b),
                    max: a.max(b),
                }),
            ]
        }

        fn arb_expectations() -> impl Strategy<Value = MockExpectations> {
            let called_with = prop::option::of(prop::collection::vec(
                prop::collection::vec(arb_arg_matcher(), 0..4),
                0..4,
            ));
            let call_count = prop::option::of(arb_call_count());
            (called_with, call_count).prop_map(|(called_with, call_count)| MockExpectations {
                called_with,
                call_count,
            })
        }

        fn arb_mock_fixture() -> impl Strategy<Value = MockFixture> {
            let symbol = "[a-zA-Z_.]{1,20}";
            let value_space = arb_mock_value_space();
            let return_values = prop::option::of(prop::collection::vec(
                prop_oneof![Just(json!(1)), Just(json!("x")), Just(json!(null))],
                0..5,
            ));
            let return_type = prop::option::of(arb_type_info_leaf());
            let expectations = prop::option::of(arb_expectations());

            (
                symbol,
                value_space,
                return_values,
                return_type,
                expectations,
            )
                .prop_map(
                    |(symbol, value_space, return_values, return_type, expectations)| MockFixture {
                        symbol,
                        value_space,
                        return_values,
                        return_type,
                        expectations,
                    },
                )
        }

        fn arb_fixture_scope() -> impl Strategy<Value = MockFixtureScope> {
            prop::collection::hash_map("[a-z_.]{1,10}", arb_mock_fixture(), 0..4).prop_map(
                |fixtures| MockFixtureScope {
                    fixtures: fixtures
                        .into_iter()
                        .map(|(k, mut f)| {
                            f.symbol = k.clone();
                            (k, f)
                        })
                        .collect(),
                },
            )
        }

        fn arb_fixture_config() -> impl Strategy<Value = MockFixtureConfig> {
            let global = arb_fixture_scope();
            let files = prop::collection::hash_map("[a-z/]{1,15}", arb_fixture_scope(), 0..3);
            let functions = prop::collection::hash_map("[a-z/:]{1,20}", arb_fixture_scope(), 0..3);

            (global, files, functions).prop_map(|(global, files, functions)| MockFixtureConfig {
                global,
                files,
                functions,
            })
        }

        proptest! {
            #[test]
            fn fixture_yaml_roundtrip(fixture in arb_mock_fixture()) {
                let yaml = serde_yaml::to_string(&fixture).unwrap();
                let back: MockFixture = serde_yaml::from_str(&yaml).unwrap();
                prop_assert_eq!(&back, &fixture);
            }

            #[test]
            fn fixture_json_roundtrip(fixture in arb_mock_fixture()) {
                let json_str = serde_json::to_string(&fixture).unwrap();
                let back: MockFixture = serde_json::from_str(&json_str).unwrap();
                prop_assert_eq!(&back, &fixture);
            }

            #[test]
            fn config_yaml_roundtrip(config in arb_fixture_config()) {
                let yaml = serde_yaml::to_string(&config).unwrap();
                let back: MockFixtureConfig = serde_yaml::from_str(&yaml).unwrap();
                prop_assert_eq!(&back, &config);
            }

            #[test]
            fn arg_matcher_json_roundtrip(matcher in arb_arg_matcher()) {
                let json_str = serde_json::to_string(&matcher).unwrap();
                let back: ArgMatcher = serde_json::from_str(&json_str).unwrap();
                prop_assert_eq!(&back, &matcher);
            }

            #[test]
            fn call_count_json_roundtrip(exp in arb_call_count()) {
                let json_str = serde_json::to_string(&exp).unwrap();
                let back: CallCountExpectation = serde_json::from_str(&json_str).unwrap();
                prop_assert_eq!(&back, &exp);
            }

            // Semantic: resolve_fixture always prefers innermost scope.
            #[test]
            fn resolve_prefers_function_over_file(
                vs_global in arb_mock_value_space(),
                vs_file in arb_mock_value_space(),
                vs_func in arb_mock_value_space(),
            ) {
                let config = MockFixtureConfig {
                    global: MockFixtureScope {
                        fixtures: [(
                            "s".into(),
                            MockFixture {
                                symbol: "s".into(),
                                value_space: vs_global,
                                return_values: None,
                                return_type: None,
                                expectations: None,
                            },
                        )]
                        .into_iter()
                        .collect(),
                    },
                    files: [(
                        "f".into(),
                        MockFixtureScope {
                            fixtures: [(
                                "s".into(),
                                MockFixture {
                                    symbol: "s".into(),
                                    value_space: vs_file,
                                    return_values: None,
                                    return_type: None,
                                    expectations: None,
                                },
                            )]
                            .into_iter()
                            .collect(),
                        },
                    )]
                    .into_iter()
                    .collect(),
                    functions: [(
                        "f:fn".into(),
                        MockFixtureScope {
                            fixtures: [(
                                "s".into(),
                                MockFixture {
                                    symbol: "s".into(),
                                    value_space: vs_func.clone(),
                                    return_values: None,
                                    return_type: None,
                                    expectations: None,
                                },
                            )]
                            .into_iter()
                            .collect(),
                        },
                    )]
                    .into_iter()
                    .collect(),
                };

                let resolved = resolve_fixture(&config, "f", "fn", "s").unwrap();
                prop_assert_eq!(&resolved.value_space, &vs_func);
            }

            // Semantic: validate_fixture_types catches all type mismatches.
            #[test]
            fn validation_catches_string_as_int(s in "[a-z]{1,10}") {
                let config = MockFixtureConfig {
                    global: MockFixtureScope {
                        fixtures: [(
                            "x".into(),
                            MockFixture {
                                symbol: "x".into(),
                                value_space: MockValueSpace::FixedSet { values: vec![json!(s.clone())] },
                                return_values: Some(vec![json!(s)]),
                                return_type: Some(TypeInfo::Int { int_width: None, int_signed: None }),
                                expectations: None,
                            },
                        )]
                        .into_iter()
                        .collect(),
                    },
                    ..Default::default()
                };

                let errors = validate_fixture_types(&config);
                prop_assert!(!errors.is_empty(), "should detect string-as-int mismatch");
            }

            // Semantic: exact call count validates correctly.
            #[test]
            fn exact_count_passes_iff_equal(expected in 0u32..20, actual in 0u32..20) {
                let fixture = MockFixture {
                    symbol: "f".into(),
                    value_space: MockValueSpace::Autonomous,
                    return_values: None,
                    return_type: None,
                    expectations: Some(MockExpectations {
                        called_with: None,
                        call_count: Some(CallCountExpectation::Exact { n: expected }),
                    }),
                };

                let calls: Vec<_> = (0..actual).map(|_| MockCallRecord { args: vec![] }).collect();
                let errors = validate_expectations(&fixture, &calls);

                if expected == actual {
                    prop_assert!(errors.is_empty());
                } else {
                    prop_assert!(!errors.is_empty());
                }
            }
        }
    }
}
