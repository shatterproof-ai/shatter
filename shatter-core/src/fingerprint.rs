//! Per-function fingerprinting for staleness detection.
//!
//! A fingerprint is a stable SHA-256 hash of a function's source text,
//! parameter types, and branch structure. When a fingerprint matches a
//! previously cached value, the function is unchanged and can be skipped
//! during re-exploration.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use sha2::{Digest, Sha256};

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
/// fingerprints are available when computing callers. Out-of-scope callees
/// (not in `analyses`) are ignored. Cycles are broken by processing remaining
/// functions with partial callee fingerprints.
///
/// Returns a map from function name to deep fingerprint.
pub fn compute_deep_fingerprints(
    file_path: &Path,
    analyses: &[FunctionAnalysis],
) -> Result<HashMap<String, String>, std::io::Error> {
    let name_set: HashSet<&str> = analyses.iter().map(|a| a.name.as_str()).collect();

    // Compute shallow fingerprints for all functions.
    let mut shallow: HashMap<String, String> = HashMap::new();
    for func in analyses {
        let source = extract_function_source(file_path, func.start_line, func.end_line)?;
        shallow.insert(func.name.clone(), compute_function_fingerprint(&source, func));
    }

    // Build in-scope callee sets per function.
    let callees_map: HashMap<&str, HashSet<String>> = analyses
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

    // Kahn's algorithm: process leaves (no in-scope callees) first.
    // out_degree = number of unprocessed in-scope callees.
    let mut out_degree: HashMap<&str, usize> = analyses
        .iter()
        .map(|f| {
            (
                f.name.as_str(),
                callees_map.get(f.name.as_str()).map_or(0, HashSet::len),
            )
        })
        .collect();

    // Reverse: callee → list of callers.
    let mut reverse: HashMap<&str, Vec<&str>> = HashMap::new();
    for (&caller, callees) in &callees_map {
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

    let mut deep: HashMap<String, String> = HashMap::new();

    while let Some(func_name) = queue.pop() {
        if let Some(sfp) = shallow.get(func_name) {
            let callees = callees_map
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
            let callees = callees_map
                .get(func.name.as_str())
                .cloned()
                .unwrap_or_default();
            deep.insert(
                func.name.clone(),
                compute_deep_fingerprint(sfp, &deep, &callees),
            );
        }
    }

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
        }
    }

    #[test]
    fn deep_fingerprints_single_function_no_deps() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        std::fs::write(&file, "function leaf() { return 1; }\n").unwrap();

        let analyses = vec![make_analysis("leaf", 1, 1, vec![])];
        let fps = compute_deep_fingerprints(&file, &analyses).unwrap();

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
        let fps = compute_deep_fingerprints(&file, &analyses).unwrap();

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

        let fps1 = compute_deep_fingerprints(&file1, &analyses).unwrap();
        let fps2 = compute_deep_fingerprints(&file2, &analyses).unwrap();

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
        let fps = compute_deep_fingerprints(&file, &analyses).unwrap();

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

        let fps = compute_deep_fingerprints(&file, &analyses).unwrap();
        assert_eq!(fps.len(), 4);

        // All should have valid 64-char hex fingerprints.
        for (_, fp) in &fps {
            assert_eq!(fp.len(), 64);
        }
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
    }
}
