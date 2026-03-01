//! Per-function fingerprinting for staleness detection.
//!
//! A fingerprint is a stable SHA-256 hash of a function's source text,
//! parameter types, and branch structure. When a fingerprint matches a
//! previously cached value, the function is unchanged and can be skipped
//! during re-exploration.

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
}
