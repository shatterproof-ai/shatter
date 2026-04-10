//! Markdown specification report generation.
//!
//! Given a function's analysis results and observed behaviors (with invariants),
//! generates a human- and agent-readable markdown specification in the format
//! described in PLAN.md section 4.3.

use std::fmt::Write;

use serde::{Deserialize, Serialize};

use crate::adapter_selection::AdapterSelectionResult;
use crate::behavior::BehaviorMap;
use crate::execution_record::{ErrorInfo, ExecutionRecord};
use crate::invariants::Invariant;
use crate::protocol::{ExternalDependency, FunctionAnalysis};
use crate::types::TypeInfo;

// ---------------------------------------------------------------------------
// AnnotatedCluster
// ---------------------------------------------------------------------------

/// A behavior cluster enriched with invariants for report generation.
///
/// Built from a [`clustering::BehaviorCluster`](crate::clustering::BehaviorCluster)
/// after invariant detection has been run. Contains the actual execution records
/// (not just indices) and detected invariants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotatedCluster {
    /// Unique identifier within the function's clusters.
    pub id: u32,
    /// Human-readable signature (e.g. "returns empty array when input is negative").
    pub signature: String,
    /// Execution records in this cluster.
    pub specimens: Vec<ExecutionRecord>,
    /// Invariants detected over input values.
    pub input_invariants: Vec<Invariant>,
    /// Invariants detected over output values.
    pub output_invariants: Vec<Invariant>,
    /// Summary of side effects observed in this cluster.
    pub side_effect_summary: Vec<String>,
}

// ---------------------------------------------------------------------------
// FunctionSpec — input to the reporter
// ---------------------------------------------------------------------------

/// All data needed to generate a markdown specification for one function.
pub struct FunctionSpec<'a> {
    /// Static analysis results (name, params, return type, dependencies).
    pub analysis: &'a FunctionAnalysis,
    /// Behavior clusters with invariants.
    pub clusters: &'a [AnnotatedCluster],
    /// Edge case clusters (behaviors with unusual or boundary inputs).
    pub edge_cases: &'a [EdgeCase],
    /// Adapter selection result, if available. When present, the report
    /// distinguishes active adapters from suggested adapters.
    pub adapter_selection: Option<&'a AdapterSelectionResult>,
}

/// A single edge case to include in the Edge Cases section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeCase {
    /// Description of the edge condition (e.g. "Empty items array").
    pub condition: String,
    /// What happens (e.g. "returns { cost: 0, method: \"none\" }").
    pub outcome: String,
}

// ---------------------------------------------------------------------------
// Markdown generation
// ---------------------------------------------------------------------------

/// Generate a markdown specification report for a function.
///
/// The output follows the format from PLAN.md section 4.3:
/// - Function signature heading
/// - Parameters section with tested ranges
/// - One "Behavior N" section per cluster
/// - Edge Cases section
/// - Dependencies section
pub fn generate_markdown(spec: &FunctionSpec<'_>) -> String {
    let mut out = String::new();

    write_function_heading(&mut out, spec.analysis);
    write_parameters_section(&mut out, spec.analysis);
    if let Some(selection) = spec.adapter_selection {
        write_adapter_selection_section(&mut out, selection);
    } else {
        write_adapter_hints_section(&mut out, spec.analysis);
    }
    write_behavior_sections(&mut out, spec.clusters);
    write_edge_cases_section(&mut out, spec.edge_cases);
    write_dependencies_section(&mut out, &spec.analysis.dependencies);

    out
}

/// Generate markdown from a `BehaviorMap` (simpler path without clustering).
///
/// Useful when invariant detection and clustering haven't been run yet.
/// Each behavior becomes its own section.
pub fn generate_markdown_from_behavior_map(
    analysis: &FunctionAnalysis,
    behavior_map: &BehaviorMap,
) -> String {
    let clusters: Vec<AnnotatedCluster> = behavior_map
        .behaviors
        .iter()
        .map(|b| {
            let signature = build_signature_from_behavior(b);
            let record = execution_record_from_behavior(&analysis.name, b);
            AnnotatedCluster {
                id: b.id,
                signature,
                specimens: vec![record],
                input_invariants: vec![],
                output_invariants: vec![],
                side_effect_summary: b
                    .side_effects
                    .iter()
                    .map(|se| format!("{se:?}"))
                    .collect(),
            }
        })
        .collect();

    let spec = FunctionSpec {
        analysis,
        clusters: &clusters,
        edge_cases: &[],
        adapter_selection: None,
    };
    generate_markdown(&spec)
}

// ---------------------------------------------------------------------------
// Section writers
// ---------------------------------------------------------------------------

fn write_function_heading(out: &mut String, analysis: &FunctionAnalysis) {
    let display_params = match &analysis.invocation_model {
        crate::protocol::InvocationModel::Direct => &analysis.params,
        crate::protocol::InvocationModel::Adapter { synthetic_params, .. } => synthetic_params,
    };
    let params_str = display_params
        .iter()
        .map(|p| format!("{}: {}", p.name, format_type(&p.typ)))
        .collect::<Vec<_>>()
        .join(", ");
    let return_str = format_type(&analysis.return_type);

    let _ = writeln!(
        out,
        "# Function: {name}({params}): {ret}\n",
        name = analysis.name,
        params = params_str,
        ret = return_str,
    );

    if let crate::protocol::InvocationModel::Adapter { adapter_id, .. } = &analysis.invocation_model {
        let _ = writeln!(out, "_Invocation: adapter-owned via `{adapter_id}`_\n");
    }
}

fn write_parameters_section(out: &mut String, analysis: &FunctionAnalysis) {
    if analysis.params.is_empty() {
        return;
    }
    out.push_str("## Parameters\n");
    for param in &analysis.params {
        let _ = writeln!(
            out,
            "- `{name}`: {typ}",
            name = param.name,
            typ = format_type(&param.typ),
        );
    }
    out.push('\n');
}

fn write_adapter_hints_section(out: &mut String, analysis: &FunctionAnalysis) {
    if analysis.adapter_hints.is_empty() {
        return;
    }

    out.push_str("## Adapter Hints\n");
    for hint in &analysis.adapter_hints {
        let apply = hint
            .adapter
            .apply
            .as_ref()
            .map(|policy| format!(" ({})", format!("{policy:?}").to_lowercase()))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "- `{}`{} [{}]",
            hint.adapter.id,
            apply,
            format!("{:?}", hint.confidence).to_lowercase(),
        );
        if !hint.reasons.is_empty() {
            let _ = writeln!(out, "  reasons: {}", hint.reasons.join("; "));
        }
        if !hint.requirements.is_empty() {
            let reqs = hint
                .requirements
                .iter()
                .map(|req| match &req.reason {
                    Some(reason) => format!("{} ({reason})", req.adapter_id),
                    None => req.adapter_id.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(out, "  requirements: {reqs}");
        }
        if !hint.conflicts.is_empty() {
            let conflicts = hint
                .conflicts
                .iter()
                .map(|conflict| match &conflict.reason {
                    Some(reason) => format!("{} ({reason})", conflict.adapter_id),
                    None => conflict.adapter_id.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(out, "  conflicts: {conflicts}");
        }
    }
    out.push('\n');
}

fn write_adapter_selection_section(out: &mut String, selection: &AdapterSelectionResult) {
    if selection.is_empty() {
        return;
    }

    if !selection.active.is_empty() {
        out.push_str("## Active Adapters\n");
        for active in &selection.active {
            let _ = writeln!(out, "- `{}` ({})", active.adapter.id, active.provenance);
            if !active.reasons.is_empty() {
                let _ = writeln!(out, "  reasons: {}", active.reasons.join("; "));
            }
        }
        out.push('\n');
    }

    if !selection.suggested.is_empty() {
        out.push_str("## Suggested Adapters\n");
        for suggested in &selection.suggested {
            let _ = writeln!(
                out,
                "- `{}` [{}]",
                suggested.adapter.id,
                format!("{:?}", suggested.confidence).to_lowercase(),
            );
            if !suggested.reasons.is_empty() {
                let _ = writeln!(out, "  reasons: {}", suggested.reasons.join("; "));
            }
        }
        out.push('\n');
    }
}

fn write_behavior_sections(out: &mut String, clusters: &[AnnotatedCluster]) {
    for (i, cluster) in clusters.iter().enumerate() {
        let _ = writeln!(
            out,
            "## Behavior {num}: {sig}",
            num = i + 1,
            sig = cluster.signature,
        );

        // When condition — derive from specimens
        if let Some(when) = derive_when_condition(cluster) {
            let _ = writeln!(out, "**When:** {when}");
        }

        // Returns or Throws
        write_outcome(out, cluster);

        // Invariants
        let all_invariants: Vec<&Invariant> = cluster
            .input_invariants
            .iter()
            .chain(&cluster.output_invariants)
            .collect();
        if !all_invariants.is_empty() {
            let descs: Vec<&str> = all_invariants.iter().map(|inv| inv.description.as_str()).collect();
            let _ = writeln!(out, "**Invariant:** {}", descs.join("; "));
        }

        // External calls from specimens
        write_calls(out, cluster);

        // Performance
        write_performance(out, cluster);

        out.push('\n');
    }
}

fn write_outcome(out: &mut String, cluster: &AnnotatedCluster) {
    if cluster.specimens.is_empty() {
        return;
    }

    // Check if any specimen threw an error
    let errors: Vec<&ErrorInfo> = cluster
        .specimens
        .iter()
        .filter_map(|s| s.thrown_error.as_ref())
        .collect();

    if !errors.is_empty() {
        let err = &errors[0];
        let _ = writeln!(out, "**Throws:** {}(\"{}\")", err.error_type, err.message);
        return;
    }

    // Summarize return values
    let returns: Vec<&serde_json::Value> = cluster
        .specimens
        .iter()
        .filter_map(|s| s.return_value.as_ref())
        .collect();

    if !returns.is_empty() {
        if returns.len() == 1 {
            let _ = writeln!(out, "**Returns:** {}", format_json_short(returns[0]));
        } else {
            // Show range or representative values
            let _ = writeln!(out, "**Returns:** {}", summarize_return_values(&returns));
        }
    }
}

fn write_calls(out: &mut String, cluster: &AnnotatedCluster) {
    // Collect unique external calls across specimens
    let mut call_symbols: Vec<String> = Vec::new();
    for specimen in &cluster.specimens {
        for call in &specimen.calls_to_external {
            if !call_symbols.contains(&call.symbol) {
                call_symbols.push(call.symbol.clone());
            }
        }
    }
    if !call_symbols.is_empty() {
        for symbol in &call_symbols {
            let _ = writeln!(out, "**Calls:** {symbol}");
        }
    }
}

fn write_performance(out: &mut String, cluster: &AnnotatedCluster) {
    if cluster.specimens.is_empty() {
        return;
    }
    let times: Vec<f64> = cluster.specimens.iter().map(|s| s.wall_time_ms).collect();
    let heaps: Vec<i64> = cluster.specimens.iter().map(|s| s.heap_used_bytes).collect();

    let avg_time = times.iter().sum::<f64>() / times.len() as f64;
    let max_heap = heaps.iter().copied().max().unwrap_or(0);

    let _ = writeln!(
        out,
        "**Performance:** {avg_time:.1}ms avg, {heap}",
        heap = format_bytes(max_heap.max(0) as u64),
    );
}

fn write_edge_cases_section(out: &mut String, edge_cases: &[EdgeCase]) {
    if edge_cases.is_empty() {
        return;
    }
    out.push_str("## Edge Cases\n");
    for ec in edge_cases {
        let _ = writeln!(out, "- {} \u{2192} {}", ec.condition, ec.outcome);
    }
    out.push('\n');
}

fn write_dependencies_section(out: &mut String, deps: &[ExternalDependency]) {
    if deps.is_empty() {
        return;
    }
    out.push_str("## Dependencies\n");
    for dep in deps {
        let call_info = if dep.call_sites.is_empty() {
            String::new()
        } else {
            let count = dep.call_sites.len();
            let noun = if count == 1 { "site" } else { "sites" };
            format!(" ({count} call {noun})")
        };
        let _ = writeln!(out, "- {symbol}{info}", symbol = dep.symbol, info = call_info);
    }
    out.push('\n');
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_type(typ: &TypeInfo) -> String {
    match typ {
        TypeInfo::Int => "number".to_string(),
        TypeInfo::Float => "number".to_string(),
        TypeInfo::Str => "string".to_string(),
        TypeInfo::Bool => "boolean".to_string(),
        TypeInfo::Array { element } => format!("Array<{}>", format_type(element)),
        TypeInfo::Object { fields } => {
            if fields.is_empty() {
                "object".to_string()
            } else {
                let parts: Vec<String> = fields
                    .iter()
                    .map(|(name, typ)| format!("{name}: {}", format_type(typ)))
                    .collect();
                format!("{{ {} }}", parts.join(", "))
            }
        }
        TypeInfo::Union { variants } => {
            let parts: Vec<String> = variants.iter().map(format_type).collect();
            parts.join(" | ")
        }
        TypeInfo::Nullable { inner } => format!("{} | null", format_type(inner)),
        TypeInfo::Complex { kind, inner, .. } => {
            let base = format!("{kind:?}");
            if let Some(inner) = inner {
                format!("{base}<{}>", format_type(inner))
            } else {
                base
            }
        }
        TypeInfo::Opaque { label, .. } => format!("opaque({label})"),
        TypeInfo::Unknown => "unknown".to_string(),
    }
}

fn format_json_short(value: &serde_json::Value) -> String {
    let s = value.to_string();
    if s.len() > 60 {
        format!("{}...", &s[..57])
    } else {
        s
    }
}

fn summarize_return_values(values: &[&serde_json::Value]) -> String {
    // For numeric values, show range
    let numbers: Vec<f64> = values
        .iter()
        .filter_map(|v| v.as_f64())
        .collect();

    if numbers.len() == values.len() && !numbers.is_empty() {
        let min = numbers.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = numbers.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        if (min - max).abs() < f64::EPSILON {
            return format!("{min}");
        }
        return format!("{min}-{max}");
    }

    // Otherwise show first value as representative
    format_json_short(values[0])
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0B".to_string();
    }
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("<{}KB", bytes / 1024 + 1)
    } else {
        format!("<{}MB", bytes / (1024 * 1024) + 1)
    }
}

fn derive_when_condition(cluster: &AnnotatedCluster) -> Option<String> {
    // Use the cluster signature as the "when" condition if it's descriptive enough.
    // In future, this will be derived from path constraints.
    if cluster.signature.is_empty() {
        return None;
    }
    Some(cluster.signature.clone())
}

fn build_signature_from_behavior(behavior: &crate::behavior::Behavior) -> String {
    if let Some(ref error) = behavior.thrown_error {
        return format!("throws {}(\"{}\")", error.error_type, error.message);
    }
    match &behavior.return_value {
        Some(val) => format!("returns {}", format_json_short(val)),
        None => "returns void".to_string(),
    }
}

fn execution_record_from_behavior(
    function_id: &str,
    behavior: &crate::behavior::Behavior,
) -> ExecutionRecord {
    ExecutionRecord {
        function_id: function_id.to_string(),
        input_hash: 0,
        parameters: behavior.input_args.clone(),
        branch_path: behavior.branch_path.clone(),
        scope_events: vec![],
        lines_executed: vec![],
        calls_to_external: vec![],
        path_constraints: vec![],
        return_value: behavior.return_value.clone(),
        thrown_error: behavior.thrown_error.clone(),
        side_effects: behavior.side_effects.clone(),
        wall_time_ms: 0.0,
        cpu_time_us: 0,
        heap_used_bytes: 0,
        heap_allocated_bytes: 0,
        timestamp: String::new(),
        engine_version: String::new(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_record::{ErrorInfo, ExternalCall};
    use crate::invariants::{ComparisonOp, InvariantKind, InvariantTarget};
    use crate::nondeterminism::Confidence;
    use crate::protocol::{
        AdapterHint, AdapterRelation, DependencyKind, ExecutionAdapter, ExecutionAdapterApply,
        ExternalDependency,
    };
    use crate::types::{ParamInfo, TypeInfo};
    use serde_json::json;

    /// Helper: build a simple invariant with just a description.
    fn make_invariant(description: &str, target: InvariantTarget) -> Invariant {
        Invariant {
            description: description.to_string(),
            target,
            kind: InvariantKind::NumericComparison {
                path: vec![],
                op: ComparisonOp::Gt,
                value: 0.0,
            },
        }
    }

    fn make_analysis(
        name: &str,
        params: Vec<ParamInfo>,
        return_type: TypeInfo,
        deps: Vec<ExternalDependency>,
    ) -> FunctionAnalysis {
        FunctionAnalysis {
            name: name.to_string(),
            params,
            branches: vec![],
            dependencies: deps,
            return_type,
            start_line: 1,
            end_line: 50,
            exported: true,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: vec![],
            source_file: None,
            adapter_hints: vec![],
            invocation_model: crate::protocol::InvocationModel::Direct,
        }
    }

    fn make_record(
        function_id: &str,
        params: Vec<serde_json::Value>,
        return_value: Option<serde_json::Value>,
        thrown_error: Option<ErrorInfo>,
    ) -> ExecutionRecord {
        ExecutionRecord {
            function_id: function_id.to_string(),
            input_hash: 0,
            parameters: params,
            branch_path: vec![],
            scope_events: vec![],
            lines_executed: vec![],
            calls_to_external: vec![],
            path_constraints: vec![],
            return_value,
            thrown_error,
            side_effects: vec![],
            wall_time_ms: 0.1,
            cpu_time_us: 100,
            heap_used_bytes: 512,
            heap_allocated_bytes: 1024,
            timestamp: String::new(),
            engine_version: String::new(),
        }
    }

    #[test]
    fn function_heading_includes_signature() {
        let analysis = make_analysis(
            "calculateShipping",
            vec![
                ParamInfo { name: "order".to_string(), typ: TypeInfo::Object { fields: vec![] }, type_name: None },
            ],
            TypeInfo::Object { fields: vec![] },
            vec![],
        );
        let spec = FunctionSpec {
            analysis: &analysis,
            clusters: &[],
            edge_cases: &[],
            adapter_selection: None,
        };
        let md = generate_markdown(&spec);
        assert!(
            md.starts_with("# Function: calculateShipping(order: object): object\n"),
            "got: {md}"
        );
    }

    #[test]
    fn parameters_section_lists_params() {
        let analysis = make_analysis(
            "add",
            vec![
                ParamInfo { name: "a".to_string(), typ: TypeInfo::Int, type_name: None },
                ParamInfo { name: "b".to_string(), typ: TypeInfo::Int, type_name: None },
            ],
            TypeInfo::Int,
            vec![],
        );
        let spec = FunctionSpec {
            analysis: &analysis,
            clusters: &[],
            edge_cases: &[],
            adapter_selection: None,
        };
        let md = generate_markdown(&spec);
        assert!(md.contains("## Parameters\n"), "got: {md}");
        assert!(md.contains("- `a`: number\n"), "got: {md}");
        assert!(md.contains("- `b`: number\n"), "got: {md}");
    }

    #[test]
    fn no_parameters_section_when_empty() {
        let analysis = make_analysis("noop", vec![], TypeInfo::Unknown, vec![]);
        let spec = FunctionSpec {
            analysis: &analysis,
            clusters: &[],
            edge_cases: &[],
            adapter_selection: None,
        };
        let md = generate_markdown(&spec);
        assert!(!md.contains("## Parameters"), "got: {md}");
    }

    #[test]
    fn adapter_invocation_uses_synthetic_params_and_note() {
        let mut analysis = make_analysis("useTeamSwitch", vec![], TypeInfo::Unknown, vec![]);
        analysis.invocation_model = crate::protocol::InvocationModel::Adapter {
            adapter_id: "ts/react-hooks".to_string(),
            synthetic_params: vec![ParamInfo {
                name: "teamId".to_string(),
                typ: TypeInfo::Str,
                type_name: None,
            }],
            scenario_schema: None,
        };
        let spec = FunctionSpec {
            analysis: &analysis,
            clusters: &[],
            edge_cases: &[],
            adapter_selection: None,
        };
        let md = generate_markdown(&spec);
        assert!(md.contains("# Function: useTeamSwitch(teamId: string): unknown"), "got: {md}");
        assert!(md.contains("_Invocation: adapter-owned via `ts/react-hooks`_"), "got: {md}");
    }

    #[test]
    fn behavior_section_with_return_value() {
        let analysis = make_analysis("classify", vec![], TypeInfo::Str, vec![]);
        let cluster = AnnotatedCluster {
            id: 0,
            signature: "input is positive".to_string(),
            specimens: vec![make_record("classify", vec![json!(5)], Some(json!("positive")), None)],
            input_invariants: vec![make_invariant("x > 0", InvariantTarget::Input)],
            output_invariants: vec![],
            side_effect_summary: vec![],
        };
        let spec = FunctionSpec {
            analysis: &analysis,
            clusters: &[cluster],
            edge_cases: &[],
            adapter_selection: None,
        };
        let md = generate_markdown(&spec);

        assert!(md.contains("## Behavior 1: input is positive\n"), "got: {md}");
        assert!(md.contains("**When:** input is positive\n"), "got: {md}");
        assert!(md.contains("**Returns:** \"positive\"\n"), "got: {md}");
        assert!(md.contains("**Invariant:** x > 0\n"), "got: {md}");
        assert!(md.contains("**Performance:** 0.1ms avg"), "got: {md}");
    }

    #[test]
    fn behavior_section_with_thrown_error() {
        let analysis = make_analysis("validate", vec![], TypeInfo::Unknown, vec![]);
        let cluster = AnnotatedCluster {
            id: 0,
            signature: "input is null".to_string(),
            specimens: vec![make_record(
                "validate",
                vec![json!(null)],
                None,
                Some(ErrorInfo {
                    error_type: "TypeError".to_string(),
                    message: "input is null".to_string(),
                    stack: None, error_category: None }),
            )],
            input_invariants: vec![],
            output_invariants: vec![make_invariant("always throws, never returns", InvariantTarget::Output)],
            side_effect_summary: vec![],
        };
        let spec = FunctionSpec {
            analysis: &analysis,
            clusters: &[cluster],
            edge_cases: &[],
            adapter_selection: None,
        };
        let md = generate_markdown(&spec);

        assert!(md.contains("**Throws:** TypeError(\"input is null\")\n"), "got: {md}");
        assert!(md.contains("**Invariant:** always throws, never returns\n"), "got: {md}");
    }

    #[test]
    fn behavior_section_with_external_calls() {
        let analysis = make_analysis("checkout", vec![], TypeInfo::Unknown, vec![
            ExternalDependency {
                kind: DependencyKind::FunctionCall,
                symbol: "rateService.getExpressRate".to_string(),
                source_module: String::new(),
                return_type: TypeInfo::Float,
                param_types: vec![],
                call_sites: vec![15],
            },
        ]);
        let mut record = make_record("checkout", vec![], Some(json!(12.99)), None);
        record.calls_to_external = vec![ExternalCall {
            symbol: "rateService.getExpressRate".to_string(),
            args: vec![json!("90210")],
            return_value: json!(12.99),
        }];
        let cluster = AnnotatedCluster {
            id: 0,
            signature: "express shipping".to_string(),
            specimens: vec![record],
            input_invariants: vec![],
            output_invariants: vec![],
            side_effect_summary: vec![],
        };
        let spec = FunctionSpec {
            analysis: &analysis,
            clusters: &[cluster],
            edge_cases: &[],
            adapter_selection: None,
        };
        let md = generate_markdown(&spec);
        assert!(md.contains("**Calls:** rateService.getExpressRate\n"), "got: {md}");
    }

    #[test]
    fn edge_cases_section() {
        let analysis = make_analysis("process", vec![], TypeInfo::Unknown, vec![]);
        let edge_cases = vec![
            EdgeCase {
                condition: "Empty items array".to_string(),
                outcome: "returns { cost: 0, method: \"none\" }".to_string(),
            },
            EdgeCase {
                condition: "Null destination".to_string(),
                outcome: "throws TypeError".to_string(),
            },
        ];
        let spec = FunctionSpec {
            analysis: &analysis,
            clusters: &[],
            edge_cases: &edge_cases,
            adapter_selection: None,
        };
        let md = generate_markdown(&spec);
        assert!(md.contains("## Edge Cases\n"), "got: {md}");
        assert!(md.contains("- Empty items array \u{2192} returns { cost: 0, method: \"none\" }\n"), "got: {md}");
        assert!(md.contains("- Null destination \u{2192} throws TypeError\n"), "got: {md}");
    }

    #[test]
    fn no_edge_cases_section_when_empty() {
        let analysis = make_analysis("noop", vec![], TypeInfo::Unknown, vec![]);
        let spec = FunctionSpec {
            analysis: &analysis,
            clusters: &[],
            edge_cases: &[],
            adapter_selection: None,
        };
        let md = generate_markdown(&spec);
        assert!(!md.contains("## Edge Cases"), "got: {md}");
    }

    #[test]
    fn dependencies_section() {
        let deps = vec![
            ExternalDependency {
                kind: DependencyKind::FunctionCall,
                symbol: "rateService.getExpressRate".to_string(),
                source_module: "rate-service".to_string(),
                return_type: TypeInfo::Float,
                param_types: vec![TypeInfo::Str],
                call_sites: vec![15],
            },
            ExternalDependency {
                kind: DependencyKind::FunctionCall,
                symbol: "taxService.calculate".to_string(),
                source_module: "tax-service".to_string(),
                return_type: TypeInfo::Float,
                param_types: vec![],
                call_sites: vec![20, 25],
            },
        ];
        let analysis = make_analysis("checkout", vec![], TypeInfo::Unknown, deps);
        let spec = FunctionSpec {
            analysis: &analysis,
            clusters: &[],
            edge_cases: &[],
            adapter_selection: None,
        };
        let md = generate_markdown(&spec);
        assert!(md.contains("## Dependencies\n"), "got: {md}");
        assert!(md.contains("- rateService.getExpressRate (1 call site)\n"), "got: {md}");
        assert!(md.contains("- taxService.calculate (2 call sites)\n"), "got: {md}");
    }

    #[test]
    fn no_dependencies_section_when_empty() {
        let analysis = make_analysis("noop", vec![], TypeInfo::Unknown, vec![]);
        let spec = FunctionSpec {
            analysis: &analysis,
            clusters: &[],
            edge_cases: &[],
            adapter_selection: None,
        };
        let md = generate_markdown(&spec);
        assert!(!md.contains("## Dependencies"), "got: {md}");
    }

    #[test]
    fn adapter_hints_section_lists_generic_hint_details() {
        let mut analysis = make_analysis("classify", vec![], TypeInfo::Unknown, vec![]);
        analysis.adapter_hints = vec![AdapterHint {
            adapter: ExecutionAdapter {
                id: "ts/browser-globals".into(),
                apply: Some(ExecutionAdapterApply::Suggest),
                options: None,
            },
            confidence: Confidence::Medium,
            reasons: vec!["uses window and document".into()],
            requirements: vec![AdapterRelation {
                adapter_id: "ts/dom-runtime".into(),
                reason: Some("needs DOM globals".into()),
            }],
            conflicts: vec![AdapterRelation {
                adapter_id: "ts/node-only".into(),
                reason: Some("mutually exclusive runtime".into()),
            }],
        }];
        let spec = FunctionSpec {
            analysis: &analysis,
            clusters: &[],
            edge_cases: &[],
            adapter_selection: None,
        };

        let md = generate_markdown(&spec);
        assert!(md.contains("## Adapter Hints\n"), "got: {md}");
        assert!(md.contains("`ts/browser-globals` (suggest) [medium]"), "got: {md}");
        assert!(md.contains("reasons: uses window and document"), "got: {md}");
        assert!(md.contains("requirements: ts/dom-runtime (needs DOM globals)"), "got: {md}");
        assert!(md.contains("conflicts: ts/node-only (mutually exclusive runtime)"), "got: {md}");
    }

    #[test]
    fn adapter_selection_section_distinguishes_active_from_suggested() {
        use crate::adapter_selection::{
            AdapterSelectionResult, SelectedAdapter, SelectionProvenance, SuggestedAdapter,
        };

        let analysis = make_analysis("myFunc", vec![], TypeInfo::Unknown, vec![]);
        let selection = AdapterSelectionResult {
            active: vec![SelectedAdapter {
                adapter: ExecutionAdapter {
                    id: "ts/module-resolution".into(),
                    apply: Some(ExecutionAdapterApply::Required),
                    options: None,
                },
                provenance: SelectionProvenance::ExplicitConfig,
                reasons: vec!["user configured".into()],
            }],
            suggested: vec![SuggestedAdapter {
                adapter: ExecutionAdapter {
                    id: "ts/react-hooks".into(),
                    apply: Some(ExecutionAdapterApply::Suggest),
                    options: None,
                },
                confidence: Confidence::Medium,
                reasons: vec!["imports react".into(), "calls useCallback".into()],
            }],
            rejected: vec![],
        };
        let spec = FunctionSpec {
            analysis: &analysis,
            clusters: &[],
            edge_cases: &[],
            adapter_selection: Some(&selection),
        };

        let md = generate_markdown(&spec);
        assert!(md.contains("## Active Adapters\n"), "got: {md}");
        assert!(md.contains("`ts/module-resolution` (config)"), "got: {md}");
        assert!(md.contains("reasons: user configured"), "got: {md}");
        assert!(md.contains("## Suggested Adapters\n"), "got: {md}");
        assert!(md.contains("`ts/react-hooks` [medium]"), "got: {md}");
        assert!(md.contains("reasons: imports react; calls useCallback"), "got: {md}");
        // Should NOT contain the old "Adapter Hints" section.
        assert!(!md.contains("## Adapter Hints"), "got: {md}");
    }

    #[test]
    fn full_spec_matches_expected_format() {
        let analysis = make_analysis(
            "calculateShipping",
            vec![
                ParamInfo {
                    name: "order".to_string(),
                    typ: TypeInfo::Object {
                        fields: vec![
                            ("items".to_string(), TypeInfo::Array { element: Box::new(TypeInfo::Unknown) }),
                            ("priority".to_string(), TypeInfo::Str),
                        ],
                    },
                    type_name: None,
                },
            ],
            TypeInfo::Object { fields: vec![] },
            vec![
                ExternalDependency {
                    kind: DependencyKind::FunctionCall,
                    symbol: "rateService.getExpressRate".to_string(),
                    source_module: String::new(),
                    return_type: TypeInfo::Float,
                    param_types: vec![],
                    call_sites: vec![10],
                },
            ],
        );

        let clusters = vec![
            AnnotatedCluster {
                id: 0,
                signature: "free shipping for large orders".to_string(),
                specimens: vec![make_record(
                    "calculateShipping",
                    vec![json!({"items": [1,2,3,4,5], "priority": "standard"})],
                    Some(json!({"cost": 0, "method": "standard"})),
                    None,
                )],
                input_invariants: vec![],
                output_invariants: vec![make_invariant("cost is always 0", InvariantTarget::Output)],
                side_effect_summary: vec![],
            },
            AnnotatedCluster {
                id: 1,
                signature: "express shipping calculation".to_string(),
                specimens: vec![{
                    let mut r = make_record(
                        "calculateShipping",
                        vec![json!({"items": [1], "priority": "express"})],
                        Some(json!({"cost": 12.99, "method": "express"})),
                        None,
                    );
                    r.wall_time_ms = 0.3;
                    r.heap_used_bytes = 2000;
                    r.calls_to_external = vec![ExternalCall {
                        symbol: "rateService.getExpressRate".to_string(),
                        args: vec![json!("90210")],
                        return_value: json!(12.99),
                    }];
                    r
                }],
                input_invariants: vec![],
                output_invariants: vec![],
                side_effect_summary: vec![],
            },
        ];

        let edge_cases = vec![EdgeCase {
            condition: "Empty items array".to_string(),
            outcome: "returns { cost: 0, method: \"none\" }".to_string(),
        }];

        let spec = FunctionSpec {
            analysis: &analysis,
            clusters: &clusters,
            edge_cases: &edge_cases,
            adapter_selection: None,
        };

        let md = generate_markdown(&spec);

        // Verify structure
        assert!(md.contains("# Function: calculateShipping("), "missing heading: {md}");
        assert!(md.contains("## Parameters\n"), "missing parameters: {md}");
        assert!(md.contains("## Behavior 1: free shipping for large orders\n"), "missing behavior 1: {md}");
        assert!(md.contains("## Behavior 2: express shipping calculation\n"), "missing behavior 2: {md}");
        assert!(md.contains("**Invariant:** cost is always 0\n"), "missing invariant: {md}");
        assert!(md.contains("**Calls:** rateService.getExpressRate\n"), "missing calls: {md}");
        assert!(md.contains("## Edge Cases\n"), "missing edge cases: {md}");
        assert!(md.contains("## Dependencies\n"), "missing dependencies: {md}");
    }

    #[test]
    fn generate_from_behavior_map_produces_sections() {
        use crate::behavior::{Behavior, BehaviorMap};

        let analysis = make_analysis(
            "abs",
            vec![ParamInfo { name: "x".to_string(), typ: TypeInfo::Int, type_name: None }],
            TypeInfo::Int,
            vec![],
        );
        let map = BehaviorMap {
            function_id: "abs".to_string(),
            behaviors: vec![
                Behavior {
                    id: 0,
                    input_args: vec![json!(5)],
                    return_value: Some(json!(5)),
                    thrown_error: None,
                    branch_path: vec![],
                    side_effects: vec![],
                    dependency_trace: None,
                    mock_values: vec![],
                },
                Behavior {
                    id: 1,
                    input_args: vec![json!(-3)],
                    return_value: Some(json!(3)),
                    thrown_error: None,
                    branch_path: vec![],
                    side_effects: vec![],
                    dependency_trace: None,
                    mock_values: vec![],
                },
            ],
            fingerprint: None,
            nondeterministic_fields: vec![],
        };

        let md = generate_markdown_from_behavior_map(&analysis, &map);
        assert!(md.contains("# Function: abs(x: number): number\n"), "got: {md}");
        assert!(md.contains("## Behavior 1:"), "got: {md}");
        assert!(md.contains("## Behavior 2:"), "got: {md}");
    }

    #[test]
    fn format_type_handles_all_variants() {
        assert_eq!(format_type(&TypeInfo::Int), "number");
        assert_eq!(format_type(&TypeInfo::Float), "number");
        assert_eq!(format_type(&TypeInfo::Str), "string");
        assert_eq!(format_type(&TypeInfo::Bool), "boolean");
        assert_eq!(format_type(&TypeInfo::Unknown), "unknown");
        assert_eq!(
            format_type(&TypeInfo::Array { element: Box::new(TypeInfo::Int) }),
            "Array<number>"
        );
        assert_eq!(
            format_type(&TypeInfo::Nullable { inner: Box::new(TypeInfo::Str) }),
            "string | null"
        );
        assert_eq!(
            format_type(&TypeInfo::Union { variants: vec![TypeInfo::Str, TypeInfo::Int] }),
            "string | number"
        );
        assert_eq!(
            format_type(&TypeInfo::Object { fields: vec![("x".to_string(), TypeInfo::Int)] }),
            "{ x: number }"
        );
        assert_eq!(format_type(&TypeInfo::Object { fields: vec![] }), "object");
        assert_eq!(
            format_type(&TypeInfo::Opaque { label: "net.Socket".to_string(), static_opacity: None, medium_opacity: None }),
            "opaque(net.Socket)"
        );
    }

    #[test]
    fn format_bytes_produces_human_readable() {
        assert_eq!(format_bytes(0), "0B");
        assert_eq!(format_bytes(500), "500B");
        assert_eq!(format_bytes(1024), "<2KB");
        assert_eq!(format_bytes(2000), "<2KB");
        assert_eq!(format_bytes(1024 * 1024), "<2MB");
    }

    #[test]
    fn summarize_return_values_numeric_range() {
        let vals = vec![json!(5.0), json!(10.0), json!(15.0)];
        let refs: Vec<&serde_json::Value> = vals.iter().collect();
        let summary = summarize_return_values(&refs);
        assert_eq!(summary, "5-15");
    }

    #[test]
    fn summarize_return_values_single_numeric() {
        let vals = vec![json!(42.0)];
        let refs: Vec<&serde_json::Value> = vals.iter().collect();
        let summary = summarize_return_values(&refs);
        assert_eq!(summary, "42");
    }

    #[test]
    fn summarize_return_values_non_numeric_uses_first() {
        let vals = vec![json!("hello"), json!("world")];
        let refs: Vec<&serde_json::Value> = vals.iter().collect();
        let summary = summarize_return_values(&refs);
        assert_eq!(summary, "\"hello\"");
    }

    #[test]
    fn multiple_invariants_joined_with_semicolon() {
        let analysis = make_analysis("f", vec![], TypeInfo::Unknown, vec![]);
        let cluster = AnnotatedCluster {
            id: 0,
            signature: "normal case".to_string(),
            specimens: vec![make_record("f", vec![], Some(json!(1)), None)],
            input_invariants: vec![
                make_invariant("x > 0", InvariantTarget::Input),
                make_invariant("x < 100", InvariantTarget::Input),
            ],
            output_invariants: vec![
                make_invariant("result >= x", InvariantTarget::Output),
            ],
            side_effect_summary: vec![],
        };
        let spec = FunctionSpec {
            analysis: &analysis,
            clusters: &[cluster],
            edge_cases: &[],
            adapter_selection: None,
        };
        let md = generate_markdown(&spec);
        assert!(md.contains("**Invariant:** x > 0; x < 100; result >= x\n"), "got: {md}");
    }

    #[test]
    fn invariant_round_trips() {
        let inv = make_invariant("x > 0", InvariantTarget::Input);
        let json = serde_json::to_string(&inv).expect("serialize");
        let deserialized: Invariant = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(inv, deserialized);
    }

    #[test]
    fn annotated_cluster_round_trips() {
        let cluster = AnnotatedCluster {
            id: 0,
            signature: "returns positive".to_string(),
            specimens: vec![],
            input_invariants: vec![make_invariant("x > 0", InvariantTarget::Input)],
            output_invariants: vec![],
            side_effect_summary: vec!["console.log".to_string()],
        };
        let json = serde_json::to_string(&cluster).expect("serialize");
        let deserialized: AnnotatedCluster = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cluster, deserialized);
    }

    #[test]
    fn edge_case_round_trips() {
        let ec = EdgeCase {
            condition: "empty input".to_string(),
            outcome: "returns null".to_string(),
        };
        let json = serde_json::to_string(&ec).expect("serialize");
        let deserialized: EdgeCase = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(ec, deserialized);
    }
}
