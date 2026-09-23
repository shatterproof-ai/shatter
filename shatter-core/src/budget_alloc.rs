//! Static execution-budget allocation for scans (str-03mfx.1).
//!
//! `shatter scan` historically gave every function the same exploration
//! budget. This module computes a static score per function from its
//! analysis and splits a layer's fixed execution total proportionally, with
//! per-function floors and ceilings, so the total is conserved by
//! construction. See `docs/superpowers/specs/2026-09-23-static-budget-allocation-design.md`.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::protocol::{DependencyKind, FunctionAnalysis};
use crate::types::TypeInfo;

/// Integer weights of the static score. These are the only tunable; the
/// budget-allocation benchmark (`task bench-budget-alloc`, child D of
/// str-03mfx) is what justifies changing them.
#[derive(Debug, Clone, Copy)]
pub struct Weights {
    pub branches: u32,
    pub opaque: u32,
    pub loops: u32,
    pub deps: u32,
    /// Per `ModuleImport` dependency (imports go opaque unless mocked).
    pub module_imports: u32,
    pub complex_params: u32,
    pub nesting: u32,
    /// Applied per 20 source lines.
    pub lines_per_20: u32,
}

pub const WEIGHTS: Weights = Weights {
    branches: 1,
    opaque: 2,
    loops: 3,
    deps: 1,
    module_imports: 2,
    complex_params: 2,
    nesting: 1,
    lines_per_20: 1,
};

/// Static features of a function that predict how much exploration it can absorb.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BudgetFeatures {
    pub branches: u32,
    /// Branches with no symbolic condition (opaque to Z3).
    pub opaque_branches: u32,
    pub loops: u32,
    pub deps: u32,
    /// Dependencies of kind `ModuleImport`.
    pub module_imports: u32,
    /// Parameters typed `Str`, `Array`, `Object`, `Union`, or `Nullable` of one of those.
    pub complex_params: u32,
    /// Maximum container depth over all parameters (scalar = 0).
    pub param_nesting: u32,
    pub lines: u32,
}

fn is_complex(t: &TypeInfo) -> bool {
    match t {
        TypeInfo::Str
        | TypeInfo::Array { .. }
        | TypeInfo::Object { .. }
        | TypeInfo::Union { .. } => true,
        TypeInfo::Nullable { inner } => is_complex(inner),
        _ => false,
    }
}

fn nesting(t: &TypeInfo) -> u32 {
    match t {
        TypeInfo::Array { element } => 1 + nesting(element),
        TypeInfo::Nullable { inner } => 1 + nesting(inner),
        TypeInfo::Object { fields } => {
            1 + fields.iter().map(|(_, f)| nesting(f)).max().unwrap_or(0)
        }
        TypeInfo::Union { variants, .. } => 1 + variants.iter().map(nesting).max().unwrap_or(0),
        _ => 0,
    }
}

/// Extract the static features of a function from its analysis.
pub fn features(analysis: &FunctionAnalysis) -> BudgetFeatures {
    BudgetFeatures {
        branches: analysis.branches.len() as u32,
        opaque_branches: analysis
            .branches
            .iter()
            .filter(|b| b.condition.is_none())
            .count() as u32,
        loops: analysis.loops.len() as u32,
        deps: analysis.dependencies.len() as u32,
        module_imports: analysis
            .dependencies
            .iter()
            .filter(|d| matches!(d.kind, DependencyKind::ModuleImport))
            .count() as u32,
        complex_params: analysis.params.iter().filter(|p| is_complex(&p.typ)).count() as u32,
        param_nesting: analysis
            .params
            .iter()
            .map(|p| nesting(&p.typ))
            .max()
            .unwrap_or(0),
        lines: analysis.end_line.saturating_sub(analysis.start_line) + 1,
    }
}

/// `1 + Σ weight × feature`; always at least 1.0 so every function keeps a
/// positive share.
pub fn score(f: &BudgetFeatures) -> f64 {
    let w = WEIGHTS;
    1.0 + f64::from(w.branches * f.branches)
        + f64::from(w.opaque * f.opaque_branches)
        + f64::from(w.loops * f.loops)
        + f64::from(w.deps * f.deps)
        + f64::from(w.module_imports * f.module_imports)
        + f64::from(w.complex_params * f.complex_params)
        + f64::from(w.nesting * f.param_nesting)
        + f64::from(w.lines_per_20) * (f64::from(f.lines) / 20.0)
}

/// One function's demand: relative score and absolute bounds in executions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Demand {
    pub score: f64,
    pub floor: u32,
    pub ceiling: u32,
}

/// Why an allocation could not honour the requested bounds exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Infeasible {
    /// Floors summed past the total and were scaled down proportionally.
    FloorsScaled { requested: u64, total: u32 },
    /// Ceilings summed below the total; `dropped` executions were not allocated.
    CeilingsBind { dropped: u32 },
}

/// Result of [`allocate`].
#[derive(Debug, Clone, PartialEq)]
pub struct Allocation {
    pub per_function: Vec<u32>,
    /// Executions actually allocated (equals the requested total unless ceilings bind).
    pub total: u32,
    pub infeasible: Option<Infeasible>,
}

/// Water-filling split of `total` executions across `demands`.
///
/// 1. Floors are honoured first (scaled down proportionally if they exceed
///    `total`); ceilings below their floor are raised to the floor.
/// 2. The remainder is distributed proportionally to score among functions
///    not yet at their ceiling, rounding down; any function that would exceed
///    its ceiling is fixed there and the pass repeats.
/// 3. Rounding leftovers go one at a time to open functions in descending
///    score, ties by ascending index.
///
/// Conserves the total whenever `Σ floor ≤ total ≤ Σ ceiling`, never panics,
/// and is deterministic.
pub fn allocate(demands: &[Demand], total: u32) -> Allocation {
    let n = demands.len();
    if n == 0 {
        return Allocation {
            per_function: vec![],
            total: 0,
            infeasible: None,
        };
    }
    let mut floors: Vec<u32> = demands.iter().map(|d| d.floor).collect();
    let ceilings: Vec<u32> = demands.iter().map(|d| d.ceiling.max(d.floor)).collect();
    let mut infeasible = None;

    let floor_sum: u64 = floors.iter().map(|&f| u64::from(f)).sum();
    if floor_sum > u64::from(total) {
        for f in floors.iter_mut() {
            *f = ((u64::from(*f) * u64::from(total)) / floor_sum) as u32;
        }
        infeasible = Some(Infeasible::FloorsScaled {
            requested: floor_sum,
            total,
        });
    }
    let ceiling_sum: u64 = ceilings.iter().map(|&c| u64::from(c)).sum();
    if ceiling_sum < u64::from(total) {
        let allocated = ceiling_sum as u32;
        return Allocation {
            per_function: ceilings,
            total: allocated,
            infeasible: Some(Infeasible::CeilingsBind {
                dropped: total - allocated,
            }),
        };
    }

    let mut shares = floors.clone();
    let mut open: Vec<bool> = (0..n).map(|i| shares[i] < ceilings[i]).collect();
    let mut remaining: u32 = total - shares.iter().sum::<u32>();

    loop {
        if remaining == 0 || !open.iter().any(|&o| o) {
            break;
        }
        let score_sum: f64 = (0..n)
            .filter(|&i| open[i])
            .map(|i| demands[i].score.max(0.0))
            .sum();
        let open_count = open.iter().filter(|&&o| o).count() as u32;
        let mut closed_any = false;
        let mut handed: u32 = 0;
        for i in 0..n {
            if !open[i] {
                continue;
            }
            let want = if score_sum > 0.0 {
                (f64::from(remaining) * demands[i].score.max(0.0) / score_sum).floor() as u32
            } else {
                remaining / open_count
            };
            let room = ceilings[i] - shares[i];
            if want >= room {
                shares[i] = ceilings[i];
                handed += room;
                open[i] = false;
                closed_any = true;
            } else {
                shares[i] += want;
                handed += want;
            }
        }
        remaining -= handed;
        if !closed_any {
            break;
        }
    }

    // Rounding leftovers: descending score, then ascending index, one unit each, cycling.
    let mut order: Vec<usize> = (0..n).filter(|&i| open[i]).collect();
    order.sort_by(|&a, &b| {
        demands[b]
            .score
            .partial_cmp(&demands[a].score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    while remaining > 0 && !order.is_empty() {
        let mut progressed = false;
        for &i in &order {
            if remaining == 0 {
                break;
            }
            if shares[i] < ceilings[i] {
                shares[i] += 1;
                remaining -= 1;
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
        order.retain(|&i| shares[i] < ceilings[i]);
    }

    let allocated: u32 = shares.iter().sum();
    Allocation {
        per_function: shares,
        total: allocated,
        infeasible,
    }
}

/// Shared budget surplus within a topological layer.
///
/// Functions that terminate early (worklist exhausted, coverage plateau, full
/// branch coverage) donate their unused execution budget here. Functions still
/// discovering new paths can claim from the surplus when their initial budget
/// runs out.
///
/// Each layer gets a fresh `BudgetSurplus` — budget from layer N does not carry
/// over to layer N+1.
#[derive(Debug)]
pub struct BudgetSurplus {
    /// Remaining surplus executions available for claiming.
    available: AtomicU32,
}

impl Default for BudgetSurplus {
    fn default() -> Self {
        Self::new()
    }
}

impl BudgetSurplus {
    /// Create a new empty surplus (used at the start of each layer).
    pub fn new() -> Self {
        Self {
            available: AtomicU32::new(0),
        }
    }

    /// Donate unused budget to the shared surplus.
    pub fn donate(&self, amount: u32) {
        if amount > 0 {
            self.available.fetch_add(amount, Ordering::Release);
        }
    }

    /// Try to claim up to `requested` executions from the surplus.
    ///
    /// Returns the number actually claimed (may be less than requested if the
    /// surplus is partially depleted, or 0 if less than `min_claim` is
    /// available).
    pub fn try_claim(&self, requested: u32, min_claim: u32) -> u32 {
        let mut current = self.available.load(Ordering::Acquire);
        loop {
            if current < min_claim {
                return 0;
            }
            let to_claim = current.min(requested);
            match self.available.compare_exchange_weak(
                current,
                current - to_claim,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return to_claim,
                Err(updated) => current = updated,
            }
        }
    }

    /// Current surplus available (for diagnostics/testing).
    pub fn available(&self) -> u32 {
        self.available.load(Ordering::Acquire)
    }
}

/// Policy governing when a function may claim surplus budget.
#[derive(Debug, Clone)]
pub struct ClaimPolicy {
    /// Minimum hit rate (new paths / last N executions) to qualify for claiming.
    pub min_hit_rate: f64,
    /// Window size for measuring recent hit rate.
    pub window: u32,
    /// Maximum fraction of total surplus a single function can claim at once.
    pub max_claim_fraction: f64,
}

impl Default for ClaimPolicy {
    fn default() -> Self {
        Self {
            min_hit_rate: 0.1,
            window: 10,
            max_claim_fraction: 0.5,
        }
    }
}

impl ClaimPolicy {
    /// Determine whether a function should be allowed to claim surplus budget,
    /// based on its recent exploration productivity.
    ///
    /// `recent_new_paths` is the number of new paths discovered in the last
    /// `window` executions.
    pub fn should_claim(&self, recent_new_paths: u32) -> bool {
        if self.window == 0 {
            return false;
        }
        let hit_rate = recent_new_paths as f64 / self.window as f64;
        hit_rate >= self.min_hit_rate
    }

    /// Compute the maximum number of executions this function should claim,
    /// given the current surplus.
    pub fn max_claimable(&self, surplus_available: u32) -> u32 {
        (surplus_available as f64 * self.max_claim_fraction).floor() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{
        BoundOp, BranchInfo, BranchType, DependencyKind, ExternalDependency, FunctionAnalysis,
        InductionVar, InvocationModel, LoopInfo,
    };
    use crate::sym_expr::SymExpr;
    use crate::types::{ParamInfo, TypeInfo};
    use proptest::prelude::*;

    fn int() -> TypeInfo {
        TypeInfo::Int {
            int_width: None,
            int_signed: None,
        }
    }

    fn loop_info(i: u32) -> LoopInfo {
        LoopInfo {
            loop_id: i,
            line: 2 + i,
            induction_var: InductionVar {
                name: "i".into(),
                init_expr: SymExpr::Unknown,
                step_expr: SymExpr::Unknown,
                bound_expr: SymExpr::Unknown,
                bound_op: BoundOp::Lt,
            },
        }
    }

    fn analysis(
        branches: Vec<BranchInfo>,
        params: Vec<TypeInfo>,
        deps: Vec<ExternalDependency>,
        loops: u32,
        lines: u32,
    ) -> FunctionAnalysis {
        FunctionAnalysis {
            name: "f".into(),
            exported: true,
            params: params
                .into_iter()
                .enumerate()
                .map(|(i, typ)| ParamInfo {
                    name: format!("p{i}"),
                    typ,
                    type_name: None,
                })
                .collect(),
            branches,
            dependencies: deps,
            return_type: int(),
            start_line: 1,
            end_line: lines,
            literals: vec![],
            crypto_boundaries: vec![],
            loops: (0..loops).map(loop_info).collect(),
            source_file: None,
            adapter_hints: vec![],
            invocation_model: InvocationModel::Direct,
        }
    }

    fn branch(id: u32, opaque: bool) -> BranchInfo {
        BranchInfo {
            id,
            line: id + 1,
            condition_text: format!("c{id}"),
            condition: if opaque {
                None
            } else {
                Some(SymExpr::Unknown)
            },
            branch_type: BranchType::If,
        }
    }

    fn dep(kind: DependencyKind) -> ExternalDependency {
        ExternalDependency {
            kind,
            symbol: "s".into(),
            source_module: "m".into(),
            return_type: int(),
            param_types: vec![],
            call_sites: vec![],
        }
    }

    #[test]
    fn features_counts_every_dimension() {
        let a = analysis(
            vec![branch(0, false), branch(1, true), branch(2, true)],
            vec![
                int(),
                TypeInfo::Str,
                TypeInfo::Array {
                    element: Box::new(TypeInfo::Object {
                        fields: vec![("k".into(), TypeInfo::Str)],
                    }),
                },
                TypeInfo::Nullable {
                    inner: Box::new(int()),
                },
            ],
            vec![
                dep(DependencyKind::FunctionCall),
                dep(DependencyKind::ModuleImport),
            ],
            2,
            41,
        );
        let f = features(&a);
        assert_eq!(f.branches, 3);
        assert_eq!(f.opaque_branches, 2);
        assert_eq!(f.loops, 2);
        assert_eq!(f.deps, 2);
        assert_eq!(f.module_imports, 1);
        assert_eq!(f.complex_params, 2, "Str and Array; Nullable<Int> is not complex");
        assert_eq!(f.param_nesting, 2, "Array -> Object");
        assert_eq!(f.lines, 41);
    }

    #[test]
    fn score_is_at_least_one_and_monotone_in_each_feature() {
        let base = BudgetFeatures::default();
        assert!((score(&base) - 1.0).abs() < 1e-9);
        let bumps: Vec<Box<dyn Fn(&mut BudgetFeatures)>> = vec![
            Box::new(|f| f.branches += 1),
            Box::new(|f| f.opaque_branches += 1),
            Box::new(|f| f.loops += 1),
            Box::new(|f| f.deps += 1),
            Box::new(|f| f.module_imports += 1),
            Box::new(|f| f.complex_params += 1),
            Box::new(|f| f.param_nesting += 1),
            Box::new(|f| f.lines += 20),
        ];
        for bump in bumps {
            let mut f = base.clone();
            bump(&mut f);
            assert!(score(&f) > score(&base), "{f:?}");
        }
    }

    fn d(score: f64, floor: u32, ceiling: u32) -> Demand {
        Demand {
            score,
            floor,
            ceiling,
        }
    }

    #[test]
    fn allocate_uniform_for_equal_scores() {
        let a = allocate(&[d(1.0, 10, 1000), d(1.0, 10, 1000), d(1.0, 10, 1000)], 300);
        assert_eq!(a.per_function, vec![100, 100, 100]);
        assert_eq!(a.total, 300);
        assert!(a.infeasible.is_none());
    }

    #[test]
    fn allocate_proportional_and_conserving() {
        let a = allocate(&[d(1.0, 0, 1000), d(3.0, 0, 1000)], 100);
        assert_eq!(a.per_function, vec![25, 75]);
    }

    #[test]
    fn allocate_respects_ceiling_and_redistributes() {
        // cross-check finding 2: scores [1, 99], total 100, floor 20, ceiling 100 must not exceed 100.
        let a = allocate(&[d(1.0, 20, 100), d(99.0, 20, 100)], 100);
        assert_eq!(a.per_function.iter().sum::<u32>(), 100);
        assert_eq!(a.per_function, vec![20, 80]);
        let b = allocate(&[d(1.0, 0, 30), d(9.0, 0, 30), d(1.0, 0, 1000)], 100);
        assert_eq!(b.per_function.iter().sum::<u32>(), 100);
        assert_eq!(b.per_function[1], 30, "capped at ceiling");
        assert!(
            b.per_function[2] > b.per_function[0],
            "residue goes to the open function with the higher share"
        );
    }

    #[test]
    fn allocate_scales_floors_when_infeasible() {
        let a = allocate(&[d(1.0, 60, 100), d(1.0, 60, 100)], 100);
        assert_eq!(a.per_function.iter().sum::<u32>(), 100);
        assert!(matches!(
            a.infeasible,
            Some(Infeasible::FloorsScaled {
                requested: 120,
                total: 100
            })
        ));
    }

    #[test]
    fn allocate_reports_binding_ceilings() {
        let a = allocate(&[d(1.0, 0, 10), d(1.0, 0, 10)], 100);
        assert_eq!(a.per_function, vec![10, 10]);
        assert_eq!(a.total, 20);
        assert!(matches!(a.infeasible, Some(Infeasible::CeilingsBind { dropped: 80 })));
    }

    #[test]
    fn allocate_degenerate_inputs_do_not_panic() {
        assert!(allocate(&[], 100).per_function.is_empty());
        assert_eq!(
            allocate(&[d(1.0, 5, 3)], 100).per_function,
            vec![5],
            "ceiling below floor is raised to floor"
        );
        assert_eq!(allocate(&[d(1.0, 0, 10)], 0).per_function, vec![0]);
        assert_eq!(
            allocate(&[d(0.0, 0, 10), d(0.0, 0, 10)], 10)
                .per_function
                .iter()
                .sum::<u32>(),
            10,
            "zero scores split evenly"
        );
    }

    #[test]
    fn allocate_remainder_goes_by_descending_score_then_index() {
        let a = allocate(&[d(1.0, 0, 100), d(2.0, 0, 100), d(2.0, 0, 100)], 7);
        // proportional floors: 1.4, 2.8, 2.8 -> 1, 2, 2 = 5; remainder 2 -> index 1 then index 2
        assert_eq!(a.per_function, vec![1, 3, 3]);
    }

    #[test]
    fn claim_policy_thresholds() {
        let p = ClaimPolicy {
            min_hit_rate: 0.1,
            window: 10,
            max_claim_fraction: 0.5,
        };
        assert!(!p.should_claim(0));
        assert!(p.should_claim(1));
        assert_eq!(p.max_claimable(50), 25);
        assert!(!ClaimPolicy { window: 0, ..p }.should_claim(5));
        let s = BudgetSurplus::new();
        s.donate(7);
        assert_eq!(s.try_claim(10, 1), 7);
        assert_eq!(s.available(), 0);
    }

    proptest! {
        #[test]
        fn allocate_conserves_total_when_feasible(
            demands in prop::collection::vec((0.0f64..100.0, 0u32..50, 50u32..500), 1..12),
            total_frac in 0.0f64..=1.0,
        ) {
            let ds: Vec<Demand> = demands.iter().map(|(s, f, c)| d(*s, *f, *c)).collect();
            let lo: u32 = ds.iter().map(|x| x.floor).sum();
            let hi: u32 = ds.iter().map(|x| x.ceiling).sum();
            let total = lo + ((hi - lo) as f64 * total_frac) as u32;
            let a = allocate(&ds, total);
            prop_assert_eq!(a.per_function.iter().sum::<u32>(), total);
            prop_assert_eq!(a.total, total);
            prop_assert!(a.infeasible.is_none());
            for (share, dm) in a.per_function.iter().zip(&ds) {
                prop_assert!(*share >= dm.floor && *share <= dm.ceiling, "{} not in [{}, {}]", share, dm.floor, dm.ceiling);
            }
            // Proportionality governs the increment above the floor, so compare
            // (share - floor) among functions that ended strictly inside their bounds.
            let open: Vec<(f64, u32)> = a
                .per_function
                .iter()
                .zip(&ds)
                .filter(|(s, dm)| **s > dm.floor && **s < dm.ceiling)
                .map(|(s, dm)| (dm.score, *s - dm.floor))
                .collect();
            for (i, (si, ai)) in open.iter().enumerate() {
                for (sj, aj) in open.iter().skip(i + 1) {
                    if si > sj { prop_assert!(ai >= aj); }
                    if sj > si { prop_assert!(aj >= ai); }
                }
            }
            prop_assert_eq!(allocate(&ds, total).per_function, a.per_function, "deterministic");
        }

        #[test]
        fn allocate_never_panics(
            demands in prop::collection::vec((0.0f64..1000.0, 0u32..1000, 0u32..1000), 0..12),
            total in 0u32..100_000,
        ) {
            let ds: Vec<Demand> = demands.iter().map(|(s, f, c)| d(*s, *f, *c)).collect();
            let a = allocate(&ds, total);
            prop_assert_eq!(a.per_function.len(), ds.len());
            prop_assert!(a.total <= total);
        }
    }
}
