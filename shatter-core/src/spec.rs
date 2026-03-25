//! Behavior specification output for explored functions.
//!
//! A [`FunctionSpec`] captures the complete behavioral specification of a function:
//! equivalence classes grouped by branch path, preconditions, postconditions,
//! concrete examples, and provenance (proven vs observed). The spec can be
//! rendered as human-readable markdown, machine-readable JSON, or YAML with
//! property descriptions.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::coverage_metrics::DiscoveryMethod;
use crate::equivalence::{BranchPath, EquivalenceClass, Precondition};
use crate::execution_record::{ErrorInfo, SideEffect};
use crate::explorer::ObservationOutput;
use crate::fingerprint::compute_deep_fingerprints;
use crate::invariants::{ClassifiedInvariant, InvariantKind};
use crate::protocol::FunctionAnalysis;

/// Error type for spec bundle I/O operations.
#[derive(Debug)]
pub enum SpecIoError {
    /// Filesystem I/O error.
    Io(std::io::Error),
    /// JSON serialization or deserialization error.
    Json(serde_json::Error),
}

impl fmt::Display for SpecIoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "spec I/O error: {e}"),
            Self::Json(e) => write!(f, "spec JSON error: {e}"),
        }
    }
}

impl std::error::Error for SpecIoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for SpecIoError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for SpecIoError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// Whether a property was proven by constraint solving or merely observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// Confirmed by Z3 constraint solving — holds for all inputs on this path.
    Proven,
    /// Observed across all sampled inputs but not formally proven.
    Observed,
}

// ---------------------------------------------------------------------------
// YAML property descriptions — human-friendly invariant representation
// ---------------------------------------------------------------------------

/// High-level category for an invariant property description.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvariantCategory {
    NumericBound,
    Nullability,
    StringConstraint,
    ReturnTypeInvariant,
    ErrorInvariant,
    InputOutputRelation,
    BooleanConstant,
    ConstantValue,
}

/// Categorical confidence derived from the numeric confidence score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceLevel {
    High,
    Medium,
    Low,
}

const CONFIDENCE_HIGH_THRESHOLD: f64 = 0.95;
const CONFIDENCE_MEDIUM_THRESHOLD: f64 = 0.75;

impl ConfidenceLevel {
    fn from_score(score: f64) -> Self {
        if score >= CONFIDENCE_HIGH_THRESHOLD {
            Self::High
        } else if score >= CONFIDENCE_MEDIUM_THRESHOLD {
            Self::Medium
        } else {
            Self::Low
        }
    }
}

/// A human-friendly invariant for YAML spec output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpecInvariant {
    /// Human-readable property description.
    pub property: String,
    /// High-level invariant category.
    pub kind: InvariantCategory,
    /// Categorical confidence level.
    pub confidence: ConfidenceLevel,
}

impl From<&ClassifiedInvariant> for SpecInvariant {
    fn from(ci: &ClassifiedInvariant) -> Self {
        Self {
            property: ci.label.clone(),
            kind: categorize_invariant_kind(&ci.invariant.kind),
            confidence: ConfidenceLevel::from_score(ci.confidence),
        }
    }
}

fn categorize_invariant_kind(kind: &InvariantKind) -> InvariantCategory {
    match kind {
        InvariantKind::NumericComparison { .. } => InvariantCategory::NumericBound,
        InvariantKind::NumericConstant { .. } => InvariantCategory::ConstantValue,
        InvariantKind::NotNull { .. } | InvariantKind::IsNull { .. } => {
            InvariantCategory::Nullability
        }
        InvariantKind::StringNonEmpty { .. } | InvariantKind::StringLength { .. } => {
            InvariantCategory::StringConstraint
        }
        InvariantKind::OutputEqualsInput { .. } => InvariantCategory::InputOutputRelation,
        InvariantKind::AlwaysTrue { .. } | InvariantKind::AlwaysFalse { .. } => {
            InvariantCategory::BooleanConstant
        }
    }
}

/// A postcondition describing what a function does on a given path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Postcondition {
    /// The function returns a value matching this pattern.
    Returns {
        /// A representative return value (from the canonical example).
        value: serde_json::Value,
    },
    /// The function throws an error.
    Throws {
        /// Error type and message.
        error: ErrorInfo,
    },
    /// The function returns void / undefined / null.
    ReturnsVoid,
}

/// A concrete input/output example for an equivalence class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConcreteExample {
    /// Input arguments.
    pub inputs: Vec<serde_json::Value>,
    /// Return value, if the function returned normally.
    pub return_value: Option<serde_json::Value>,
    /// Error info, if the function threw.
    pub thrown_error: Option<ErrorInfo>,
}

/// One behavioral equivalence class within a function specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpecClass {
    /// Human-readable label for this class (e.g., "returns positive result").
    pub label: String,
    /// The branch path identifying this class.
    pub branch_path: BranchPath,
    /// Preconditions: constraints on inputs that lead to this path.
    pub preconditions: Vec<Precondition>,
    /// Postcondition: what the function does on this path.
    pub postcondition: Postcondition,
    /// Side effects observed on this path.
    pub side_effects: Vec<SideEffect>,
    /// At least one concrete example.
    pub examples: Vec<ConcreteExample>,
    /// How many executions fell into this class.
    pub sample_count: usize,
    /// Whether preconditions are proven or observed.
    pub precondition_provenance: Provenance,
    /// Whether postconditions are proven or observed.
    pub postcondition_provenance: Provenance,
    /// Detected invariants for this equivalence class (empty if invariant detection is disabled).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariants: Vec<ClassifiedInvariant>,
}

/// Per-file spec bundle: all function specs from a single source file.
///
/// Used by `--output` to write a single JSON file containing specs for every
/// explored function in a source file, keyed by file path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileSpecBundle {
    /// Source file path (e.g., "src/math.ts").
    pub file: String,
    /// Specs for each explored function in this file.
    pub functions: Vec<FunctionSpec>,
}

/// Complete behavioral specification of a function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionSpec {
    /// Fully qualified function name.
    pub function_name: String,
    /// Source file and location (e.g., "src/math.ts:10").
    pub location: Option<String>,
    /// Equivalence classes describing distinct behaviors.
    pub classes: Vec<SpecClass>,
    /// Total iterations used during exploration.
    pub iterations: u32,
    /// Line coverage achieved.
    pub lines_covered: usize,
    /// Total lines in the function.
    pub total_lines: u32,
    /// Function-wide invariants (across all classes). Empty if invariant detection is disabled.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariants: Vec<ClassifiedInvariant>,
    /// SHA-256 fingerprint of the function's source, params, and branches.
    /// Used for staleness detection across runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// Fields identified as nondeterministic during exploration.
    /// Used by spec diff to exclude nondeterministic fields from postcondition comparison.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nondeterministic_fields: Vec<crate::nondeterminism::NondeterministicField>,
}

/// Build a [`FunctionSpec`] from an observation output and its equivalence classes.
///
/// Equivalence classes are produced by [`crate::equivalence::group_into_classes`].
/// Branches discovered via Z3 get `Provenance::Proven`; all others get `Observed`.
pub fn build_spec(
    result: &ObservationOutput,
    eq_classes: &[EquivalenceClass],
    location: Option<String>,
    fingerprint: Option<String>,
) -> FunctionSpec {
    let z3_branches = z3_branch_set(&result.discoveries);
    let classes = eq_classes
        .iter()
        .enumerate()
        .map(|(i, ec)| build_spec_class(i, ec, &z3_branches))
        .collect();

    FunctionSpec {
        function_name: result.function_name.clone(),
        location,
        classes,
        iterations: result.iterations,
        lines_covered: result.lines_covered,
        total_lines: result.total_lines,
        invariants: vec![],
        fingerprint,
        nondeterministic_fields: result.nondeterministic_fields.clone(),
    }
}

/// Build a [`FunctionSpec`] with invariant detection enabled.
///
/// Convenience wrapper: builds the spec via [`build_spec`], then enriches it
/// with invariants via [`detect_spec_invariants`].
pub fn build_spec_with_invariants(
    result: &ObservationOutput,
    eq_classes: &[EquivalenceClass],
    location: Option<String>,
    fingerprint: Option<String>,
) -> FunctionSpec {
    let mut spec = build_spec(result, eq_classes, location, fingerprint);
    detect_spec_invariants(&mut spec, result, eq_classes);
    spec
}

/// Enrich a [`FunctionSpec`] with Daikon-style invariant detection.
///
/// Detects invariants at both the function-wide level (across all executions)
/// and the per-class level (for classes with at least 2 samples). Mutates
/// `spec` in place so callers can compose observation, spec construction,
/// and invariant detection as separate pipeline stages.
pub fn detect_spec_invariants(
    spec: &mut FunctionSpec,
    result: &ObservationOutput,
    eq_classes: &[EquivalenceClass],
) {
    use crate::invariants::{
        detect_classified_invariants, records_from_raw_results, InvariantTarget,
    };

    let all_records = records_from_raw_results(&result.function_name, &result.raw_results);

    // Function-wide invariants (across all executions)
    let mut function_invariants =
        detect_classified_invariants(&all_records, InvariantTarget::Input);
    function_invariants.extend(detect_classified_invariants(
        &all_records,
        InvariantTarget::Output,
    ));
    spec.invariants = function_invariants;

    // Per-class invariants: match spec classes to equivalence classes by branch path
    for (spec_class, ec) in spec.classes.iter_mut().zip(eq_classes.iter()) {
        let class_records: Vec<_> = all_records
            .iter()
            .filter(|r| ec.all_inputs.contains(&r.parameters))
            .cloned()
            .collect();

        if class_records.len() >= 2 {
            let mut class_invs =
                detect_classified_invariants(&class_records, InvariantTarget::Input);
            class_invs.extend(detect_classified_invariants(
                &class_records,
                InvariantTarget::Output,
            ));
            spec_class.invariants = class_invs;
        }
    }
}

/// Collect branch IDs discovered via Z3 into a lookup set.
fn z3_branch_set(discoveries: &[(u32, DiscoveryMethod)]) -> HashSet<u32> {
    discoveries
        .iter()
        .filter(|(_, method)| *method == DiscoveryMethod::Z3)
        .map(|(id, _)| *id)
        .collect()
}

/// Build a single [`SpecClass`] from an equivalence class.
///
/// If every branch in the class's path was discovered by Z3, both precondition
/// and postcondition provenance are `Proven`; otherwise `Observed`.
fn build_spec_class(index: usize, ec: &EquivalenceClass, z3_branches: &HashSet<u32>) -> SpecClass {
    let postcondition = if let Some(ref err_msg) = ec.canonical_thrown_error {
        let (error_type, message) = match err_msg.split_once(": ") {
            Some((t, m)) => (t.to_string(), m.to_string()),
            None => ("Error".to_string(), err_msg.clone()),
        };
        Postcondition::Throws {
            error: ErrorInfo {
                error_type,
                message,
                stack: None, error_category: None },
        }
    } else {
        match &ec.canonical_return_value {
            Some(v) if !v.is_null() => Postcondition::Returns { value: v.clone() },
            _ => Postcondition::ReturnsVoid,
        }
    };

    let label = format_class_label(index, &postcondition);

    let canonical_example = ConcreteExample {
        inputs: ec.canonical_example.clone(),
        return_value: ec.canonical_return_value.clone(),
        thrown_error: ec.canonical_thrown_error.as_ref().map(|msg| {
            let (error_type, message) = match msg.split_once(": ") {
                Some((t, m)) => (t.to_string(), m.to_string()),
                None => ("Error".to_string(), msg.clone()),
            };
            ErrorInfo {
                error_type,
                message,
                stack: None, error_category: None }
        }),
    };

    // A class is proven if every branch step in its path was solved by Z3.
    let all_z3 = !ec.branch_path.0.is_empty()
        && ec
            .branch_path
            .0
            .iter()
            .all(|step| z3_branches.contains(&step.branch_id));
    let provenance = if all_z3 {
        Provenance::Proven
    } else {
        Provenance::Observed
    };

    SpecClass {
        label,
        branch_path: ec.branch_path.clone(),
        preconditions: ec.common_preconditions.clone(),
        postcondition,
        side_effects: vec![],
        examples: vec![canonical_example],
        sample_count: ec.all_inputs.len(),
        precondition_provenance: provenance,
        postcondition_provenance: provenance,
        invariants: vec![],
    }
}

/// Generate a human-readable label for a spec class.
fn format_class_label(index: usize, postcondition: &Postcondition) -> String {
    let outcome = match postcondition {
        Postcondition::Returns { value } => format!("returns {}", format_value_short(value)),
        Postcondition::Throws { error } => {
            format!("throws {}: {}", error.error_type, error.message)
        }
        Postcondition::ReturnsVoid => "returns void".to_string(),
    };
    format!("Class {} — {}", index + 1, outcome)
}

/// Format the spec as human-readable markdown.
pub fn format_spec_markdown(spec: &FunctionSpec) -> String {
    let mut out = String::new();

    // Title
    out.push_str(&format!("# Specification: `{}`\n\n", spec.function_name));

    if let Some(ref loc) = spec.location {
        out.push_str(&format!("**Location:** `{loc}`\n\n"));
    }

    // Summary
    out.push_str(&format!(
        "**Behavioral classes:** {}  \n",
        spec.classes.len()
    ));
    out.push_str(&format!(
        "**Exploration:** {} iterations, {}/{} lines covered",
        spec.iterations, spec.lines_covered, spec.total_lines,
    ));
    if spec.total_lines > 0 {
        let pct = (spec.lines_covered as f64 / spec.total_lines as f64 * 100.0).min(100.0);
        out.push_str(&format!(" ({pct:.0}%)"));
    }
    out.push_str("\n\n");

    // Function-wide invariants
    if !spec.invariants.is_empty() {
        out.push_str("**Function invariants:**\n");
        for ci in &spec.invariants {
            out.push_str(&format!(
                "- {} [{}] ({}/{})\n",
                ci.invariant.description,
                ci.confidence,
                ci.satisfied_count,
                ci.total_count,
            ));
        }
        out.push('\n');
    }

    out.push_str("---\n\n");

    // Classes
    for (i, class) in spec.classes.iter().enumerate() {
        out.push_str(&format!("## {}\n\n", class.label));

        let pre_badge = provenance_badge(class.precondition_provenance);
        let post_badge = provenance_badge(class.postcondition_provenance);

        // Preconditions
        if class.preconditions.is_empty() {
            out.push_str(&format!("**Preconditions** {pre_badge}: _(none derived)_\n\n"));
        } else {
            out.push_str(&format!("**Preconditions** {pre_badge}:\n"));
            for pre in &class.preconditions {
                out.push_str(&format!("- {}\n", format_precondition(pre)));
            }
            out.push('\n');
        }

        // Postcondition
        out.push_str(&format!(
            "**Postcondition** {post_badge}: {}\n\n",
            format_postcondition(&class.postcondition)
        ));

        // Side effects
        if !class.side_effects.is_empty() {
            out.push_str("**Side effects:**\n");
            for effect in &class.side_effects {
                out.push_str(&format!("- {}\n", format_side_effect(effect)));
            }
            out.push('\n');
        }

        // Invariants (per-class)
        if !class.invariants.is_empty() {
            out.push_str("**Invariants:**\n");
            for ci in &class.invariants {
                out.push_str(&format!(
                    "- {} [{}] ({}/{})\n",
                    ci.invariant.description,
                    ci.confidence,
                    ci.satisfied_count,
                    ci.total_count,
                ));
            }
            out.push('\n');
        }

        // Examples
        out.push_str(&format!(
            "**Example** ({} execution(s) observed):\n",
            class.sample_count
        ));
        for example in &class.examples {
            let inputs_str = example
                .inputs
                .iter()
                .map(format_value_short)
                .collect::<Vec<_>>()
                .join(", ");

            let outcome = if let Some(ref err) = example.thrown_error {
                format!("throws {}: {}", err.error_type, err.message)
            } else {
                match &example.return_value {
                    Some(v) if !v.is_null() => format!("-> {}", format_value_short(v)),
                    _ => "-> void".to_string(),
                }
            };

            out.push_str(&format!(
                "```\n{}({inputs_str}) {outcome}\n```\n",
                spec.function_name
            ));
        }

        if i + 1 < spec.classes.len() {
            out.push_str("\n---\n\n");
        }
    }

    out
}

/// Format the spec as machine-readable JSON.
///
/// Returns a `Result` because serialization can theoretically fail.
pub fn format_spec_json(spec: &FunctionSpec) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(spec)
}

/// Format a collection of per-file spec bundles as machine-readable JSON.
pub fn format_file_spec_json(bundles: &[FileSpecBundle]) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(bundles)
}

// ---------------------------------------------------------------------------
// YAML output views — convert ClassifiedInvariant → SpecInvariant
//
// These private structs mirror FunctionSpec / SpecClass / FileSpecBundle but
// use Vec<SpecInvariant> for the invariants fields so that YAML output shows
// human-friendly property descriptions instead of raw ClassifiedInvariant data.
// They are Serialize-only (no Deserialize) — the JSON / binary representations
// of FunctionSpec are unchanged.
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct SpecClassYaml<'a> {
    label: &'a str,
    branch_path: &'a BranchPath,
    preconditions: &'a Vec<Precondition>,
    postcondition: &'a Postcondition,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    side_effects: &'a Vec<SideEffect>,
    examples: &'a Vec<ConcreteExample>,
    sample_count: usize,
    precondition_provenance: Provenance,
    postcondition_provenance: Provenance,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    invariants: Vec<SpecInvariant>,
}

#[derive(Serialize)]
struct FunctionSpecYaml<'a> {
    function_name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: &'a Option<String>,
    classes: Vec<SpecClassYaml<'a>>,
    iterations: u32,
    lines_covered: usize,
    total_lines: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    invariants: Vec<SpecInvariant>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fingerprint: &'a Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    nondeterministic_fields: &'a Vec<crate::nondeterminism::NondeterministicField>,
}

#[derive(Serialize)]
struct FileSpecBundleYaml<'a> {
    file: &'a str,
    functions: Vec<FunctionSpecYaml<'a>>,
}

fn to_spec_class_yaml(c: &SpecClass) -> SpecClassYaml<'_> {
    SpecClassYaml {
        label: &c.label,
        branch_path: &c.branch_path,
        preconditions: &c.preconditions,
        postcondition: &c.postcondition,
        side_effects: &c.side_effects,
        examples: &c.examples,
        sample_count: c.sample_count,
        precondition_provenance: c.precondition_provenance,
        postcondition_provenance: c.postcondition_provenance,
        invariants: c.invariants.iter().map(SpecInvariant::from).collect(),
    }
}

fn to_function_spec_yaml(s: &FunctionSpec) -> FunctionSpecYaml<'_> {
    FunctionSpecYaml {
        function_name: &s.function_name,
        location: &s.location,
        classes: s.classes.iter().map(to_spec_class_yaml).collect(),
        iterations: s.iterations,
        lines_covered: s.lines_covered,
        total_lines: s.total_lines,
        invariants: s.invariants.iter().map(SpecInvariant::from).collect(),
        fingerprint: &s.fingerprint,
        nondeterministic_fields: &s.nondeterministic_fields,
    }
}

/// Format the spec as YAML with human-friendly property descriptions.
///
/// Invariants are rendered as [`SpecInvariant`] (with `property:`, `kind:`,
/// and `confidence:` fields) rather than raw [`ClassifiedInvariant`] data.
/// The invariants section is omitted when no invariants are present.
pub fn format_spec_yaml(spec: &FunctionSpec) -> Result<String, serde_yaml::Error> {
    serde_yaml::to_string(&to_function_spec_yaml(spec))
}

/// Format a collection of per-file spec bundles as YAML with human-friendly property descriptions.
///
/// Same invariant conversion as [`format_spec_yaml`] — each function's invariants
/// appear as `SpecInvariant` property descriptions.
pub fn format_file_spec_yaml(bundles: &[FileSpecBundle]) -> Result<String, serde_yaml::Error> {
    let views: Vec<FileSpecBundleYaml<'_>> = bundles
        .iter()
        .map(|b| FileSpecBundleYaml {
            file: &b.file,
            functions: b.functions.iter().map(to_function_spec_yaml).collect(),
        })
        .collect();
    serde_yaml::to_string(&views)
}

/// Write a [`FileSpecBundle`] to disk using atomic write (temp file + rename).
///
/// Creates parent directories if they don't exist. Writes to a `.json.tmp`
/// sibling first, then renames to the final path to avoid partial reads.
pub fn write_file_spec_bundle(bundle: &FileSpecBundle, path: &Path) -> Result<(), SpecIoError> {
    let json = serde_json::to_string_pretty(bundle)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &json)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Read a [`FileSpecBundle`] from disk.
pub fn read_file_spec_bundle(path: &Path) -> Result<FileSpecBundle, SpecIoError> {
    let contents = fs::read_to_string(path)?;
    let bundle = serde_json::from_str(&contents)?;
    Ok(bundle)
}

/// Result of comparing current function analyses against an existing spec bundle.
///
/// Used by `--output` incremental mode to decide which functions need re-exploration
/// vs which can be carried over from the previous spec.
#[derive(Debug, Clone, PartialEq)]
pub struct IncrementalPlan {
    /// Functions whose fingerprints changed or are new (need re-exploration).
    pub stale: Vec<String>,
    /// Functions whose fingerprints match the existing spec (reuse old spec).
    pub fresh: Vec<String>,
    /// Functions present in old spec but absent from current analysis (deleted).
    pub removed: Vec<String>,
}

/// Compare current function analyses against an existing spec to determine staleness.
///
/// Computes deep fingerprints (incorporating callee dependencies) for each function
/// in `current_analyses`, then compares against `existing.functions[name].fingerprint`.
/// A function is fresh only if its deep fingerprint matches. This means a caller is
/// marked stale when any of its in-scope callees change, even if the caller's own
/// source is unchanged.
///
/// Functions in the existing bundle not present in current analysis are removed.
///
/// Returns an error if source extraction fails (e.g., file unreadable).
pub fn compute_incremental_plan(
    file_path: &Path,
    current_analyses: &[FunctionAnalysis],
    existing: &FileSpecBundle,
    external_fingerprints: &std::collections::HashMap<String, String>,
) -> Result<IncrementalPlan, std::io::Error> {
    let existing_by_name: std::collections::HashMap<&str, &FunctionSpec> = existing
        .functions
        .iter()
        .map(|f| (f.function_name.as_str(), f))
        .collect();

    let deep_fps = compute_deep_fingerprints(file_path, current_analyses, external_fingerprints)?;

    let mut stale = Vec::new();
    let mut fresh = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();

    for func in current_analyses {
        seen_names.insert(func.name.clone());

        let current_fp = deep_fps.get(&func.name);

        match (existing_by_name.get(func.name.as_str()), current_fp) {
            (Some(spec), Some(fp)) if spec.fingerprint.as_deref() == Some(fp.as_str()) => {
                fresh.push(func.name.clone());
            }
            _ => {
                stale.push(func.name.clone());
            }
        }
    }

    let removed: Vec<String> = existing
        .functions
        .iter()
        .filter(|f| !seen_names.contains(&f.function_name))
        .map(|f| f.function_name.clone())
        .collect();

    Ok(IncrementalPlan {
        stale,
        fresh,
        removed,
    })
}

/// Merge newly explored specs with fresh specs carried over from an existing bundle.
///
/// `new_specs` contains specs for functions that were re-explored (stale).
/// Functions in `current_function_names` that are NOT in `new_specs` are carried
/// over from `existing` (they were fresh and skipped). Functions not in
/// `current_function_names` are dropped (they were removed from the source).
pub fn merge_file_spec_bundles(
    existing: &FileSpecBundle,
    new_specs: &[FunctionSpec],
    current_function_names: &HashSet<String>,
) -> FileSpecBundle {
    let new_names: HashSet<&str> = new_specs.iter().map(|s| s.function_name.as_str()).collect();

    let mut merged: Vec<FunctionSpec> = new_specs.to_vec();

    // Carry over fresh specs from existing bundle (not re-explored, still in source)
    for old_spec in &existing.functions {
        if current_function_names.contains(&old_spec.function_name)
            && !new_names.contains(old_spec.function_name.as_str())
        {
            merged.push(old_spec.clone());
        }
    }

    FileSpecBundle {
        file: existing.file.clone(),
        functions: merged,
    }
}

/// Badge text for provenance level.
fn provenance_badge(p: Provenance) -> &'static str {
    match p {
        Provenance::Proven => "[proven]",
        Provenance::Observed => "[observed]",
    }
}

/// Format a precondition as a human-readable string.
fn format_precondition(pre: &Precondition) -> String {
    match pre {
        Precondition::AllPositive { param_index } => {
            format!("param[{param_index}] > 0")
        }
        Precondition::AllNegative { param_index } => {
            format!("param[{param_index}] < 0")
        }
        Precondition::AllZero { param_index } => {
            format!("param[{param_index}] == 0")
        }
        Precondition::AllEqual { param_index, value } => {
            format!("param[{param_index}] == {}", format_value_short(value))
        }
        Precondition::SameType {
            param_index,
            type_name,
        } => {
            format!("typeof param[{param_index}] == \"{type_name}\"")
        }
    }
}

/// Format a postcondition as a human-readable string.
fn format_postcondition(post: &Postcondition) -> String {
    match post {
        Postcondition::Returns { value } => {
            format!("returns {}", format_value_short(value))
        }
        Postcondition::Throws { error } => {
            format!("throws {}: {}", error.error_type, error.message)
        }
        Postcondition::ReturnsVoid => "returns void".to_string(),
    }
}

/// Format a side effect as a human-readable string.
fn format_side_effect(effect: &SideEffect) -> String {
    match effect {
        SideEffect::ConsoleOutput { level, message } => {
            format!("console.{level}(\"{message}\")")
        }
        SideEffect::FileWrite { path, .. } => format!("writes to file: {path}"),
        SideEffect::NetworkRequest { method, url, .. } => format!("{method} {url}"),
        SideEffect::EnvironmentRead { variable, value } => {
            let val = value.as_deref().unwrap_or("null");
            format!("reads env: {variable}={val}")
        }
        SideEffect::GlobalMutation { name } => format!("mutates global: {name}"),
        SideEffect::ThrownError {
            error_type,
            message,
            ..
        } => {
            format!("throws {error_type}: {message}")
        }
        SideEffect::GlobalStateChange {
            variable,
            before,
            after,
        } => {
            format!(
                "{variable}: {} -> {}",
                format_value_short(before),
                format_value_short(after)
            )
        }
    }
}

/// Format a JSON value for display, truncating long values.
fn format_value_short(v: &serde_json::Value) -> String {
    let s = v.to_string();
    if s.len() > 40 {
        format!("{}...", &s[..37])
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equivalence::{BranchPath, BranchStep, EquivalenceClass, Precondition};
    use crate::execution_record::ErrorInfo;
    use crate::explorer::ObservationOutput;
    use serde_json::json;

    fn make_exploration_result(
        name: &str,
        iterations: u32,
        unique_paths: usize,
    ) -> ObservationOutput {
        ObservationOutput {
            function_name: name.to_string(),
            iterations,
            unique_paths,
            lines_covered: 8,
            total_lines: 10,
            new_path_executions: vec![],
            raw_results: vec![],
            discoveries: vec![],
            nondeterministic_fields: vec![], float_probe_results: vec![], boundary_results: vec![], shrunk_witnesses: std::collections::HashMap::new(), mcdc_summary: None, shrink_stats: crate::shrink::ShrinkStats::default(), abandoned_frontiers: vec![], opaque_suggestions: vec![],
        }
    }

    fn make_eq_class(
        branch_steps: Vec<(u32, bool)>,
        canonical_inputs: Vec<serde_json::Value>,
        canonical_return: Option<serde_json::Value>,
        canonical_error: Option<String>,
        preconditions: Vec<Precondition>,
        sample_count: usize,
    ) -> EquivalenceClass {
        let branch_path = BranchPath(
            branch_steps
                .into_iter()
                .map(|(id, taken)| BranchStep {
                    branch_id: id,
                    taken,
                })
                .collect(),
        );

        let all_inputs = (0..sample_count)
            .map(|_| canonical_inputs.clone())
            .collect();

        EquivalenceClass {
            branch_path,
            canonical_example: canonical_inputs,
            all_inputs,
            common_preconditions: preconditions,
            canonical_return_value: canonical_return,
            canonical_thrown_error: canonical_error,
        }
    }

    #[test]
    fn build_spec_from_exploration_and_classes() {
        let result = make_exploration_result("classifyNumber", 50, 3);
        let classes = vec![
            make_eq_class(
                vec![(0, true)],
                vec![json!(5)],
                Some(json!("positive")),
                None,
                vec![Precondition::AllPositive { param_index: 0 }],
                10,
            ),
            make_eq_class(
                vec![(0, false), (1, true)],
                vec![json!(-3)],
                Some(json!("negative")),
                None,
                vec![Precondition::AllNegative { param_index: 0 }],
                8,
            ),
            make_eq_class(
                vec![(0, false), (1, false)],
                vec![json!(0)],
                Some(json!("zero")),
                None,
                vec![Precondition::AllZero { param_index: 0 }],
                2,
            ),
        ];

        let spec = build_spec(&result, &classes, Some("math.ts:10".to_string()), None);

        assert_eq!(spec.function_name, "classifyNumber");
        assert_eq!(spec.location.as_deref(), Some("math.ts:10"));
        assert_eq!(spec.classes.len(), 3);
        assert_eq!(spec.iterations, 50);
        assert_eq!(spec.lines_covered, 8);
        assert_eq!(spec.total_lines, 10);

        // Check first class
        assert!(spec.classes[0].label.contains("positive"));
        assert_eq!(spec.classes[0].preconditions.len(), 1);
        assert_eq!(
            spec.classes[0].postcondition,
            Postcondition::Returns {
                value: json!("positive")
            }
        );
        assert_eq!(spec.classes[0].examples.len(), 1);
        assert_eq!(spec.classes[0].examples[0].inputs, vec![json!(5)]);
        assert_eq!(spec.classes[0].sample_count, 10);
    }

    #[test]
    fn build_spec_with_error_class() {
        let result = make_exploration_result("safeDivide", 20, 2);
        let classes = vec![
            make_eq_class(
                vec![(0, false)],
                vec![json!(10), json!(2)],
                Some(json!(5)),
                None,
                vec![],
                15,
            ),
            make_eq_class(
                vec![(0, true)],
                vec![json!(1), json!(0)],
                None,
                Some("Error: division by zero".to_string()),
                vec![Precondition::AllZero { param_index: 1 }],
                5,
            ),
        ];

        let spec = build_spec(&result, &classes, None, None);

        assert_eq!(spec.classes.len(), 2);

        let error_class = &spec.classes[1];
        assert!(error_class.label.contains("throws"));
        assert!(matches!(
            &error_class.postcondition,
            Postcondition::Throws { error } if error.message == "division by zero"
        ));
        assert!(error_class.examples[0].thrown_error.is_some());
    }

    #[test]
    fn build_spec_with_void_return() {
        let result = make_exploration_result("logMessage", 10, 1);
        let classes = vec![make_eq_class(
            vec![],
            vec![json!("hello")],
            None,
            None,
            vec![],
            10,
        )];

        let spec = build_spec(&result, &classes, None, None);
        assert_eq!(spec.classes[0].postcondition, Postcondition::ReturnsVoid);
        assert!(spec.classes[0].label.contains("void"));
    }

    #[test]
    fn build_spec_empty_classes() {
        let result = make_exploration_result("unused", 0, 0);
        let spec = build_spec(&result, &[], None, None);

        assert!(spec.classes.is_empty());
        assert_eq!(spec.function_name, "unused");
    }

    #[test]
    fn z3_discovered_branches_get_proven_provenance() {
        use crate::coverage_metrics::DiscoveryMethod;

        let mut result = make_exploration_result("classify", 50, 3);
        // Branch 0 solved by Z3, branch 1 found randomly
        result.discoveries = vec![
            (0, DiscoveryMethod::Z3),
            (1, DiscoveryMethod::Random),
        ];

        let classes = vec![
            // Class with only branch 0 (Z3) → Proven
            make_eq_class(
                vec![(0, true)],
                vec![json!(5)],
                Some(json!("positive")),
                None,
                vec![Precondition::AllPositive { param_index: 0 }],
                10,
            ),
            // Class with branch 0 (Z3) + branch 1 (Random) → Observed (not all Z3)
            make_eq_class(
                vec![(0, false), (1, true)],
                vec![json!(-3)],
                Some(json!("negative")),
                None,
                vec![Precondition::AllNegative { param_index: 0 }],
                8,
            ),
            // Class with only branch 1 (Random) → Observed
            make_eq_class(
                vec![(1, false)],
                vec![json!(0)],
                Some(json!("zero")),
                None,
                vec![Precondition::AllZero { param_index: 0 }],
                2,
            ),
        ];

        let spec = build_spec(&result, &classes, None, None);

        assert_eq!(spec.classes[0].precondition_provenance, Provenance::Proven);
        assert_eq!(spec.classes[0].postcondition_provenance, Provenance::Proven);

        assert_eq!(spec.classes[1].precondition_provenance, Provenance::Observed);
        assert_eq!(spec.classes[1].postcondition_provenance, Provenance::Observed);

        assert_eq!(spec.classes[2].precondition_provenance, Provenance::Observed);
        assert_eq!(spec.classes[2].postcondition_provenance, Provenance::Observed);
    }

    #[test]
    fn all_z3_branches_means_proven() {
        use crate::coverage_metrics::DiscoveryMethod;

        let mut result = make_exploration_result("allProven", 20, 2);
        result.discoveries = vec![
            (0, DiscoveryMethod::Z3),
            (1, DiscoveryMethod::Z3),
        ];

        let classes = vec![make_eq_class(
            vec![(0, true), (1, false)],
            vec![json!(42)],
            Some(json!(true)),
            None,
            vec![],
            5,
        )];

        let spec = build_spec(&result, &classes, None, None);
        assert_eq!(spec.classes[0].precondition_provenance, Provenance::Proven);
        assert_eq!(spec.classes[0].postcondition_provenance, Provenance::Proven);
    }

    #[test]
    fn empty_branch_path_stays_observed() {
        use crate::coverage_metrics::DiscoveryMethod;

        let mut result = make_exploration_result("noBranches", 10, 1);
        result.discoveries = vec![(0, DiscoveryMethod::Z3)];

        // Empty branch path (e.g., straight-line code)
        let classes = vec![make_eq_class(
            vec![],
            vec![json!("hello")],
            None,
            None,
            vec![],
            10,
        )];

        let spec = build_spec(&result, &classes, None, None);
        assert_eq!(spec.classes[0].precondition_provenance, Provenance::Observed);
    }

    #[test]
    fn format_spec_markdown_contains_all_sections() {
        let result = make_exploration_result("classify", 50, 2);
        let classes = vec![
            make_eq_class(
                vec![(0, true)],
                vec![json!(5)],
                Some(json!("positive")),
                None,
                vec![Precondition::AllPositive { param_index: 0 }],
                10,
            ),
            make_eq_class(
                vec![(0, false)],
                vec![json!(-3)],
                Some(json!("negative")),
                None,
                vec![Precondition::AllNegative { param_index: 0 }],
                8,
            ),
        ];

        let spec = build_spec(&result, &classes, Some("math.ts:5".to_string()), None);
        let md = format_spec_markdown(&spec);

        assert!(md.contains("# Specification: `classify`"), "missing title");
        assert!(md.contains("**Location:** `math.ts:5`"), "missing location");
        assert!(
            md.contains("**Behavioral classes:** 2"),
            "missing class count"
        );
        assert!(md.contains("50 iterations"), "missing iterations");
        assert!(md.contains("80%"), "missing coverage pct");
        assert!(md.contains("## Class 1"), "missing class 1 heading");
        assert!(md.contains("## Class 2"), "missing class 2 heading");
        assert!(md.contains("[observed]"), "missing provenance badge");
        assert!(md.contains("param[0] > 0"), "missing precondition");
        assert!(md.contains("param[0] < 0"), "missing precondition");
        assert!(md.contains("returns \"positive\""), "missing postcondition");
        assert!(md.contains("returns \"negative\""), "missing postcondition");
        assert!(md.contains("**Example**"), "missing example section");
        assert!(md.contains("classify(5)"), "missing example call");
    }

    #[test]
    fn format_spec_markdown_error_class() {
        let result = make_exploration_result("boom", 10, 1);
        let classes = vec![make_eq_class(
            vec![(0, true)],
            vec![json!(null)],
            None,
            Some("TypeError: null input".to_string()),
            vec![],
            3,
        )];

        let spec = build_spec(&result, &classes, None, None);
        let md = format_spec_markdown(&spec);

        assert!(md.contains("throws TypeError: null input"), "missing error");
        assert!(md.contains("boom(null)"), "missing example");
    }

    #[test]
    fn format_spec_json_round_trips() {
        let result = make_exploration_result("add", 20, 1);
        let classes = vec![make_eq_class(
            vec![(0, true)],
            vec![json!(1), json!(2)],
            Some(json!(3)),
            None,
            vec![Precondition::AllPositive { param_index: 0 }],
            15,
        )];

        let spec = build_spec(&result, &classes, Some("math.ts:1".to_string()), None);
        let json_str = format_spec_json(&spec).expect("json serialization");

        let deserialized: FunctionSpec =
            serde_json::from_str(&json_str).expect("json deserialization");
        assert_eq!(deserialized.function_name, "add");
        assert_eq!(deserialized.classes.len(), 1);
        assert_eq!(deserialized.classes[0].examples.len(), 1);
        assert_eq!(deserialized.classes[0].examples[0].inputs, vec![json!(1), json!(2)]);
        assert_eq!(
            deserialized.classes[0].postcondition,
            Postcondition::Returns { value: json!(3) }
        );
    }

    #[test]
    fn format_spec_json_contains_expected_fields() {
        let result = make_exploration_result("fn1", 10, 1);
        let classes = vec![make_eq_class(
            vec![],
            vec![json!(42)],
            Some(json!(84)),
            None,
            vec![],
            5,
        )];

        let spec = build_spec(&result, &classes, None, None);
        let json_str = format_spec_json(&spec).expect("json serialization");
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("json parse");

        assert_eq!(parsed["function_name"], "fn1");
        assert!(parsed["location"].is_null());
        assert!(parsed["classes"].is_array());
        assert_eq!(parsed["classes"][0]["sample_count"], 5);
        assert_eq!(
            parsed["classes"][0]["precondition_provenance"],
            "observed"
        );
        assert_eq!(
            parsed["classes"][0]["postcondition_provenance"],
            "observed"
        );
    }

    #[test]
    fn provenance_serialization_round_trips() {
        for p in [Provenance::Proven, Provenance::Observed] {
            let json_str = serde_json::to_string(&p).expect("serialize");
            let deserialized: Provenance =
                serde_json::from_str(&json_str).expect("deserialize");
            assert_eq!(p, deserialized);
        }
    }

    #[test]
    fn postcondition_serialization_round_trips() {
        let postconditions = vec![
            Postcondition::Returns {
                value: json!("hello"),
            },
            Postcondition::Throws {
                error: ErrorInfo {
                    error_type: "Error".to_string(),
                    message: "boom".to_string(),
                    stack: None, error_category: None },
            },
            Postcondition::ReturnsVoid,
        ];

        for post in &postconditions {
            let json_str = serde_json::to_string(post).expect("serialize");
            let deserialized: Postcondition =
                serde_json::from_str(&json_str).expect("deserialize");
            assert_eq!(*post, deserialized);
        }
    }

    #[test]
    fn format_precondition_all_variants() {
        assert_eq!(
            format_precondition(&Precondition::AllPositive { param_index: 0 }),
            "param[0] > 0"
        );
        assert_eq!(
            format_precondition(&Precondition::AllNegative { param_index: 1 }),
            "param[1] < 0"
        );
        assert_eq!(
            format_precondition(&Precondition::AllZero { param_index: 2 }),
            "param[2] == 0"
        );
        assert_eq!(
            format_precondition(&Precondition::AllEqual {
                param_index: 0,
                value: json!("hello"),
            }),
            "param[0] == \"hello\""
        );
        assert_eq!(
            format_precondition(&Precondition::SameType {
                param_index: 0,
                type_name: "number".to_string(),
            }),
            "typeof param[0] == \"number\""
        );
    }

    #[test]
    fn format_side_effect_all_variants() {
        assert_eq!(
            format_side_effect(&SideEffect::ConsoleOutput {
                level: "info".into(),
                message: "test".into()
            }),
            "console.info(\"test\")"
        );
        assert_eq!(
            format_side_effect(&SideEffect::FileWrite {
                path: "/tmp/out".into(),
                content: None,
            }),
            "writes to file: /tmp/out"
        );
        assert_eq!(
            format_side_effect(&SideEffect::NetworkRequest {
                method: "GET".into(),
                url: "http://api.test".into(),
                body: None,
            }),
            "GET http://api.test"
        );
        assert_eq!(
            format_side_effect(&SideEffect::GlobalMutation {
                name: "counter".into()
            }),
            "mutates global: counter"
        );
    }

    #[test]
    fn format_spec_markdown_no_preconditions() {
        let result = make_exploration_result("fn1", 10, 1);
        let classes = vec![make_eq_class(
            vec![],
            vec![json!(1)],
            Some(json!(2)),
            None,
            vec![],
            5,
        )];

        let spec = build_spec(&result, &classes, None, None);
        let md = format_spec_markdown(&spec);

        assert!(md.contains("_(none derived)_"), "should note no preconditions");
    }

    #[test]
    fn format_spec_markdown_no_location() {
        let result = make_exploration_result("fn1", 10, 1);
        let spec = build_spec(&result, &[], None, None);
        let md = format_spec_markdown(&spec);

        assert!(!md.contains("**Location:**"), "should not have location");
    }

    #[test]
    fn full_spec_round_trip_with_all_fields() {
        let spec = FunctionSpec {
            function_name: "complexFn".to_string(),
            location: Some("src/complex.ts:42".to_string()),
            classes: vec![
                SpecClass {
                    label: "Class 1 — returns 42".to_string(),
                    branch_path: BranchPath(vec![BranchStep {
                        branch_id: 0,
                        taken: true,
                    }]),
                    preconditions: vec![Precondition::AllPositive { param_index: 0 }],
                    postcondition: Postcondition::Returns { value: json!(42) },
                    side_effects: vec![SideEffect::ConsoleOutput {
                        level: "info".into(),
                        message: "processed".into(),
                    }],
                    examples: vec![ConcreteExample {
                        inputs: vec![json!(5)],
                        return_value: Some(json!(42)),
                        thrown_error: None,
                    }],
                    sample_count: 10,
                    precondition_provenance: Provenance::Proven,
                    postcondition_provenance: Provenance::Observed,
                    invariants: vec![],
                },
                SpecClass {
                    label: "Class 2 — throws Error: bad input".to_string(),
                    branch_path: BranchPath(vec![BranchStep {
                        branch_id: 0,
                        taken: false,
                    }]),
                    preconditions: vec![],
                    postcondition: Postcondition::Throws {
                        error: ErrorInfo {
                            error_type: "Error".to_string(),
                            message: "bad input".to_string(),
                            stack: None, error_category: None },
                    },
                    side_effects: vec![],
                    examples: vec![ConcreteExample {
                        inputs: vec![json!(-1)],
                        return_value: None,
                        thrown_error: Some(ErrorInfo {
                            error_type: "Error".to_string(),
                            message: "bad input".to_string(),
                            stack: None, error_category: None }),
                    }],
                    sample_count: 5,
                    precondition_provenance: Provenance::Observed,
                    postcondition_provenance: Provenance::Observed,
                    invariants: vec![],
                },
            ],
            iterations: 100,
            lines_covered: 15,
            total_lines: 20,
            invariants: vec![],
            fingerprint: None,
            nondeterministic_fields: vec![],
       
        };

        let json_str = serde_json::to_string_pretty(&spec).expect("serialize");
        let deserialized: FunctionSpec =
            serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(spec, deserialized);
    }

    #[test]
    fn format_spec_markdown_includes_function_invariants() {
        use crate::invariants::{
            ClassifiedInvariant, Invariant, InvariantKind, InvariantTarget, ComparisonOp,
        };

        let mut spec = build_spec(
            &make_exploration_result("fn1", 10, 1),
            &[make_eq_class(vec![], vec![json!(1)], Some(json!(2)), None, vec![], 5)],
            None,
            None,
        );
        spec.invariants = vec![ClassifiedInvariant {
            invariant: Invariant {
                description: "x > 0".to_string(),
                target: InvariantTarget::Input,
                kind: InvariantKind::NumericComparison {
                    path: vec!["x".to_string()],
                    op: ComparisonOp::Gt,
                    value: 0.0,
                },
            },
            target: InvariantTarget::Input,
            label: "input.x > 0".to_string(),
            confidence: 1.0,
            satisfied_count: 10,
            total_count: 10,
        }];

        let md = format_spec_markdown(&spec);
        assert!(md.contains("Function invariants:"), "should have function invariants section");
        assert!(md.contains("x > 0"), "should contain invariant description");
    }

    #[test]
    fn format_spec_markdown_includes_class_invariants() {
        use crate::invariants::{
            ClassifiedInvariant, Invariant, InvariantKind, InvariantTarget, ComparisonOp,
        };

        let mut spec = build_spec(
            &make_exploration_result("fn1", 10, 1),
            &[make_eq_class(vec![], vec![json!(1)], Some(json!(2)), None, vec![], 5)],
            None,
            None,
        );
        spec.classes[0].invariants = vec![ClassifiedInvariant {
            invariant: Invariant {
                description: "y >= 0".to_string(),
                target: InvariantTarget::Output,
                kind: InvariantKind::NumericComparison {
                    path: vec![],
                    op: ComparisonOp::Ge,
                    value: 0.0,
                },
            },
            target: InvariantTarget::Output,
            label: "output >= 0".to_string(),
            confidence: 1.0,
            satisfied_count: 5,
            total_count: 5,
        }];

        let md = format_spec_markdown(&spec);
        assert!(md.contains("**Invariants:**"), "should have per-class invariants section");
        assert!(md.contains("y >= 0"), "should contain invariant description");
    }

    #[test]
    fn invariants_skipped_in_json_when_empty() {
        let spec = build_spec(
            &make_exploration_result("fn1", 10, 1),
            &[make_eq_class(vec![], vec![json!(1)], Some(json!(2)), None, vec![], 5)],
            None,
            None,
        );

        let json_str = format_spec_json(&spec).expect("json serialization");
        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("parse");

        assert!(parsed.get("invariants").is_none(), "empty invariants should be skipped");
        assert!(
            parsed["classes"][0].get("invariants").is_none(),
            "empty class invariants should be skipped"
        );
    }

    #[test]
    fn invariants_present_in_json_when_populated() {
        use crate::invariants::{
            ClassifiedInvariant, Invariant, InvariantKind, InvariantTarget, ComparisonOp,
        };

        let mut spec = build_spec(
            &make_exploration_result("fn1", 10, 1),
            &[make_eq_class(vec![], vec![json!(1)], Some(json!(2)), None, vec![], 5)],
            None,
            None,
        );
        spec.invariants = vec![ClassifiedInvariant {
            invariant: Invariant {
                description: "x > 0".to_string(),
                target: InvariantTarget::Input,
                kind: InvariantKind::NumericComparison {
                    path: vec!["x".to_string()],
                    op: ComparisonOp::Gt,
                    value: 0.0,
                },
            },
            target: InvariantTarget::Input,
            label: "input.x > 0".to_string(),
            confidence: 1.0,
            satisfied_count: 10,
            total_count: 10,
        }];

        let json_str = format_spec_json(&spec).expect("json serialization");
        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("parse");

        assert!(parsed["invariants"].is_array(), "invariants should be present when populated");
        assert_eq!(parsed["invariants"][0]["label"], "input.x > 0");
    }

    #[test]
    fn file_spec_bundle_round_trip() {
        let spec = build_spec(
            &make_exploration_result("add", 10, 2),
            &[],
            Some("src/math.ts:1".to_string()),
            None,
        );
        let bundle = FileSpecBundle {
            file: "src/math.ts".to_string(),
            functions: vec![spec],
        };
        let bundles = vec![bundle];
        let json = format_file_spec_json(&bundles).expect("serialize");
        let parsed: Vec<FileSpecBundle> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].file, "src/math.ts");
        assert_eq!(parsed[0].functions.len(), 1);
        assert_eq!(parsed[0].functions[0].function_name, "add");
    }

    #[test]
    fn write_and_read_file_spec_bundle_round_trip() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let path = dir.path().join("spec.json");

        let bundle = FileSpecBundle {
            file: "src/math.ts".to_string(),
            functions: vec![build_spec(
                &make_exploration_result("add", 10, 2),
                &[make_eq_class(
                    vec![(0, true)],
                    vec![json!(1), json!(2)],
                    Some(json!(3)),
                    None,
                    vec![Precondition::AllPositive { param_index: 0 }],
                    5,
                )],
                Some("src/math.ts:1".to_string()),
                Some("abc123".to_string()),
            )],
        };

        write_file_spec_bundle(&bundle, &path).expect("write");
        assert!(path.exists(), "output file should exist");

        // No temp file left behind
        let tmp_path = path.with_extension("json.tmp");
        assert!(!tmp_path.exists(), "temp file should be cleaned up");

        let loaded = read_file_spec_bundle(&path).expect("read");
        assert_eq!(bundle, loaded);
    }

    #[test]
    fn write_file_spec_bundle_creates_parent_dirs() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let path = dir.path().join("nested").join("deep").join("spec.json");

        let bundle = FileSpecBundle {
            file: "src/lib.ts".to_string(),
            functions: vec![],
        };

        write_file_spec_bundle(&bundle, &path).expect("write with nested dirs");
        let loaded = read_file_spec_bundle(&path).expect("read");
        assert_eq!(loaded.file, "src/lib.ts");
    }

    #[test]
    fn read_file_spec_bundle_nonexistent_file() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let path = dir.path().join("does_not_exist.json");

        let err = read_file_spec_bundle(&path).unwrap_err();
        assert!(
            matches!(err, SpecIoError::Io(ref e) if e.kind() == std::io::ErrorKind::NotFound),
            "expected NotFound, got: {err}"
        );
    }

    #[test]
    fn read_file_spec_bundle_invalid_json() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not valid json {{{").expect("write bad json");

        let err = read_file_spec_bundle(&path).unwrap_err();
        assert!(matches!(err, SpecIoError::Json(_)), "expected Json error, got: {err}");
    }

    #[test]
    fn spec_io_error_display() {
        let io_err = SpecIoError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file not found",
        ));
        assert!(io_err.to_string().contains("spec I/O error"));

        let json_err = SpecIoError::from(
            serde_json::from_str::<FileSpecBundle>("bad").unwrap_err(),
        );
        assert!(json_err.to_string().contains("spec JSON error"));
    }

    // --- IncrementalPlan tests ---

    use crate::protocol::BranchInfo;
    use crate::types::ParamInfo;

    fn make_analysis(name: &str, start: u32, end: u32) -> crate::protocol::FunctionAnalysis {
        crate::protocol::FunctionAnalysis {
            name: name.to_string(),
            exported: true,
            params: vec![ParamInfo {
                name: "x".into(),
                typ: crate::types::TypeInfo::Int,
                type_name: None,
            }],
            branches: vec![BranchInfo {
                id: 0,
                line: start + 1,
                condition_text: "x > 0".into(),
                condition: None,
                branch_type: crate::protocol::BranchType::If,
            }],
            dependencies: vec![],
            return_type: crate::types::TypeInfo::Int,
            start_line: start,
            end_line: end,
            literals: vec![],
            crypto_boundaries: vec![],
        }
    }

    fn make_spec_with_fingerprint(name: &str, fingerprint: Option<&str>) -> FunctionSpec {
        FunctionSpec {
            function_name: name.to_string(),
            location: None,
            classes: vec![],
            iterations: 10,
            lines_covered: 5,
            total_lines: 10,
            invariants: vec![],
            fingerprint: fingerprint.map(|s| s.to_string()),
            nondeterministic_fields: vec![],
        }
    }

    #[test]
    fn incremental_plan_all_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        let source = "function add(x) {\n  if (x > 0) return x;\n  return 0;\n}\n";
        std::fs::write(&file, source).unwrap();

        let analysis = make_analysis("add", 1, 4);
        let deep_fps =
            crate::fingerprint::compute_deep_fingerprints(&file, &[analysis.clone()], &std::collections::HashMap::new()).unwrap();
        let fp = &deep_fps["add"];

        let existing = FileSpecBundle {
            file: "test.ts".to_string(),
            functions: vec![make_spec_with_fingerprint("add", Some(fp))],
        };

        let plan = compute_incremental_plan(&file, &[analysis], &existing, &std::collections::HashMap::new()).unwrap();
        assert!(plan.stale.is_empty(), "expected no stale, got: {:?}", plan.stale);
        assert_eq!(plan.fresh, vec!["add"]);
        assert!(plan.removed.is_empty());
    }

    #[test]
    fn incremental_plan_stale_on_fingerprint_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        std::fs::write(&file, "function add(x) {\n  if (x > 0) return x;\n  return 0;\n}\n").unwrap();

        let analysis = make_analysis("add", 1, 4);

        let existing = FileSpecBundle {
            file: "test.ts".to_string(),
            functions: vec![make_spec_with_fingerprint("add", Some("old_fp_mismatch"))],
        };

        let plan = compute_incremental_plan(&file, &[analysis], &existing, &std::collections::HashMap::new()).unwrap();
        assert_eq!(plan.stale, vec!["add"]);
        assert!(plan.fresh.is_empty());
        assert!(plan.removed.is_empty());
    }

    #[test]
    fn incremental_plan_stale_when_no_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        std::fs::write(&file, "function add(x) {\n  if (x > 0) return x;\n  return 0;\n}\n").unwrap();

        let analysis = make_analysis("add", 1, 4);

        let existing = FileSpecBundle {
            file: "test.ts".to_string(),
            functions: vec![make_spec_with_fingerprint("add", None)],
        };

        let plan = compute_incremental_plan(&file, &[analysis], &existing, &std::collections::HashMap::new()).unwrap();
        assert_eq!(plan.stale, vec!["add"]);
        assert!(plan.fresh.is_empty());
    }

    #[test]
    fn incremental_plan_new_function() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        std::fs::write(&file, "function add(x) {\n  if (x > 0) return x;\n  return 0;\n}\n").unwrap();

        let analysis = make_analysis("add", 1, 4);
        let existing = FileSpecBundle {
            file: "test.ts".to_string(),
            functions: vec![],
        };

        let plan = compute_incremental_plan(&file, &[analysis], &existing, &std::collections::HashMap::new()).unwrap();
        assert_eq!(plan.stale, vec!["add"]);
        assert!(plan.fresh.is_empty());
        assert!(plan.removed.is_empty());
    }

    #[test]
    fn incremental_plan_removed_function() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        std::fs::write(&file, "function add(x) {\n  if (x > 0) return x;\n  return 0;\n}\n").unwrap();

        let existing = FileSpecBundle {
            file: "test.ts".to_string(),
            functions: vec![
                make_spec_with_fingerprint("add", Some("old")),
                make_spec_with_fingerprint("removed_fn", Some("old2")),
            ],
        };

        let analysis = make_analysis("add", 1, 4);
        let plan = compute_incremental_plan(&file, &[analysis], &existing, &std::collections::HashMap::new()).unwrap();
        assert_eq!(plan.removed, vec!["removed_fn"]);
    }

    fn make_analysis_with_deps(
        name: &str,
        start: u32,
        end: u32,
        deps: Vec<&str>,
    ) -> crate::protocol::FunctionAnalysis {
        use crate::protocol::{DependencyKind, ExternalDependency};
        let mut a = make_analysis(name, start, end);
        a.dependencies = deps
            .into_iter()
            .map(|s| ExternalDependency {
                kind: DependencyKind::FunctionCall,
                symbol: s.to_string(),
                source_module: String::new(),
                return_type: crate::types::TypeInfo::Unknown,
                param_types: vec![],
                call_sites: vec![],
            })
            .collect();
        a
    }

    #[test]
    fn incremental_plan_caller_stale_when_callee_changes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        // V1: leaf returns 1
        let source_v1 =
            "function leaf(x) {\n  if (x > 0) return 1;\n  return 0;\n}\nfunction caller(x) {\n  if (x > 0) return leaf(x);\n  return 0;\n}\n";
        std::fs::write(&file, source_v1).unwrap();

        let analyses_v1 = vec![
            make_analysis("leaf", 1, 3),
            make_analysis_with_deps("caller", 5, 7, vec!["leaf"]),
        ];

        let deep_fps_v1 =
            crate::fingerprint::compute_deep_fingerprints(&file, &analyses_v1, &std::collections::HashMap::new()).unwrap();

        let existing = FileSpecBundle {
            file: "test.ts".to_string(),
            functions: vec![
                make_spec_with_fingerprint("leaf", Some(&deep_fps_v1["leaf"])),
                make_spec_with_fingerprint("caller", Some(&deep_fps_v1["caller"])),
            ],
        };

        // V2: leaf returns 2 (changed), caller unchanged
        let source_v2 =
            "function leaf(x) {\n  if (x > 0) return 2;\n  return 0;\n}\nfunction caller(x) {\n  if (x > 0) return leaf(x);\n  return 0;\n}\n";
        std::fs::write(&file, source_v2).unwrap();

        let analyses_v2 = vec![
            make_analysis("leaf", 1, 3),
            make_analysis_with_deps("caller", 5, 7, vec!["leaf"]),
        ];

        let plan = compute_incremental_plan(&file, &analyses_v2, &existing, &std::collections::HashMap::new()).unwrap();

        // Both should be stale: leaf changed directly, caller transitively.
        assert!(plan.stale.contains(&"leaf".to_string()), "leaf should be stale");
        assert!(
            plan.stale.contains(&"caller".to_string()),
            "caller should be stale when callee changes"
        );
        assert!(plan.fresh.is_empty());
    }

    #[test]
    fn merge_preserves_fresh_drops_removed() {
        let existing = FileSpecBundle {
            file: "test.ts".to_string(),
            functions: vec![
                make_spec_with_fingerprint("fresh_fn", Some("fp1")),
                make_spec_with_fingerprint("stale_fn", Some("fp2")),
                make_spec_with_fingerprint("removed_fn", Some("fp3")),
            ],
        };

        let new_stale_spec = make_spec_with_fingerprint("stale_fn", Some("fp2_new"));
        let current_names: HashSet<String> =
            ["fresh_fn", "stale_fn"].iter().map(|s| s.to_string()).collect();

        let merged = merge_file_spec_bundles(&existing, &[new_stale_spec], &current_names);

        assert_eq!(merged.functions.len(), 2);
        let names: Vec<&str> = merged.functions.iter().map(|f| f.function_name.as_str()).collect();
        assert!(names.contains(&"stale_fn"), "should contain re-explored stale_fn");
        assert!(names.contains(&"fresh_fn"), "should carry over fresh_fn");
        assert!(!names.contains(&"removed_fn"), "should drop removed_fn");

        // Verify stale_fn has new fingerprint
        let stale = merged.functions.iter().find(|f| f.function_name == "stale_fn").unwrap();
        assert_eq!(stale.fingerprint.as_deref(), Some("fp2_new"));

        // Verify fresh_fn has old fingerprint
        let fresh = merged.functions.iter().find(|f| f.function_name == "fresh_fn").unwrap();
        assert_eq!(fresh.fingerprint.as_deref(), Some("fp1"));
    }

    #[test]
    fn merge_with_empty_existing() {
        let existing = FileSpecBundle {
            file: "test.ts".to_string(),
            functions: vec![],
        };

        let new_spec = make_spec_with_fingerprint("add", Some("fp1"));
        let current_names: HashSet<String> = ["add"].iter().map(|s| s.to_string()).collect();

        let merged = merge_file_spec_bundles(&existing, &[new_spec], &current_names);
        assert_eq!(merged.functions.len(), 1);
        assert_eq!(merged.functions[0].function_name, "add");
    }

    #[test]
    fn merge_with_no_new_specs() {
        let existing = FileSpecBundle {
            file: "test.ts".to_string(),
            functions: vec![
                make_spec_with_fingerprint("fn1", Some("fp1")),
                make_spec_with_fingerprint("fn2", Some("fp2")),
            ],
        };

        let current_names: HashSet<String> =
            ["fn1", "fn2"].iter().map(|s| s.to_string()).collect();

        let merged = merge_file_spec_bundles(&existing, &[], &current_names);
        assert_eq!(merged.functions.len(), 2);
    }

    // -----------------------------------------------------------------------
    // YAML property description tests
    // -----------------------------------------------------------------------

    #[test]
    fn confidence_level_thresholds() {
        assert_eq!(ConfidenceLevel::from_score(1.0), ConfidenceLevel::High);
        assert_eq!(ConfidenceLevel::from_score(0.95), ConfidenceLevel::High);
        assert_eq!(ConfidenceLevel::from_score(0.94), ConfidenceLevel::Medium);
        assert_eq!(ConfidenceLevel::from_score(0.75), ConfidenceLevel::Medium);
        assert_eq!(ConfidenceLevel::from_score(0.74), ConfidenceLevel::Low);
        assert_eq!(ConfidenceLevel::from_score(0.0), ConfidenceLevel::Low);
    }

    #[test]
    fn categorize_invariant_kinds() {
        use crate::invariants::{ComparisonOp, InvariantKind};

        assert_eq!(
            categorize_invariant_kind(&InvariantKind::NumericComparison {
                path: vec![],
                op: ComparisonOp::Gt,
                value: 0.0,
            }),
            InvariantCategory::NumericBound
        );
        assert_eq!(
            categorize_invariant_kind(&InvariantKind::NumericConstant {
                path: vec![],
                value: 42.0,
            }),
            InvariantCategory::ConstantValue
        );
        assert_eq!(
            categorize_invariant_kind(&InvariantKind::NotNull { path: vec![] }),
            InvariantCategory::Nullability
        );
        assert_eq!(
            categorize_invariant_kind(&InvariantKind::IsNull { path: vec![] }),
            InvariantCategory::Nullability
        );
        assert_eq!(
            categorize_invariant_kind(&InvariantKind::StringNonEmpty { path: vec![] }),
            InvariantCategory::StringConstraint
        );
        assert_eq!(
            categorize_invariant_kind(&InvariantKind::StringLength {
                path: vec![],
                op: ComparisonOp::Ge,
                value: 1,
            }),
            InvariantCategory::StringConstraint
        );
        assert_eq!(
            categorize_invariant_kind(&InvariantKind::OutputEqualsInput {
                output_path: vec![],
                param_index: 0,
                input_path: vec![],
            }),
            InvariantCategory::InputOutputRelation
        );
        assert_eq!(
            categorize_invariant_kind(&InvariantKind::AlwaysTrue { path: vec![] }),
            InvariantCategory::BooleanConstant
        );
        assert_eq!(
            categorize_invariant_kind(&InvariantKind::AlwaysFalse { path: vec![] }),
            InvariantCategory::BooleanConstant
        );
    }

    #[test]
    fn spec_invariant_from_classified() {
        use crate::invariants::{
            ClassifiedInvariant, ComparisonOp, Invariant, InvariantKind, InvariantTarget,
        };

        let ci = ClassifiedInvariant {
            invariant: Invariant {
                description: "x > 0".to_string(),
                target: InvariantTarget::Input,
                kind: InvariantKind::NumericComparison {
                    path: vec!["x".to_string()],
                    op: ComparisonOp::Gt,
                    value: 0.0,
                },
            },
            target: InvariantTarget::Input,
            label: "input.x > 0".to_string(),
            confidence: 1.0,
            satisfied_count: 10,
            total_count: 10,
        };

        let si = SpecInvariant::from(&ci);
        assert_eq!(si.property, "input.x > 0");
        assert_eq!(si.kind, InvariantCategory::NumericBound);
        assert_eq!(si.confidence, ConfidenceLevel::High);
    }

    #[test]
    fn spec_invariant_medium_confidence() {
        use crate::invariants::{
            ClassifiedInvariant, Invariant, InvariantKind, InvariantTarget,
        };

        let ci = ClassifiedInvariant {
            invariant: Invariant {
                description: "x is not null".to_string(),
                target: InvariantTarget::Input,
                kind: InvariantKind::NotNull {
                    path: vec!["x".to_string()],
                },
            },
            target: InvariantTarget::Input,
            label: "input.x is not null".to_string(),
            confidence: 0.8,
            satisfied_count: 8,
            total_count: 10,
        };

        let si = SpecInvariant::from(&ci);
        assert_eq!(si.confidence, ConfidenceLevel::Medium);
        assert_eq!(si.kind, InvariantCategory::Nullability);
    }

    #[test]
    fn spec_invariant_serialization_round_trips() {
        let si = SpecInvariant {
            property: "input.x > 0".to_string(),
            kind: InvariantCategory::NumericBound,
            confidence: ConfidenceLevel::High,
        };

        let yaml = serde_yaml::to_string(&si).expect("serialize");
        let deserialized: SpecInvariant = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(si, deserialized);

        let json = serde_json::to_string(&si).expect("json serialize");
        let deserialized: SpecInvariant = serde_json::from_str(&json).expect("json deserialize");
        assert_eq!(si, deserialized);
    }

    // ── format_spec_yaml invariant output ────────────────────────────────

    #[test]
    fn yaml_invariants_use_property_field() {
        use crate::invariants::{
            ClassifiedInvariant, ComparisonOp, Invariant, InvariantKind, InvariantTarget,
        };

        let ci = ClassifiedInvariant {
            invariant: Invariant {
                description: "x > 0".to_string(),
                target: InvariantTarget::Input,
                kind: InvariantKind::NumericComparison {
                    path: vec!["x".to_string()],
                    op: ComparisonOp::Gt,
                    value: 0.0,
                },
            },
            target: InvariantTarget::Input,
            label: "input.x is always positive".to_string(),
            confidence: 1.0,
            satisfied_count: 5,
            total_count: 5,
        };

        let mut spec = FunctionSpec {
            function_name: "myFn".to_string(),
            location: None,
            classes: vec![],
            iterations: 10,
            lines_covered: 3,
            total_lines: 3,
            invariants: vec![ci],
            fingerprint: None,
            nondeterministic_fields: vec![],
        };
        // Populate a per-class invariant too
        spec.classes.push(SpecClass {
            label: "Class 1 — returns true".to_string(),
            branch_path: crate::equivalence::BranchPath(vec![]),
            preconditions: vec![],
            postcondition: Postcondition::ReturnsVoid,
            side_effects: vec![],
            examples: vec![],
            sample_count: 5,
            precondition_provenance: Provenance::Observed,
            postcondition_provenance: Provenance::Observed,
            invariants: vec![ClassifiedInvariant {
                invariant: Invariant {
                    description: "output is not null".to_string(),
                    target: InvariantTarget::Output,
                    kind: InvariantKind::NotNull { path: vec![] },
                },
                target: InvariantTarget::Output,
                label: "output is not null".to_string(),
                confidence: 0.9,
                satisfied_count: 5,
                total_count: 5,
            }],
        });

        let yaml = format_spec_yaml(&spec).expect("yaml serialization should succeed");

        // Must contain property: field (SpecInvariant format)
        assert!(
            yaml.contains("property:"),
            "YAML should contain 'property:' key, got:\n{yaml}"
        );
        assert!(
            yaml.contains("input.x is always positive"),
            "YAML should contain the invariant label, got:\n{yaml}"
        );
        assert!(
            yaml.contains("output is not null"),
            "YAML should contain per-class invariant label, got:\n{yaml}"
        );

        // Must NOT contain raw ClassifiedInvariant fields
        assert!(
            !yaml.contains("satisfied_count"),
            "YAML should not contain raw 'satisfied_count' field, got:\n{yaml}"
        );
        assert!(
            !yaml.contains("total_count"),
            "YAML should not contain raw 'total_count' field, got:\n{yaml}"
        );
    }

    #[test]
    fn yaml_invariants_absent_when_empty() {
        let spec = FunctionSpec {
            function_name: "myFn".to_string(),
            location: None,
            classes: vec![],
            iterations: 5,
            lines_covered: 2,
            total_lines: 4,
            invariants: vec![],
            fingerprint: None,
            nondeterministic_fields: vec![],
        };

        let yaml = format_spec_yaml(&spec).expect("yaml serialization should succeed");

        assert!(
            !yaml.contains("invariants:"),
            "YAML should not contain 'invariants:' key when invariants are empty, got:\n{yaml}"
        );
    }

    // ── Property-based tests ─────────────────────────────────────────────

    mod proptests {
        use super::*;
        use crate::test_arbitraries::{arb_classified_invariant, arb_function_spec};
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn function_spec_json_roundtrip(spec in arb_function_spec()) {
                let json_str = serde_json::to_string_pretty(&spec).expect("json serialize");
                let deserialized: FunctionSpec =
                    serde_json::from_str(&json_str).expect("json deserialize");
                prop_assert_eq!(spec, deserialized);
            }

            #[test]
            fn function_spec_yaml_roundtrip(spec in arb_function_spec()) {
                let yaml_str = serde_yaml::to_string(&spec).expect("yaml serialize");
                let deserialized: FunctionSpec =
                    serde_yaml::from_str(&yaml_str).expect("yaml deserialize");
                prop_assert_eq!(spec, deserialized);
            }

            #[test]
            fn format_spec_markdown_structural_validity(spec in arb_function_spec()) {
                let md = format_spec_markdown(&spec);

                // Always non-empty and contains the function name
                prop_assert!(!md.is_empty());
                prop_assert!(
                    md.contains(&spec.function_name),
                    "markdown should contain function name '{}', got:\n{}",
                    spec.function_name,
                    &md[..md.len().min(200)]
                );

                // All class labels appear in the output
                for class in &spec.classes {
                    prop_assert!(
                        md.contains(&class.label),
                        "markdown should contain class label '{}', got:\n{}",
                        class.label,
                        &md[..md.len().min(500)]
                    );
                }
            }

            #[test]
            fn invariant_conversion_preserves_label(ci in arb_classified_invariant()) {
                let si = SpecInvariant::from(&ci);
                prop_assert_eq!(&si.property, &ci.label);
            }

            #[test]
            fn invariant_conversion_maps_confidence(ci in arb_classified_invariant()) {
                let si = SpecInvariant::from(&ci);
                let expected = if ci.confidence >= 0.95 {
                    ConfidenceLevel::High
                } else if ci.confidence >= 0.75 {
                    ConfidenceLevel::Medium
                } else {
                    ConfidenceLevel::Low
                };
                prop_assert_eq!(si.confidence, expected);
            }

            #[test]
            fn invariant_conversion_maps_category(ci in arb_classified_invariant()) {
                let si = SpecInvariant::from(&ci);
                // Verify the category is a valid InvariantCategory matching the kind
                let expected = match &ci.invariant.kind {
                    crate::invariants::InvariantKind::NumericComparison { .. } => InvariantCategory::NumericBound,
                    crate::invariants::InvariantKind::NumericConstant { .. } => InvariantCategory::ConstantValue,
                    crate::invariants::InvariantKind::NotNull { .. } | crate::invariants::InvariantKind::IsNull { .. } => InvariantCategory::Nullability,
                    crate::invariants::InvariantKind::StringNonEmpty { .. } | crate::invariants::InvariantKind::StringLength { .. } => InvariantCategory::StringConstraint,
                    crate::invariants::InvariantKind::OutputEqualsInput { .. } => InvariantCategory::InputOutputRelation,
                    crate::invariants::InvariantKind::AlwaysTrue { .. } | crate::invariants::InvariantKind::AlwaysFalse { .. } => InvariantCategory::BooleanConstant,
                };
                prop_assert_eq!(si.kind, expected);
            }

        }

        // ── merge_file_spec_bundles properties ──────────────────────

        fn arb_file_spec_bundle(
            max_fns: usize,
        ) -> impl Strategy<Value = FileSpecBundle> {
            prop::collection::vec(arb_function_spec(), 0..=max_fns).prop_map(|mut specs| {
                let mut seen = std::collections::HashSet::new();
                specs.retain(|s| seen.insert(s.function_name.clone()));
                FileSpecBundle {
                    file: "test.ts".to_string(),
                    functions: specs,
                }
            })
        }

        proptest! {
            #[test]
            fn merge_output_count(
                existing in arb_file_spec_bundle(5),
                new_specs_raw in prop::collection::vec(arb_function_spec(), 0..=3),
            ) {
                // Deduplicate new_specs
                let mut seen = std::collections::HashSet::new();
                let new_specs: Vec<FunctionSpec> = new_specs_raw
                    .into_iter()
                    .filter(|s| seen.insert(s.function_name.clone()))
                    .collect();

                // current_function_names = all existing + all new
                let current_names: HashSet<String> = existing
                    .functions
                    .iter()
                    .map(|f| f.function_name.clone())
                    .chain(new_specs.iter().map(|f| f.function_name.clone()))
                    .collect();

                let merged = merge_file_spec_bundles(&existing, &new_specs, &current_names);

                // Every function in current_names should appear exactly once
                prop_assert_eq!(merged.functions.len(), current_names.len());

                let merged_names: HashSet<&str> =
                    merged.functions.iter().map(|f| f.function_name.as_str()).collect();
                prop_assert_eq!(merged_names.len(), merged.functions.len(),
                    "merged bundle has duplicate function names");
            }

            #[test]
            fn merge_new_specs_override_existing(
                existing in arb_file_spec_bundle(4),
                override_fp in "[a-f0-9]{8}",
            ) {
                if existing.functions.is_empty() {
                    return Ok(());
                }
                // Create a new_spec that overrides the first existing function
                let mut override_spec = existing.functions[0].clone();
                override_spec.fingerprint = Some(override_fp.clone());

                let current_names: HashSet<String> = existing
                    .functions
                    .iter()
                    .map(|f| f.function_name.clone())
                    .collect();

                let merged = merge_file_spec_bundles(
                    &existing,
                    &[override_spec],
                    &current_names,
                );

                let result = merged
                    .functions
                    .iter()
                    .find(|f| f.function_name == existing.functions[0].function_name)
                    .expect("overridden function should be in merged result");

                prop_assert_eq!(result.fingerprint.as_deref(), Some(override_fp.as_str()),
                    "new_spec should override existing spec");
            }

            #[test]
            fn merge_drops_removed(
                existing in arb_file_spec_bundle(5),
            ) {
                if existing.functions.is_empty() {
                    return Ok(());
                }
                // Remove the first function from current_names
                let removed_name = existing.functions[0].function_name.clone();
                let current_names: HashSet<String> = existing
                    .functions
                    .iter()
                    .skip(1)
                    .map(|f| f.function_name.clone())
                    .collect();

                let merged = merge_file_spec_bundles(&existing, &[], &current_names);

                let merged_names: HashSet<&str> =
                    merged.functions.iter().map(|f| f.function_name.as_str()).collect();
                prop_assert!(!merged_names.contains(removed_name.as_str()),
                    "removed function should not appear in merged result");
            }

            #[test]
            fn merge_result_subset_of_current_names(
                existing in arb_file_spec_bundle(5),
                new_specs_raw in prop::collection::vec(arb_function_spec(), 0..=3),
            ) {
                let mut seen = std::collections::HashSet::new();
                let new_specs: Vec<FunctionSpec> = new_specs_raw
                    .into_iter()
                    .filter(|s| seen.insert(s.function_name.clone()))
                    .collect();

                // Use a subset of names as current (some existing may be "removed")
                let current_names: HashSet<String> = existing
                    .functions
                    .iter()
                    .map(|f| f.function_name.clone())
                    .chain(new_specs.iter().map(|f| f.function_name.clone()))
                    .collect();

                let merged = merge_file_spec_bundles(&existing, &new_specs, &current_names);

                for f in &merged.functions {
                    prop_assert!(
                        current_names.contains(&f.function_name),
                        "merged function '{}' not in current_function_names",
                        f.function_name
                    );
                }
            }

            #[test]
            fn merge_idempotent_with_empty_new_specs(
                existing in arb_file_spec_bundle(5),
            ) {
                let current_names: HashSet<String> = existing
                    .functions
                    .iter()
                    .map(|f| f.function_name.clone())
                    .collect();

                let merged = merge_file_spec_bundles(&existing, &[], &current_names);

                // Same functions, same count
                prop_assert_eq!(merged.functions.len(), existing.functions.len());

                let merged_names: HashSet<&str> =
                    merged.functions.iter().map(|f| f.function_name.as_str()).collect();
                for f in &existing.functions {
                    prop_assert!(merged_names.contains(f.function_name.as_str()));
                }
            }
        }
    }
}
