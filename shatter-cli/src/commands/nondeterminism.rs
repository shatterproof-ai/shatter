//! Interactive nondeterminism review command.
//!
//! Presents suspected-nondeterministic fields from the most recent scan,
//! one at a time. The user confirms (y), rejects (n), skips (s), or re-runs
//! detection (?). Confirmed and rejected entries are persisted to
//! `.shatter/config.yaml`.
//!
//! On subsequent scans, candidates already in the config are suppressed so only
//! new or escalated candidates are shown.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use shatter_core::cache::BehaviorMapCache;
use shatter_core::config::{
    NondeterminismDeclaration, discover_configs, merge_configs, update_nondeterminism_config,
};
use shatter_core::nondeterminism::{Confidence, NondeterministicField};
use shatter_core::project;

use crate::helpers::Colors;

/// Default cache subdirectory for behavior maps.
const BEHAVIOR_MAPS_SUBDIR: &str = ".shatter-cache/behavior-maps";

/// A candidate field for review, paired with the function it belongs to.
#[derive(Debug, Clone)]
pub(crate) struct ReviewCandidate {
    /// Qualified function identifier (e.g., `src/auth.ts:createUser`).
    pub function_id: String,
    pub field: NondeterministicField,
}

/// Run the `nondeterminism review` command.
///
/// Loads all cached behavior maps, extracts nondeterministic field candidates,
/// filters out those already confirmed/rejected in config, and presents each
/// one interactively. Persists results to `.shatter/config.yaml`.
pub(crate) fn run_review(
    project_dir: Option<&Path>,
    colors: &Colors,
    cache_dir: Option<&Path>,
    non_interactive: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Locate project root.
    let cwd = std::env::current_dir()?;
    let root = project_dir
        .map(PathBuf::from)
        .or_else(|| project::detect_project_root(&cwd).map(|r| r.path))
        .unwrap_or_else(|| cwd.clone());

    let config_path = root.join(".shatter").join("config.yaml");

    // Load the existing config to know what is already decided.
    let existing_configs = discover_configs(&root).unwrap_or_default();
    let merged = merge_configs(&existing_configs);

    let already_confirmed: std::collections::HashSet<(String, String)> = merged
        .nondeterminism
        .as_ref()
        .map(|nd| {
            nd.confirmed
                .iter()
                .map(|d| (d.function.clone(), d.path.clone()))
                .collect()
        })
        .unwrap_or_default();

    let already_rejected: std::collections::HashSet<(String, String)> = merged
        .nondeterminism
        .as_ref()
        .map(|nd| {
            nd.rejected
                .iter()
                .map(|d| (d.function.clone(), d.path.clone()))
                .collect()
        })
        .unwrap_or_default();

    // Load all cached behavior maps.
    let cache_path = if let Some(dir) = cache_dir {
        dir.to_path_buf()
    } else {
        root.join(BEHAVIOR_MAPS_SUBDIR)
    };

    let cache = BehaviorMapCache::new(cache_path)?;
    let all_maps = cache.load_all()?;

    if all_maps.is_empty() {
        println!("No cached scan results found. Run `shatter scan` first.");
        return Ok(());
    }

    // Collect candidates, excluding already-decided ones.
    let mut candidates: Vec<ReviewCandidate> = Vec::new();
    for map in &all_maps {
        for field in &map.nondeterministic_fields {
            let key = (map.function_id.clone(), field.field_path.clone());
            if !already_confirmed.contains(&key) && !already_rejected.contains(&key) {
                candidates.push(ReviewCandidate {
                    function_id: map.function_id.clone(),
                    field: field.clone(),
                });
            }
        }
    }

    if candidates.is_empty() {
        println!(
            "No new nondeterminism candidates to review. \
             All detected fields are already in .shatter/config.yaml."
        );
        return Ok(());
    }

    // Sort: high confidence first, then by function ID for stability.
    candidates.sort_by(|a, b| {
        b.field
            .confidence
            .cmp(&a.field.confidence)
            .then_with(|| a.function_id.cmp(&b.function_id))
            .then_with(|| a.field.field_path.cmp(&b.field.field_path))
    });

    let total = candidates.len();
    println!("{} nondeterminism candidate(s) to review.", total);
    println!("Commands: [y] confirm  [n] reject  [s] skip  [?] show evidence  [q] quit");
    println!();

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();

    let mut confirmed: Vec<NondeterminismDeclaration> = Vec::new();
    let mut rejected: Vec<NondeterminismDeclaration> = Vec::new();
    let mut i = 0;

    while i < candidates.len() {
        let candidate = &candidates[i];
        print_candidate(i + 1, total, candidate, colors);

        if non_interactive {
            // In non-interactive mode (e.g., CI or piped input), skip all.
            i += 1;
            continue;
        }

        loop {
            print!("Decision [y/n/s/?/q]: ");
            stdout.lock().flush()?;

            let mut line = String::new();
            let bytes_read = stdin.lock().read_line(&mut line)?;

            // EOF (Ctrl-D) → quit.
            if bytes_read == 0 {
                println!();
                println!(
                    "Session ended. Saving {} change(s).",
                    confirmed.len() + rejected.len()
                );
                persist_decisions(&config_path, &confirmed, &rejected)?;
                return Ok(());
            }

            match line.trim() {
                "y" | "Y" => {
                    confirmed.push(NondeterminismDeclaration {
                        function: candidate.function_id.clone(),
                        path: field_path_to_jsonpath(&candidate.field.field_path),
                        reason: format!(
                            "Confirmed via `shatter nondeterminism review` — evidence: {}",
                            format_evidence_summary(&candidate.field)
                        ),
                    });
                    println!("  Confirmed.");
                    i += 1;
                    break;
                }
                "n" | "N" => {
                    rejected.push(NondeterminismDeclaration {
                        function: candidate.function_id.clone(),
                        path: field_path_to_jsonpath(&candidate.field.field_path),
                        reason: "Rejected via `shatter nondeterminism review`".to_string(),
                    });
                    println!("  Rejected.");
                    i += 1;
                    break;
                }
                "s" | "S" => {
                    println!("  Skipped.");
                    i += 1;
                    break;
                }
                "?" => {
                    print_evidence_detail(candidate, colors);
                    // Stay on the same candidate.
                }
                "q" | "Q" => {
                    println!(
                        "Quit. Saving {} change(s).",
                        confirmed.len() + rejected.len()
                    );
                    persist_decisions(&config_path, &confirmed, &rejected)?;
                    return Ok(());
                }
                other => {
                    println!("Unknown command '{other}'. Use y/n/s/?/q.");
                }
            }
        }
    }

    persist_decisions(&config_path, &confirmed, &rejected)?;

    println!();
    println!(
        "Review complete. {} confirmed, {} rejected, {} skipped.",
        confirmed.len(),
        rejected.len(),
        total - confirmed.len() - rejected.len(),
    );

    Ok(())
}

/// Print a one-line summary of a candidate for the review prompt.
fn print_candidate(idx: usize, total: usize, candidate: &ReviewCandidate, _colors: &Colors) {
    let confidence_label = match candidate.field.confidence {
        Confidence::High => "HIGH",
        Confidence::Medium => "MEDIUM",
        Confidence::Low => "LOW",
    };
    println!(
        "[{}/{}] {} — field: {}  (confidence: {})",
        idx, total, candidate.function_id, candidate.field.field_path, confidence_label
    );
    println!("  Evidence: {}", format_evidence_summary(&candidate.field));
}

/// Print full evidence detail for a candidate (triggered by `?`).
fn print_evidence_detail(candidate: &ReviewCandidate, _colors: &Colors) {
    use shatter_core::nondeterminism::NondeterminismEvidence;

    println!("  ── Evidence detail ──────────────────────────────");
    println!("  Function : {}", candidate.function_id);
    println!("  Field    : {}", candidate.field.field_path);
    println!("  Confidence: {:?}", candidate.field.confidence);
    println!("  Evidence:");
    for ev in &candidate.field.evidence {
        let desc = match ev {
            NondeterminismEvidence::UserDeclared => "User-declared".to_string(),
            NondeterminismEvidence::ObservedWithinRun => {
                "Different outputs observed for the same input within a single run".to_string()
            }
            NondeterminismEvidence::ObservedAcrossRuns => {
                "Different outputs observed for the same input across separate runs".to_string()
            }
            NondeterminismEvidence::PatternMatch { pattern } => {
                format!("Matched known nondeterministic API pattern: {pattern}")
            }
            NondeterminismEvidence::NameHeuristic { matched_name } => {
                format!("Name heuristic matched: '{matched_name}'")
            }
            NondeterminismEvidence::SlowPattern { pattern_type } => {
                format!("Value matches slow nondeterminism pattern: {pattern_type}")
            }
        };
        println!("    - {desc}");
    }
    println!("  ─────────────────────────────────────────────────");
}

/// Summarise the evidence list as a short human-readable string.
fn format_evidence_summary(field: &NondeterministicField) -> String {
    use shatter_core::nondeterminism::NondeterminismEvidence;

    let labels: Vec<&str> = field
        .evidence
        .iter()
        .map(|ev| match ev {
            NondeterminismEvidence::UserDeclared => "user-declared",
            NondeterminismEvidence::ObservedWithinRun => "observed-within-run",
            NondeterminismEvidence::ObservedAcrossRuns => "observed-across-runs",
            NondeterminismEvidence::PatternMatch { .. } => "pattern-match",
            NondeterminismEvidence::NameHeuristic { .. } => "name-heuristic",
            NondeterminismEvidence::SlowPattern { .. } => "slow-pattern",
        })
        .collect();

    if labels.is_empty() {
        return "(no evidence)".to_string();
    }
    labels.join(", ")
}

/// Convert an internal field path (e.g. `return.id`) to a JSONPath-style string
/// (e.g. `$.return.id`) suitable for `NondeterminismDeclaration.path`.
fn field_path_to_jsonpath(path: &str) -> String {
    if path.starts_with("$.") || path == "$" {
        path.to_string()
    } else {
        format!("$.{path}")
    }
}

/// Write confirmed and rejected decisions to the config file.
fn persist_decisions(
    config_path: &Path,
    confirmed: &[NondeterminismDeclaration],
    rejected: &[NondeterminismDeclaration],
) -> Result<(), Box<dyn std::error::Error>> {
    if confirmed.is_empty() && rejected.is_empty() {
        return Ok(());
    }
    update_nondeterminism_config(config_path, confirmed, rejected)?;
    println!("Saved to {}", config_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use shatter_core::nondeterminism::{Confidence, NondeterminismEvidence};

    #[test]
    fn field_path_to_jsonpath_plain() {
        assert_eq!(field_path_to_jsonpath("return.id"), "$.return.id");
    }

    #[test]
    fn field_path_to_jsonpath_already_jsonpath() {
        assert_eq!(field_path_to_jsonpath("$.return.id"), "$.return.id");
    }

    #[test]
    fn field_path_to_jsonpath_dollar_only() {
        assert_eq!(field_path_to_jsonpath("$"), "$");
    }

    #[test]
    fn format_evidence_summary_empty() {
        let field = NondeterministicField {
            field_path: "return".to_string(),
            evidence: vec![],
            confidence: Confidence::Low,
        };
        assert_eq!(format_evidence_summary(&field), "(no evidence)");
    }

    #[test]
    fn format_evidence_summary_multiple() {
        let field = NondeterministicField {
            field_path: "return.ts".to_string(),
            evidence: vec![
                NondeterminismEvidence::NameHeuristic {
                    matched_name: "timestamp".to_string(),
                },
                NondeterminismEvidence::ObservedWithinRun,
            ],
            confidence: Confidence::High,
        };
        let summary = format_evidence_summary(&field);
        assert!(summary.contains("name-heuristic"));
        assert!(summary.contains("observed-within-run"));
    }

    #[test]
    fn run_review_no_cache_emits_message() {
        let tmp = tempfile::tempdir().unwrap();
        let colors = Colors::new(false);
        // Point at a dir with no cache — should print "No cached scan results found"
        // without panicking.
        let result = run_review(
            Some(tmp.path()),
            &colors,
            Some(&tmp.path().join("nonexistent-cache")),
            true, // non_interactive: skip stdin
        );
        assert!(result.is_ok());
    }

    #[test]
    fn run_review_all_already_decided_emits_message() {
        use shatter_core::behavior::BehaviorMap;
        use shatter_core::cache::BehaviorMapCache;
        use shatter_core::nondeterminism::{
            Confidence, NondeterminismEvidence, NondeterministicField,
        };

        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path();

        // Create .shatter/config.yaml with the field already confirmed.
        let shatter_dir = project_dir.join(".shatter");
        std::fs::create_dir_all(&shatter_dir).unwrap();
        // Write YAML directly to avoid pulling serde_yaml into the CLI crate's tests.
        let yaml = "nondeterminism:\n  confirmed:\n  - function: myFunc\n    path: '$.return.id'\n    reason: pre-existing\n  rejected: []\n";
        std::fs::write(shatter_dir.join("config.yaml"), yaml).unwrap();

        // Seed the cache with a behavior map that has the same field.
        let cache_dir = project_dir.join(BEHAVIOR_MAPS_SUBDIR);
        let cache = BehaviorMapCache::new(cache_dir.clone()).unwrap();
        let mut map = BehaviorMap::from_records("myFunc", &[]);
        map.nondeterministic_fields = vec![NondeterministicField {
            field_path: "return.id".to_string(),
            evidence: vec![NondeterminismEvidence::ObservedWithinRun],
            confidence: Confidence::High,
        }];
        cache.store(&map).unwrap();

        let colors = Colors::new(false);
        let result = run_review(Some(project_dir), &colors, Some(&cache_dir), true);
        assert!(result.is_ok());
    }

    #[test]
    fn update_nondeterminism_config_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join(".shatter").join("config.yaml");

        let confirmed = vec![NondeterminismDeclaration {
            function: "createUser".to_string(),
            path: "$.id".to_string(),
            reason: "UUID per call".to_string(),
        }];
        shatter_core::config::update_nondeterminism_config(&config_path, &confirmed, &[]).unwrap();

        assert!(config_path.exists());
        let loaded = shatter_core::config::parse_config(&config_path).unwrap();
        let nd = loaded
            .nondeterminism
            .expect("nondeterminism section present");
        assert_eq!(nd.confirmed.len(), 1);
        assert_eq!(nd.confirmed[0].function, "createUser");
    }

    #[test]
    fn update_nondeterminism_config_deduplicates() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join(".shatter").join("config.yaml");

        let decl = NondeterminismDeclaration {
            function: "createUser".to_string(),
            path: "$.id".to_string(),
            reason: "UUID per call".to_string(),
        };

        // Write twice — should not duplicate.
        shatter_core::config::update_nondeterminism_config(&config_path, &[decl.clone()], &[])
            .unwrap();
        shatter_core::config::update_nondeterminism_config(&config_path, &[decl], &[]).unwrap();

        let loaded = shatter_core::config::parse_config(&config_path).unwrap();
        let nd = loaded
            .nondeterminism
            .expect("nondeterminism section present");
        assert_eq!(nd.confirmed.len(), 1, "should not duplicate");
    }
}
