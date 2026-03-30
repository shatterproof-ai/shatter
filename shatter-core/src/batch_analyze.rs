//! Batch analyze: send Analyze requests for discovered files and aggregate into a function registry.
//!
//! The [`FunctionRegistry`] is a unified collection of all functions found across
//! all source files. Each [`FunctionEntry`] records the file path, function metadata,
//! and branch count. This registry is the input to call graph construction.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::analysis_cache::AnalysisCache;
use crate::crypto_registry::{CryptoDetectionSummary, CryptoRegistry};
use crate::discovery::Language;
use crate::frontend::{Frontend, FrontendError};
use crate::protocol::{
    Command as ProtoCommand, CryptoBoundary, ExternalDependency, FunctionAnalysis, ResponseResult,
};
use crate::types::{ParamInfo, TypeInfo};

/// A single function entry in the registry.
#[derive(Debug, Clone)]
pub struct FunctionEntry {
    /// Path to the source file containing this function.
    pub file_path: PathBuf,
    /// Name of the function.
    pub name: String,
    /// Whether the function is exported (public) from its module.
    pub exported: bool,
    /// Parameter types.
    pub params: Vec<ParamInfo>,
    /// Return type.
    pub return_type: TypeInfo,
    /// Dependencies (calls to other project functions or external symbols).
    pub dependencies: Vec<ExternalDependency>,
    /// Cryptographic API boundaries detected during analysis.
    pub crypto_boundaries: Vec<CryptoBoundary>,
    /// Number of branch points in the function.
    pub branch_count: usize,
    /// First line of the function in source.
    pub start_line: u32,
    /// Last line of the function in source.
    pub end_line: u32,
}

/// Aggregated results of analyzing all discovered source files.
#[derive(Debug, Clone)]
pub struct FunctionRegistry {
    /// All function entries, keyed by a qualified identifier (file_path::function_name).
    entries: Vec<FunctionEntry>,
    /// Index from qualified name to position in entries vec.
    index: HashMap<String, usize>,
}

impl FunctionRegistry {
    /// Construct a registry from pre-built entries and index.
    ///
    /// This is primarily useful in tests that need to build a registry without
    /// going through the async `batch_analyze` pipeline.
    pub fn from_raw(entries: Vec<FunctionEntry>, index: HashMap<String, usize>) -> Self {
        Self { entries, index }
    }

    /// Number of functions in the registry.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over all function entries.
    pub fn entries(&self) -> &[FunctionEntry] {
        &self.entries
    }

    /// Look up a function by its qualified name (file_path::function_name).
    pub fn get(&self, qualified_name: &str) -> Option<&FunctionEntry> {
        self.index.get(qualified_name).map(|&i| &self.entries[i])
    }

    /// Get all functions from a specific file.
    pub fn functions_in_file(&self, file_path: &Path) -> Vec<&FunctionEntry> {
        self.entries
            .iter()
            .filter(|e| e.file_path == file_path)
            .collect()
    }

    /// Get all exported functions.
    pub fn exported_functions(&self) -> Vec<&FunctionEntry> {
        self.entries.iter().filter(|e| e.exported).collect()
    }

    /// Build a qualified name for a function in a file.
    pub fn qualified_name(file_path: &Path, function_name: &str) -> String {
        format!("{}::{}", file_path.display(), function_name)
    }
}

/// Errors that can occur during batch analysis.
#[derive(Debug, thiserror::Error)]
pub enum BatchAnalyzeError {
    /// A frontend communication error.
    #[error("frontend error for {file}: {source}")]
    Frontend {
        file: String,
        source: FrontendError,
    },
    /// No frontend available for a language.
    #[error("no frontend configured for language: {0:?}")]
    NoFrontend(Language),
}

/// Analyze multiple files using the appropriate frontends and build a [`FunctionRegistry`].
///
/// For each `(file_path, language)` pair, sends an Analyze request to the corresponding
/// frontend and aggregates results. Files are grouped by language to minimize frontend
/// switching overhead.
pub async fn batch_analyze(
    frontends: &mut HashMap<Language, Frontend>,
    files: &[(PathBuf, Language)],
    analysis_cache: Option<&AnalysisCache>,
    project_root: Option<&str>,
) -> Result<FunctionRegistry, BatchAnalyzeError> {
    let mut entries = Vec::new();
    let mut index = HashMap::new();

    // Load crypto registry once for classifying dependencies across all files.
    let crypto_registry = match CryptoRegistry::load() {
        Ok(r) => Some(r),
        Err(e) => {
            log::warn!("failed to load crypto registry, skipping crypto classification: {e}");
            None
        }
    };

    for (file_path, language) in files {
        // Check the analysis cache before calling the frontend.
        if let Some(cache) = analysis_cache
            && let Ok(Some(cached_functions)) = cache.lookup(file_path)
        {
            for func in cached_functions {
                let entry = function_entry_from_analysis(
                    file_path.clone(),
                    func,
                    crypto_registry.as_ref(),
                    language.as_registry_str(),
                );
                let qualified =
                    FunctionRegistry::qualified_name(&entry.file_path, &entry.name);
                let idx = entries.len();
                index.insert(qualified, idx);
                entries.push(entry);
            }
            continue;
        }

        let frontend = frontends
            .get_mut(language)
            .ok_or(BatchAnalyzeError::NoFrontend(*language))?;

        let response = frontend
            .send(ProtoCommand::Analyze {
                file: file_path.to_string_lossy().into_owned(),
                function: None,
                project_root: project_root.map(String::from),
            })
            .await
            .map_err(|e| BatchAnalyzeError::Frontend {
                file: file_path.to_string_lossy().into_owned(),
                source: e,
            })?;

        let functions = match response.result {
            ResponseResult::Analyze { functions } => functions,
            ResponseResult::Error {
                code,
                message,
                details,
            } => {
                return Err(BatchAnalyzeError::Frontend {
                    file: file_path.to_string_lossy().into_owned(),
                    source: FrontendError::Protocol {
                        code,
                        message,
                        details,
                    },
                });
            }
            other => {
                return Err(BatchAnalyzeError::Frontend {
                    file: file_path.to_string_lossy().into_owned(),
                    source: FrontendError::Protocol {
                        code: crate::protocol::ErrorCode::InvalidRequest,
                        message: format!("unexpected analyze response: {other:?}"),
                        details: None,
                    },
                });
            }
        };

        // Store fresh analysis results in the cache.
        if let Some(cache) = analysis_cache
            && let Err(e) = cache.store(file_path, &functions)
        {
            log::warn!("failed to cache analysis for {}: {e}", file_path.display());
        }

        for func in functions {
            let entry = function_entry_from_analysis(
                file_path.clone(),
                func,
                crypto_registry.as_ref(),
                language.as_registry_str(),
            );
            let qualified =
                FunctionRegistry::qualified_name(&entry.file_path, &entry.name);
            let idx = entries.len();
            index.insert(qualified, idx);
            entries.push(entry);
        }
    }

    Ok(FunctionRegistry { entries, index })
}

/// Convert a protocol [`FunctionAnalysis`] into a [`FunctionEntry`],
/// enriching with crypto boundary classification when a registry is available.
fn function_entry_from_analysis(
    file_path: PathBuf,
    analysis: FunctionAnalysis,
    crypto_registry: Option<&CryptoRegistry>,
    language: &str,
) -> FunctionEntry {
    // Use the real source file if the frontend reported one (barrel re-exports).
    let effective_path = analysis
        .source_file
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or(file_path);

    let branch_count = analysis.branches.len();
    let raw_boundaries = crypto_registry
        .map(|r| r.classify_all_dependencies(&analysis.dependencies, language))
        .unwrap_or_default();

    let summary = CryptoDetectionSummary::from_boundaries(raw_boundaries);
    if !summary.boundaries.is_empty() {
        log::debug!(
            "crypto detection for {}: {} boundaries, layers {:?}, {} high-confidence",
            analysis.name,
            summary.boundaries.len(),
            summary.layers_used,
            summary.high_confidence_count,
        );
    }

    FunctionEntry {
        file_path: effective_path,
        name: analysis.name,
        exported: analysis.exported,
        params: analysis.params,
        return_type: analysis.return_type,
        dependencies: analysis.dependencies,
        crypto_boundaries: summary.boundaries,
        branch_count,
        start_line: analysis.start_line,
        end_line: analysis.end_line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::Language;
    use crate::protocol::{
        BranchInfo, BranchType, DependencyKind, ExternalDependency, FunctionAnalysis,
    };
    use crate::types::{ParamInfo, TypeInfo};

    fn make_analysis(name: &str, exported: bool, branch_count: usize) -> FunctionAnalysis {
        FunctionAnalysis {
            name: name.to_string(),
            exported,
            params: vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Int,
                type_name: None,
            }],
            branches: (0..branch_count)
                .map(|i| BranchInfo {
                    id: i as u32,
                    line: (i + 1) as u32,
                    condition_text: format!("cond_{i}"),
                    condition: None,
                    branch_type: BranchType::If,
                })
                .collect(),
            dependencies: vec![],
            return_type: TypeInfo::Int,
            start_line: 1,
            end_line: 10,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
        }
    }

    fn make_analysis_with_deps(
        name: &str,
        exported: bool,
        deps: Vec<&str>,
    ) -> FunctionAnalysis {
        let mut a = make_analysis(name, exported, 0);
        a.dependencies = deps
            .into_iter()
            .map(|d| ExternalDependency {
                kind: DependencyKind::FunctionCall,
                symbol: d.to_string(),
                source_module: String::new(),
                return_type: TypeInfo::Unknown,
                param_types: vec![],
                call_sites: vec![],
            })
            .collect();
        a
    }

    #[test]
    fn function_entry_from_analysis_preserves_fields() {
        let analysis = FunctionAnalysis {
            name: "myFunc".into(),
            exported: true,
            params: vec![
                ParamInfo {
                    name: "a".into(),
                    typ: TypeInfo::Int,
                    type_name: None,
                },
                ParamInfo {
                    name: "b".into(),
                    typ: TypeInfo::Str,
                    type_name: None,
                },
            ],
            branches: vec![
                BranchInfo {
                    id: 0,
                    line: 5,
                    condition_text: "a > 0".into(),
                    condition: None,
                    branch_type: BranchType::If,
                },
                BranchInfo {
                    id: 1,
                    line: 10,
                    condition_text: "b.len() > 3".into(),
                    condition: None,
                    branch_type: BranchType::If,
                },
            ],
            dependencies: vec![ExternalDependency {
                kind: DependencyKind::FunctionCall,
                symbol: "helper".into(),
                source_module: "./utils".into(),
                return_type: TypeInfo::Int,
                param_types: vec![TypeInfo::Int],
                call_sites: vec![7],
            }],
            return_type: TypeInfo::Bool,
            start_line: 1,
            end_line: 15,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
        };

        let entry = function_entry_from_analysis(PathBuf::from("src/app.ts"), analysis, None, "typescript");

        assert_eq!(entry.file_path, PathBuf::from("src/app.ts"));
        assert_eq!(entry.name, "myFunc");
        assert!(entry.exported);
        assert_eq!(entry.params.len(), 2);
        assert_eq!(entry.return_type, TypeInfo::Bool);
        assert_eq!(entry.dependencies.len(), 1);
        assert_eq!(entry.dependencies[0].symbol, "helper");
        assert_eq!(entry.branch_count, 2);
        assert_eq!(entry.start_line, 1);
        assert_eq!(entry.end_line, 15);
    }

    #[test]
    fn function_entry_unexported_function() {
        let analysis = make_analysis("private_helper", false, 0);
        let entry = function_entry_from_analysis(PathBuf::from("src/utils.ts"), analysis, None, "typescript");
        assert!(!entry.exported);
    }

    #[test]
    fn function_entry_uses_source_file_when_present() {
        let mut analysis = make_analysis("barrelAdd", true, 1);
        analysis.source_file = Some("/project/src/math.ts".into());
        let entry = function_entry_from_analysis(
            PathBuf::from("/project/src/index.ts"),
            analysis,
            None,
            "typescript",
        );
        assert_eq!(entry.file_path, PathBuf::from("/project/src/math.ts"));
        assert_eq!(entry.name, "barrelAdd");
    }

    #[test]
    fn function_entry_ignores_source_file_when_absent() {
        let analysis = make_analysis("directFunc", true, 1);
        let entry = function_entry_from_analysis(
            PathBuf::from("/project/src/app.ts"),
            analysis,
            None,
            "typescript",
        );
        assert_eq!(entry.file_path, PathBuf::from("/project/src/app.ts"));
    }

    #[test]
    fn function_registry_qualified_name_format() {
        let qn = FunctionRegistry::qualified_name(Path::new("src/app.ts"), "myFunc");
        assert_eq!(qn, "src/app.ts::myFunc");
    }

    #[test]
    fn function_registry_from_entries() {
        let mut entries = Vec::new();
        let mut index = HashMap::new();

        let e1 = FunctionEntry {
            file_path: PathBuf::from("src/app.ts"),
            name: "funcA".into(),
            exported: true,
            params: vec![],
            return_type: TypeInfo::Int,
            dependencies: vec![],
            branch_count: 2,
            start_line: 1,
            end_line: 10,
            crypto_boundaries: vec![],
        };
        index.insert(
            FunctionRegistry::qualified_name(Path::new("src/app.ts"), "funcA"),
            0,
        );
        entries.push(e1);

        let e2 = FunctionEntry {
            file_path: PathBuf::from("src/app.ts"),
            name: "funcB".into(),
            exported: false,
            params: vec![],
            return_type: TypeInfo::Str,
            dependencies: vec![],
            branch_count: 0,
            start_line: 11,
            end_line: 20,
            crypto_boundaries: vec![],
        };
        index.insert(
            FunctionRegistry::qualified_name(Path::new("src/app.ts"), "funcB"),
            1,
        );
        entries.push(e2);

        let e3 = FunctionEntry {
            file_path: PathBuf::from("src/utils.ts"),
            name: "helper".into(),
            exported: true,
            params: vec![],
            return_type: TypeInfo::Unknown,
            dependencies: vec![],
            branch_count: 1,
            start_line: 1,
            end_line: 5,
            crypto_boundaries: vec![],
        };
        index.insert(
            FunctionRegistry::qualified_name(Path::new("src/utils.ts"), "helper"),
            2,
        );
        entries.push(e3);

        let registry = FunctionRegistry { entries, index };

        assert_eq!(registry.len(), 3);
        assert!(!registry.is_empty());
    }

    #[test]
    fn function_registry_get_by_qualified_name() {
        let mut entries = Vec::new();
        let mut index = HashMap::new();

        let entry = FunctionEntry {
            file_path: PathBuf::from("src/app.ts"),
            name: "funcA".into(),
            exported: true,
            params: vec![],
            return_type: TypeInfo::Int,
            dependencies: vec![],
            branch_count: 3,
            start_line: 1,
            end_line: 10,
            crypto_boundaries: vec![],
        };
        index.insert("src/app.ts::funcA".to_string(), 0);
        entries.push(entry);

        let registry = FunctionRegistry { entries, index };

        let found = registry.get("src/app.ts::funcA");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "funcA");
        assert_eq!(found.unwrap().branch_count, 3);

        assert!(registry.get("src/app.ts::nonexistent").is_none());
    }

    #[test]
    fn function_registry_functions_in_file() {
        let mut entries = Vec::new();
        let mut index = HashMap::new();

        for (i, (file, name)) in [
            ("src/a.ts", "f1"),
            ("src/a.ts", "f2"),
            ("src/b.ts", "g1"),
        ]
        .iter()
        .enumerate()
        {
            index.insert(format!("{file}::{name}"), i);
            entries.push(FunctionEntry {
                file_path: PathBuf::from(file),
                name: name.to_string(),
                exported: true,
                params: vec![],
                return_type: TypeInfo::Unknown,
                dependencies: vec![],
                branch_count: 0,
                start_line: 1,
                end_line: 10,
                crypto_boundaries: vec![],
            });
        }

        let registry = FunctionRegistry { entries, index };

        let a_funcs = registry.functions_in_file(Path::new("src/a.ts"));
        assert_eq!(a_funcs.len(), 2);

        let b_funcs = registry.functions_in_file(Path::new("src/b.ts"));
        assert_eq!(b_funcs.len(), 1);
        assert_eq!(b_funcs[0].name, "g1");

        let c_funcs = registry.functions_in_file(Path::new("src/c.ts"));
        assert!(c_funcs.is_empty());
    }

    #[test]
    fn function_registry_exported_functions() {
        let mut entries = Vec::new();
        let mut index = HashMap::new();

        for (i, (name, exported)) in [("pub_fn", true), ("priv_fn", false), ("pub_fn2", true)]
            .iter()
            .enumerate()
        {
            index.insert(format!("file.ts::{name}"), i);
            entries.push(FunctionEntry {
                file_path: PathBuf::from("file.ts"),
                name: name.to_string(),
                exported: *exported,
                params: vec![],
                return_type: TypeInfo::Unknown,
                dependencies: vec![],
                branch_count: 0,
                start_line: 1,
                end_line: 10,
                crypto_boundaries: vec![],
            });
        }

        let registry = FunctionRegistry { entries, index };
        let exported = registry.exported_functions();
        assert_eq!(exported.len(), 2);
        assert!(exported.iter().all(|e| e.exported));
    }

    #[test]
    fn empty_function_registry() {
        let registry = FunctionRegistry {
            entries: vec![],
            index: HashMap::new(),
        };
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert!(registry.entries().is_empty());
        assert!(registry.get("anything").is_none());
    }

    #[test]
    fn function_entry_preserves_dependencies() {
        let analysis = make_analysis_with_deps("caller", true, vec!["dep1", "dep2"]);
        let entry = function_entry_from_analysis(PathBuf::from("src/main.ts"), analysis, None, "typescript");

        assert_eq!(entry.dependencies.len(), 2);
        assert_eq!(entry.dependencies[0].symbol, "dep1");
        assert_eq!(entry.dependencies[1].symbol, "dep2");
    }

    #[test]
    fn function_entry_branch_count_matches_branches() {
        let analysis = make_analysis("branchy", true, 5);
        let entry = function_entry_from_analysis(PathBuf::from("src/app.ts"), analysis, None, "typescript");
        assert_eq!(entry.branch_count, 5);
    }

    #[test]
    fn function_entry_zero_branches() {
        let analysis = make_analysis("simple", true, 0);
        let entry = function_entry_from_analysis(PathBuf::from("src/app.ts"), analysis, None, "typescript");
        assert_eq!(entry.branch_count, 0);
    }

    // Integration-style test using the noop frontend
    use std::path::Path as StdPath;

    fn noop_frontend_path() -> PathBuf {
        let manifest_dir = StdPath::new(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.join("../protocol/noop-frontend.sh")
    }

    fn noop_config() -> crate::frontend::FrontendConfig {
        let mut config = crate::frontend::FrontendConfig::new(PathBuf::from("bash"));
        config.args = vec![noop_frontend_path().to_string_lossy().into_owned()];
        config.request_timeout = std::time::Duration::from_secs(5);
        config
    }

    #[tokio::test]
    async fn batch_analyze_with_noop_frontend() {
        let config = noop_config();
        let frontend = Frontend::spawn(&config).await.expect("spawn failed");

        let mut frontends = HashMap::new();
        frontends.insert(Language::TypeScript, frontend);

        let files = vec![
            (PathBuf::from("src/app.ts"), Language::TypeScript),
            (PathBuf::from("src/utils.ts"), Language::TypeScript),
        ];

        let registry = batch_analyze(&mut frontends, &files, None, None)
            .await
            .expect("batch analyze failed");

        // Noop frontend returns one "stub" function per file
        assert_eq!(registry.len(), 2);
        assert_eq!(
            registry.functions_in_file(Path::new("src/app.ts")).len(),
            1
        );
        assert_eq!(
            registry.functions_in_file(Path::new("src/utils.ts")).len(),
            1
        );

        // Each stub function is named "stub"
        for entry in registry.entries() {
            assert_eq!(entry.name, "stub");
        }

        // Shutdown frontends
        for (_, frontend) in frontends {
            frontend.shutdown().await.expect("shutdown failed");
        }
    }

    #[tokio::test]
    async fn batch_analyze_no_frontend_for_language_returns_error() {
        let frontends: HashMap<Language, Frontend> = HashMap::new();

        let files = vec![(PathBuf::from("src/app.ts"), Language::TypeScript)];

        let result = batch_analyze(&mut frontends.into_iter().collect(), &files, None, None).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, BatchAnalyzeError::NoFrontend(Language::TypeScript)));
    }

    #[tokio::test]
    async fn batch_analyze_empty_file_list() {
        let mut frontends: HashMap<Language, Frontend> = HashMap::new();

        let files: Vec<(PathBuf, Language)> = vec![];

        let registry = batch_analyze(&mut frontends, &files, None, None)
            .await
            .expect("batch analyze failed");

        assert!(registry.is_empty());
    }

    #[tokio::test]
    async fn batch_analyze_multiple_languages_with_noop() {
        let ts_config = noop_config();
        let go_config = noop_config();

        let ts_frontend = Frontend::spawn(&ts_config).await.expect("spawn ts");
        let go_frontend = Frontend::spawn(&go_config).await.expect("spawn go");

        let mut frontends = HashMap::new();
        frontends.insert(Language::TypeScript, ts_frontend);
        frontends.insert(Language::Go, go_frontend);

        let files = vec![
            (PathBuf::from("src/app.ts"), Language::TypeScript),
            (PathBuf::from("pkg/handler.go"), Language::Go),
        ];

        let registry = batch_analyze(&mut frontends, &files, None, None)
            .await
            .expect("batch analyze failed");

        assert_eq!(registry.len(), 2);
        assert_eq!(
            registry.functions_in_file(Path::new("src/app.ts")).len(),
            1
        );
        assert_eq!(
            registry.functions_in_file(Path::new("pkg/handler.go")).len(),
            1
        );

        for (_, frontend) in frontends {
            frontend.shutdown().await.expect("shutdown failed");
        }
    }
}
