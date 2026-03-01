//! Core sample selection: stratified proportional sampling.
//!
//! Given a set of functions and a call graph, selects a representative subset
//! using stratified proportional sampling across four axes:
//! 1. Module/directory — parent directory of the source file
//! 2. Complexity tier — branch count buckets (0-1, 2-5, 6-15, 16+)
//! 3. Dependency depth — topological layer from the call graph
//! 4. Function kind — pure, I/O, constructor, handler
//!
//! Selection within each stratum uses a stable hash of (file_path, function_name, seed)
//! for deterministic, reproducible results. Dependency closure ensures that if a
//! selected function calls another project function, the callee is also included
//! (without counting against the budget).

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

use crate::batch_analyze::FunctionEntry;
use crate::call_graph::CallGraph;

/// How many functions to sample.
#[derive(Debug, Clone)]
pub enum SampleBudget {
    /// A percentage of total functions (0.0..=100.0).
    Percentage(f64),
    /// An absolute function count.
    Absolute(usize),
}

/// Configuration for core sample selection.
#[derive(Debug, Clone)]
pub struct CoreSampleConfig {
    /// Budget: how many functions to select.
    pub budget: SampleBudget,
    /// Seed for deterministic selection.
    pub seed: u64,
    /// Root directory of the scan (for computing relative module paths).
    pub scan_root: String,
}

/// Result of core sample selection.
#[derive(Debug)]
pub struct CoreSampleResult {
    /// Qualified names of functions selected by sampling.
    pub selected: HashSet<String>,
    /// Additional functions included for dependency closure (not counted against budget).
    pub dependency_closure: HashSet<String>,
    /// Per-stratum breakdown for reporting.
    pub strata_summary: Vec<StratumInfo>,
}

impl CoreSampleResult {
    /// All functions that should be included (selected + closure).
    pub fn all_included(&self) -> HashSet<String> {
        self.selected
            .union(&self.dependency_closure)
            .cloned()
            .collect()
    }
}

/// Summary of one stratum for reporting.
#[derive(Debug, Clone)]
pub struct StratumInfo {
    /// Human-readable label, e.g. "src/auth | simple(2-5) | depth=1 | pure".
    pub label: String,
    /// Total functions in this stratum.
    pub total: usize,
    /// Functions sampled from this stratum.
    pub sampled: usize,
    /// Names of sampled functions.
    pub names: Vec<String>,
}

/// Complexity tier based on branch count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ComplexityTier {
    Trivial,   // 0-1 branches
    Simple,    // 2-5 branches
    Moderate,  // 6-15 branches
    Complex,   // 16+ branches
}

impl ComplexityTier {
    fn from_branch_count(count: usize) -> Self {
        match count {
            0..=1 => Self::Trivial,
            2..=5 => Self::Simple,
            6..=15 => Self::Moderate,
            _ => Self::Complex,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Trivial => "trivial(0-1)",
            Self::Simple => "simple(2-5)",
            Self::Moderate => "moderate(6-15)",
            Self::Complex => "complex(16+)",
        }
    }
}

/// Function kind classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FunctionKind {
    Pure,
    Io,
    Constructor,
    Handler,
}

impl FunctionKind {
    fn classify(entry: &FunctionEntry) -> Self {
        let name_lower = entry.name.to_lowercase();
        // Extract just the function name (after last :: if qualified).
        let short_name = name_lower.rsplit("::").next().unwrap_or(&name_lower);

        // Check for known function name patterns in order of specificity.
        if short_name.contains("handle")
            || short_name.contains("on_")
            || short_name.contains("process")
        {
            return Self::Handler;
        }
        if short_name.starts_with("new")
            || short_name.starts_with("create")
            || short_name.starts_with("init")
            || short_name.starts_with("build")
            || short_name.starts_with("make")
        {
            return Self::Constructor;
        }
        // I/O: has external dependencies or common I/O-related names.
        if !entry.dependencies.is_empty()
            || short_name.contains("read")
            || short_name.contains("write")
            || short_name.contains("fetch")
            || short_name.contains("send")
            || short_name.contains("save")
            || short_name.contains("load")
            || short_name.contains("delete")
        {
            return Self::Io;
        }
        Self::Pure
    }

    fn label(self) -> &'static str {
        match self {
            Self::Pure => "pure",
            Self::Io => "io",
            Self::Constructor => "constructor",
            Self::Handler => "handler",
        }
    }
}

/// Composite stratum key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct StratumKey {
    module: String,
    complexity: ComplexityTier,
    depth: usize,
    kind: FunctionKind,
}

impl StratumKey {
    fn label(&self) -> String {
        format!(
            "{} | {} | depth={} | {}",
            self.module,
            self.complexity.label(),
            self.depth,
            self.kind.label(),
        )
    }
}

/// Parse a core-sample argument like "50%", "50", "10%", "200".
pub fn parse_sample_budget(s: &str) -> Result<SampleBudget, String> {
    let s = s.trim();
    if let Some(pct) = s.strip_suffix('%') {
        let val: f64 = pct
            .trim()
            .parse()
            .map_err(|_| format!("invalid percentage: {s}"))?;
        if !(0.0..=100.0).contains(&val) {
            return Err(format!("percentage must be 0-100, got {val}"));
        }
        Ok(SampleBudget::Percentage(val))
    } else {
        let val: usize = s.parse().map_err(|_| format!("invalid count: {s}"))?;
        Ok(SampleBudget::Absolute(val))
    }
}

/// Compute a default seed from a project directory and optional git HEAD.
pub fn default_seed(scan_dir: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    scan_dir.hash(&mut hasher);
    // Try to incorporate git HEAD for reproducibility across unchanged commits.
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(scan_dir)
        .output()
        && output.status.success()
    {
        output.stdout.hash(&mut hasher);
    }
    hasher.finish()
}

/// Select a representative core sample from the given functions.
///
/// The algorithm:
/// 1. Classify each function into a stratum (module x complexity x depth x kind).
/// 2. Allocate the budget proportionally across non-empty strata (min 1 each).
/// 3. Select within each stratum by stable hash ordering.
/// 4. Add transitive callees of selected functions (dependency closure).
pub fn select_core_sample(
    entries: &[FunctionEntry],
    call_graph: &CallGraph,
    config: &CoreSampleConfig,
) -> CoreSampleResult {
    if entries.is_empty() {
        return CoreSampleResult {
            selected: HashSet::new(),
            dependency_closure: HashSet::new(),
            strata_summary: Vec::new(),
        };
    }

    // Compute the depth layer for each function.
    let layers = call_graph.topological_layers();
    let mut depth_map: HashMap<&str, usize> = HashMap::new();
    for (layer_idx, layer) in layers.iter().enumerate() {
        for name in layer {
            depth_map.insert(name.as_str(), layer_idx);
        }
    }

    // Build qualified name -> entry mapping.
    let entry_map: HashMap<&str, &FunctionEntry> = entries
        .iter()
        .map(|e| (e.name.as_str(), e))
        .collect();

    // Classify each function into a stratum.
    let mut strata: HashMap<StratumKey, Vec<&str>> = HashMap::new();
    for entry in entries {
        let module = module_from_path(&entry.file_path, &config.scan_root);
        let complexity = ComplexityTier::from_branch_count(entry.branch_count);
        let depth = depth_map.get(entry.name.as_str()).copied().unwrap_or(0);
        let kind = FunctionKind::classify(entry);

        let key = StratumKey {
            module,
            complexity,
            depth,
            kind,
        };
        strata.entry(key).or_default().push(&entry.name);
    }

    // Compute total budget.
    let total = entries.len();
    let raw_budget = match config.budget {
        SampleBudget::Percentage(pct) => ((pct / 100.0) * total as f64).round() as usize,
        SampleBudget::Absolute(n) => n,
    };
    let budget = raw_budget.min(total).max(1);

    // If budget >= total, select everything.
    if budget >= total {
        let selected: HashSet<String> = entries.iter().map(|e| e.name.clone()).collect();
        let strata_summary = build_strata_summary(&strata, &selected);
        return CoreSampleResult {
            selected,
            dependency_closure: HashSet::new(),
            strata_summary,
        };
    }

    // Allocate budget proportionally using largest-remainder method.
    let allocations = allocate_budget(&strata, budget);

    // Select within each stratum by stable hash.
    let mut selected = HashSet::new();
    let mut strata_summary = Vec::new();

    let mut sorted_keys: Vec<&StratumKey> = allocations.keys().collect();
    sorted_keys.sort_by_key(|k| k.label());

    for key in sorted_keys {
        let allocation = allocations[key];
        let members = &strata[key];

        // Sort members by stable hash for deterministic selection.
        let mut scored: Vec<(&str, u64)> = members
            .iter()
            .map(|&name| {
                let file_path = entry_map
                    .get(name)
                    .map_or("", |e| e.file_path.to_str().unwrap_or(""));
                (name, stable_hash(file_path, name, config.seed))
            })
            .collect();
        scored.sort_by_key(|&(_, hash)| hash);

        let chosen: Vec<String> = scored
            .iter()
            .take(allocation)
            .map(|&(name, _)| name.to_string())
            .collect();

        for name in &chosen {
            selected.insert(name.clone());
        }

        strata_summary.push(StratumInfo {
            label: key.label(),
            total: members.len(),
            sampled: chosen.len(),
            names: chosen,
        });
    }

    // Dependency closure: add transitive callees.
    let dependency_closure = compute_dependency_closure(&selected, call_graph);

    CoreSampleResult {
        selected,
        dependency_closure,
        strata_summary,
    }
}

/// Extract a module label from a file path relative to the scan root.
fn module_from_path(file_path: &Path, scan_root: &str) -> String {
    let rel = file_path.strip_prefix(scan_root).unwrap_or(file_path);
    // Use the parent directory as the module label.
    rel.parent()
        .map(|p| {
            let s = p.to_string_lossy().to_string();
            if s.is_empty() {
                ".".to_string()
            } else {
                s
            }
        })
        .unwrap_or_else(|| ".".to_string())
}

/// Compute a stable hash for deterministic selection within a stratum.
fn stable_hash(file_path: &str, function_name: &str, seed: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    file_path.hash(&mut hasher);
    function_name.hash(&mut hasher);
    hasher.finish()
}

/// Allocation entry: (key, floor allocation, fractional remainder).
struct Allocation {
    key: StratumKey,
    count: usize,
    remainder: f64,
}

/// Allocate budget across strata using the largest-remainder method.
///
/// Each non-empty stratum gets at least 1. The remaining budget is distributed
/// proportionally, with ties broken by fractional remainder.
fn allocate_budget(
    strata: &HashMap<StratumKey, Vec<&str>>,
    budget: usize,
) -> HashMap<StratumKey, usize> {
    let non_empty: Vec<(&StratumKey, usize)> = strata
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| (k, v.len()))
        .collect();

    let num_strata = non_empty.len();
    if num_strata == 0 {
        return HashMap::new();
    }

    // If budget can't even give 1 per stratum, give 1 to the largest strata.
    if budget < num_strata {
        let mut by_size: Vec<(&StratumKey, usize)> = non_empty;
        by_size.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.label().cmp(&b.0.label())));
        return by_size
            .into_iter()
            .take(budget)
            .map(|(k, _)| (k.clone(), 1))
            .collect();
    }

    let total: usize = non_empty.iter().map(|(_, sz)| sz).sum();

    // Give each stratum floor(proportion * budget), minimum 1.
    // Then distribute remainders.
    let mut allocations: Vec<Allocation> = Vec::new();
    let mut allocated = 0usize;

    for (key, size) in &non_empty {
        let exact = (*size as f64 / total as f64) * budget as f64;
        let floored = (exact.floor() as usize).max(1).min(*size);
        let remainder = exact - floored as f64;
        allocated += floored;
        allocations.push(Allocation {
            key: (*key).clone(),
            count: floored,
            remainder,
        });
    }

    // Distribute leftover budget by largest remainder.
    let leftover = budget.saturating_sub(allocated);
    if leftover > 0 {
        allocations.sort_by(|a, b| {
            b.remainder
                .partial_cmp(&a.remainder)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for alloc in allocations.iter_mut().take(leftover) {
            let stratum_size = strata[&alloc.key].len();
            if alloc.count < stratum_size {
                alloc.count += 1;
            }
        }
    }

    allocations
        .into_iter()
        .map(|a| (a.key, a.count))
        .collect()
}

/// Compute the transitive closure of callees for the selected set.
fn compute_dependency_closure(
    selected: &HashSet<String>,
    call_graph: &CallGraph,
) -> HashSet<String> {
    let mut closure = HashSet::new();
    let mut stack: Vec<String> = selected.iter().cloned().collect();

    while let Some(func) = stack.pop() {
        for callee in call_graph.callees_of(&func) {
            let callee_str = callee.to_string();
            if !selected.contains(&callee_str) && closure.insert(callee_str.clone()) {
                stack.push(callee_str);
            }
        }
    }

    closure
}

/// Build strata summary when all functions are selected (budget >= total).
fn build_strata_summary(
    strata: &HashMap<StratumKey, Vec<&str>>,
    selected: &HashSet<String>,
) -> Vec<StratumInfo> {
    let mut summary: Vec<StratumInfo> = strata
        .iter()
        .map(|(key, members)| {
            let names: Vec<String> = members
                .iter()
                .filter(|n| selected.contains(**n))
                .map(|n| n.to_string())
                .collect();
            StratumInfo {
                label: key.label(),
                total: members.len(),
                sampled: names.len(),
                names,
            }
        })
        .collect();
    summary.sort_by(|a, b| a.label.cmp(&b.label));
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch_analyze::{FunctionEntry, FunctionRegistry};
    use crate::types::TypeInfo;
    use std::path::PathBuf;

    fn make_entry(
        name: &str,
        file: &str,
        branch_count: usize,
        deps: Vec<String>,
    ) -> FunctionEntry {
        FunctionEntry {
            file_path: PathBuf::from(file),
            name: name.to_string(),
            exported: true,
            params: vec![],
            return_type: TypeInfo::Int,
            dependencies: deps
                .into_iter()
                .map(|d| crate::protocol::ExternalDependency {
                    symbol: d,
                    kind: crate::protocol::DependencyKind::FunctionCall,
                    source_module: String::new(),
                    return_type: TypeInfo::Int,
                    param_types: vec![],
                    call_sites: vec![],
                })
                .collect(),
            branch_count,
        }
    }

    fn make_registry(entries: Vec<FunctionEntry>) -> FunctionRegistry {
        let mut index = std::collections::HashMap::new();
        for (i, e) in entries.iter().enumerate() {
            index.insert(e.name.clone(), i);
        }
        FunctionRegistry::from_raw(entries, index)
    }

    #[test]
    fn test_parse_budget_percentage() {
        match parse_sample_budget("50%").unwrap() {
            SampleBudget::Percentage(p) => assert!((p - 50.0).abs() < f64::EPSILON),
            _ => panic!("expected percentage"),
        }
    }

    #[test]
    fn test_parse_budget_absolute() {
        match parse_sample_budget("42").unwrap() {
            SampleBudget::Absolute(n) => assert_eq!(n, 42),
            _ => panic!("expected absolute"),
        }
    }

    #[test]
    fn test_parse_budget_invalid() {
        assert!(parse_sample_budget("abc").is_err());
        assert!(parse_sample_budget("150%").is_err());
    }

    #[test]
    fn test_stable_selection() {
        let entries = vec![
            make_entry("fn_a", "/src/a.ts", 0, vec![]),
            make_entry("fn_b", "/src/b.ts", 3, vec![]),
            make_entry("fn_c", "/src/c.ts", 8, vec![]),
            make_entry("fn_d", "/src/d.ts", 20, vec![]),
        ];
        let registry = make_registry(entries.clone());
        let cg = CallGraph::from_registry(&registry);
        let config = CoreSampleConfig {
            budget: SampleBudget::Percentage(50.0),
            seed: 12345,
            scan_root: "/".to_string(),
        };

        let r1 = select_core_sample(&entries, &cg, &config);
        let r2 = select_core_sample(&entries, &cg, &config);
        assert_eq!(
            r1.selected, r2.selected,
            "same seed should produce same selection"
        );
    }

    #[test]
    fn test_different_seed_different_selection() {
        let entries: Vec<FunctionEntry> = (0..20)
            .map(|i| {
                make_entry(
                    &format!("fn_{i}"),
                    &format!("/src/mod{}/f.ts", i % 4),
                    i % 10,
                    vec![],
                )
            })
            .collect();
        let registry = make_registry(entries.clone());
        let cg = CallGraph::from_registry(&registry);

        let r1 = select_core_sample(
            &entries,
            &cg,
            &CoreSampleConfig {
                budget: SampleBudget::Percentage(30.0),
                seed: 111,
                scan_root: "/".to_string(),
            },
        );
        let r2 = select_core_sample(
            &entries,
            &cg,
            &CoreSampleConfig {
                budget: SampleBudget::Percentage(30.0),
                seed: 999,
                scan_root: "/".to_string(),
            },
        );
        assert_ne!(
            r1.selected, r2.selected,
            "different seeds should likely produce different selections"
        );
    }

    #[test]
    fn test_dependency_closure() {
        let entries = vec![
            make_entry("fn_a", "/src/a.ts", 5, vec!["fn_b".into()]),
            make_entry("fn_b", "/src/b.ts", 0, vec!["fn_c".into()]),
            make_entry("fn_c", "/src/c.ts", 0, vec![]),
            make_entry("fn_d", "/src/d.ts", 0, vec![]),
        ];
        let registry = make_registry(entries.clone());
        let cg = CallGraph::from_registry(&registry);

        // Budget=1 to select just one function.
        let config = CoreSampleConfig {
            budget: SampleBudget::Absolute(1),
            seed: 0,
            scan_root: "/".to_string(),
        };
        let result = select_core_sample(&entries, &cg, &config);
        let all = result.all_included();

        // Whatever was selected, its transitive callees should be in closure.
        if result.selected.contains("fn_a") {
            assert!(
                all.contains("fn_b"),
                "fn_b should be in closure (callee of fn_a)"
            );
            assert!(
                all.contains("fn_c"),
                "fn_c should be in closure (transitive callee)"
            );
        }
    }

    #[test]
    fn test_budget_exceeds_total() {
        let entries = vec![
            make_entry("fn_a", "/src/a.ts", 0, vec![]),
            make_entry("fn_b", "/src/b.ts", 0, vec![]),
        ];
        let registry = make_registry(entries.clone());
        let cg = CallGraph::from_registry(&registry);
        let config = CoreSampleConfig {
            budget: SampleBudget::Absolute(100),
            seed: 0,
            scan_root: "/".to_string(),
        };
        let result = select_core_sample(&entries, &cg, &config);
        assert_eq!(result.selected.len(), 2, "budget > total should select all");
    }

    #[test]
    fn test_min_one_per_stratum() {
        // 4 functions each in a different complexity tier -> 4 strata.
        let entries = vec![
            make_entry("fn_a", "/src/a.ts", 0, vec![]),
            make_entry("fn_b", "/src/b.ts", 3, vec![]),
            make_entry("fn_c", "/src/c.ts", 8, vec![]),
            make_entry("fn_d", "/src/d.ts", 20, vec![]),
        ];
        let registry = make_registry(entries.clone());
        let cg = CallGraph::from_registry(&registry);
        let config = CoreSampleConfig {
            budget: SampleBudget::Absolute(4),
            seed: 0,
            scan_root: "/".to_string(),
        };
        let result = select_core_sample(&entries, &cg, &config);
        assert_eq!(result.selected.len(), 4);
    }

    #[test]
    fn test_empty_entries() {
        let cg = CallGraph::from_registry(&make_registry(vec![]));
        let config = CoreSampleConfig {
            budget: SampleBudget::Percentage(50.0),
            seed: 0,
            scan_root: "/".to_string(),
        };
        let result = select_core_sample(&[], &cg, &config);
        assert!(result.selected.is_empty());
    }

    #[test]
    fn test_complexity_tier_classification() {
        assert_eq!(ComplexityTier::from_branch_count(0), ComplexityTier::Trivial);
        assert_eq!(ComplexityTier::from_branch_count(1), ComplexityTier::Trivial);
        assert_eq!(ComplexityTier::from_branch_count(2), ComplexityTier::Simple);
        assert_eq!(ComplexityTier::from_branch_count(5), ComplexityTier::Simple);
        assert_eq!(
            ComplexityTier::from_branch_count(6),
            ComplexityTier::Moderate
        );
        assert_eq!(
            ComplexityTier::from_branch_count(15),
            ComplexityTier::Moderate
        );
        assert_eq!(
            ComplexityTier::from_branch_count(16),
            ComplexityTier::Complex
        );
        assert_eq!(
            ComplexityTier::from_branch_count(100),
            ComplexityTier::Complex
        );
    }

    #[test]
    fn test_function_kind_classification() {
        let handler = make_entry("handleRequest", "/src/a.ts", 0, vec![]);
        assert_eq!(FunctionKind::classify(&handler), FunctionKind::Handler);

        let ctor = make_entry("createUser", "/src/a.ts", 0, vec![]);
        assert_eq!(FunctionKind::classify(&ctor), FunctionKind::Constructor);

        let io = make_entry("getData", "/src/a.ts", 0, vec!["db.query".into()]);
        assert_eq!(FunctionKind::classify(&io), FunctionKind::Io);

        let pure = make_entry("add", "/src/a.ts", 0, vec![]);
        assert_eq!(FunctionKind::classify(&pure), FunctionKind::Pure);
    }
}
