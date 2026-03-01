//! Automatic mock generation for external dependencies.
//!
//! Classifies external dependencies into categories (I/O, library, utility)
//! and generates sensible default [`MockConfig`]s without requiring user
//! configuration. Users can override defaults via `.shatter/config.yaml`.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::protocol::{ExternalDependency, MockBehavior, MockConfig};
use crate::scope::{DependencyAction, ScopeMatcher};
use crate::types::TypeInfo;

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
const FS_MODULES: &[&str] = &["fs", "node:fs", "fs/promises", "node:fs/promises", "path", "node:path"];
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
    "crypto-js", "chalk", "debug", "ms",
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
}
