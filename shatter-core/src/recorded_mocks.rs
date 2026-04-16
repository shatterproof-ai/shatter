//! Recording mode: capture external dependency call I/O during exploration
//! and persist as YAML fixtures for seeding future autonomous runs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::execution_record::ExternalCall;
use crate::protocol::{ExecuteResult, ExternalDependency, MockBehavior, MockConfig};

/// Subdirectory within `shatter-artifacts/` for recorded mock fixtures.
///
/// Legacy location was `.shatter/recorded-mocks/`; callers should check both.
pub const RECORDED_MOCKS_DIR: &str = "recorded-mocks";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single observed external call during a recording session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DepObservation {
    /// Arguments passed to the external call.
    pub args: Vec<serde_json::Value>,
    /// Return value from the call (or null if the call threw).
    pub return_value: serde_json::Value,
    /// Error message if the call failed, None on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Wall-clock time of the individual execution that contained this call.
    pub latency_ms: f64,
}

/// Aggregated recordings for a single external dependency symbol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalDepBehavior {
    /// Fully qualified symbol name (e.g. "axios:get").
    pub symbol: String,
    /// Module the symbol is imported from.
    pub source_module: String,
    /// All observed call instances.
    pub observations: Vec<DepObservation>,
}

/// Top-level structure persisted as a YAML fixture file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordedMockFile {
    /// Function that was explored.
    pub function_id: String,
    /// Source file containing the function.
    pub file: String,
    /// ISO 8601 timestamp of the recording session.
    pub recorded_at: String,
    /// Per-dependency behavior observations.
    pub dependencies: Vec<ExternalDepBehavior>,
}

/// Errors from recording I/O operations.
#[derive(Debug)]
pub enum RecordError {
    Io(std::io::Error),
    Yaml(serde_yaml::Error),
}

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Yaml(e) => write!(f, "YAML error: {e}"),
        }
    }
}

impl std::error::Error for RecordError {}

impl From<std::io::Error> for RecordError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_yaml::Error> for RecordError {
    fn from(e: serde_yaml::Error) -> Self {
        Self::Yaml(e)
    }
}

// ---------------------------------------------------------------------------
// Building passthrough mocks for record mode
// ---------------------------------------------------------------------------

/// Create `MockConfig` entries that pass through to real implementations
/// while tracking all calls. Used in `--record` mode.
pub fn build_passthrough_mocks(deps: &[ExternalDependency]) -> Vec<MockConfig> {
    deps.iter()
        .map(|dep| MockConfig {
            symbol: dep.symbol.clone(),
            return_values: vec![],
            should_track_calls: true,
            default_behavior: MockBehavior::Passthrough,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Aggregation: raw execution results → per-symbol behavior maps
// ---------------------------------------------------------------------------

/// Aggregate `calls_to_external` from exploration results into per-symbol
/// behavior maps.
pub fn aggregate_recordings(
    raw_results: &[(Vec<serde_json::Value>, Vec<MockConfig>, ExecuteResult)],
    deps: &[ExternalDependency],
) -> Vec<ExternalDepBehavior> {
    // Build a lookup from symbol → source_module for enrichment.
    let module_lookup: HashMap<&str, &str> = deps
        .iter()
        .map(|d| (d.symbol.as_str(), d.source_module.as_str()))
        .collect();

    // Group observations by symbol.
    let mut by_symbol: HashMap<String, Vec<DepObservation>> = HashMap::new();

    for (_inputs, _mocks, exec_result) in raw_results {
        let latency = exec_result.performance.wall_time_ms;
        for call in &exec_result.calls_to_external {
            let obs = external_call_to_observation(call, latency);
            by_symbol.entry(call.symbol.clone()).or_default().push(obs);
        }
    }

    // Build sorted output for deterministic YAML.
    let mut behaviors: Vec<ExternalDepBehavior> = by_symbol
        .into_iter()
        .map(|(symbol, observations)| {
            let source_module = module_lookup
                .get(symbol.as_str())
                .copied()
                .unwrap_or("")
                .to_string();
            ExternalDepBehavior {
                symbol,
                source_module,
                observations,
            }
        })
        .collect();

    behaviors.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    behaviors
}

fn external_call_to_observation(call: &ExternalCall, latency_ms: f64) -> DepObservation {
    DepObservation {
        args: call.args.clone(),
        return_value: call.return_value.clone(),
        error: None,
        latency_ms,
    }
}

// ---------------------------------------------------------------------------
// Build the top-level recording file
// ---------------------------------------------------------------------------

/// Wrap aggregated behaviors into a serializable file structure.
pub fn build_recorded_mock_file(
    function_id: &str,
    file: &str,
    dependencies: Vec<ExternalDepBehavior>,
) -> RecordedMockFile {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let recorded_at = format!("{}Z", now.as_secs());

    RecordedMockFile {
        function_id: function_id.to_string(),
        file: file.to_string(),
        recorded_at,
        dependencies,
    }
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

/// Sanitize a file path for use as a directory component.
fn sanitize_path_component(path: &str) -> String {
    path.replace(['/', '\\'], "_")
        .trim_start_matches('_')
        .to_string()
}

/// Compute the output path for a recorded mock file.
pub fn recorded_mock_path(shatter_dir: &Path, file: &str, function: &str) -> PathBuf {
    shatter_dir
        .join(RECORDED_MOCKS_DIR)
        .join(sanitize_path_component(file))
        .join(format!("{function}.yaml"))
}

/// Save a recorded mock file to `<base_dir>/recorded-mocks/`.
pub fn save_recorded_mocks(
    mock_file: &RecordedMockFile,
    shatter_dir: &Path,
) -> Result<PathBuf, RecordError> {
    let path = recorded_mock_path(shatter_dir, &mock_file.file, &mock_file.function_id);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let yaml = serde_yaml::to_string(mock_file)?;
    std::fs::write(&path, yaml)?;

    Ok(path)
}

/// Load a recorded mock file from disk.
pub fn load_recorded_mocks(path: &Path) -> Result<RecordedMockFile, RecordError> {
    let contents = std::fs::read_to_string(path)?;
    let mock_file: RecordedMockFile = serde_yaml::from_str(&contents)?;
    Ok(mock_file)
}

/// Check if a recorded mock file exists for this file+function pair.
pub fn find_recorded_mocks(shatter_dir: &Path, file: &str, function: &str) -> Option<PathBuf> {
    let path = recorded_mock_path(shatter_dir, file, function);
    if path.exists() { Some(path) } else { None }
}

// ---------------------------------------------------------------------------
// Conversion: recorded mocks → MockConfig for seeded replay
// ---------------------------------------------------------------------------

/// Convert recorded observations into `MockConfig` entries for seeded replay.
///
/// Each dependency's observed return values become the `return_values` array,
/// with `RepeatLast` as the fallback behavior.
pub fn recorded_mocks_to_mock_configs(mock_file: &RecordedMockFile) -> Vec<MockConfig> {
    mock_file
        .dependencies
        .iter()
        .map(|dep| {
            let return_values: Vec<serde_json::Value> = dep
                .observations
                .iter()
                .map(|obs| obs.return_value.clone())
                .collect();

            MockConfig {
                symbol: dep.symbol.clone(),
                return_values,
                should_track_calls: true,
                default_behavior: MockBehavior::RepeatLast,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ExecuteResult, PerformanceMetrics};
    use serde_json::json;

    fn round_trip<T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug>(
        value: &T,
    ) {
        let yaml = serde_yaml::to_string(value).expect("serialize");
        let back: T = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(*value, back, "roundtrip failed for yaml:\n{yaml}");
    }

    fn sample_observation() -> DepObservation {
        DepObservation {
            args: vec![json!("90210"), json!(true)],
            return_value: json!({"rate": 12.99}),
            error: None,
            latency_ms: 1.5,
        }
    }

    fn sample_observation_with_error() -> DepObservation {
        DepObservation {
            args: vec![json!("invalid")],
            return_value: json!(null),
            error: Some("404 Not Found".into()),
            latency_ms: 0.8,
        }
    }

    fn sample_behavior() -> ExternalDepBehavior {
        ExternalDepBehavior {
            symbol: "rateService:getExpressRate".into(),
            source_module: "rate-service".into(),
            observations: vec![sample_observation(), sample_observation_with_error()],
        }
    }

    fn sample_mock_file() -> RecordedMockFile {
        RecordedMockFile {
            function_id: "calculateShipping".into(),
            file: "src/shipping.ts".into(),
            recorded_at: "2026-03-12T01:00:00Z".into(),
            dependencies: vec![sample_behavior()],
        }
    }

    fn empty_exec_result() -> ExecuteResult {
        ExecuteResult {
            return_value: Some(json!(null)),
            thrown_error: None,
            branch_path: vec![],
            scope_events: vec![],
            loop_body_states: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            side_effects: vec![],
            path_constraints: vec![],
            performance: PerformanceMetrics {
                wall_time_ms: 1.0,
                cpu_time_us: 800,
                heap_used_bytes: 1024,
                heap_allocated_bytes: 2048,
            },
            capture_truncation: None,
            discovered_dependencies: vec![],
            connection_failures: vec![],
            runtime_crypto_boundaries: vec![],
        }
    }

    // -- Serde roundtrip tests --

    #[test]
    fn dep_observation_serde_roundtrip() {
        round_trip(&sample_observation());
    }

    #[test]
    fn dep_observation_with_error_serde_roundtrip() {
        round_trip(&sample_observation_with_error());
    }

    #[test]
    fn external_dep_behavior_serde_roundtrip() {
        round_trip(&sample_behavior());
    }

    #[test]
    fn recorded_mock_file_serde_roundtrip() {
        round_trip(&sample_mock_file());
    }

    // -- build_passthrough_mocks --

    #[test]
    fn passthrough_mocks_all_passthrough_and_tracked() {
        let deps = vec![
            ExternalDependency {
                kind: crate::protocol::DependencyKind::FunctionCall,
                symbol: "fs:readFileSync".into(),
                source_module: "fs".into(),
                return_type: crate::types::TypeInfo::Str,
                param_types: vec![crate::types::TypeInfo::Str],
                call_sites: vec![10],
            },
            ExternalDependency {
                kind: crate::protocol::DependencyKind::FunctionCall,
                symbol: "axios:get".into(),
                source_module: "axios".into(),
                return_type: crate::types::TypeInfo::Str,
                param_types: vec![crate::types::TypeInfo::Str],
                call_sites: vec![20, 30],
            },
        ];

        let mocks = build_passthrough_mocks(&deps);
        assert_eq!(mocks.len(), 2);
        for mock in &mocks {
            assert_eq!(mock.default_behavior, MockBehavior::Passthrough);
            assert!(mock.should_track_calls);
            assert!(mock.return_values.is_empty());
        }
        assert_eq!(mocks[0].symbol, "fs:readFileSync");
        assert_eq!(mocks[1].symbol, "axios:get");
    }

    #[test]
    fn passthrough_mocks_empty_deps() {
        assert!(build_passthrough_mocks(&[]).is_empty());
    }

    // -- aggregate_recordings --

    #[test]
    fn aggregate_groups_by_symbol() {
        let call1 = ExternalCall {
            symbol: "db:query".into(),
            args: vec![json!("SELECT 1")],
            return_value: json!({"rows": []}),
        };
        let call2 = ExternalCall {
            symbol: "db:query".into(),
            args: vec![json!("SELECT 2")],
            return_value: json!({"rows": [1]}),
        };
        let call3 = ExternalCall {
            symbol: "cache:get".into(),
            args: vec![json!("key1")],
            return_value: json!("value1"),
        };

        let mut result1 = empty_exec_result();
        result1.calls_to_external = vec![call1, call3];
        let mut result2 = empty_exec_result();
        result2.calls_to_external = vec![call2];

        let raw_results = vec![
            (vec![json!(1)], vec![], result1),
            (vec![json!(2)], vec![], result2),
        ];

        let deps = vec![
            ExternalDependency {
                kind: crate::protocol::DependencyKind::FunctionCall,
                symbol: "db:query".into(),
                source_module: "database".into(),
                return_type: crate::types::TypeInfo::Str,
                param_types: vec![],
                call_sites: vec![],
            },
            ExternalDependency {
                kind: crate::protocol::DependencyKind::FunctionCall,
                symbol: "cache:get".into(),
                source_module: "cache-lib".into(),
                return_type: crate::types::TypeInfo::Str,
                param_types: vec![],
                call_sites: vec![],
            },
        ];

        let behaviors = aggregate_recordings(&raw_results, &deps);
        assert_eq!(behaviors.len(), 2);

        // Sorted by symbol: cache:get first, then db:query.
        assert_eq!(behaviors[0].symbol, "cache:get");
        assert_eq!(behaviors[0].source_module, "cache-lib");
        assert_eq!(behaviors[0].observations.len(), 1);

        assert_eq!(behaviors[1].symbol, "db:query");
        assert_eq!(behaviors[1].source_module, "database");
        assert_eq!(behaviors[1].observations.len(), 2);
    }

    #[test]
    fn aggregate_empty_results() {
        let behaviors = aggregate_recordings(&[], &[]);
        assert!(behaviors.is_empty());
    }

    #[test]
    fn aggregate_no_external_calls() {
        let result = empty_exec_result();
        let raw_results = vec![(vec![], vec![], result)];
        let behaviors = aggregate_recordings(&raw_results, &[]);
        assert!(behaviors.is_empty());
    }

    #[test]
    fn aggregate_preserves_args_and_return_values() {
        let call = ExternalCall {
            symbol: "api:fetch".into(),
            args: vec![json!("https://example.com"), json!({"method": "POST"})],
            return_value: json!({"status": 200, "data": [1, 2, 3]}),
        };

        let mut result = empty_exec_result();
        result.calls_to_external = vec![call];

        let raw_results = vec![(vec![], vec![], result)];
        let behaviors = aggregate_recordings(&raw_results, &[]);

        assert_eq!(behaviors.len(), 1);
        let obs = &behaviors[0].observations[0];
        assert_eq!(
            obs.args,
            vec![json!("https://example.com"), json!({"method": "POST"})]
        );
        assert_eq!(obs.return_value, json!({"status": 200, "data": [1, 2, 3]}));
        assert!(obs.error.is_none());
    }

    // -- recorded_mocks_to_mock_configs --

    #[test]
    fn conversion_to_mock_configs() {
        let mock_file = sample_mock_file();
        let configs = recorded_mocks_to_mock_configs(&mock_file);

        assert_eq!(configs.len(), 1);
        let config = &configs[0];
        assert_eq!(config.symbol, "rateService:getExpressRate");
        assert_eq!(config.return_values.len(), 2);
        assert_eq!(config.return_values[0], json!({"rate": 12.99}));
        assert_eq!(config.return_values[1], json!(null));
        assert!(config.should_track_calls);
        assert_eq!(config.default_behavior, MockBehavior::RepeatLast);
    }

    // -- save/load roundtrip --

    #[test]
    fn save_load_roundtrip() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let mock_file = sample_mock_file();

        let path = save_recorded_mocks(&mock_file, tmp.path()).expect("save");
        assert!(path.exists());
        assert!(path.to_string_lossy().contains(RECORDED_MOCKS_DIR));

        let loaded = load_recorded_mocks(&path).expect("load");
        // Compare everything except recorded_at (timestamp).
        assert_eq!(loaded.function_id, mock_file.function_id);
        assert_eq!(loaded.file, mock_file.file);
        assert_eq!(loaded.dependencies, mock_file.dependencies);
    }

    // -- sanitize_path_component --

    #[test]
    fn sanitize_strips_slashes() {
        assert_eq!(sanitize_path_component("src/foo/bar.ts"), "src_foo_bar.ts");
    }

    #[test]
    fn sanitize_strips_leading_underscore() {
        assert_eq!(
            sanitize_path_component("/absolute/path.ts"),
            "absolute_path.ts"
        );
    }

    #[test]
    fn sanitize_handles_backslashes() {
        assert_eq!(
            sanitize_path_component("src\\foo\\bar.ts"),
            "src_foo_bar.ts"
        );
    }

    // -- Proptest --

    mod proptests {
        use super::super::*;
        use proptest::prelude::*;
        use serde_json::json;

        fn arb_dep_observation() -> impl Strategy<Value = DepObservation> {
            (
                prop::collection::vec(
                    prop_oneof![
                        Just(json!(null)),
                        Just(json!(42)),
                        Just(json!("test")),
                        Just(json!(true)),
                    ],
                    0..4,
                ),
                prop_oneof![
                    Just(json!(null)),
                    Just(json!(200)),
                    Just(json!("ok")),
                    Just(json!({"status": 200})),
                ],
                prop::option::of("[a-z ]{0,30}".prop_map(String::from)),
                0.0f64..1000.0,
            )
                .prop_map(|(args, return_value, error, latency_ms)| DepObservation {
                    args,
                    return_value,
                    error,
                    latency_ms,
                })
        }

        fn arb_external_dep_behavior() -> impl Strategy<Value = ExternalDepBehavior> {
            (
                "[a-z]+:[a-zA-Z]+",
                "[a-z-]+",
                prop::collection::vec(arb_dep_observation(), 0..5),
            )
                .prop_map(|(symbol, source_module, observations)| {
                    ExternalDepBehavior {
                        symbol,
                        source_module,
                        observations,
                    }
                })
        }

        proptest! {
            #[test]
            fn dep_observation_roundtrip(obs in arb_dep_observation()) {
                let yaml = serde_yaml::to_string(&obs).unwrap();
                let back: DepObservation = serde_yaml::from_str(&yaml).unwrap();
                prop_assert_eq!(&back.args, &obs.args);
                prop_assert_eq!(&back.return_value, &obs.return_value);
                prop_assert_eq!(&back.error, &obs.error);
            }

            #[test]
            fn external_dep_behavior_roundtrip(behavior in arb_external_dep_behavior()) {
                let yaml = serde_yaml::to_string(&behavior).unwrap();
                let back: ExternalDepBehavior = serde_yaml::from_str(&yaml).unwrap();
                prop_assert_eq!(&back.symbol, &behavior.symbol);
                prop_assert_eq!(&back.source_module, &behavior.source_module);
                prop_assert_eq!(back.observations.len(), behavior.observations.len());
            }

            #[test]
            fn passthrough_mocks_count_equals_deps(
                n in 0usize..10,
            ) {
                let deps: Vec<ExternalDependency> = (0..n)
                    .map(|i| ExternalDependency {
                        kind: crate::protocol::DependencyKind::FunctionCall,
                        symbol: format!("dep{i}"),
                        source_module: format!("mod{i}"),
                        return_type: crate::types::TypeInfo::Str,
                        param_types: vec![],
                        call_sites: vec![],
                    })
                    .collect();

                let mocks = build_passthrough_mocks(&deps);
                prop_assert_eq!(mocks.len(), deps.len());
            }
        }
    }
}
