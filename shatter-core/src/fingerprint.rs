//! Per-function fingerprinting for staleness detection.
//!
//! A fingerprint is a stable SHA-256 hash of a function's source text,
//! parameter types, and branch structure. When a fingerprint matches a
//! previously cached value, the function is unchanged and can be skipped
//! during re-exploration.
//!
//! The module supports two modes:
//! - **Single-file**: [`compute_deep_fingerprints`] computes deep FPs within one file.
//! - **Cross-file**: [`compute_cross_file_deep_fingerprints`] uses a [`CallGraph`] to
//!   compose fingerprints across file boundaries, and [`compute_cross_file_staleness`]
//!   propagates staleness transitively through the call graph.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::call_graph::CallGraph;
use crate::protocol::FunctionAnalysis;

/// Compute a hex-encoded SHA-256 fingerprint for a function.
///
/// The fingerprint incorporates:
/// - The function's source text (verbatim, including whitespace)
/// - Parameter names and types (sorted by name for stability)
/// - Branch structure: IDs, lines, condition text, types (sorted by ID)
///
/// Any change to these inputs produces a different fingerprint.
pub fn compute_function_fingerprint(source_text: &str, analysis: &FunctionAnalysis) -> String {
    let mut hasher = Sha256::new();

    // Hash source text.
    hasher.update(b"source:");
    hasher.update(source_text.as_bytes());

    // Hash parameters (sorted by name for determinism).
    let mut params: Vec<_> = analysis
        .params
        .iter()
        .map(|p| {
            let mut s = String::new();
            let _ = write!(s, "{}:{:?}", p.name, p.typ);
            if let Some(ref tn) = p.type_name {
                let _ = write!(s, "/{tn}");
            }
            s
        })
        .collect();
    params.sort();

    hasher.update(b"params:");
    for p in &params {
        hasher.update(p.as_bytes());
        hasher.update(b"\n");
    }

    // Hash branch structure (sorted by branch ID for determinism).
    let mut branches: Vec<_> = analysis
        .branches
        .iter()
        .map(|b| {
            let mut s = String::new();
            let _ = write!(s, "{}:{}:{:?}:{}", b.id, b.line, b.branch_type, b.condition_text);
            s
        })
        .collect();
    branches.sort();

    hasher.update(b"branches:");
    for b in &branches {
        hasher.update(b.as_bytes());
        hasher.update(b"\n");
    }

    format!("{:x}", hasher.finalize())
}

/// Compute a deep fingerprint that incorporates callee fingerprints.
///
/// Extends the shallow fingerprint by hashing in the deep fingerprints of all
/// in-scope callees (sorted by name for determinism). Callees not present in
/// `callee_deep_fingerprints` are ignored (they are out-of-scope and assumed stable).
///
/// Because scans process functions leaves-first, callee deep fingerprints are
/// always available before the caller is processed.
pub fn compute_deep_fingerprint(
    shallow_fingerprint: &str,
    callee_deep_fingerprints: &HashMap<String, String>,
    callees: &HashSet<String>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"shallow:");
    hasher.update(shallow_fingerprint.as_bytes());

    let mut callee_fps: Vec<(&str, &str)> = callees
        .iter()
        .filter_map(|c| {
            callee_deep_fingerprints
                .get(c)
                .map(|fp| (c.as_str(), fp.as_str()))
        })
        .collect();
    callee_fps.sort_by_key(|(name, _)| *name);

    hasher.update(b"callees:");
    for (name, fp) in &callee_fps {
        hasher.update(name.as_bytes());
        hasher.update(b"=");
        hasher.update(fp.as_bytes());
        hasher.update(b"\n");
    }

    format!("{:x}", hasher.finalize())
}

/// Compute deep fingerprints for all functions in a single file.
///
/// For each analysis, computes a shallow fingerprint from source text + metadata,
/// then composes it with callee deep fingerprints. Functions are processed in
/// dependency order (leaves first via Kahn's algorithm on out-edges) so callee
/// fingerprints are available when computing callers. Cycles are broken by
/// processing remaining functions with partial callee fingerprints.
///
/// `external_fingerprints` provides deep fingerprints for cross-file callees
/// (looked up from cache). These are seeded into the deep fingerprint map so
/// that cross-file dependency changes propagate to callers' fingerprints.
/// The return map contains only functions from `analyses`, not external entries.
///
/// Returns a map from function name to deep fingerprint.
pub fn compute_deep_fingerprints(
    file_path: &Path,
    analyses: &[FunctionAnalysis],
    external_fingerprints: &HashMap<String, String>,
) -> Result<HashMap<String, String>, std::io::Error> {
    let name_set: HashSet<&str> = analyses.iter().map(|a| a.name.as_str()).collect();

    // Compute shallow fingerprints for all functions.
    let mut shallow: HashMap<String, String> = HashMap::new();
    for func in analyses {
        let source = extract_function_source(file_path, func.start_line, func.end_line)?;
        shallow.insert(func.name.clone(), compute_function_fingerprint(&source, func));
    }

    // Build in-scope callee sets (for Kahn's algorithm ordering).
    let infile_callees_map: HashMap<&str, HashSet<String>> = analyses
        .iter()
        .map(|func| {
            let callees: HashSet<String> = func
                .dependencies
                .iter()
                .map(|d| d.symbol.clone())
                .filter(|s| name_set.contains(s.as_str()))
                .collect();
            (func.name.as_str(), callees)
        })
        .collect();

    // Build full callee sets (including cross-file deps) for deep FP computation.
    let all_callees_map: HashMap<&str, HashSet<String>> = analyses
        .iter()
        .map(|func| {
            let callees: HashSet<String> = func
                .dependencies
                .iter()
                .map(|d| d.symbol.clone())
                .collect();
            (func.name.as_str(), callees)
        })
        .collect();

    // Kahn's algorithm: process leaves (no in-file callees) first.
    let mut out_degree: HashMap<&str, usize> = analyses
        .iter()
        .map(|f| {
            (
                f.name.as_str(),
                infile_callees_map
                    .get(f.name.as_str())
                    .map_or(0, HashSet::len),
            )
        })
        .collect();

    // Reverse: callee → list of callers (in-file only, for topo ordering).
    let mut reverse: HashMap<&str, Vec<&str>> = HashMap::new();
    for (&caller, callees) in &infile_callees_map {
        for callee in callees {
            reverse.entry(callee.as_str()).or_default().push(caller);
        }
    }

    let mut queue: Vec<&str> = out_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(&name, _)| name)
        .collect();
    queue.sort();

    // Seed with external fingerprints so cross-file callees are available.
    let mut deep: HashMap<String, String> = external_fingerprints.clone();

    while let Some(func_name) = queue.pop() {
        if let Some(sfp) = shallow.get(func_name) {
            let callees = all_callees_map
                .get(func_name)
                .cloned()
                .unwrap_or_default();
            deep.insert(
                func_name.to_string(),
                compute_deep_fingerprint(sfp, &deep, &callees),
            );
        }

        if let Some(callers) = reverse.get(func_name) {
            for &caller in callers {
                if let Some(deg) = out_degree.get_mut(caller) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push(caller);
                        queue.sort();
                    }
                }
            }
        }
    }

    // Cycle remnants: process with partial callee fingerprints.
    for func in analyses {
        if !deep.contains_key(&func.name)
            && let Some(sfp) = shallow.get(&func.name)
        {
            let callees = all_callees_map
                .get(func.name.as_str())
                .cloned()
                .unwrap_or_default();
            deep.insert(
                func.name.clone(),
                compute_deep_fingerprint(sfp, &deep, &callees),
            );
        }
    }

    // Filter to only functions from this file (don't leak external entries).
    deep.retain(|k, _| name_set.contains(k.as_str()));

    Ok(deep)
}

/// Extract the source text of a function from a file given line boundaries.
///
/// Reads lines `start_line..=end_line` (1-indexed) from the file and joins
/// them with newlines. Returns an error if the file cannot be read.
pub fn extract_function_source(
    file_path: &Path,
    start_line: u32,
    end_line: u32,
) -> Result<String, std::io::Error> {
    let contents = std::fs::read_to_string(file_path)?;
    let lines: Vec<&str> = contents.lines().collect();
    let start = (start_line as usize).saturating_sub(1);
    let end = (end_line as usize).min(lines.len());
    Ok(lines[start..end].join("\n"))
}

// ---------------------------------------------------------------------------
// Cross-file fingerprint registry and staleness analysis
// ---------------------------------------------------------------------------

/// Cross-file registry of shallow and deep fingerprints, keyed by qualified
/// function name (`file_path::function_name`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FingerprintRegistry {
    shallow: HashMap<String, String>,
    deep: HashMap<String, String>,
    /// Which qualified callee names were incorporated into each function's deep FP.
    dependencies: HashMap<String, HashSet<String>>,
}

impl FingerprintRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_shallow(&mut self, qualified_name: &str, fp: String) {
        self.shallow.insert(qualified_name.to_string(), fp);
    }

    pub fn set_deep(&mut self, qualified_name: &str, fp: String) {
        self.deep.insert(qualified_name.to_string(), fp);
    }

    pub fn set_dependencies(&mut self, qualified_name: &str, deps: HashSet<String>) {
        self.dependencies.insert(qualified_name.to_string(), deps);
    }

    pub fn shallow(&self, qualified_name: &str) -> Option<&str> {
        self.shallow.get(qualified_name).map(String::as_str)
    }

    pub fn deep(&self, qualified_name: &str) -> Option<&str> {
        self.deep.get(qualified_name).map(String::as_str)
    }

    pub fn dependencies(&self, qualified_name: &str) -> Option<&HashSet<String>> {
        self.dependencies.get(qualified_name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.shallow.keys().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.shallow.len()
    }

    pub fn is_empty(&self) -> bool {
        self.shallow.is_empty()
    }
}

/// Compute deep fingerprints for all functions across files using a [`CallGraph`].
///
/// Uses the call graph's topological ordering (leaves first) to ensure callee
/// deep FPs are available before processing callers. Functions not present in
/// `shallow_fps` are skipped. Cycles are broken by processing remaining
/// functions with partial callee fingerprints (same strategy as the single-file
/// version).
///
/// `shallow_fps` maps qualified name → shallow fingerprint. The call graph
/// provides cross-file dependency edges.
pub fn compute_cross_file_deep_fingerprints(
    shallow_fps: &HashMap<String, String>,
    call_graph: &CallGraph,
) -> FingerprintRegistry {
    let mut registry = FingerprintRegistry::new();
    for (name, fp) in shallow_fps {
        registry.set_shallow(name, fp.clone());
    }

    let layers = call_graph.topological_layers();
    let mut deep_map: HashMap<String, String> = HashMap::new();

    for layer in &layers {
        for func_name in layer {
            let sfp = match shallow_fps.get(func_name) {
                Some(fp) => fp,
                None => continue,
            };

            let callees_vec = call_graph.callees_of(func_name);
            let callees: HashSet<String> = callees_vec.into_iter().map(String::from).collect();
            let dfp = compute_deep_fingerprint(sfp, &deep_map, &callees);

            deep_map.insert(func_name.clone(), dfp.clone());
            registry.set_deep(func_name, dfp);
            registry.set_dependencies(func_name, callees);
        }
    }

    // Handle any functions in shallow_fps but not in the call graph (isolated).
    for (name, sfp) in shallow_fps {
        if !deep_map.contains_key(name) {
            let dfp = compute_deep_fingerprint(sfp, &deep_map, &HashSet::new());
            deep_map.insert(name.clone(), dfp.clone());
            registry.set_deep(name, dfp);
            registry.set_dependencies(name, HashSet::new());
        }
    }

    registry
}

/// Why a function was marked stale in cross-file staleness analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StalenessReason {
    /// The function's own source, params, or branches changed.
    SourceChanged,
    /// A transitive callee's fingerprint changed. Contains the callee's qualified name.
    CalleeChanged(String),
    /// The function is new (not in the previous registry).
    New,
    /// The previous registry had no fingerprint for this function.
    NoPreviousFingerprint,
}

/// Result of cross-file staleness analysis using qualified names.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossFileIncrementalPlan {
    /// Qualified names of functions needing re-exploration.
    pub stale: Vec<String>,
    /// Qualified names of functions whose deep FP matches (reuse cache).
    pub fresh: Vec<String>,
    /// Qualified names present in old registry but absent now (deleted).
    pub removed: Vec<String>,
    /// For each stale function, why it's stale.
    pub stale_reasons: HashMap<String, StalenessReason>,
}

/// Compare current fingerprints against previous ones to determine cross-file staleness.
///
/// A function is stale if its deep fingerprint differs from the previous registry,
/// or if it's new / has no previous fingerprint. Staleness propagates transitively
/// through the call graph: if function X is directly stale, all transitive callers
/// of X are also marked stale (with [`StalenessReason::CalleeChanged`]).
pub fn compute_cross_file_staleness(
    current: &FingerprintRegistry,
    previous: &FingerprintRegistry,
    call_graph: &CallGraph,
) -> CrossFileIncrementalPlan {
    let current_names: HashSet<&str> = current.names().collect();
    let previous_names: HashSet<&str> = previous.names().collect();

    // Phase 1: identify directly changed functions.
    let mut directly_stale: Vec<String> = Vec::new();
    let mut direct_reasons: HashMap<String, StalenessReason> = HashMap::new();
    let mut fresh: Vec<String> = Vec::new();

    for name in &current_names {
        let current_deep = current.deep(name);
        let previous_deep = previous.deep(name);

        match (current_deep, previous_deep) {
            (Some(cur), Some(prev)) if cur == prev => {
                fresh.push(name.to_string());
            }
            (Some(_), Some(_)) => {
                directly_stale.push(name.to_string());
                direct_reasons.insert(name.to_string(), StalenessReason::SourceChanged);
            }
            (_, None) if previous_names.contains(name) => {
                directly_stale.push(name.to_string());
                direct_reasons.insert(name.to_string(), StalenessReason::NoPreviousFingerprint);
            }
            _ => {
                directly_stale.push(name.to_string());
                direct_reasons.insert(name.to_string(), StalenessReason::New);
            }
        }
    }

    // Phase 2: propagate staleness transitively through the call graph.
    let seed_refs: Vec<&str> = directly_stale.iter().map(String::as_str).collect();
    let all_affected = call_graph.transitive_callers_of(&seed_refs);

    let mut stale_reasons: HashMap<String, StalenessReason> = direct_reasons;

    // Move transitively-stale functions from fresh to stale.
    let mut final_fresh: Vec<String> = Vec::new();
    let mut propagated_stale: Vec<String> = Vec::new();

    for name in fresh {
        if all_affected.contains(&name) {
            // Find the direct callee that caused this propagation.
            let reason = find_stale_callee(&name, &stale_reasons, call_graph)
                .unwrap_or_else(|| StalenessReason::CalleeChanged(String::new()));
            stale_reasons.insert(name.clone(), reason);
            propagated_stale.push(name);
        } else {
            final_fresh.push(name);
        }
    }

    let mut all_stale = directly_stale;
    all_stale.extend(propagated_stale);

    // Phase 3: detect removed functions.
    let removed: Vec<String> = previous_names
        .iter()
        .filter(|name| !current_names.contains(*name))
        .map(|name| name.to_string())
        .collect();

    CrossFileIncrementalPlan {
        stale: all_stale,
        fresh: final_fresh,
        removed,
        stale_reasons,
    }
}

/// Find a direct callee of `func_name` that is stale, for the CalleeChanged reason.
fn find_stale_callee(
    func_name: &str,
    stale_reasons: &HashMap<String, StalenessReason>,
    call_graph: &CallGraph,
) -> Option<StalenessReason> {
    for callee in call_graph.callees_of(func_name) {
        if stale_reasons.contains_key(callee) {
            return Some(StalenessReason::CalleeChanged(callee.to_string()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{BranchInfo, BranchType};
    use crate::types::{ParamInfo, TypeInfo};
    use std::collections::{HashMap, HashSet};

    fn sample_analysis() -> FunctionAnalysis {
        FunctionAnalysis {
            name: "add".to_string(),
            exported: true,
            params: vec![
                ParamInfo {
                    name: "a".into(),
                    typ: TypeInfo::Int,
                    type_name: None,
                },
                ParamInfo {
                    name: "b".into(),
                    typ: TypeInfo::Int,
                    type_name: None,
                },
            ],
            branches: vec![BranchInfo {
                id: 0,
                line: 3,
                condition_text: "a > 0".into(),
                condition: None,
                branch_type: BranchType::If,
            }],
            dependencies: vec![],
            return_type: TypeInfo::Int,
            start_line: 1,
            end_line: 5,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
        }
    }

    #[test]
    fn identical_inputs_produce_same_fingerprint() {
        let analysis = sample_analysis();
        let source = "function add(a, b) {\n  if (a > 0) return a + b;\n  return b;\n}";

        let fp1 = compute_function_fingerprint(source, &analysis);
        let fp2 = compute_function_fingerprint(source, &analysis);

        assert_eq!(fp1, fp2);
        assert_eq!(fp1.len(), 64); // SHA-256 hex = 64 chars
    }

    #[test]
    fn different_source_text_produces_different_fingerprint() {
        let analysis = sample_analysis();
        let src1 = "function add(a, b) { return a + b; }";
        let src2 = "function add(a, b) { return a - b; }";

        let fp1 = compute_function_fingerprint(src1, &analysis);
        let fp2 = compute_function_fingerprint(src2, &analysis);

        assert_ne!(fp1, fp2);
    }

    #[test]
    fn different_param_types_produces_different_fingerprint() {
        let source = "function add(a, b) { return a + b; }";

        let analysis1 = sample_analysis();
        let mut analysis2 = sample_analysis();
        analysis2.params[0].typ = TypeInfo::Float;

        let fp1 = compute_function_fingerprint(source, &analysis1);
        let fp2 = compute_function_fingerprint(source, &analysis2);

        assert_ne!(fp1, fp2);
    }

    #[test]
    fn different_branch_structure_produces_different_fingerprint() {
        let source = "function add(a, b) { return a + b; }";

        let analysis1 = sample_analysis();
        let mut analysis2 = sample_analysis();
        analysis2.branches.push(BranchInfo {
            id: 1,
            line: 4,
            condition_text: "b > 0".into(),
            condition: None,
            branch_type: BranchType::If,
        });

        let fp1 = compute_function_fingerprint(source, &analysis1);
        let fp2 = compute_function_fingerprint(source, &analysis2);

        assert_ne!(fp1, fp2);
    }

    #[test]
    fn param_order_does_not_affect_fingerprint() {
        let source = "function add(a, b) { return a + b; }";

        let analysis1 = sample_analysis();
        let mut analysis2 = sample_analysis();
        analysis2.params.reverse();

        let fp1 = compute_function_fingerprint(source, &analysis1);
        let fp2 = compute_function_fingerprint(source, &analysis2);

        // Params are sorted by name internally, so order shouldn't matter.
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn whitespace_change_in_source_produces_different_fingerprint() {
        let analysis = sample_analysis();
        let src1 = "function add(a, b) { return a + b; }";
        let src2 = "function add(a, b) {  return a + b; }";

        let fp1 = compute_function_fingerprint(src1, &analysis);
        let fp2 = compute_function_fingerprint(src2, &analysis);

        assert_ne!(fp1, fp2);
    }

    #[test]
    fn extract_function_source_reads_correct_lines() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        std::fs::write(
            &file,
            "line1\nline2\nline3\nline4\nline5\n",
        )
        .unwrap();

        let source = extract_function_source(&file, 2, 4).unwrap();
        assert_eq!(source, "line2\nline3\nline4");
    }

    #[test]
    fn extract_function_source_handles_single_line() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        std::fs::write(&file, "only line\n").unwrap();

        let source = extract_function_source(&file, 1, 1).unwrap();
        assert_eq!(source, "only line");
    }

    // --- deep fingerprint tests ---

    #[test]
    fn deep_fp_same_inputs_same_output() {
        let callee_fps: HashMap<String, String> =
            [("leaf".into(), "aaa".into())].into_iter().collect();
        let callees: HashSet<String> = ["leaf".into()].into_iter().collect();

        let fp1 = compute_deep_fingerprint("shallow1", &callee_fps, &callees);
        let fp2 = compute_deep_fingerprint("shallow1", &callee_fps, &callees);
        assert_eq!(fp1, fp2);
        assert_eq!(fp1.len(), 64);
    }

    #[test]
    fn deep_fp_changes_when_callee_fp_changes() {
        let callees: HashSet<String> = ["leaf".into()].into_iter().collect();

        let fps1: HashMap<String, String> =
            [("leaf".into(), "aaa".into())].into_iter().collect();
        let fps2: HashMap<String, String> =
            [("leaf".into(), "bbb".into())].into_iter().collect();

        let fp1 = compute_deep_fingerprint("shallow1", &fps1, &callees);
        let fp2 = compute_deep_fingerprint("shallow1", &fps2, &callees);
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn deep_fp_ignores_out_of_scope_callees() {
        let callees: HashSet<String> = ["leaf".into()].into_iter().collect();
        let callee_fps: HashMap<String, String> =
            [("leaf".into(), "aaa".into()), ("other".into(), "bbb".into())]
                .into_iter()
                .collect();

        // "other" is in the map but not in callees — should be ignored
        let fp_with_extra = compute_deep_fingerprint("shallow1", &callee_fps, &callees);

        let callee_fps_minimal: HashMap<String, String> =
            [("leaf".into(), "aaa".into())].into_iter().collect();
        let fp_minimal = compute_deep_fingerprint("shallow1", &callee_fps_minimal, &callees);

        assert_eq!(fp_with_extra, fp_minimal);
    }

    #[test]
    fn deep_fp_no_callees_is_deterministic() {
        let empty_fps: HashMap<String, String> = HashMap::new();
        let empty_callees: HashSet<String> = HashSet::new();

        let fp1 = compute_deep_fingerprint("shallow1", &empty_fps, &empty_callees);
        let fp2 = compute_deep_fingerprint("shallow1", &empty_fps, &empty_callees);
        assert_eq!(fp1, fp2);
        // Deep FP differs from the shallow FP string itself (it's a hash).
        assert_ne!(fp1, "shallow1");
    }

    #[test]
    fn deep_fp_differs_with_different_shallow() {
        let empty_fps: HashMap<String, String> = HashMap::new();
        let empty_callees: HashSet<String> = HashSet::new();

        let fp1 = compute_deep_fingerprint("shallow1", &empty_fps, &empty_callees);
        let fp2 = compute_deep_fingerprint("shallow2", &empty_fps, &empty_callees);
        assert_ne!(fp1, fp2);
    }

    // --- compute_deep_fingerprints (file-level) tests ---

    use crate::protocol::{DependencyKind, ExternalDependency};

    fn make_analysis(
        name: &str,
        start_line: u32,
        end_line: u32,
        deps: Vec<&str>,
    ) -> FunctionAnalysis {
        FunctionAnalysis {
            name: name.to_string(),
            exported: true,
            params: vec![],
            branches: vec![],
            dependencies: deps
                .into_iter()
                .map(|s| ExternalDependency {
                    kind: DependencyKind::FunctionCall,
                    symbol: s.to_string(),
                    source_module: String::new(),
                    return_type: TypeInfo::Unknown,
                    param_types: vec![],
                    call_sites: vec![],
                })
                .collect(),
            return_type: TypeInfo::Unknown,
            start_line,
            end_line,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
        }
    }

    #[test]
    fn deep_fingerprints_single_function_no_deps() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        std::fs::write(&file, "function leaf() { return 1; }\n").unwrap();

        let analyses = vec![make_analysis("leaf", 1, 1, vec![])];
        let fps = compute_deep_fingerprints(&file, &analyses, &HashMap::new()).unwrap();

        assert_eq!(fps.len(), 1);
        assert!(fps.contains_key("leaf"));
        assert_eq!(fps["leaf"].len(), 64);
    }

    #[test]
    fn deep_fingerprints_caller_incorporates_callee() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        std::fs::write(
            &file,
            "function leaf() { return 1; }\nfunction caller() { return leaf(); }\n",
        )
        .unwrap();

        let analyses = vec![
            make_analysis("leaf", 1, 1, vec![]),
            make_analysis("caller", 2, 2, vec!["leaf"]),
        ];
        let fps = compute_deep_fingerprints(&file, &analyses, &HashMap::new()).unwrap();

        assert_eq!(fps.len(), 2);

        // caller's deep FP should differ from a standalone computation without callees.
        let caller_shallow = compute_function_fingerprint(
            "function caller() { return leaf(); }",
            &analyses[1],
        );
        let caller_no_deps = compute_deep_fingerprint(
            &caller_shallow,
            &HashMap::new(),
            &HashSet::new(),
        );
        assert_ne!(fps["caller"], caller_no_deps);
    }

    #[test]
    fn deep_fingerprints_callee_change_propagates_to_caller() {
        let dir = tempfile::tempdir().unwrap();
        let file1 = dir.path().join("v1.ts");
        let file2 = dir.path().join("v2.ts");

        std::fs::write(
            &file1,
            "function leaf() { return 1; }\nfunction caller() { return leaf(); }\n",
        )
        .unwrap();
        std::fs::write(
            &file2,
            "function leaf() { return 2; }\nfunction caller() { return leaf(); }\n",
        )
        .unwrap();

        let analyses = vec![
            make_analysis("leaf", 1, 1, vec![]),
            make_analysis("caller", 2, 2, vec!["leaf"]),
        ];

        let fps1 = compute_deep_fingerprints(&file1, &analyses, &HashMap::new()).unwrap();
        let fps2 = compute_deep_fingerprints(&file2, &analyses, &HashMap::new()).unwrap();

        // leaf changed → leaf's FP differs
        assert_ne!(fps1["leaf"], fps2["leaf"]);
        // caller's source is the same but callee changed → caller's deep FP differs
        assert_ne!(fps1["caller"], fps2["caller"]);
    }

    #[test]
    fn deep_fingerprints_out_of_scope_dep_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        std::fs::write(&file, "function caller() { return external(); }\n").unwrap();

        // "external" is not in analyses — should be ignored
        let analyses = vec![make_analysis("caller", 1, 1, vec!["external"])];
        let fps = compute_deep_fingerprints(&file, &analyses, &HashMap::new()).unwrap();

        assert_eq!(fps.len(), 1);
        assert!(fps.contains_key("caller"));
    }

    #[test]
    fn deep_fingerprints_diamond_dependency() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        std::fs::write(
            &file,
            "function d() { return 0; }\nfunction b() { return d(); }\nfunction c() { return d(); }\nfunction a() { return b() + c(); }\n",
        )
        .unwrap();

        let analyses = vec![
            make_analysis("d", 1, 1, vec![]),
            make_analysis("b", 2, 2, vec!["d"]),
            make_analysis("c", 3, 3, vec!["d"]),
            make_analysis("a", 4, 4, vec!["b", "c"]),
        ];

        let fps = compute_deep_fingerprints(&file, &analyses, &HashMap::new()).unwrap();
        assert_eq!(fps.len(), 4);

        // All should have valid 64-char hex fingerprints.
        for fp in fps.values() {
            assert_eq!(fp.len(), 64);
        }
    }


    // --- FingerprintRegistry tests ---

    #[test]
    fn registry_basic_crud() {
        let mut reg = FingerprintRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);

        reg.set_shallow("a.ts::foo", "shallow1".into());
        reg.set_deep("a.ts::foo", "deep1".into());
        reg.set_dependencies("a.ts::foo", ["a.ts::bar".into()].into_iter().collect());

        assert_eq!(reg.len(), 1);
        assert!(!reg.is_empty());
        assert_eq!(reg.shallow("a.ts::foo"), Some("shallow1"));
        assert_eq!(reg.deep("a.ts::foo"), Some("deep1"));
        assert!(reg.dependencies("a.ts::foo").unwrap().contains("a.ts::bar"));
        assert_eq!(reg.shallow("nonexistent"), None);
    }

    #[test]
    fn registry_names_iteration() {
        let mut reg = FingerprintRegistry::new();
        reg.set_shallow("a.ts::foo", "s1".into());
        reg.set_shallow("b.ts::bar", "s2".into());

        let names: HashSet<&str> = reg.names().collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains("a.ts::foo"));
        assert!(names.contains("b.ts::bar"));
    }

    // --- compute_cross_file_deep_fingerprints tests ---

    mod cross_file {
        use super::*;
        use crate::batch_analyze::{FunctionEntry, FunctionRegistry};
        use crate::call_graph::CallGraph;
        use crate::protocol::{DependencyKind, ExternalDependency};
        use std::path::PathBuf;

        type FuncSpec<'a> = (&'a str, &'a str, Vec<(&'a str, &'a str)>);

        fn make_registry_for_graph(
            funcs: &[FuncSpec<'_>],
        ) -> FunctionRegistry {
            let mut entries = Vec::new();
            let mut index = HashMap::new();
            for (file, name, deps) in funcs {
                let qn = FunctionRegistry::qualified_name(&PathBuf::from(file), name);
                let idx = entries.len();
                index.insert(qn, idx);
                entries.push(FunctionEntry {
                    file_path: PathBuf::from(file),
                    name: name.to_string(),
                    exported: true,
                    params: vec![],
                    return_type: TypeInfo::Unknown,
                    dependencies: deps
                        .iter()
                        .map(|(sym, module)| ExternalDependency {
                            kind: DependencyKind::FunctionCall,
                            symbol: sym.to_string(),
                            source_module: module.to_string(),
                            return_type: TypeInfo::Unknown,
                            param_types: vec![],
                            call_sites: vec![],
                        })
                        .collect(),
                    branch_count: 0,
                    start_line: 1,
                    end_line: 10,
                    crypto_boundaries: vec![],
                });
            }
            FunctionRegistry::from_raw(entries, index)
        }

        #[test]
        fn cross_file_composition() {
            let reg = make_registry_for_graph(&[
                ("src/b.ts", "helper", vec![]),
                ("src/a.ts", "main", vec![("helper", "src/b.ts")]),
            ]);
            let graph = CallGraph::from_registry(&reg);

            let mut shallow = HashMap::new();
            shallow.insert("src/b.ts::helper".into(), "aaa".repeat(22)[..64].to_string());
            shallow.insert("src/a.ts::main".into(), "bbb".repeat(22)[..64].to_string());

            let fp_reg = compute_cross_file_deep_fingerprints(&shallow, &graph);

            assert_eq!(fp_reg.len(), 2);
            assert!(fp_reg.deep("src/b.ts::helper").is_some());
            assert!(fp_reg.deep("src/a.ts::main").is_some());

            // main's deep FP should differ from a standalone computation without callees.
            let main_standalone = compute_deep_fingerprint(
                shallow.get("src/a.ts::main").unwrap(),
                &HashMap::new(),
                &HashSet::new(),
            );
            assert_ne!(fp_reg.deep("src/a.ts::main").unwrap(), main_standalone);
        }

        #[test]
        fn cross_file_callee_change_propagates() {
            let reg = make_registry_for_graph(&[
                ("src/b.ts", "helper", vec![]),
                ("src/a.ts", "main", vec![("helper", "src/b.ts")]),
            ]);
            let graph = CallGraph::from_registry(&reg);

            // Version 1: helper has one shallow FP.
            let mut shallow_v1 = HashMap::new();
            shallow_v1.insert("src/b.ts::helper".into(), "a".repeat(64));
            shallow_v1.insert("src/a.ts::main".into(), "b".repeat(64));
            let reg1 = compute_cross_file_deep_fingerprints(&shallow_v1, &graph);

            // Version 2: helper's shallow FP changes, main's stays the same.
            let mut shallow_v2 = HashMap::new();
            shallow_v2.insert("src/b.ts::helper".into(), "c".repeat(64));
            shallow_v2.insert("src/a.ts::main".into(), "b".repeat(64));
            let reg2 = compute_cross_file_deep_fingerprints(&shallow_v2, &graph);

            // helper's deep FP changes.
            assert_ne!(reg1.deep("src/b.ts::helper"), reg2.deep("src/b.ts::helper"));
            // main's deep FP also changes (callee changed).
            assert_ne!(reg1.deep("src/a.ts::main"), reg2.deep("src/a.ts::main"));
        }

        #[test]
        fn cross_file_diamond() {
            // a::top → b::left, a::top → c::right, b::left → d::leaf, c::right → d::leaf
            let reg = make_registry_for_graph(&[
                ("src/d.ts", "leaf", vec![]),
                ("src/b.ts", "left", vec![("leaf", "src/d.ts")]),
                ("src/c.ts", "right", vec![("leaf", "src/d.ts")]),
                ("src/a.ts", "top", vec![("left", "src/b.ts"), ("right", "src/c.ts")]),
            ]);
            let graph = CallGraph::from_registry(&reg);

            let shallow: HashMap<String, String> = [
                ("src/d.ts::leaf", "d"),
                ("src/b.ts::left", "b"),
                ("src/c.ts::right", "c"),
                ("src/a.ts::top", "a"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.repeat(64)))
            .collect();

            let fp_reg = compute_cross_file_deep_fingerprints(&shallow, &graph);
            assert_eq!(fp_reg.len(), 4);

            // All should have 64-char hex deep FPs.
            for name in fp_reg.names() {
                let dfp = fp_reg.deep(name).unwrap();
                assert_eq!(dfp.len(), 64, "deep FP for {name} should be 64 chars");
            }
        }

        #[test]
        fn cross_file_isolated_function() {
            // A function in shallow_fps but not in the call graph.
            let reg = make_registry_for_graph(&[]);
            let graph = CallGraph::from_registry(&reg);

            let mut shallow = HashMap::new();
            shallow.insert("orphan.ts::lonely".into(), "x".repeat(64));

            let fp_reg = compute_cross_file_deep_fingerprints(&shallow, &graph);
            assert_eq!(fp_reg.len(), 1);
            assert!(fp_reg.deep("orphan.ts::lonely").is_some());
        }

        // --- compute_cross_file_staleness tests ---

        #[test]
        fn staleness_all_fresh() {
            let reg = make_registry_for_graph(&[
                ("src/a.ts", "foo", vec![]),
            ]);
            let graph = CallGraph::from_registry(&reg);

            let mut current = FingerprintRegistry::new();
            current.set_shallow("src/a.ts::foo", "s1".into());
            current.set_deep("src/a.ts::foo", "d1".into());

            let previous = current.clone();

            let plan = compute_cross_file_staleness(&current, &previous, &graph);
            assert!(plan.stale.is_empty());
            assert_eq!(plan.fresh, vec!["src/a.ts::foo"]);
            assert!(plan.removed.is_empty());
        }

        #[test]
        fn staleness_direct_source_change() {
            let reg = make_registry_for_graph(&[("src/a.ts", "foo", vec![])]);
            let graph = CallGraph::from_registry(&reg);

            let mut previous = FingerprintRegistry::new();
            previous.set_shallow("src/a.ts::foo", "s1".into());
            previous.set_deep("src/a.ts::foo", "d1".into());

            let mut current = FingerprintRegistry::new();
            current.set_shallow("src/a.ts::foo", "s2".into());
            current.set_deep("src/a.ts::foo", "d2".into());

            let plan = compute_cross_file_staleness(&current, &previous, &graph);
            assert_eq!(plan.stale, vec!["src/a.ts::foo"]);
            assert!(plan.fresh.is_empty());
            assert_eq!(
                plan.stale_reasons["src/a.ts::foo"],
                StalenessReason::SourceChanged
            );
        }

        #[test]
        fn staleness_transitive_propagation() {
            // main → helper: helper changes → main is stale (CalleeChanged).
            // When deep FPs are computed per-file (without cross-file awareness),
            // main's deep FP may stay the same even though its cross-file callee
            // changed. The transitive propagation catches this.
            let reg = make_registry_for_graph(&[
                ("src/b.ts", "helper", vec![]),
                ("src/a.ts", "main", vec![("helper", "src/b.ts")]),
            ]);
            let graph = CallGraph::from_registry(&reg);

            let mut previous = FingerprintRegistry::new();
            previous.set_shallow("src/b.ts::helper", "s1".into());
            previous.set_deep("src/b.ts::helper", "d1".into());
            previous.set_shallow("src/a.ts::main", "s2".into());
            previous.set_deep("src/a.ts::main", "d2".into());

            let mut current = FingerprintRegistry::new();
            current.set_shallow("src/b.ts::helper", "s1_changed".into());
            current.set_deep("src/b.ts::helper", "d1_changed".into());
            // main's own source unchanged, and per-file deep FP unchanged
            // (cross-file callee was out of scope during per-file computation).
            current.set_shallow("src/a.ts::main", "s2".into());
            current.set_deep("src/a.ts::main", "d2".into());

            let plan = compute_cross_file_staleness(&current, &previous, &graph);
            assert!(plan.stale.contains(&"src/b.ts::helper".to_string()));
            assert!(plan.stale.contains(&"src/a.ts::main".to_string()));
            assert!(plan.fresh.is_empty());

            assert_eq!(
                plan.stale_reasons["src/b.ts::helper"],
                StalenessReason::SourceChanged
            );
            assert!(matches!(
                &plan.stale_reasons["src/a.ts::main"],
                StalenessReason::CalleeChanged(callee) if callee == "src/b.ts::helper"
            ));
        }

        #[test]
        fn staleness_new_function() {
            let reg = make_registry_for_graph(&[("src/a.ts", "new_fn", vec![])]);
            let graph = CallGraph::from_registry(&reg);

            let mut current = FingerprintRegistry::new();
            current.set_shallow("src/a.ts::new_fn", "s1".into());
            current.set_deep("src/a.ts::new_fn", "d1".into());

            let previous = FingerprintRegistry::new();

            let plan = compute_cross_file_staleness(&current, &previous, &graph);
            assert_eq!(plan.stale, vec!["src/a.ts::new_fn"]);
            assert_eq!(
                plan.stale_reasons["src/a.ts::new_fn"],
                StalenessReason::New
            );
        }

        #[test]
        fn staleness_removed_function() {
            let reg = make_registry_for_graph(&[]);
            let graph = CallGraph::from_registry(&reg);

            let mut previous = FingerprintRegistry::new();
            previous.set_shallow("src/a.ts::deleted", "s1".into());
            previous.set_deep("src/a.ts::deleted", "d1".into());

            let current = FingerprintRegistry::new();

            let plan = compute_cross_file_staleness(&current, &previous, &graph);
            assert!(plan.stale.is_empty());
            assert!(plan.fresh.is_empty());
            assert_eq!(plan.removed, vec!["src/a.ts::deleted"]);
        }

        #[test]
        fn staleness_diamond_propagation() {
            // top → left, top → right, left → leaf, right → leaf
            // leaf changes → left, right, top all stale via transitive propagation
            let reg = make_registry_for_graph(&[
                ("src/d.ts", "leaf", vec![]),
                ("src/b.ts", "left", vec![("leaf", "src/d.ts")]),
                ("src/c.ts", "right", vec![("leaf", "src/d.ts")]),
                ("src/a.ts", "top", vec![("left", "src/b.ts"), ("right", "src/c.ts")]),
            ]);
            let graph = CallGraph::from_registry(&reg);

            let names = [
                "src/d.ts::leaf",
                "src/b.ts::left",
                "src/c.ts::right",
                "src/a.ts::top",
            ];

            let mut previous = FingerprintRegistry::new();
            for (i, name) in names.iter().enumerate() {
                previous.set_shallow(name, format!("s{i}"));
                previous.set_deep(name, format!("d{i}"));
            }

            let mut current = FingerprintRegistry::new();
            // Only leaf changes directly.
            current.set_shallow("src/d.ts::leaf", "s0_new".into());
            current.set_deep("src/d.ts::leaf", "d0_new".into());
            // Others: per-file deep FPs unchanged (cross-file callee out of scope).
            for (i, name) in names[1..].iter().enumerate() {
                current.set_shallow(name, format!("s{}", i + 1));
                current.set_deep(name, format!("d{}", i + 1));
            }

            let plan = compute_cross_file_staleness(&current, &previous, &graph);
            assert_eq!(plan.stale.len(), 4);
            assert!(plan.fresh.is_empty());
        }
    }

    #[test]
    fn deep_fingerprints_cross_file_callee_changes_caller() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        std::fs::write(&file, "function caller() { return external(); }\n").unwrap();

        // "external" is a cross-file dep — not in analyses but in external_fingerprints.
        let analyses = vec![make_analysis("caller", 1, 1, vec!["external"])];

        let ext_v1: HashMap<String, String> =
            [("external".into(), "aaa".repeat(22))].into_iter().collect();
        let ext_v2: HashMap<String, String> =
            [("external".into(), "bbb".repeat(22))].into_iter().collect();

        let fps_v1 = compute_deep_fingerprints(&file, &analyses, &ext_v1).unwrap();
        let fps_v2 = compute_deep_fingerprints(&file, &analyses, &ext_v2).unwrap();

        // caller's deep FP should change when the external callee's FP changes.
        assert_ne!(fps_v1["caller"], fps_v2["caller"]);
    }

    #[test]
    fn deep_fingerprints_external_entries_not_leaked() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        std::fs::write(&file, "function caller() { return external(); }\n").unwrap();

        let analyses = vec![make_analysis("caller", 1, 1, vec!["external"])];
        let ext: HashMap<String, String> =
            [("external".into(), "aaa".repeat(22))].into_iter().collect();

        let fps = compute_deep_fingerprints(&file, &analyses, &ext).unwrap();

        // Only functions from analyses should appear in the result.
        assert_eq!(fps.len(), 1);
        assert!(fps.contains_key("caller"));
        assert!(!fps.contains_key("external"));
    }

    #[test]
    fn deep_fingerprints_empty_external_preserves_behavior() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        std::fs::write(
            &file,
            "function leaf() { return 1; }\nfunction caller() { return leaf(); }\n",
        )
        .unwrap();

        let analyses = vec![
            make_analysis("leaf", 1, 1, vec![]),
            make_analysis("caller", 2, 2, vec!["leaf"]),
        ];

        let fps = compute_deep_fingerprints(&file, &analyses, &HashMap::new()).unwrap();
        assert_eq!(fps.len(), 2);

        // caller's deep FP should still incorporate leaf (in-file dep).
        let caller_shallow = compute_function_fingerprint(
            "function caller() { return leaf(); }",
            &analyses[1],
        );
        let caller_no_deps = compute_deep_fingerprint(
            &caller_shallow,
            &HashMap::new(),
            &HashSet::new(),
        );
        assert_ne!(fps["caller"], caller_no_deps);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::test_arbitraries::arb_function_analysis;
    use proptest::prelude::*;
    use std::collections::{HashMap, HashSet};

    proptest! {
        /// Fingerprints are deterministic: same inputs always produce the same hash.
        #[test]
        fn fingerprint_deterministic(
            source in ".*",
            analysis in arb_function_analysis(),
        ) {
            let fp1 = compute_function_fingerprint(&source, &analysis);
            let fp2 = compute_function_fingerprint(&source, &analysis);
            prop_assert_eq!(&fp1, &fp2);
        }

        /// Fingerprints are always 64-character hex strings (SHA-256).
        #[test]
        fn fingerprint_length_invariant(
            source in ".*",
            analysis in arb_function_analysis(),
        ) {
            let fp = compute_function_fingerprint(&source, &analysis);
            prop_assert_eq!(fp.len(), 64);
            prop_assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
        }

        /// Deep fingerprints are deterministic.
        #[test]
        fn deep_fingerprint_deterministic(
            shallow in "[a-f0-9]{64}",
            callee_name in "[a-z_]{1,20}",
            callee_fp in "[a-f0-9]{64}",
        ) {
            let callee_fps: HashMap<String, String> =
                [(callee_name.clone(), callee_fp)].into_iter().collect();
            let callees: HashSet<String> = [callee_name].into_iter().collect();

            let fp1 = compute_deep_fingerprint(&shallow, &callee_fps, &callees);
            let fp2 = compute_deep_fingerprint(&shallow, &callee_fps, &callees);
            prop_assert_eq!(&fp1, &fp2);
        }

        /// Deep fingerprints are always 64-character hex strings.
        #[test]
        fn deep_fingerprint_length_invariant(
            shallow in "[a-f0-9]{64}",
        ) {
            let fp = compute_deep_fingerprint(&shallow, &HashMap::new(), &HashSet::new());
            prop_assert_eq!(fp.len(), 64);
            prop_assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
        }

        /// Different source text produces different fingerprints (collision resistance).
        #[test]
        fn different_source_different_fingerprint(
            src1 in ".{1,100}",
            src2 in ".{1,100}",
            analysis in arb_function_analysis(),
        ) {
            prop_assume!(src1 != src2);
            let fp1 = compute_function_fingerprint(&src1, &analysis);
            let fp2 = compute_function_fingerprint(&src2, &analysis);
            prop_assert_ne!(fp1, fp2);
        }

        /// Cross-file deep FPs are deterministic: same inputs → same registry.
        #[test]
        fn cross_file_deterministic(
            fp1 in "[a-f0-9]{64}",
            fp2 in "[a-f0-9]{64}",
        ) {
            use crate::batch_analyze::{FunctionEntry, FunctionRegistry};
            use crate::call_graph::CallGraph;
            use crate::protocol::{DependencyKind, ExternalDependency};
            use std::path::PathBuf;

            let mut entries = Vec::new();
            let mut index = HashMap::new();

            // Two functions: leaf and caller
            let qn1 = FunctionRegistry::qualified_name(&PathBuf::from("a.ts"), "leaf");
            index.insert(qn1, 0);
            entries.push(FunctionEntry {
                file_path: PathBuf::from("a.ts"),
                name: "leaf".into(),
                exported: true,
                params: vec![],
                return_type: crate::types::TypeInfo::Unknown,
                dependencies: vec![],
                branch_count: 0,
                start_line: 1,
                end_line: 5,
                crypto_boundaries: vec![],
            });

            let qn2 = FunctionRegistry::qualified_name(&PathBuf::from("b.ts"), "caller");
            index.insert(qn2, 1);
            entries.push(FunctionEntry {
                file_path: PathBuf::from("b.ts"),
                name: "caller".into(),
                exported: true,
                params: vec![],
                return_type: crate::types::TypeInfo::Unknown,
                dependencies: vec![ExternalDependency {
                    kind: DependencyKind::FunctionCall,
                    symbol: "leaf".into(),
                    source_module: "a.ts".into(),
                    return_type: crate::types::TypeInfo::Unknown,
                    param_types: vec![],
                    call_sites: vec![],
                }],
                branch_count: 0,
                start_line: 1,
                end_line: 5,
                crypto_boundaries: vec![],
            });

            let freg = FunctionRegistry::from_raw(entries, index);
            let graph = CallGraph::from_registry(&freg);

            let mut shallow = HashMap::new();
            shallow.insert("a.ts::leaf".into(), fp1);
            shallow.insert("b.ts::caller".into(), fp2);

            let reg1 = compute_cross_file_deep_fingerprints(&shallow, &graph);
            let reg2 = compute_cross_file_deep_fingerprints(&shallow, &graph);

            prop_assert_eq!(reg1.deep("a.ts::leaf"), reg2.deep("a.ts::leaf"));
            prop_assert_eq!(reg1.deep("b.ts::caller"), reg2.deep("b.ts::caller"));
        }

        /// All deep FPs in the registry are 64-char hex strings.
        #[test]
        fn cross_file_deep_fp_length_invariant(
            fp in "[a-f0-9]{64}",
        ) {
            use crate::call_graph::CallGraph;
            use crate::batch_analyze::FunctionRegistry;

            let freg = FunctionRegistry::from_raw(vec![], HashMap::new());
            let graph = CallGraph::from_registry(&freg);

            let mut shallow = HashMap::new();
            shallow.insert("test::func".into(), fp);

            let reg = compute_cross_file_deep_fingerprints(&shallow, &graph);
            let dfp = reg.deep("test::func").unwrap();
            prop_assert_eq!(dfp.len(), 64);
            prop_assert!(dfp.chars().all(|c| c.is_ascii_hexdigit()));
        }

        /// External callee fingerprint change propagates to caller's deep fingerprint.
        #[test]
        fn external_callee_change_propagates(
            shallow in "[a-f0-9]{64}",
            ext_fp1 in "[a-f0-9]{64}",
            ext_fp2 in "[a-f0-9]{64}",
            callee_name in "[a-z_]{1,20}",
        ) {
            prop_assume!(ext_fp1 != ext_fp2);
            let callees: HashSet<String> = [callee_name.clone()].into_iter().collect();

            let ext1: HashMap<String, String> =
                [(callee_name.clone(), ext_fp1)].into_iter().collect();
            let ext2: HashMap<String, String> =
                [(callee_name, ext_fp2)].into_iter().collect();

            let fp1 = compute_deep_fingerprint(&shallow, &ext1, &callees);
            let fp2 = compute_deep_fingerprint(&shallow, &ext2, &callees);
            prop_assert_ne!(fp1, fp2);
        }
    }
}
