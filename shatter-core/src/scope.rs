//! Scope configuration: controls which files to analyze and which dependencies to mock.
//!
//! Reads a `shatter.scope.yaml` file that defines include/exclude globs for files
//! and mock/passthrough globs for dependencies.

use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

/// Error type for scope configuration operations.
#[derive(Debug, thiserror::Error)]
pub enum ScopeError {
    #[error("failed to read scope file: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to parse scope YAML: {0}")]
    Parse(#[from] serde_yaml::Error),

    #[error("invalid glob pattern: {0}")]
    InvalidPattern(#[from] globset::Error),
}

/// Top-level YAML structure wrapping the scope config under a `scope:` key.
#[derive(Debug, Deserialize, Serialize)]
struct ScopeFile {
    scope: ScopeConfig,
}

/// Scope configuration defining which files and dependencies to include, exclude, mock, or pass through.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ScopeConfig {
    /// Glob patterns for files to analyze.
    #[serde(default)]
    pub include: Vec<String>,

    /// Glob patterns for files to skip.
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Glob patterns for dependencies to mock (e.g. `"node_modules/**"`).
    #[serde(default)]
    pub mock: Vec<String>,

    /// Glob patterns for dependencies to leave as real calls (e.g. `"lodash"`).
    #[serde(default)]
    pub passthrough: Vec<String>,
}

impl Default for ScopeConfig {
    fn default() -> Self {
        Self {
            include: vec!["**/*.ts".to_string(), "**/*.go".to_string()],
            exclude: vec!["node_modules/**".to_string(), "vendor/**".to_string()],
            mock: Vec::new(),
            passthrough: Vec::new(),
        }
    }
}

impl ScopeConfig {
    /// Read and parse a scope configuration from a YAML file.
    pub fn from_file(path: &Path) -> Result<Self, ScopeError> {
        let contents = std::fs::read_to_string(path)?;
        let file: ScopeFile = serde_yaml::from_str(&contents)?;
        Ok(file.scope)
    }
}

/// What to do with a dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyAction {
    /// Replace the dependency with a mock.
    Mock,
    /// Leave the dependency as a real call.
    Passthrough,
    /// No rule matched — analyze normally.
    Analyze,
}

/// Compiled scope matcher using `GlobSet` for efficient pattern matching.
pub struct ScopeMatcher {
    include: GlobSet,
    exclude: GlobSet,
    mock: GlobSet,
    passthrough: GlobSet,
}

impl ScopeMatcher {
    /// Compile a `ScopeConfig` into a matcher.
    pub fn new(config: &ScopeConfig) -> Result<Self, ScopeError> {
        Ok(Self {
            include: build_glob_set(&config.include)?,
            exclude: build_glob_set(&config.exclude)?,
            mock: build_glob_set(&config.mock)?,
            passthrough: build_glob_set(&config.passthrough)?,
        })
    }

    /// Returns `true` if the path matches include patterns and does not match exclude patterns.
    ///
    /// If no include patterns are defined, all paths are considered included.
    pub fn is_included(&self, path: &str) -> bool {
        let included = self.include.is_empty() || self.include.is_match(path);
        let excluded = self.exclude.is_match(path);
        included && !excluded
    }

    /// Classify a dependency symbol/path as Mock, Passthrough, or Analyze.
    ///
    /// Passthrough takes precedence over Mock (more specific override).
    pub fn classify_dependency(&self, symbol: &str) -> DependencyAction {
        if self.passthrough.is_match(symbol) {
            return DependencyAction::Passthrough;
        }
        if self.mock.is_match(symbol) {
            return DependencyAction::Mock;
        }
        DependencyAction::Analyze
    }
}

fn build_glob_set(patterns: &[String]) -> Result<GlobSet, ScopeError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern)?);
    }
    Ok(builder.build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_yaml(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn yaml_round_trip() {
        let config = ScopeConfig {
            include: vec!["src/**/*.ts".to_string()],
            exclude: vec!["test/**".to_string()],
            mock: vec!["node_modules/**".to_string()],
            passthrough: vec!["lodash".to_string()],
        };

        let file = ScopeFile {
            scope: config.clone(),
        };
        let yaml = serde_yaml::to_string(&file).unwrap();
        let parsed: ScopeFile = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.scope, config);
    }

    #[test]
    fn from_file_valid_yaml() {
        let yaml = r#"
scope:
  include:
    - "src/**/*.ts"
  exclude:
    - "dist/**"
  mock:
    - "node_modules/**"
  passthrough:
    - "lodash"
"#;
        let file = write_temp_yaml(yaml);
        let config = ScopeConfig::from_file(file.path()).unwrap();

        assert_eq!(config.include, vec!["src/**/*.ts"]);
        assert_eq!(config.exclude, vec!["dist/**"]);
        assert_eq!(config.mock, vec!["node_modules/**"]);
        assert_eq!(config.passthrough, vec!["lodash"]);
    }

    #[test]
    fn from_file_missing_file_returns_io_error() {
        let result = ScopeConfig::from_file(Path::new("/nonexistent/path.yaml"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ScopeError::Io(_)));
    }

    #[test]
    fn from_file_invalid_yaml_returns_parse_error() {
        let file = write_temp_yaml("not: [valid: yaml: {{");
        let result = ScopeConfig::from_file(file.path());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ScopeError::Parse(_)));
    }

    #[test]
    fn is_included_matches_include_patterns() {
        let config = ScopeConfig {
            include: vec!["src/**/*.ts".to_string()],
            exclude: vec![],
            mock: vec![],
            passthrough: vec![],
        };
        let matcher = ScopeMatcher::new(&config).unwrap();

        assert!(matcher.is_included("src/app.ts"));
        assert!(matcher.is_included("src/deep/nested/file.ts"));
        assert!(!matcher.is_included("lib/app.ts"));
        assert!(!matcher.is_included("src/app.go"));
    }

    #[test]
    fn is_included_excludes_matching_patterns() {
        let config = ScopeConfig {
            include: vec!["**/*.ts".to_string()],
            exclude: vec!["node_modules/**".to_string()],
            mock: vec![],
            passthrough: vec![],
        };
        let matcher = ScopeMatcher::new(&config).unwrap();

        assert!(matcher.is_included("src/app.ts"));
        assert!(!matcher.is_included("node_modules/pkg/index.ts"));
    }

    #[test]
    fn is_included_with_no_include_patterns_includes_everything() {
        let config = ScopeConfig {
            include: vec![],
            exclude: vec!["dist/**".to_string()],
            mock: vec![],
            passthrough: vec![],
        };
        let matcher = ScopeMatcher::new(&config).unwrap();

        assert!(matcher.is_included("src/anything.rs"));
        assert!(!matcher.is_included("dist/bundle.js"));
    }

    #[test]
    fn classify_dependency_mock() {
        let config = ScopeConfig {
            include: vec![],
            exclude: vec![],
            mock: vec!["node_modules/**".to_string()],
            passthrough: vec![],
        };
        let matcher = ScopeMatcher::new(&config).unwrap();

        assert_eq!(
            matcher.classify_dependency("node_modules/axios"),
            DependencyAction::Mock
        );
    }

    #[test]
    fn classify_dependency_passthrough() {
        let config = ScopeConfig {
            include: vec![],
            exclude: vec![],
            mock: vec![],
            passthrough: vec!["lodash".to_string()],
        };
        let matcher = ScopeMatcher::new(&config).unwrap();

        assert_eq!(
            matcher.classify_dependency("lodash"),
            DependencyAction::Passthrough
        );
    }

    #[test]
    fn classify_dependency_default_is_analyze() {
        let config = ScopeConfig {
            include: vec![],
            exclude: vec![],
            mock: vec!["node_modules/**".to_string()],
            passthrough: vec![],
        };
        let matcher = ScopeMatcher::new(&config).unwrap();

        assert_eq!(
            matcher.classify_dependency("my-local-lib"),
            DependencyAction::Analyze
        );
    }

    #[test]
    fn passthrough_takes_precedence_over_mock() {
        let config = ScopeConfig {
            include: vec![],
            exclude: vec![],
            mock: vec!["node_modules/**".to_string()],
            passthrough: vec!["node_modules/lodash".to_string()],
        };
        let matcher = ScopeMatcher::new(&config).unwrap();

        assert_eq!(
            matcher.classify_dependency("node_modules/lodash"),
            DependencyAction::Passthrough
        );
        assert_eq!(
            matcher.classify_dependency("node_modules/axios"),
            DependencyAction::Mock
        );
    }

    #[test]
    fn acceptance_test_node_modules_mocked_lodash_passthrough() {
        let config = ScopeConfig {
            include: vec!["src/**/*.ts".to_string()],
            exclude: vec!["**/*.test.ts".to_string()],
            mock: vec!["node_modules/**".to_string()],
            passthrough: vec!["node_modules/lodash".to_string(), "node_modules/lodash/**".to_string()],
        };
        let matcher = ScopeMatcher::new(&config).unwrap();

        // File inclusion
        assert!(matcher.is_included("src/app.ts"));
        assert!(!matcher.is_included("src/app.test.ts"));

        // Dependency classification
        assert_eq!(
            matcher.classify_dependency("node_modules/lodash"),
            DependencyAction::Passthrough
        );
        assert_eq!(
            matcher.classify_dependency("node_modules/lodash/fp"),
            DependencyAction::Passthrough
        );
        assert_eq!(
            matcher.classify_dependency("node_modules/axios"),
            DependencyAction::Mock
        );
        assert_eq!(
            matcher.classify_dependency("src/utils"),
            DependencyAction::Analyze
        );
    }

    #[test]
    fn default_config_has_sensible_values() {
        let config = ScopeConfig::default();
        assert!(!config.include.is_empty());
        assert!(!config.exclude.is_empty());
        assert!(config.mock.is_empty());
        assert!(config.passthrough.is_empty());

        // Should compile without error
        let matcher = ScopeMatcher::new(&config).unwrap();
        assert!(matcher.is_included("src/app.ts"));
        assert!(!matcher.is_included("node_modules/pkg/index.ts"));
    }
}
