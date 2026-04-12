//! Adapter selection policy: merges explicit config with frontend-reported
//! hints according to a deterministic precedence.
//!
//! Precedence (highest to lowest):
//! 1. Explicit config (`Required` / `Auto` / `None` → active; `Disabled` → rejected)
//! 2. Transparent auto-apply (hint with `Auto` + `High` confidence)
//! 3. Suggest-only (everything else)

use serde::{Deserialize, Serialize};

use crate::nondeterminism::Confidence;
use crate::protocol::{AdapterHint, ExecutionAdapter, ExecutionAdapterApply, ExecutionProfile};

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Why an adapter was activated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionProvenance {
    /// User explicitly configured this adapter.
    ExplicitConfig,
    /// Transparent adapter auto-applied due to strong evidence.
    AutoApplied,
}

/// An adapter that will be active during execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectedAdapter {
    pub adapter: ExecutionAdapter,
    pub provenance: SelectionProvenance,
    /// Human-readable reasons explaining why this adapter is active.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
}

/// An adapter suggested but not activated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuggestedAdapter {
    pub adapter: ExecutionAdapter,
    pub confidence: Confidence,
    /// Human-readable reasons from the hint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
}

/// An adapter that was explicitly rejected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RejectedAdapter {
    pub adapter_id: String,
    pub reason: String,
}

/// Result of adapter selection policy evaluation.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AdapterSelectionResult {
    /// Adapters that will be active (in execution order).
    pub active: Vec<SelectedAdapter>,
    /// Adapters suggested to the user but not auto-activated.
    pub suggested: Vec<SuggestedAdapter>,
    /// Adapters explicitly disabled or conflicting.
    pub rejected: Vec<RejectedAdapter>,
}

impl AdapterSelectionResult {
    /// Build an execution profile from active adapters, or `None` if empty.
    pub fn to_execution_profile(&self) -> Option<ExecutionProfile> {
        if self.active.is_empty() {
            return None;
        }
        Some(ExecutionProfile {
            adapters: self.active.iter().map(|s| s.adapter.clone()).collect(),
        })
    }

    /// True when there are no active or suggested adapters.
    pub fn is_empty(&self) -> bool {
        self.active.is_empty() && self.suggested.is_empty()
    }
}

/// Error from adapter selection.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdapterSelectionError {
    /// Two required adapters (from explicit config) conflict with each other.
    #[error("required adapter `{adapter_id}` conflicts with active adapter `{conflicting_id}`")]
    RequiredConflict {
        adapter_id: String,
        conflicting_id: String,
    },
}

// ---------------------------------------------------------------------------
// Selection function
// ---------------------------------------------------------------------------

/// Merge explicit config with frontend-reported hints into a deterministic
/// selection result.
///
/// # Precedence
///
/// 1. **Explicit config** adapters (from `config_profile`) are always honoured:
///    `Required`/`Auto`/`None` → active, `Disabled` → rejected.
/// 2. **Hints not covered by config**: `Auto` + `High` confidence → active
///    (transparent auto-apply). Everything else → suggested.
/// 3. **Conflict resolution**: if two active adapters conflict and both are
///    from config, this is an error. If one is auto-applied, it is demoted to
///    suggested.
/// 4. **Requirement promotion**: if an active adapter requires another adapter
///    that is eligible for auto-apply (Auto + High), the required adapter is
///    promoted to active.
pub fn select_adapters(
    config_profile: Option<&ExecutionProfile>,
    hints: &[AdapterHint],
) -> Result<AdapterSelectionResult, AdapterSelectionError> {
    let mut result = AdapterSelectionResult::default();

    // Index hints by adapter id for quick lookup.
    let hint_by_id: std::collections::HashMap<&str, &AdapterHint> =
        hints.iter().map(|h| (h.adapter.id.as_str(), h)).collect();

    // Track which adapter ids are already processed.
    let mut processed: std::collections::HashSet<String> = std::collections::HashSet::new();

    // --- Phase 1: explicit config adapters ---
    if let Some(profile) = config_profile {
        for adapter in &profile.adapters {
            if !processed.insert(adapter.id.clone()) {
                // Duplicate adapter ID in config — first occurrence wins.
                continue;
            }

            if adapter.apply == Some(ExecutionAdapterApply::Disabled) {
                result.rejected.push(RejectedAdapter {
                    adapter_id: adapter.id.clone(),
                    reason: "explicitly disabled".into(),
                });
                continue;
            }

            // Active: Required, Auto, or None (defaults to active for explicit config).
            let reasons = hint_by_id
                .get(adapter.id.as_str())
                .map(|h| h.reasons.clone())
                .unwrap_or_default();

            result.active.push(SelectedAdapter {
                adapter: adapter.clone(),
                provenance: SelectionProvenance::ExplicitConfig,
                reasons,
            });
        }
    }

    // --- Phase 2: hints not covered by config ---
    for hint in hints {
        if processed.contains(&hint.adapter.id) {
            continue;
        }
        processed.insert(hint.adapter.id.clone());

        if hint.adapter.apply == Some(ExecutionAdapterApply::Disabled) {
            continue;
        }

        if is_auto_apply_eligible(hint) {
            result.active.push(SelectedAdapter {
                adapter: hint.adapter.clone(),
                provenance: SelectionProvenance::AutoApplied,
                reasons: hint.reasons.clone(),
            });
        } else {
            result.suggested.push(SuggestedAdapter {
                adapter: hint.adapter.clone(),
                confidence: hint.confidence,
                reasons: hint.reasons.clone(),
            });
        }
    }

    // --- Phase 3: conflict resolution ---
    resolve_conflicts(&mut result, &hint_by_id)?;

    // --- Phase 4: requirement promotion ---
    promote_requirements(&mut result, &hint_by_id);

    Ok(result)
}

/// An adapter hint is eligible for transparent auto-apply when its policy is
/// `Auto` and the frontend reported `High` confidence.
fn is_auto_apply_eligible(hint: &AdapterHint) -> bool {
    hint.adapter.apply == Some(ExecutionAdapterApply::Auto) && hint.confidence == Confidence::High
}

/// Check for conflicts between active adapters. If both are from config,
/// return an error. If one is auto-applied, demote it to suggested.
fn resolve_conflicts(
    result: &mut AdapterSelectionResult,
    hint_by_id: &std::collections::HashMap<&str, &AdapterHint>,
) -> Result<(), AdapterSelectionError> {
    // Collect conflict pairs to process.
    let mut demotions: Vec<usize> = Vec::new();

    for (i, selected) in result.active.iter().enumerate() {
        let Some(hint) = hint_by_id.get(selected.adapter.id.as_str()) else {
            continue;
        };
        for conflict in &hint.conflicts {
            // Find if the conflicting adapter is also active.
            if let Some((j, other)) = result
                .active
                .iter()
                .enumerate()
                .find(|(_, a)| a.adapter.id == conflict.adapter_id)
            {
                if i == j {
                    continue;
                }
                // Both from config → error.
                if selected.provenance == SelectionProvenance::ExplicitConfig
                    && other.provenance == SelectionProvenance::ExplicitConfig
                {
                    return Err(AdapterSelectionError::RequiredConflict {
                        adapter_id: selected.adapter.id.clone(),
                        conflicting_id: other.adapter.id.clone(),
                    });
                }
                // Demote the auto-applied one.
                let demote_idx = if selected.provenance == SelectionProvenance::AutoApplied {
                    i
                } else {
                    j
                };
                if !demotions.contains(&demote_idx) {
                    demotions.push(demote_idx);
                }
            }
        }
    }

    // Apply demotions (remove from active, add to suggested) in reverse order
    // to preserve indices.
    demotions.sort_unstable();
    demotions.dedup();
    for idx in demotions.into_iter().rev() {
        let removed = result.active.remove(idx);
        let confidence = hint_by_id
            .get(removed.adapter.id.as_str())
            .map(|h| h.confidence)
            .unwrap_or(Confidence::Low);
        result.suggested.push(SuggestedAdapter {
            adapter: removed.adapter,
            confidence,
            reasons: removed.reasons,
        });
    }

    Ok(())
}

/// If an active adapter requires another adapter that is not yet active but
/// is eligible for auto-apply, promote it from suggested to active.
fn promote_requirements(
    result: &mut AdapterSelectionResult,
    hint_by_id: &std::collections::HashMap<&str, &AdapterHint>,
) {
    let active_ids: std::collections::HashSet<&str> = result
        .active
        .iter()
        .map(|a| a.adapter.id.as_str())
        .collect();

    // Collect promotions to apply after iteration.
    let mut to_promote: Vec<String> = Vec::new();

    for selected in &result.active {
        let Some(hint) = hint_by_id.get(selected.adapter.id.as_str()) else {
            continue;
        };
        for req in &hint.requirements {
            if active_ids.contains(req.adapter_id.as_str()) {
                continue;
            }
            // Skip adapters that were explicitly disabled (rejected).
            if result.rejected.iter().any(|r| r.adapter_id == req.adapter_id) {
                continue;
            }
            // Check if the required adapter has a hint eligible for auto-apply.
            if let Some(req_hint) = hint_by_id.get(req.adapter_id.as_str())
                && is_auto_apply_eligible(req_hint)
                && !to_promote.contains(&req.adapter_id)
            {
                to_promote.push(req.adapter_id.clone());
            }
        }
    }

    // Move promoted adapters from suggested to active.
    for adapter_id in &to_promote {
        if let Some(pos) = result
            .suggested
            .iter()
            .position(|s| &s.adapter.id == adapter_id)
        {
            let removed = result.suggested.remove(pos);
            result.active.push(SelectedAdapter {
                adapter: removed.adapter,
                provenance: SelectionProvenance::AutoApplied,
                reasons: removed.reasons,
            });
        } else if let Some(hint) = hint_by_id.get(adapter_id.as_str()) {
            // Not in suggested yet (e.g. was already processed as active or
            // skipped). Add directly from the hint.
            result.active.push(SelectedAdapter {
                adapter: hint.adapter.clone(),
                provenance: SelectionProvenance::AutoApplied,
                reasons: hint.reasons.clone(),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Display helpers
// ---------------------------------------------------------------------------

impl std::fmt::Display for SelectionProvenance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExplicitConfig => write!(f, "config"),
            Self::AutoApplied => write!(f, "auto-applied"),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nondeterminism::Confidence;
    use crate::protocol::AdapterRelation;

    fn adapter(id: &str, apply: Option<ExecutionAdapterApply>) -> ExecutionAdapter {
        ExecutionAdapter {
            id: id.into(),
            apply,
            options: None,
        }
    }

    fn hint(id: &str, apply: Option<ExecutionAdapterApply>, confidence: Confidence) -> AdapterHint {
        AdapterHint {
            adapter: adapter(id, apply),
            confidence,
            reasons: vec![format!("detected {id}")],
            requirements: vec![],
            conflicts: vec![],
        }
    }

    #[test]
    fn empty_inputs_produces_empty_result() {
        let result = select_adapters(None, &[]).unwrap();
        assert!(result.is_empty());
        assert!(result.active.is_empty());
        assert!(result.suggested.is_empty());
        assert!(result.rejected.is_empty());
        assert_eq!(result.to_execution_profile(), None);
    }

    #[test]
    fn config_required_always_active() {
        let profile = ExecutionProfile {
            adapters: vec![adapter(
                "ts/react-hooks",
                Some(ExecutionAdapterApply::Required),
            )],
        };
        let result = select_adapters(Some(&profile), &[]).unwrap();
        assert_eq!(result.active.len(), 1);
        assert_eq!(result.active[0].adapter.id, "ts/react-hooks");
        assert_eq!(
            result.active[0].provenance,
            SelectionProvenance::ExplicitConfig
        );
    }

    #[test]
    fn config_auto_always_active() {
        let profile = ExecutionProfile {
            adapters: vec![adapter("ts/browser-dom", Some(ExecutionAdapterApply::Auto))],
        };
        let result = select_adapters(Some(&profile), &[]).unwrap();
        assert_eq!(result.active.len(), 1);
        assert_eq!(result.active[0].adapter.id, "ts/browser-dom");
    }

    #[test]
    fn config_none_apply_defaults_to_active() {
        let profile = ExecutionProfile {
            adapters: vec![adapter("ts/module-resolution", None)],
        };
        let result = select_adapters(Some(&profile), &[]).unwrap();
        assert_eq!(result.active.len(), 1);
    }

    #[test]
    fn config_disabled_goes_to_rejected() {
        let profile = ExecutionProfile {
            adapters: vec![adapter(
                "ts/react-hooks",
                Some(ExecutionAdapterApply::Disabled),
            )],
        };
        let result = select_adapters(Some(&profile), &[]).unwrap();
        assert!(result.active.is_empty());
        assert_eq!(result.rejected.len(), 1);
        assert_eq!(result.rejected[0].adapter_id, "ts/react-hooks");
        assert_eq!(result.rejected[0].reason, "explicitly disabled");
    }

    #[test]
    fn hint_auto_high_becomes_active() {
        let hints = vec![hint(
            "ts/browser-dom",
            Some(ExecutionAdapterApply::Auto),
            Confidence::High,
        )];
        let result = select_adapters(None, &hints).unwrap();
        assert_eq!(result.active.len(), 1);
        assert_eq!(result.active[0].adapter.id, "ts/browser-dom");
        assert_eq!(
            result.active[0].provenance,
            SelectionProvenance::AutoApplied
        );
    }

    #[test]
    fn hint_auto_medium_becomes_suggested() {
        let hints = vec![hint(
            "ts/browser-dom",
            Some(ExecutionAdapterApply::Auto),
            Confidence::Medium,
        )];
        let result = select_adapters(None, &hints).unwrap();
        assert!(result.active.is_empty());
        assert_eq!(result.suggested.len(), 1);
        assert_eq!(result.suggested[0].adapter.id, "ts/browser-dom");
    }

    #[test]
    fn hint_suggest_always_suggested() {
        let hints = vec![hint(
            "ts/react-hooks",
            Some(ExecutionAdapterApply::Suggest),
            Confidence::High,
        )];
        let result = select_adapters(None, &hints).unwrap();
        assert!(result.active.is_empty());
        assert_eq!(result.suggested.len(), 1);
    }

    #[test]
    fn hint_disabled_is_skipped() {
        let hints = vec![hint(
            "ts/react-hooks",
            Some(ExecutionAdapterApply::Disabled),
            Confidence::High,
        )];
        let result = select_adapters(None, &hints).unwrap();
        assert!(result.active.is_empty());
        assert!(result.suggested.is_empty());
        assert!(result.rejected.is_empty());
    }

    #[test]
    fn config_overrides_hint() {
        let profile = ExecutionProfile {
            adapters: vec![adapter(
                "ts/react-hooks",
                Some(ExecutionAdapterApply::Required),
            )],
        };
        // Hint says suggest, but config says required → config wins.
        let hints = vec![hint(
            "ts/react-hooks",
            Some(ExecutionAdapterApply::Suggest),
            Confidence::Medium,
        )];
        let result = select_adapters(Some(&profile), &hints).unwrap();
        assert_eq!(result.active.len(), 1);
        assert_eq!(
            result.active[0].provenance,
            SelectionProvenance::ExplicitConfig
        );
        // Hint reasons are attached to the config adapter.
        assert!(!result.active[0].reasons.is_empty());
        assert!(result.suggested.is_empty());
    }

    #[test]
    fn config_disable_overrides_auto_hint() {
        let profile = ExecutionProfile {
            adapters: vec![adapter(
                "ts/browser-dom",
                Some(ExecutionAdapterApply::Disabled),
            )],
        };
        let hints = vec![hint(
            "ts/browser-dom",
            Some(ExecutionAdapterApply::Auto),
            Confidence::High,
        )];
        let result = select_adapters(Some(&profile), &hints).unwrap();
        assert!(result.active.is_empty());
        assert_eq!(result.rejected.len(), 1);
    }

    #[test]
    fn conflict_between_config_adapters_is_error() {
        let profile = ExecutionProfile {
            adapters: vec![
                adapter("ts/browser-dom-a", Some(ExecutionAdapterApply::Required)),
                adapter("ts/browser-dom-b", Some(ExecutionAdapterApply::Required)),
            ],
        };
        let hints = vec![AdapterHint {
            adapter: adapter("ts/browser-dom-a", Some(ExecutionAdapterApply::Required)),
            confidence: Confidence::High,
            reasons: vec![],
            requirements: vec![],
            conflicts: vec![AdapterRelation {
                adapter_id: "ts/browser-dom-b".into(),
                reason: Some("mutually exclusive".into()),
            }],
        }];
        let err = select_adapters(Some(&profile), &hints).unwrap_err();
        assert!(matches!(
            err,
            AdapterSelectionError::RequiredConflict { .. }
        ));
    }

    #[test]
    fn conflict_demotes_auto_applied() {
        let profile = ExecutionProfile {
            adapters: vec![adapter(
                "ts/browser-dom-a",
                Some(ExecutionAdapterApply::Required),
            )],
        };
        let hints = vec![
            AdapterHint {
                adapter: adapter("ts/browser-dom-a", Some(ExecutionAdapterApply::Required)),
                confidence: Confidence::High,
                reasons: vec![],
                requirements: vec![],
                conflicts: vec![AdapterRelation {
                    adapter_id: "ts/browser-dom-b".into(),
                    reason: None,
                }],
            },
            hint(
                "ts/browser-dom-b",
                Some(ExecutionAdapterApply::Auto),
                Confidence::High,
            ),
        ];
        let result = select_adapters(Some(&profile), &hints).unwrap();
        // browser-dom-a is active (config), browser-dom-b is demoted to suggested.
        assert_eq!(result.active.len(), 1);
        assert_eq!(result.active[0].adapter.id, "ts/browser-dom-a");
        assert_eq!(result.suggested.len(), 1);
        assert_eq!(result.suggested[0].adapter.id, "ts/browser-dom-b");
    }

    #[test]
    fn requirement_promotes_eligible_hint() {
        let hints = vec![
            AdapterHint {
                adapter: adapter("ts/react-hooks", Some(ExecutionAdapterApply::Auto)),
                confidence: Confidence::High,
                reasons: vec!["uses hooks".into()],
                requirements: vec![AdapterRelation {
                    adapter_id: "ts/module-resolution".into(),
                    reason: Some("needs path resolution".into()),
                }],
                conflicts: vec![],
            },
            hint(
                "ts/module-resolution",
                Some(ExecutionAdapterApply::Auto),
                Confidence::High,
            ),
        ];
        let result = select_adapters(None, &hints).unwrap();
        // Both should be active.
        let active_ids: Vec<&str> = result
            .active
            .iter()
            .map(|a| a.adapter.id.as_str())
            .collect();
        assert!(active_ids.contains(&"ts/react-hooks"));
        assert!(active_ids.contains(&"ts/module-resolution"));
    }

    #[test]
    fn disabled_not_overridden_by_requirement() {
        // Regression: str-lj0h — an explicitly Disabled adapter was re-added
        // to active when another active adapter listed it as a requirement.
        let profile = ExecutionProfile {
            adapters: vec![
                adapter("ts/browser-dom", Some(ExecutionAdapterApply::Disabled)),
                adapter("ts/react-hooks", None),
            ],
        };
        let hints = vec![
            AdapterHint {
                adapter: adapter("ts/react-hooks", Some(ExecutionAdapterApply::Auto)),
                confidence: Confidence::High,
                reasons: vec!["uses hooks".into()],
                requirements: vec![AdapterRelation {
                    adapter_id: "ts/browser-dom".into(),
                    reason: Some("needs DOM".into()),
                }],
                conflicts: vec![],
            },
            hint(
                "ts/browser-dom",
                Some(ExecutionAdapterApply::Auto),
                Confidence::High,
            ),
        ];
        let result = select_adapters(Some(&profile), &hints).unwrap();
        assert!(
            !result.active.iter().any(|a| a.adapter.id == "ts/browser-dom"),
            "disabled adapter ts/browser-dom should not be in active"
        );
        assert_eq!(result.rejected.len(), 1);
        assert_eq!(result.rejected[0].adapter_id, "ts/browser-dom");
    }

    #[test]
    fn to_execution_profile_preserves_order() {
        let profile = ExecutionProfile {
            adapters: vec![
                adapter(
                    "ts/module-resolution",
                    Some(ExecutionAdapterApply::Required),
                ),
                adapter("ts/browser-dom", Some(ExecutionAdapterApply::Required)),
            ],
        };
        let result = select_adapters(Some(&profile), &[]).unwrap();
        let ep = result.to_execution_profile().unwrap();
        assert_eq!(ep.adapters[0].id, "ts/module-resolution");
        assert_eq!(ep.adapters[1].id, "ts/browser-dom");
    }

    #[test]
    fn serialization_roundtrip() {
        let result = AdapterSelectionResult {
            active: vec![SelectedAdapter {
                adapter: adapter("ts/browser-dom", Some(ExecutionAdapterApply::Auto)),
                provenance: SelectionProvenance::AutoApplied,
                reasons: vec!["uses window".into()],
            }],
            suggested: vec![SuggestedAdapter {
                adapter: adapter("ts/react-hooks", Some(ExecutionAdapterApply::Suggest)),
                confidence: Confidence::Medium,
                reasons: vec!["imports react".into()],
            }],
            rejected: vec![RejectedAdapter {
                adapter_id: "ts/old-adapter".into(),
                reason: "explicitly disabled".into(),
            }],
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: AdapterSelectionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, deserialized);
    }

    // -----------------------------------------------------------------------
    // Property tests
    // -----------------------------------------------------------------------

    #[cfg(test)]
    mod prop {
        use super::*;
        use crate::test_arbitraries::{arb_adapter_hint, arb_execution_profile};
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn config_adapters_never_silently_dropped(
                profile in arb_execution_profile(),
                hints in proptest::collection::vec(arb_adapter_hint(), 0..5),
            ) {
                let result = select_adapters(Some(&profile), &hints);
                match result {
                    Ok(r) => {
                        for adapter in &profile.adapters {
                            let in_active = r.active.iter().any(|a| a.adapter.id == adapter.id);
                            let in_rejected = r.rejected.iter().any(|a| a.adapter_id == adapter.id);
                            prop_assert!(
                                in_active || in_rejected,
                                "config adapter {} not in active or rejected",
                                adapter.id
                            );
                        }
                    }
                    Err(AdapterSelectionError::RequiredConflict { .. }) => {
                        // Conflict errors are valid — they represent
                        // explicitly surfaced failures.
                    }
                }
            }

            #[test]
            fn disabled_never_active(
                profile in arb_execution_profile(),
                hints in proptest::collection::vec(arb_adapter_hint(), 0..5),
            ) {
                if let Ok(r) = select_adapters(Some(&profile), &hints) {
                    // Build first-occurrence map: only the first config entry
                    // per adapter ID determines the effective policy.
                    let mut first_apply: std::collections::HashMap<&str, Option<&ExecutionAdapterApply>> =
                        std::collections::HashMap::new();
                    for adapter in &profile.adapters {
                        first_apply
                            .entry(&adapter.id)
                            .or_insert(adapter.apply.as_ref());
                    }
                    for (id, apply) in &first_apply {
                        if *apply == Some(&ExecutionAdapterApply::Disabled) {
                            prop_assert!(
                                !r.active.iter().any(|a| a.adapter.id == *id),
                                "disabled adapter {} found in active",
                                id
                            );
                        }
                    }
                }
            }

            #[test]
            fn deterministic(
                profile in proptest::option::of(arb_execution_profile()),
                hints in proptest::collection::vec(arb_adapter_hint(), 0..5),
            ) {
                let r1 = select_adapters(profile.as_ref(), &hints);
                let r2 = select_adapters(profile.as_ref(), &hints);
                prop_assert_eq!(r1, r2);
            }

            #[test]
            fn serialization_roundtrip_prop(
                profile in proptest::option::of(arb_execution_profile()),
                hints in proptest::collection::vec(arb_adapter_hint(), 0..3),
            ) {
                if let Ok(result) = select_adapters(profile.as_ref(), &hints) {
                    let json = serde_json::to_string(&result).unwrap();
                    let deserialized: AdapterSelectionResult =
                        serde_json::from_str(&json).unwrap();
                    prop_assert_eq!(result, deserialized);
                }
            }
        }
    }
}
