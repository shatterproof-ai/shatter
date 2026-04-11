//! Mockability analysis and refactoring recommendations for external dependencies.
//!
//! Classifies each [`ExternalDependency`] as easy or hard to mock based on
//! structural patterns (dynamic dispatch, closures, subprocess spawning,
//! multi-layer indirection). Hard-to-mock dependencies get actionable
//! refactoring recommendations: extract to parameter, wrap in named function,
//! or move behind an interface.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::protocol::{DependencyKind, ExternalDependency};

// ---------------------------------------------------------------------------
// Detection patterns (symbol/module substrings that indicate hard-to-mock calls)
// ---------------------------------------------------------------------------

/// Module names that indicate subprocess spawning.
const SUBPROCESS_MODULES: &[&str] = &[
    "child_process",
    "node:child_process",
    "os/exec",
    "subprocess",
];

/// Symbol substrings that indicate subprocess spawning.
const SUBPROCESS_SYMBOLS: &[&str] = &[
    "exec",
    "execSync",
    "execFile",
    "execFileSync",
    "spawn",
    "spawnSync",
    "fork",
    "Command",
];

/// Symbol patterns that indicate dynamic dispatch or computed property access.
const DYNAMIC_DISPATCH_PATTERNS: &[&str] = &[
    "[",       // computed property access: obj[key]()
    "apply",   // Function.prototype.apply
    "call",    // Function.prototype.call
    "bind",    // Function.prototype.bind
    "Reflect", // Reflect.apply, Reflect.get, etc.
    "Proxy",   // Proxy-based dispatch
];

/// Symbol patterns indicating closure-based invocation (callbacks to external code).
const CLOSURE_PATTERNS: &[&str] = &[
    "callback",
    "handler",
    "listener",
    "onEvent",
    "subscribe",
    "observe",
    "then",    // Promise.then(callback)
    "forEach", // array.forEach(externalFn)
    "map",     // array.map(externalFn) when external
    "filter",  // array.filter(externalFn) when external
    "reduce",  // array.reduce(externalFn) when external
];

/// Minimum number of call sites before we flag multi-layer indirection.
const MULTI_LAYER_CALL_SITE_THRESHOLD: usize = 3;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// How difficult a dependency is to mock in tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MockDifficulty {
    /// Can be mocked with standard techniques (simple function stub, jest.mock, etc.).
    Easy,
    /// Requires refactoring for clean testability.
    Hard,
}

/// Why a dependency is hard to mock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardToMockReason {
    /// Call goes through dynamic dispatch or computed property access.
    DynamicDispatch,
    /// Call is inside a closure passed to external code.
    ClosureCallback,
    /// Dependency spawns a subprocess with complex argument construction.
    SubprocessSpawning,
    /// Multiple layers of indirection (called from many sites, suggesting
    /// the dependency is deeply embedded).
    MultiLayerIndirection,
}

impl fmt::Display for HardToMockReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DynamicDispatch => write!(f, "through dynamic dispatch"),
            Self::ClosureCallback => write!(f, "inside a closure passed to external code"),
            Self::SubprocessSpawning => write!(f, "subprocess call that cannot be mocked"),
            Self::MultiLayerIndirection => {
                write!(f, "through multiple layers of indirection")
            }
        }
    }
}

/// What kind of refactoring would make the dependency easier to mock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefactoringSuggestion {
    /// Extract the call to a named function parameter for dependency injection.
    ExtractToParameter,
    /// Wrap the call in a named function that can be replaced in test configuration.
    WrapInNamedFunction,
    /// Move the dependency behind an interface or trait for substitution in tests.
    MoveBehindInterface,
}

impl fmt::Display for RefactoringSuggestion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExtractToParameter => {
                write!(
                    f,
                    "Extract to a named function parameter for dependency injection"
                )
            }
            Self::WrapInNamedFunction => {
                write!(
                    f,
                    "Wrap in a function that can be replaced in test configuration"
                )
            }
            Self::MoveBehindInterface => {
                write!(
                    f,
                    "Move behind an interface/trait for substitution in tests"
                )
            }
        }
    }
}

/// Mockability classification for a single external dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MockabilityAssessment {
    /// The dependency symbol being assessed.
    pub symbol: String,
    /// The source module of the dependency.
    pub source_module: String,
    /// Overall difficulty classification.
    pub difficulty: MockDifficulty,
    /// Reasons why this dependency is hard to mock (empty for easy dependencies).
    pub reasons: Vec<HardToMockReason>,
    /// Suggested refactoring actions (empty for easy dependencies).
    pub suggestions: Vec<RefactoringSuggestion>,
    /// First call site line number (for report positioning).
    pub first_call_site: Option<u32>,
}

/// A single refactoring recommendation for the scan report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefactoringRecommendation {
    /// The dependency symbol.
    pub symbol: String,
    /// The source module.
    pub source_module: String,
    /// Line number of the first call site.
    pub line: Option<u32>,
    /// Why this dependency is hard to mock.
    pub reason: HardToMockReason,
    /// Actionable suggestion.
    pub suggestion: RefactoringSuggestion,
}

impl fmt::Display for RefactoringRecommendation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let location = self
            .line
            .map(|l| format!(" (line {l})"))
            .unwrap_or_default();
        write!(
            f,
            "  `{sym}`{loc}:\n    Call to `{sym}` is {reason}.\n    Suggestion: {suggestion}.",
            sym = self.symbol,
            loc = location,
            reason = self.reason,
            suggestion = self.suggestion,
        )
    }
}

// ---------------------------------------------------------------------------
// Detection logic
// ---------------------------------------------------------------------------

/// Detect whether a symbol name suggests dynamic dispatch.
fn is_dynamic_dispatch(dep: &ExternalDependency) -> bool {
    if dep.kind == DependencyKind::PropertyAccess {
        return true;
    }
    DYNAMIC_DISPATCH_PATTERNS
        .iter()
        .any(|pat| dep.symbol.contains(pat))
}

/// Detect whether a symbol name suggests closure-based invocation.
fn is_closure_callback(dep: &ExternalDependency) -> bool {
    let lower = dep.symbol.to_lowercase();
    CLOSURE_PATTERNS.iter().any(|pat| lower.contains(pat))
}

/// Detect whether a dependency involves subprocess spawning.
fn is_subprocess_spawning(dep: &ExternalDependency) -> bool {
    let module_match = SUBPROCESS_MODULES
        .iter()
        .any(|m| dep.source_module == *m || dep.source_module.starts_with(&format!("{m}/")));
    if module_match {
        return true;
    }
    SUBPROCESS_SYMBOLS
        .iter()
        .any(|s| dep.symbol == *s || dep.symbol.ends_with(&format!(".{s}")))
}

/// Detect whether a dependency has multi-layer indirection (called from many sites).
fn is_multi_layer_indirection(dep: &ExternalDependency) -> bool {
    dep.call_sites.len() >= MULTI_LAYER_CALL_SITE_THRESHOLD
}

/// Select the appropriate refactoring suggestion for a given reason.
fn suggestion_for_reason(reason: HardToMockReason) -> RefactoringSuggestion {
    match reason {
        HardToMockReason::DynamicDispatch => RefactoringSuggestion::ExtractToParameter,
        HardToMockReason::ClosureCallback => RefactoringSuggestion::ExtractToParameter,
        HardToMockReason::SubprocessSpawning => RefactoringSuggestion::WrapInNamedFunction,
        HardToMockReason::MultiLayerIndirection => RefactoringSuggestion::MoveBehindInterface,
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Assess the mockability of a single external dependency.
///
/// Checks for dynamic dispatch, closure callbacks, subprocess spawning, and
/// multi-layer indirection. Dependencies with any hard-to-mock reason are
/// classified as [`MockDifficulty::Hard`] with corresponding suggestions.
pub fn assess_mockability(dep: &ExternalDependency) -> MockabilityAssessment {
    let mut reasons = Vec::new();

    if is_dynamic_dispatch(dep) {
        reasons.push(HardToMockReason::DynamicDispatch);
    }
    if is_closure_callback(dep) {
        reasons.push(HardToMockReason::ClosureCallback);
    }
    if is_subprocess_spawning(dep) {
        reasons.push(HardToMockReason::SubprocessSpawning);
    }
    if is_multi_layer_indirection(dep) {
        reasons.push(HardToMockReason::MultiLayerIndirection);
    }

    let difficulty = if reasons.is_empty() {
        MockDifficulty::Easy
    } else {
        MockDifficulty::Hard
    };

    let suggestions: Vec<RefactoringSuggestion> =
        reasons.iter().map(|r| suggestion_for_reason(*r)).collect();

    MockabilityAssessment {
        symbol: dep.symbol.clone(),
        source_module: dep.source_module.clone(),
        difficulty,
        reasons,
        suggestions,
        first_call_site: dep.call_sites.first().copied(),
    }
}

/// Assess mockability for all dependencies in a list.
///
/// Returns assessments for all dependencies, regardless of difficulty.
pub fn assess_all(deps: &[ExternalDependency]) -> Vec<MockabilityAssessment> {
    deps.iter().map(assess_mockability).collect()
}

/// Generate refactoring recommendations from a list of dependencies.
///
/// Only produces recommendations for hard-to-mock dependencies.
/// Each reason gets its own recommendation entry for clarity.
pub fn generate_recommendations(deps: &[ExternalDependency]) -> Vec<RefactoringRecommendation> {
    let mut recs = Vec::new();

    for dep in deps {
        let assessment = assess_mockability(dep);
        if assessment.difficulty == MockDifficulty::Easy {
            continue;
        }

        for (reason, suggestion) in assessment.reasons.iter().zip(assessment.suggestions.iter()) {
            recs.push(RefactoringRecommendation {
                symbol: dep.symbol.clone(),
                source_module: dep.source_module.clone(),
                line: dep.call_sites.first().copied(),
                reason: *reason,
                suggestion: *suggestion,
            });
        }
    }

    recs
}

/// Format refactoring recommendations as a human-readable section for a scan report.
///
/// Returns an empty string if there are no recommendations.
pub fn format_recommendations(recs: &[RefactoringRecommendation]) -> String {
    if recs.is_empty() {
        return String::new();
    }

    let mut out = String::from("Refactoring Recommendations:\n");
    for rec in recs {
        out.push_str(&format!("{rec}\n"));
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::DependencyKind;
    use crate::types::TypeInfo;

    fn make_dep(
        symbol: &str,
        source_module: &str,
        kind: DependencyKind,
        call_sites: Vec<u32>,
    ) -> ExternalDependency {
        ExternalDependency {
            kind,
            symbol: symbol.to_string(),
            source_module: source_module.to_string(),
            return_type: TypeInfo::Unknown,
            param_types: vec![],
            call_sites,
        }
    }

    // --- Dynamic dispatch detection ---

    #[test]
    fn detects_property_access_as_dynamic_dispatch() {
        let dep = make_dep(
            "service.process",
            "my-service",
            DependencyKind::PropertyAccess,
            vec![42],
        );
        let assessment = assess_mockability(&dep);
        assert_eq!(assessment.difficulty, MockDifficulty::Hard);
        assert!(
            assessment
                .reasons
                .contains(&HardToMockReason::DynamicDispatch)
        );
    }

    #[test]
    fn detects_computed_access_as_dynamic_dispatch() {
        let dep = make_dep("obj[key]", "my-lib", DependencyKind::FunctionCall, vec![10]);
        let assessment = assess_mockability(&dep);
        assert_eq!(assessment.difficulty, MockDifficulty::Hard);
        assert!(
            assessment
                .reasons
                .contains(&HardToMockReason::DynamicDispatch)
        );
    }

    #[test]
    fn detects_reflect_apply_as_dynamic_dispatch() {
        let dep = make_dep(
            "Reflect.apply",
            "builtin",
            DependencyKind::FunctionCall,
            vec![5],
        );
        let assessment = assess_mockability(&dep);
        assert!(
            assessment
                .reasons
                .contains(&HardToMockReason::DynamicDispatch)
        );
    }

    // --- Closure callback detection ---

    #[test]
    fn detects_callback_pattern() {
        let dep = make_dep(
            "eventEmitter.subscribe",
            "events",
            DependencyKind::MethodCall,
            vec![20],
        );
        let assessment = assess_mockability(&dep);
        assert!(
            assessment
                .reasons
                .contains(&HardToMockReason::ClosureCallback)
        );
    }

    #[test]
    fn detects_then_as_closure() {
        let dep = make_dep(
            "promise.then",
            "builtin",
            DependencyKind::MethodCall,
            vec![15],
        );
        let assessment = assess_mockability(&dep);
        assert!(
            assessment
                .reasons
                .contains(&HardToMockReason::ClosureCallback)
        );
    }

    // --- Subprocess spawning detection ---

    #[test]
    fn detects_child_process_exec() {
        let dep = make_dep(
            "exec",
            "child_process",
            DependencyKind::FunctionCall,
            vec![18],
        );
        let assessment = assess_mockability(&dep);
        assert_eq!(assessment.difficulty, MockDifficulty::Hard);
        assert!(
            assessment
                .reasons
                .contains(&HardToMockReason::SubprocessSpawning)
        );
    }

    #[test]
    fn detects_node_child_process_spawn() {
        let dep = make_dep(
            "spawn",
            "node:child_process",
            DependencyKind::FunctionCall,
            vec![25],
        );
        let assessment = assess_mockability(&dep);
        assert!(
            assessment
                .reasons
                .contains(&HardToMockReason::SubprocessSpawning)
        );
    }

    #[test]
    fn detects_spawn_symbol_without_module() {
        let dep = make_dep("cp.spawn", "utils", DependencyKind::MethodCall, vec![30]);
        let assessment = assess_mockability(&dep);
        assert!(
            assessment
                .reasons
                .contains(&HardToMockReason::SubprocessSpawning)
        );
    }

    // --- Multi-layer indirection detection ---

    #[test]
    fn detects_multi_layer_indirection() {
        let dep = make_dep(
            "logger.info",
            "winston",
            DependencyKind::MethodCall,
            vec![10, 25, 42, 60],
        );
        let assessment = assess_mockability(&dep);
        assert!(
            assessment
                .reasons
                .contains(&HardToMockReason::MultiLayerIndirection)
        );
    }

    #[test]
    fn few_call_sites_not_multi_layer() {
        let dep = make_dep(
            "logger.info",
            "winston",
            DependencyKind::MethodCall,
            vec![10],
        );
        let assessment = assess_mockability(&dep);
        assert!(
            !assessment
                .reasons
                .contains(&HardToMockReason::MultiLayerIndirection)
        );
    }

    // --- Easy dependency classification ---

    #[test]
    fn simple_function_call_is_easy() {
        let dep = make_dep("readFile", "fs", DependencyKind::FunctionCall, vec![10]);
        let assessment = assess_mockability(&dep);
        assert_eq!(assessment.difficulty, MockDifficulty::Easy);
        assert!(assessment.reasons.is_empty());
        assert!(assessment.suggestions.is_empty());
    }

    // --- Suggestion mapping ---

    #[test]
    fn dynamic_dispatch_suggests_extract_to_parameter() {
        let dep = make_dep("obj[key]", "lib", DependencyKind::FunctionCall, vec![10]);
        let assessment = assess_mockability(&dep);
        assert!(
            assessment
                .suggestions
                .contains(&RefactoringSuggestion::ExtractToParameter)
        );
    }

    #[test]
    fn subprocess_suggests_wrap_in_named_function() {
        let dep = make_dep(
            "exec",
            "child_process",
            DependencyKind::FunctionCall,
            vec![18],
        );
        let assessment = assess_mockability(&dep);
        assert!(
            assessment
                .suggestions
                .contains(&RefactoringSuggestion::WrapInNamedFunction)
        );
    }

    #[test]
    fn multi_layer_suggests_move_behind_interface() {
        let dep = make_dep(
            "db.query",
            "pg",
            DependencyKind::MethodCall,
            vec![10, 25, 42],
        );
        let assessment = assess_mockability(&dep);
        assert!(
            assessment
                .suggestions
                .contains(&RefactoringSuggestion::MoveBehindInterface)
        );
    }

    // --- Batch assessment ---

    #[test]
    fn assess_all_returns_all_deps() {
        let deps = vec![
            make_dep("readFile", "fs", DependencyKind::FunctionCall, vec![10]),
            make_dep(
                "exec",
                "child_process",
                DependencyKind::FunctionCall,
                vec![18],
            ),
        ];
        let assessments = assess_all(&deps);
        assert_eq!(assessments.len(), 2);
        assert_eq!(assessments[0].difficulty, MockDifficulty::Easy);
        assert_eq!(assessments[1].difficulty, MockDifficulty::Hard);
    }

    // --- Recommendation generation ---

    #[test]
    fn generate_recommendations_skips_easy() {
        let deps = vec![make_dep(
            "readFile",
            "fs",
            DependencyKind::FunctionCall,
            vec![10],
        )];
        let recs = generate_recommendations(&deps);
        assert!(recs.is_empty());
    }

    #[test]
    fn generate_recommendations_for_hard_deps() {
        let deps = vec![
            make_dep(
                "exec",
                "child_process",
                DependencyKind::FunctionCall,
                vec![18],
            ),
            make_dep("obj[key]", "lib", DependencyKind::FunctionCall, vec![42]),
        ];
        let recs = generate_recommendations(&deps);
        assert_eq!(recs.len(), 2);

        assert_eq!(recs[0].symbol, "exec");
        assert_eq!(recs[0].reason, HardToMockReason::SubprocessSpawning);
        assert_eq!(
            recs[0].suggestion,
            RefactoringSuggestion::WrapInNamedFunction
        );
        assert_eq!(recs[0].line, Some(18));

        assert_eq!(recs[1].symbol, "obj[key]");
        assert_eq!(recs[1].reason, HardToMockReason::DynamicDispatch);
        assert_eq!(
            recs[1].suggestion,
            RefactoringSuggestion::ExtractToParameter
        );
    }

    #[test]
    fn multiple_reasons_produce_multiple_recommendations() {
        // A dependency that is both subprocess and multi-layer
        let dep = make_dep(
            "exec",
            "child_process",
            DependencyKind::FunctionCall,
            vec![10, 20, 30],
        );
        let recs = generate_recommendations(&[dep]);
        // subprocess + multi-layer = 2 recommendations
        assert_eq!(recs.len(), 2);
        let reasons: Vec<_> = recs.iter().map(|r| r.reason).collect();
        assert!(reasons.contains(&HardToMockReason::SubprocessSpawning));
        assert!(reasons.contains(&HardToMockReason::MultiLayerIndirection));
    }

    // --- Format output ---

    #[test]
    fn format_empty_recommendations() {
        let output = format_recommendations(&[]);
        assert!(output.is_empty());
    }

    #[test]
    fn format_recommendations_includes_header() {
        let recs = vec![RefactoringRecommendation {
            symbol: "exec".to_string(),
            source_module: "child_process".to_string(),
            line: Some(18),
            reason: HardToMockReason::SubprocessSpawning,
            suggestion: RefactoringSuggestion::WrapInNamedFunction,
        }];
        let output = format_recommendations(&recs);
        assert!(output.starts_with("Refactoring Recommendations:\n"));
        assert!(output.contains("`exec`"));
        assert!(output.contains("line 18"));
        assert!(output.contains("subprocess call"));
        assert!(output.contains("Wrap in a function"));
    }

    #[test]
    fn format_recommendation_without_line() {
        let rec = RefactoringRecommendation {
            symbol: "service.process".to_string(),
            source_module: "my-service".to_string(),
            line: None,
            reason: HardToMockReason::DynamicDispatch,
            suggestion: RefactoringSuggestion::ExtractToParameter,
        };
        let output = format!("{rec}");
        assert!(!output.contains("line"));
        assert!(output.contains("`service.process`"));
    }

    // --- Serialization roundtrip ---

    #[test]
    fn assessment_serialization_roundtrip() {
        let assessment = MockabilityAssessment {
            symbol: "exec".to_string(),
            source_module: "child_process".to_string(),
            difficulty: MockDifficulty::Hard,
            reasons: vec![HardToMockReason::SubprocessSpawning],
            suggestions: vec![RefactoringSuggestion::WrapInNamedFunction],
            first_call_site: Some(18),
        };
        let json = serde_json::to_string(&assessment).expect("serialize");
        let deserialized: MockabilityAssessment = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(assessment, deserialized);
    }

    #[test]
    fn recommendation_serialization_roundtrip() {
        let rec = RefactoringRecommendation {
            symbol: "exec".to_string(),
            source_module: "child_process".to_string(),
            line: Some(18),
            reason: HardToMockReason::SubprocessSpawning,
            suggestion: RefactoringSuggestion::WrapInNamedFunction,
        };
        let json = serde_json::to_string(&rec).expect("serialize");
        let deserialized: RefactoringRecommendation =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(rec, deserialized);
    }

    // --- Property-based tests ---

    mod prop_tests {
        use super::*;
        use crate::test_arbitraries::arb_external_dependency;
        use proptest::prelude::*;

        proptest! {
            /// Every dependency gets a classification (never panics).
            #[test]
            fn assess_never_panics(dep in arb_external_dependency()) {
                let assessment = assess_mockability(&dep);
                // Symbol is always preserved
                prop_assert_eq!(&assessment.symbol, &dep.symbol);
                prop_assert_eq!(&assessment.source_module, &dep.source_module);
            }

            /// Easy dependencies have no reasons or suggestions.
            #[test]
            fn easy_has_no_reasons(dep in arb_external_dependency()) {
                let assessment = assess_mockability(&dep);
                if assessment.difficulty == MockDifficulty::Easy {
                    prop_assert!(assessment.reasons.is_empty());
                    prop_assert!(assessment.suggestions.is_empty());
                }
            }

            /// Hard dependencies always have at least one reason and suggestion.
            #[test]
            fn hard_has_reasons_and_suggestions(dep in arb_external_dependency()) {
                let assessment = assess_mockability(&dep);
                if assessment.difficulty == MockDifficulty::Hard {
                    prop_assert!(!assessment.reasons.is_empty());
                    prop_assert!(!assessment.suggestions.is_empty());
                    // Reasons and suggestions are 1:1
                    prop_assert_eq!(assessment.reasons.len(), assessment.suggestions.len());
                }
            }

            /// Recommendations count never exceeds total reasons across all deps.
            #[test]
            fn recommendations_bounded_by_reasons(
                deps in prop::collection::vec(arb_external_dependency(), 0..=5),
            ) {
                let recs = generate_recommendations(&deps);
                let total_reasons: usize = deps
                    .iter()
                    .map(|d| assess_mockability(d).reasons.len())
                    .sum();
                prop_assert!(recs.len() <= total_reasons);
            }

            /// Assessment serializes to valid JSON.
            #[test]
            fn assessment_serializes(dep in arb_external_dependency()) {
                let assessment = assess_mockability(&dep);
                let json = serde_json::to_value(&assessment);
                prop_assert!(json.is_ok(), "serialization failed: {:?}", json.err());
            }

            /// Recommendation serializes to valid JSON.
            #[test]
            fn recommendation_serializes(
                deps in prop::collection::vec(arb_external_dependency(), 1..=3),
            ) {
                let recs = generate_recommendations(&deps);
                for rec in &recs {
                    let json = serde_json::to_value(rec);
                    prop_assert!(json.is_ok(), "serialization failed: {:?}", json.err());
                }
            }

            /// format_recommendations produces non-empty output iff there are recs.
            #[test]
            fn format_empty_iff_no_recs(
                deps in prop::collection::vec(arb_external_dependency(), 0..=5),
            ) {
                let recs = generate_recommendations(&deps);
                let output = format_recommendations(&recs);
                if recs.is_empty() {
                    prop_assert!(output.is_empty());
                } else {
                    prop_assert!(output.starts_with("Refactoring Recommendations:\n"));
                }
            }
        }
    }
}
