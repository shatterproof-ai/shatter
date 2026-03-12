//! Automatic mock generation for external dependencies.
//!
//! Classifies external dependencies into categories (I/O, library, utility)
//! and generates sensible default [`MockConfig`]s without requiring user
//! configuration. Users can override defaults via `.shatter/config.yaml`.
//!
//! Error variant generation produces [`MockConfig`]s with
//! [`MockBehavior::ThrowError`] to exercise error-handling paths. The
//! [`generate_error_variant`] function dispatches on the dependency's return
//! type and I/O category to produce realistic error shapes (HTTP 4xx/5xx for
//! network mocks, connection errors for DB mocks, ENOENT for FS mocks, etc.).

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::protocol::{DependencyKind, ExternalDependency, MockBehavior, MockConfig};
use crate::scope::{DependencyAction, ScopeMatcher};
use crate::types::TypeInfo;

/// Probability that an auto-mock should use an error variant instead of a
/// success variant. Applied per-dependency during exploration to exercise
/// error-handling paths.
pub const DEFAULT_ERROR_PROBABILITY: f64 = 0.15;

/// Category of an external dependency, used to select default mock behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoCategory {
    /// File system operations (fs, path).
    FileSystem,
    /// Network operations (http, fetch, axios, net).
    Network,
    /// Database operations (pg, mysql, mongodb, redis, prisma, knex).
    Database,
    /// Pure utility functions (lodash, ramda, date-fns) — safe to call.
    PureUtility,
    /// Other external library — use type-aware stub.
    ExternalOther,
}

/// Known module prefixes for each category.
const FS_MODULES: &[&str] = &[
    "fs", "node:fs", "fs/promises", "node:fs/promises", "path", "node:path",
    // Go stdlib
    "os", "io", "io/ioutil", "bufio",
];
const NETWORK_MODULES: &[&str] = &[
    "http", "https", "node:http", "node:https", "net", "node:net",
    "axios", "node-fetch", "fetch", "got", "superagent", "request",
    "undici",
];
const DB_MODULES: &[&str] = &[
    "pg", "mysql", "mysql2", "mongodb", "redis", "ioredis",
    "prisma", "@prisma/client", "knex", "sequelize", "typeorm",
    "mongoose", "sqlite3", "better-sqlite3", "drizzle-orm",
    "database/sql", "gorm.io/gorm",
];
const PURE_UTILITY_MODULES: &[&str] = &[
    "lodash", "lodash-es", "underscore", "ramda", "date-fns",
    "dayjs", "moment", "validator", "uuid", "nanoid",
    "chalk", "debug", "ms",
    "strings", "strconv", "fmt", "math", "sort",
];

/// Classify an external dependency into an [`IoCategory`].
pub fn classify_dependency(dep: &ExternalDependency) -> IoCategory {
    let module = dep.source_module.as_str();

    if FS_MODULES.iter().any(|m| module == *m || module.starts_with(&format!("{m}/"))) {
        return IoCategory::FileSystem;
    }
    if NETWORK_MODULES.iter().any(|m| module == *m || module.starts_with(&format!("{m}/"))) {
        return IoCategory::Network;
    }
    if DB_MODULES.iter().any(|m| module == *m || module.starts_with(&format!("{m}/"))) {
        return IoCategory::Database;
    }
    if PURE_UTILITY_MODULES.iter().any(|m| module == *m || module.starts_with(&format!("{m}/"))) {
        return IoCategory::PureUtility;
    }

    IoCategory::ExternalOther
}

/// Generate a default [`MockConfig`] for a dependency based on its category.
pub fn generate_default_mock(dep: &ExternalDependency, category: IoCategory) -> MockConfig {
    match category {
        IoCategory::FileSystem => MockConfig {
            symbol: dep.symbol.clone(),
            return_values: vec![default_for_fs(&dep.symbol)],
            should_track_calls: true,
            default_behavior: MockBehavior::RepeatLast,
        },
        IoCategory::Network => MockConfig {
            symbol: dep.symbol.clone(),
            return_values: vec![json!({"status": 200, "data": {}})],
            should_track_calls: true,
            default_behavior: MockBehavior::RepeatLast,
        },
        IoCategory::Database => MockConfig {
            symbol: dep.symbol.clone(),
            return_values: vec![default_for_db(&dep.symbol)],
            should_track_calls: true,
            default_behavior: MockBehavior::RepeatLast,
        },
        IoCategory::PureUtility => MockConfig {
            symbol: dep.symbol.clone(),
            return_values: vec![],
            should_track_calls: false,
            default_behavior: MockBehavior::Passthrough,
        },
        IoCategory::ExternalOther => MockConfig {
            symbol: dep.symbol.clone(),
            return_values: vec![default_for_type(&dep.return_type)],
            should_track_calls: true,
            default_behavior: MockBehavior::RepeatLast,
        },
    }
}

/// Pick a sensible default for a file-system operation based on the symbol name.
fn default_for_fs(symbol: &str) -> Value {
    let lower = symbol.to_lowercase();
    if lower.contains("read") {
        json!("")
    } else if lower.contains("stat") || lower.contains("lstat") {
        json!({"size": 0, "isFile": true, "isDirectory": false})
    } else if lower.contains("exists") {
        json!(true)
    } else if lower.contains("readdir") || lower.contains("glob") {
        json!([])
    } else {
        // write, mkdir, unlink, etc. — return undefined/null (success)
        Value::Null
    }
}

/// Pick a sensible default for a database operation based on the symbol name.
fn default_for_db(symbol: &str) -> Value {
    let lower = symbol.to_lowercase();
    if lower.contains("query") || lower.contains("find") || lower.contains("select") || lower.contains("all") {
        json!({"rows": []})
    } else if lower.contains("insert") || lower.contains("create") || lower.contains("save") {
        json!({"rowCount": 1})
    } else if lower.contains("update") || lower.contains("delete") || lower.contains("remove") {
        json!({"rowCount": 0})
    } else {
        Value::Null
    }
}

/// Generate a type-appropriate default value from TypeInfo.
fn default_for_type(typ: &TypeInfo) -> Value {
    match typ {
        TypeInfo::Int => json!(0),
        TypeInfo::Float => json!(0.0),
        TypeInfo::Str => json!(""),
        TypeInfo::Bool => json!(false),
        TypeInfo::Array { .. } => json!([]),
        TypeInfo::Object { fields } => {
            let mut obj = serde_json::Map::new();
            for (name, field_type) in fields {
                obj.insert(name.clone(), default_for_type(field_type));
            }
            Value::Object(obj)
        }
        TypeInfo::Nullable { .. } => Value::Null,
        TypeInfo::Union { variants } => {
            if let Some(first) = variants.first() {
                default_for_type(first)
            } else {
                Value::Null
            }
        }
        TypeInfo::Complex { .. } | TypeInfo::Opaque { .. } | TypeInfo::Unknown => Value::Null,
    }
}

/// User-provided mock override from `.shatter/config.yaml`.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct MockOverride {
    /// Pre-configured return values, replacing auto-generated defaults.
    #[serde(default)]
    pub return_values: Option<Vec<Value>>,
    /// Override the default behavior.
    #[serde(default)]
    pub behavior: Option<MockBehavior>,
}

/// Generate auto-mocks for all dependencies of a function.
///
/// Returns mock configs for dependencies that:
/// 1. Are not already covered by `existing_mocks` (e.g., behavior-map mocks)
/// 2. Are classified as Mock (not Passthrough or Analyze) by the scope matcher
///
/// User overrides from config take precedence over auto-generated defaults.
pub fn generate_auto_mocks(
    deps: &[ExternalDependency],
    scope: Option<&ScopeMatcher>,
    overrides: &HashMap<String, MockOverride>,
    existing_mocks: &[MockConfig],
) -> Vec<MockConfig> {
    let already_mocked: std::collections::HashSet<&str> = existing_mocks
        .iter()
        .map(|m| m.symbol.as_str())
        .collect();

    let mut result = Vec::new();

    for dep in deps {
        // Skip if already covered by a behavior-map mock
        if already_mocked.contains(dep.symbol.as_str()) {
            continue;
        }

        // Check scope: if Passthrough, skip; if Analyze (no rule), use category logic
        if let Some(matcher) = scope {
            match matcher.classify_dependency(&dep.symbol) {
                DependencyAction::Passthrough => continue,
                DependencyAction::Mock | DependencyAction::Analyze => {}
            }
        }

        let category = classify_dependency(dep);

        // Pure utilities default to passthrough — skip generating a mock
        if category == IoCategory::PureUtility && !overrides.contains_key(&dep.symbol) {
            continue;
        }

        let mut mock = generate_default_mock(dep, category);

        // Apply user overrides
        if let Some(ov) = overrides.get(&dep.symbol) {
            if let Some(ref vals) = ov.return_values {
                mock.return_values = vals.clone();
            }
            if let Some(ref behavior) = ov.behavior {
                mock.default_behavior = behavior.clone();
            }
        }

        result.push(mock);
    }

    result
}

// ---------------------------------------------------------------------------
// Error variant generation
// ---------------------------------------------------------------------------

/// Generate a [`MockConfig`] that throws an error, producing a realistic
/// error shape based on the dependency's return type and I/O category.
///
/// Dispatch order:
/// 1. Object with a `"status"` field → HTTP 400/500 error objects
/// 2. Nullable → `null` (represents a "not found" / absent value)
/// 3. Category-aware: Network → HTTP error, Database → connection error,
///    FileSystem → ENOENT
/// 4. Fallback → `ThrowError` behavior with an empty return_values vec
pub fn generate_error_variant(dep: &ExternalDependency, category: IoCategory) -> MockConfig {
    // Check for object with "status" field → HTTP error responses
    if let TypeInfo::Object { fields } = &dep.return_type
        && fields.iter().any(|(name, _)| name == "status")
    {
        return MockConfig {
            symbol: dep.symbol.clone(),
            return_values: vec![
                json!({"status": 400, "error": "Bad Request"}),
                json!({"status": 500, "error": "Internal Server Error"}),
            ],
            should_track_calls: true,
            default_behavior: MockBehavior::RepeatLast,
        };
    }

    // Nullable → null (represents absence / not-found)
    if matches!(&dep.return_type, TypeInfo::Nullable { .. }) {
        return MockConfig {
            symbol: dep.symbol.clone(),
            return_values: vec![Value::Null],
            should_track_calls: true,
            default_behavior: MockBehavior::RepeatLast,
        };
    }

    // Category-aware error shapes
    match category {
        IoCategory::Network => MockConfig {
            symbol: dep.symbol.clone(),
            return_values: vec![
                json!({"status": 500, "error": "Internal Server Error"}),
                json!({"status": 503, "error": "Service Unavailable"}),
            ],
            should_track_calls: true,
            default_behavior: MockBehavior::ThrowError,
        },
        IoCategory::Database => MockConfig {
            symbol: dep.symbol.clone(),
            return_values: vec![
                json!({"code": "ECONNREFUSED", "message": "Connection refused"}),
            ],
            should_track_calls: true,
            default_behavior: MockBehavior::ThrowError,
        },
        IoCategory::FileSystem => MockConfig {
            symbol: dep.symbol.clone(),
            return_values: vec![
                json!({"code": "ENOENT", "message": "No such file or directory"}),
            ],
            should_track_calls: true,
            default_behavior: MockBehavior::ThrowError,
        },
        // Fallback: bare ThrowError with no pre-configured return values
        IoCategory::PureUtility | IoCategory::ExternalOther => MockConfig {
            symbol: dep.symbol.clone(),
            return_values: vec![],
            should_track_calls: true,
            default_behavior: MockBehavior::ThrowError,
        },
    }
}

/// Generate error-variant [`MockConfig`]s for dependencies that should
/// exercise error-handling paths.
///
/// Uses `error_probability` to decide per-dependency whether to generate an
/// error mock. Dependencies already covered by `existing_mocks` or marked
/// as passthrough are skipped. Pure utilities are always skipped.
pub fn generate_error_mocks(
    deps: &[ExternalDependency],
    scope: Option<&ScopeMatcher>,
    existing_mocks: &[MockConfig],
    error_probability: f64,
    rng: &mut impl rand::Rng,
) -> Vec<MockConfig> {
    let already_mocked: std::collections::HashSet<&str> = existing_mocks
        .iter()
        .map(|m| m.symbol.as_str())
        .collect();

    let mut result = Vec::new();

    for dep in deps {
        if already_mocked.contains(dep.symbol.as_str()) {
            continue;
        }

        if let Some(matcher) = scope
            && matches!(
                matcher.classify_dependency(&dep.symbol),
                DependencyAction::Passthrough
            )
        {
            continue;
        }

        let category = classify_dependency(dep);
        if category == IoCategory::PureUtility {
            continue;
        }

        if rng.random_bool(error_probability.clamp(0.0, 1.0)) {
            result.push(generate_error_variant(dep, category));
        }
    }

    result
}

/// Where a mock's return value originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueSource {
    /// Generated automatically from dependency classification and type info.
    AutoGenerated,
    /// Overridden by the user via `.shatter/config.yaml`.
    UserOverride,
    /// Inherited from an existing behavior-map mock.
    BehaviorMap,
}

/// A resolved mock parameter ready for the orchestrator, combining dependency
/// metadata with its mock configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct MockParam {
    /// Fully qualified symbol name of the mocked dependency.
    pub symbol: String,
    /// Return type of the dependency (used for value generation).
    pub return_type: TypeInfo,
    /// Classified I/O category.
    pub category: IoCategory,
    /// Estimated number of calls based on static call-site count.
    pub call_count_estimate: u32,
    /// Where the mock's return value came from.
    pub value_source: ValueSource,
}

/// Build [`MockParam`]s from dependencies and their resolved mock configs.
///
/// Filters out dependencies whose category is [`IoCategory::PureUtility`]
/// unless a matching config explicitly overrides them. Dependencies with
/// a matching config get [`ValueSource::UserOverride`]; those without get
/// [`ValueSource::AutoGenerated`].
pub fn build_mock_params(
    deps: &[ExternalDependency],
    configs: &[MockConfig],
) -> Vec<MockParam> {
    let config_by_symbol: HashMap<&str, &MockConfig> = configs
        .iter()
        .map(|c| (c.symbol.as_str(), c))
        .collect();

    let mut result = Vec::new();

    for dep in deps {
        let category = classify_dependency(dep);
        let has_config = config_by_symbol.contains_key(dep.symbol.as_str());

        // Pure utilities are passthrough unless explicitly configured
        if category == IoCategory::PureUtility && !has_config {
            continue;
        }

        let value_source = if has_config {
            ValueSource::UserOverride
        } else {
            ValueSource::AutoGenerated
        };

        result.push(MockParam {
            symbol: dep.symbol.clone(),
            return_type: dep.return_type.clone(),
            category,
            call_count_estimate: dep.call_sites.len() as u32,
            value_source,
        });
    }

    result
}

/// Create [`MockConfig`]s for dependencies discovered at execution time
/// that static analysis missed.
///
/// Converts each [`DiscoveredDependency`] into an [`ExternalDependency`]
/// (with [`TypeInfo::Unknown`] since we have no static type info) and
/// generates a default mock using the standard classification pipeline.
pub fn create_mock_params_for_discovered(
    discovered: &[crate::protocol::DiscoveredDependency],
) -> Vec<MockConfig> {
    discovered
        .iter()
        .map(|dd| {
            let dep = ExternalDependency {
                kind: DependencyKind::FunctionCall,
                symbol: dd.symbol.clone(),
                source_module: dd.source_module.clone(),
                return_type: TypeInfo::Unknown,
                param_types: vec![],
                call_sites: vec![],
            };
            let category = classify_dependency(&dep);
            generate_default_mock(&dep, category)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::DependencyKind;

    fn make_dep(symbol: &str, source_module: &str, return_type: TypeInfo) -> ExternalDependency {
        ExternalDependency {
            kind: DependencyKind::FunctionCall,
            symbol: symbol.to_string(),
            source_module: source_module.to_string(),
            return_type,
            param_types: vec![],
            call_sites: vec![1],
        }
    }

    #[test]
    fn classify_fs_modules() {
        let dep = make_dep("readFile", "fs", TypeInfo::Str);
        assert_eq!(classify_dependency(&dep), IoCategory::FileSystem);

        let dep2 = make_dep("readFile", "node:fs/promises", TypeInfo::Str);
        assert_eq!(classify_dependency(&dep2), IoCategory::FileSystem);

        let dep3 = make_dep("join", "path", TypeInfo::Str);
        assert_eq!(classify_dependency(&dep3), IoCategory::FileSystem);
    }

    #[test]
    fn classify_network_modules() {
        let dep = make_dep("get", "axios", TypeInfo::Unknown);
        assert_eq!(classify_dependency(&dep), IoCategory::Network);

        let dep2 = make_dep("fetch", "node-fetch", TypeInfo::Unknown);
        assert_eq!(classify_dependency(&dep2), IoCategory::Network);
    }

    #[test]
    fn classify_database_modules() {
        let dep = make_dep("query", "pg", TypeInfo::Unknown);
        assert_eq!(classify_dependency(&dep), IoCategory::Database);

        let dep2 = make_dep("findMany", "@prisma/client", TypeInfo::Unknown);
        assert_eq!(classify_dependency(&dep2), IoCategory::Database);
    }

    #[test]
    fn classify_pure_utility_modules() {
        let dep = make_dep("map", "lodash", TypeInfo::Unknown);
        assert_eq!(classify_dependency(&dep), IoCategory::PureUtility);

        let dep2 = make_dep("format", "date-fns", TypeInfo::Unknown);
        assert_eq!(classify_dependency(&dep2), IoCategory::PureUtility);
    }

    #[test]
    fn classify_unknown_module_as_external_other() {
        let dep = make_dep("doSomething", "my-custom-lib", TypeInfo::Int);
        assert_eq!(classify_dependency(&dep), IoCategory::ExternalOther);
    }

    #[test]
    fn default_mock_for_fs_read() {
        let dep = make_dep("readFile", "fs", TypeInfo::Str);
        let mock = generate_default_mock(&dep, IoCategory::FileSystem);
        assert_eq!(mock.symbol, "readFile");
        assert_eq!(mock.return_values, vec![json!("")]);
        assert!(mock.should_track_calls);
        assert_eq!(mock.default_behavior, MockBehavior::RepeatLast);
    }

    #[test]
    fn default_mock_for_fs_exists() {
        let dep = make_dep("existsSync", "fs", TypeInfo::Bool);
        let mock = generate_default_mock(&dep, IoCategory::FileSystem);
        assert_eq!(mock.return_values, vec![json!(true)]);
    }

    #[test]
    fn default_mock_for_network() {
        let dep = make_dep("get", "axios", TypeInfo::Unknown);
        let mock = generate_default_mock(&dep, IoCategory::Network);
        assert_eq!(mock.return_values, vec![json!({"status": 200, "data": {}})]);
        assert!(mock.should_track_calls);
    }

    #[test]
    fn default_mock_for_db_query() {
        let dep = make_dep("query", "pg", TypeInfo::Unknown);
        let mock = generate_default_mock(&dep, IoCategory::Database);
        assert_eq!(mock.return_values, vec![json!({"rows": []})]);
    }

    #[test]
    fn default_mock_for_db_insert() {
        let dep = make_dep("insert", "pg", TypeInfo::Unknown);
        let mock = generate_default_mock(&dep, IoCategory::Database);
        assert_eq!(mock.return_values, vec![json!({"rowCount": 1})]);
    }

    #[test]
    fn default_mock_for_pure_utility_is_passthrough() {
        let dep = make_dep("map", "lodash", TypeInfo::Unknown);
        let mock = generate_default_mock(&dep, IoCategory::PureUtility);
        assert_eq!(mock.default_behavior, MockBehavior::Passthrough);
        assert!(mock.return_values.is_empty());
        assert!(!mock.should_track_calls);
    }

    #[test]
    fn default_mock_for_external_other_uses_type() {
        let dep = make_dep("compute", "my-lib", TypeInfo::Int);
        let mock = generate_default_mock(&dep, IoCategory::ExternalOther);
        assert_eq!(mock.return_values, vec![json!(0)]);
    }

    #[test]
    fn default_for_type_object() {
        let typ = TypeInfo::Object {
            fields: vec![
                ("name".to_string(), TypeInfo::Str),
                ("age".to_string(), TypeInfo::Int),
            ],
        };
        let val = default_for_type(&typ);
        assert_eq!(val, json!({"name": "", "age": 0}));
    }

    #[test]
    fn generate_auto_mocks_skips_existing() {
        let deps = vec![make_dep("query", "pg", TypeInfo::Unknown)];
        let existing = vec![MockConfig {
            symbol: "query".to_string(),
            return_values: vec![json!({"rows": [1, 2, 3]})],
            should_track_calls: true,
            default_behavior: MockBehavior::RepeatLast,
        }];

        let result = generate_auto_mocks(&deps, None, &HashMap::new(), &existing);
        assert!(result.is_empty());
    }

    #[test]
    fn generate_auto_mocks_skips_pure_utilities() {
        let deps = vec![make_dep("map", "lodash", TypeInfo::Unknown)];
        let result = generate_auto_mocks(&deps, None, &HashMap::new(), &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn generate_auto_mocks_applies_overrides() {
        let deps = vec![make_dep("query", "pg", TypeInfo::Unknown)];
        let mut overrides = HashMap::new();
        overrides.insert(
            "query".to_string(),
            MockOverride {
                return_values: Some(vec![json!({"rows": [{"id": 1}]})]),
                behavior: None,
            },
        );

        let result = generate_auto_mocks(&deps, None, &overrides, &[]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].return_values, vec![json!({"rows": [{"id": 1}]})]);
    }

    #[test]
    fn generate_auto_mocks_respects_scope_passthrough() {
        let deps = vec![make_dep("map", "lodash", TypeInfo::Unknown)];
        let scope_config = crate::scope::ScopeConfig {
            include: vec![],
            exclude: vec![],
            mock: vec![],
            passthrough: vec!["map".to_string()],
        };
        let matcher = ScopeMatcher::new(&scope_config).unwrap();

        let result = generate_auto_mocks(&deps, Some(&matcher), &HashMap::new(), &[]);
        assert!(result.is_empty());
    }

    /// Integration test: full auto-mock pipeline for a function with mixed dependencies.
    /// Validates that dependencies are classified, mocks are generated with correct
    /// return values, pure utilities are skipped, and user overrides are applied.
    #[test]
    fn auto_mock_pipeline_mixed_dependencies() {
        // Simulate a function that depends on fs.readFileSync, axios.get,
        // pg.query, lodash.map, and a custom library.
        let deps = vec![
            make_dep("readFileSync", "fs", TypeInfo::Str),
            make_dep("get", "axios", TypeInfo::Unknown),
            make_dep("query", "pg", TypeInfo::Unknown),
            make_dep("map", "lodash", TypeInfo::Unknown),
            make_dep("compute", "my-analytics-lib", TypeInfo::Int),
        ];

        // User override: custom return value for the database query mock
        let mut overrides = HashMap::new();
        overrides.insert(
            "query".to_string(),
            MockOverride {
                return_values: Some(vec![json!({"rows": [{"id": 1, "name": "alice"}]})]),
                behavior: None,
            },
        );

        let mocks = generate_auto_mocks(&deps, None, &overrides, &[]);

        // 4 mocks generated: fs, network, db (overridden), and external other.
        // lodash (pure utility) is skipped.
        assert_eq!(mocks.len(), 4, "expected 4 mocks (lodash skipped)");

        let by_symbol: HashMap<&str, &MockConfig> =
            mocks.iter().map(|m| (m.symbol.as_str(), m)).collect();

        // fs.readFileSync → filesystem category, returns ""
        let fs_mock = by_symbol["readFileSync"];
        assert_eq!(fs_mock.return_values, vec![json!("")]);
        assert!(fs_mock.should_track_calls);
        assert_eq!(fs_mock.default_behavior, MockBehavior::RepeatLast);

        // axios.get → network category, returns {status: 200, data: {}}
        let net_mock = by_symbol["get"];
        assert_eq!(net_mock.return_values, vec![json!({"status": 200, "data": {}})]);
        assert!(net_mock.should_track_calls);

        // pg.query → database category, but overridden with custom rows
        let db_mock = by_symbol["query"];
        assert_eq!(
            db_mock.return_values,
            vec![json!({"rows": [{"id": 1, "name": "alice"}]})]
        );
        assert!(db_mock.should_track_calls);

        // my-analytics-lib.compute → external other, type-aware default for Int
        let ext_mock = by_symbol["compute"];
        assert_eq!(ext_mock.return_values, vec![json!(0)]);
        assert!(ext_mock.should_track_calls);

        // Verify lodash.map is NOT in the mock list
        assert!(!by_symbol.contains_key("map"));
    }

    /// Integration test: auto-mocks are compatible with the Instrument command format.
    /// Verifies that generated MockConfig values serialize correctly for the protocol.
    #[test]
    fn auto_mock_configs_serialize_for_protocol() {
        let deps = vec![
            make_dep("existsSync", "fs", TypeInfo::Bool),
            make_dep("query", "pg", TypeInfo::Unknown),
        ];

        let mocks = generate_auto_mocks(&deps, None, &HashMap::new(), &[]);
        assert_eq!(mocks.len(), 2);

        // Verify each mock serializes to valid JSON (required for protocol)
        for mock in &mocks {
            let json = serde_json::to_value(mock).expect("mock should serialize");
            assert!(json.get("symbol").is_some());
            assert!(json.get("return_values").is_some());
            assert!(json.get("should_track_calls").is_some());
            assert!(json.get("default_behavior").is_some());
        }
    }

    /// Integration test: auto-mocks respect scope configuration.
    /// Dependencies marked as passthrough in scope config are excluded from mocking.
    #[test]
    fn auto_mock_pipeline_with_scope_rules() {
        let deps = vec![
            make_dep("readFile", "fs", TypeInfo::Str),
            make_dep("get", "axios", TypeInfo::Unknown),
            make_dep("query", "pg", TypeInfo::Unknown),
        ];

        // Scope config: mark axios.get as passthrough (e.g., user wants real HTTP)
        let scope_config = crate::scope::ScopeConfig {
            include: vec![],
            exclude: vec![],
            mock: vec![],
            passthrough: vec!["get".to_string()],
        };
        let matcher = ScopeMatcher::new(&scope_config).unwrap();

        let mocks = generate_auto_mocks(&deps, Some(&matcher), &HashMap::new(), &[]);

        // Only 2 mocks: fs.readFile and pg.query. axios.get is passthrough.
        assert_eq!(mocks.len(), 2);
        let symbols: Vec<&str> = mocks.iter().map(|m| m.symbol.as_str()).collect();
        assert!(symbols.contains(&"readFile"));
        assert!(symbols.contains(&"query"));
        assert!(!symbols.contains(&"get"));
    }

    #[test]
    fn classify_go_stdlib_modules() {
        // Go filesystem
        let dep = make_dep("os.ReadFile", "os", TypeInfo::Str);
        assert_eq!(classify_dependency(&dep), IoCategory::FileSystem);

        let dep2 = make_dep("io.Copy", "io", TypeInfo::Unknown);
        assert_eq!(classify_dependency(&dep2), IoCategory::FileSystem);

        // Go network (net/http matches via "net" prefix)
        let dep3 = make_dep("http.Get", "net/http", TypeInfo::Unknown);
        assert_eq!(classify_dependency(&dep3), IoCategory::Network);

        // Go database
        let dep4 = make_dep("sql.Open", "database/sql", TypeInfo::Unknown);
        assert_eq!(classify_dependency(&dep4), IoCategory::Database);

        // Go pure utilities
        let dep5 = make_dep("strings.TrimSpace", "strings", TypeInfo::Str);
        assert_eq!(classify_dependency(&dep5), IoCategory::PureUtility);

        let dep6 = make_dep("strconv.Atoi", "strconv", TypeInfo::Int);
        assert_eq!(classify_dependency(&dep6), IoCategory::PureUtility);

        let dep7 = make_dep("fmt.Sprintf", "fmt", TypeInfo::Str);
        assert_eq!(classify_dependency(&dep7), IoCategory::PureUtility);
    }

    #[test]
    fn generate_auto_mocks_for_multiple_deps() {
        let deps = vec![
            make_dep("readFile", "fs", TypeInfo::Str),
            make_dep("get", "axios", TypeInfo::Unknown),
            make_dep("map", "lodash", TypeInfo::Unknown),
            make_dep("compute", "my-lib", TypeInfo::Int),
        ];

        let result = generate_auto_mocks(&deps, None, &HashMap::new(), &[]);
        // lodash (pure utility) is skipped
        assert_eq!(result.len(), 3);
        let symbols: Vec<&str> = result.iter().map(|m| m.symbol.as_str()).collect();
        assert!(symbols.contains(&"readFile"));
        assert!(symbols.contains(&"get"));
        assert!(symbols.contains(&"compute"));
    }

    // --- build_mock_params tests ---

    #[test]
    fn build_mock_params_empty_deps() {
        let result = build_mock_params(&[], &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn build_mock_params_skips_pure_utility_without_config() {
        let deps = vec![
            make_dep("map", "lodash", TypeInfo::Unknown),
            make_dep("format", "date-fns", TypeInfo::Str),
        ];
        let result = build_mock_params(&deps, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn build_mock_params_includes_pure_utility_with_config() {
        let deps = vec![make_dep("map", "lodash", TypeInfo::Unknown)];
        let configs = vec![MockConfig {
            symbol: "map".to_string(),
            return_values: vec![json!([1, 2, 3])],
            should_track_calls: true,
            default_behavior: MockBehavior::RepeatLast,
        }];

        let result = build_mock_params(&deps, &configs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].symbol, "map");
        assert_eq!(result[0].category, IoCategory::PureUtility);
        assert_eq!(result[0].value_source, ValueSource::UserOverride);
    }

    #[test]
    fn build_mock_params_auto_generated_without_config() {
        let deps = vec![make_dep("readFile", "fs", TypeInfo::Str)];
        let result = build_mock_params(&deps, &[]);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].symbol, "readFile");
        assert_eq!(result[0].return_type, TypeInfo::Str);
        assert_eq!(result[0].category, IoCategory::FileSystem);
        assert_eq!(result[0].value_source, ValueSource::AutoGenerated);
    }

    #[test]
    fn build_mock_params_user_override_with_config() {
        let deps = vec![make_dep("query", "pg", TypeInfo::Unknown)];
        let configs = vec![MockConfig {
            symbol: "query".to_string(),
            return_values: vec![json!({"rows": [{"id": 1}]})],
            should_track_calls: true,
            default_behavior: MockBehavior::RepeatLast,
        }];

        let result = build_mock_params(&deps, &configs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].value_source, ValueSource::UserOverride);
        assert_eq!(result[0].category, IoCategory::Database);
    }

    #[test]
    fn build_mock_params_call_count_from_call_sites() {
        let mut dep = make_dep("get", "axios", TypeInfo::Unknown);
        dep.call_sites = vec![10, 25, 42];

        let result = build_mock_params(&[dep], &[]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].call_count_estimate, 3);
    }

    /// Mixed dependencies: fs, network, db, pure utility, and custom lib.
    /// Validates correct classification, filtering, and value source assignment.
    #[test]
    fn build_mock_params_mixed_deps() {
        let deps = vec![
            make_dep("readFile", "fs", TypeInfo::Str),
            make_dep("get", "axios", TypeInfo::Unknown),
            make_dep("query", "pg", TypeInfo::Unknown),
            make_dep("map", "lodash", TypeInfo::Unknown),
            make_dep("compute", "my-lib", TypeInfo::Int),
        ];
        let configs = vec![MockConfig {
            symbol: "query".to_string(),
            return_values: vec![json!({"rows": []})],
            should_track_calls: true,
            default_behavior: MockBehavior::RepeatLast,
        }];

        let result = build_mock_params(&deps, &configs);

        // 4 results: fs, network, db (with config), custom. lodash skipped.
        assert_eq!(result.len(), 4);

        let by_symbol: HashMap<&str, &MockParam> =
            result.iter().map(|p| (p.symbol.as_str(), p)).collect();

        assert!(!by_symbol.contains_key("map"));

        assert_eq!(by_symbol["readFile"].category, IoCategory::FileSystem);
        assert_eq!(by_symbol["readFile"].value_source, ValueSource::AutoGenerated);

        assert_eq!(by_symbol["get"].category, IoCategory::Network);
        assert_eq!(by_symbol["get"].value_source, ValueSource::AutoGenerated);

        assert_eq!(by_symbol["query"].category, IoCategory::Database);
        assert_eq!(by_symbol["query"].value_source, ValueSource::UserOverride);

        assert_eq!(by_symbol["compute"].category, IoCategory::ExternalOther);
        assert_eq!(by_symbol["compute"].value_source, ValueSource::AutoGenerated);
    }

    // --- Error variant generation tests ---

    #[test]
    fn error_variant_object_with_status_field() {
        let dep = make_dep(
            "fetchUser",
            "axios",
            TypeInfo::Object {
                fields: vec![
                    ("status".to_string(), TypeInfo::Int),
                    ("data".to_string(), TypeInfo::Unknown),
                ],
            },
        );
        let mock = generate_error_variant(&dep, IoCategory::Network);

        assert_eq!(mock.symbol, "fetchUser");
        assert_eq!(mock.return_values.len(), 2);
        assert_eq!(mock.return_values[0]["status"], 400);
        assert_eq!(mock.return_values[1]["status"], 500);
        // Object-with-status uses RepeatLast (returns error objects, doesn't throw)
        assert_eq!(mock.default_behavior, MockBehavior::RepeatLast);
    }

    #[test]
    fn error_variant_nullable_returns_null() {
        let dep = make_dep(
            "findUser",
            "my-db",
            TypeInfo::Nullable {
                inner: Box::new(TypeInfo::Object {
                    fields: vec![("id".to_string(), TypeInfo::Int)],
                }),
            },
        );
        let mock = generate_error_variant(&dep, IoCategory::Database);

        assert_eq!(mock.return_values, vec![Value::Null]);
        assert_eq!(mock.default_behavior, MockBehavior::RepeatLast);
    }

    #[test]
    fn error_variant_network_category() {
        let dep = make_dep("get", "axios", TypeInfo::Unknown);
        let mock = generate_error_variant(&dep, IoCategory::Network);

        assert_eq!(mock.default_behavior, MockBehavior::ThrowError);
        assert!(mock.return_values.iter().any(|v| v["status"] == 500));
        assert!(mock.return_values.iter().any(|v| v["status"] == 503));
    }

    #[test]
    fn error_variant_database_category() {
        let dep = make_dep("query", "pg", TypeInfo::Unknown);
        let mock = generate_error_variant(&dep, IoCategory::Database);

        assert_eq!(mock.default_behavior, MockBehavior::ThrowError);
        assert_eq!(mock.return_values[0]["code"], "ECONNREFUSED");
    }

    #[test]
    fn error_variant_filesystem_category() {
        let dep = make_dep("readFile", "fs", TypeInfo::Str);
        let mock = generate_error_variant(&dep, IoCategory::FileSystem);

        assert_eq!(mock.default_behavior, MockBehavior::ThrowError);
        assert_eq!(mock.return_values[0]["code"], "ENOENT");
    }

    #[test]
    fn error_variant_fallback_throws() {
        let dep = make_dep("compute", "my-lib", TypeInfo::Int);
        let mock = generate_error_variant(&dep, IoCategory::ExternalOther);

        assert_eq!(mock.default_behavior, MockBehavior::ThrowError);
        assert!(mock.return_values.is_empty());
    }

    #[test]
    fn generate_error_mocks_respects_probability() {
        let deps = vec![
            make_dep("readFile", "fs", TypeInfo::Str),
            make_dep("get", "axios", TypeInfo::Unknown),
            make_dep("query", "pg", TypeInfo::Unknown),
        ];

        // probability=0 → no error mocks
        let mut rng = rand::rng();
        let result = generate_error_mocks(&deps, None, &[], 0.0, &mut rng);
        assert!(result.is_empty());

        // probability=1 → all get error mocks
        let result = generate_error_mocks(&deps, None, &[], 1.0, &mut rng);
        assert_eq!(result.len(), 3);
        for mock in &result {
            assert_eq!(mock.default_behavior, MockBehavior::ThrowError);
        }
    }

    #[test]
    fn generate_error_mocks_skips_existing_and_pure_utility() {
        let deps = vec![
            make_dep("readFile", "fs", TypeInfo::Str),
            make_dep("map", "lodash", TypeInfo::Unknown),
        ];
        let existing = vec![MockConfig {
            symbol: "readFile".to_string(),
            return_values: vec![json!("")],
            should_track_calls: true,
            default_behavior: MockBehavior::RepeatLast,
        }];

        // Even with probability=1, existing and pure utility are skipped
        let mut rng = rand::rng();
        let result = generate_error_mocks(&deps, None, &existing, 1.0, &mut rng);
        assert!(result.is_empty());
    }

    #[test]
    fn default_error_probability_value() {
        assert!((DEFAULT_ERROR_PROBABILITY - 0.15).abs() < f64::EPSILON);
    }

    // --- Property-based tests ---

    mod prop_tests {
        use super::*;
        use crate::test_arbitraries::arb_external_dependency;
        use proptest::prelude::*;

        fn arb_io_category() -> impl Strategy<Value = IoCategory> {
            prop_oneof![
                Just(IoCategory::FileSystem),
                Just(IoCategory::Network),
                Just(IoCategory::Database),
                Just(IoCategory::PureUtility),
                Just(IoCategory::ExternalOther),
            ]
        }

        proptest! {
            /// Error variant always preserves the dependency symbol.
            #[test]
            fn error_variant_preserves_symbol(
                dep in arb_external_dependency(),
                category in arb_io_category(),
            ) {
                let mock = generate_error_variant(&dep, category);
                prop_assert_eq!(&mock.symbol, &dep.symbol);
                prop_assert!(mock.should_track_calls);
            }

            /// Error variant behavior is either ThrowError or RepeatLast
            /// (RepeatLast for object-with-status and nullable types).
            #[test]
            fn error_variant_behavior_is_valid(
                dep in arb_external_dependency(),
                category in arb_io_category(),
            ) {
                let mock = generate_error_variant(&dep, category);
                prop_assert!(
                    mock.default_behavior == MockBehavior::ThrowError
                        || mock.default_behavior == MockBehavior::RepeatLast,
                    "unexpected behavior: {:?}", mock.default_behavior,
                );
            }

            /// Error variant serializes to valid JSON (required for protocol).
            #[test]
            fn error_variant_serializes(
                dep in arb_external_dependency(),
                category in arb_io_category(),
            ) {
                let mock = generate_error_variant(&dep, category);
                let json = serde_json::to_value(&mock);
                prop_assert!(json.is_ok(), "serialization failed: {:?}", json.err());
            }

            /// generate_error_mocks with probability 0 always returns empty.
            #[test]
            fn error_mocks_prob_zero_always_empty(
                deps in prop::collection::vec(arb_external_dependency(), 0..=5),
            ) {
                let mut rng = rand::rng();
                let result = generate_error_mocks(&deps, None, &[], 0.0, &mut rng);
                prop_assert!(result.is_empty());
            }

            /// generate_error_mocks never exceeds dependency count.
            #[test]
            fn error_mocks_never_exceed_dep_count(
                deps in prop::collection::vec(arb_external_dependency(), 0..=5),
                prob in 0.0..=1.0_f64,
            ) {
                let mut rng = rand::rng();
                let result = generate_error_mocks(&deps, None, &[], prob, &mut rng);
                prop_assert!(result.len() <= deps.len());
            }
        }
    }
}
