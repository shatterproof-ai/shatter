---
slug: engine-path-identity-budget-config
kind: new
title: "Random explorer and concolic orchestrator use different path identities (bucketed path_hash vs raw hash_branch_path); define one canonical identity and test it on shared traces"
priority: P2
type: task
labels: [audit-2026-09-22, architecture, parity, explorer, orchestrator]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Random explorer and concolic orchestrator use different path identities (bucketed path_hash vs raw hash_branch_path); define one canonical identity and test it on shared traces

Scope note: the slug is kept from the earlier combined draft. This issue now covers **path identity only**. Budget semantics moved to explore-budget-semantics. The scan/observe `orchestrator::ExploreConfig` literals moved to a note on str-qwua7.6.2 (qwua7-6-2-scan-observe-config-literals). Relocating `hash_branch_path` into a leaf module is already str-qwua7.29 and is not repeated here.

## Problem

The two engines decide "is this a new path?" with different functions:

- The random explorer uses `path_hash` (`shatter-core/src/explorer.rs:566`): scope-aware, loop iterations collapsed into buckets (`LoopBuckets`), with a `legacy_path_hash` fallback (`:575`) over lines, error and return value when there is no `branch_path`.
- The concolic orchestrator uses `hash_branch_path` (`shatter-core/src/orchestrator.rs:864`): the raw `(branch_id, taken)` sequence through `DefaultHasher`, with no loop bucketing and no fallback. It drives the unique-path budget (`orchestrator.rs:1621-1627`, via `:1736`) and fuzz-phase novelty (`:3002`, `:3113`).
- The random explorer's own shrink-witness selection calls the concolic hash, `crate::orchestrator::hash_branch_path`, at `explorer.rs:1704`, `:1788`, `:1833` and `:1886`. So one engine uses two identities.

Consequences: "paths" in a report mean different things depending on `--concolic`. A loop function inflates the concolic path count, which also consumes the concolic unique-path budget. The random explorer's shrinker can pick a witness for a path that its own explorer treats as a duplicate, or the reverse.

## Evidence

The audit's fixture (`audits/2026-09-22/areas/core-engine.md:10-25`; not in any test directory):

```go
func Loopy(n int) int {
    s := 0
    for i := 0; i < n && i < 50; i++ { s += i }
    if s > 100 { return 1 }
    return 0
}
```

Result: 16 paths under `--concolic` (40 iterations) against 6 under random (100 iterations), for 2 return behaviours. The different budgets and inputs mean this comparison **illustrates** the problem but does not prove it. The proof in the acceptance criteria below uses identical traces.

## Acceptance criteria

- [ ] A written definition of the canonical path identity, in the doc comment of the one function that computes it. It states whether loops are bucketed (and with which `LoopBuckets`), how scopes are treated, and what the fallback is when `branch_path` is empty. The default proposal is the random explorer's `path_hash` semantics. Choosing otherwise needs a reason in the close note.
- [ ] Both engines (the random explorer's novelty check, the concolic orchestrator's unique-path budget and fuzz novelty, and both shrink-witness selections) call that one function. `grep -n "hash_branch_path" shatter-core/src` finds no call site used for path novelty or witness selection. Remaining uses, if any, are listed in the close note with the reason.
- [ ] Trace-level tests feed the **same** `ExecuteResult` values to the identity function as each engine calls it, and assert equal identities. Cases:
  - two Loopy traces with 3 and 4 loop iterations that fall in the same bucket (same identity);
  - two traces whose loop counts fall in different buckets (different identities);
  - a trace with an empty `branch_path` (fallback path);
  - a trace with nested scopes.

  These tests do not compare discovered path counts between engines, because the two engines legitimately explore different inputs under finite budgets.
- [ ] Loopy is added as a checked-in fixture (for example `examples/go/` via the examples repo, or a core test fixture directory), with its expected distinct identities (2 return behaviours; the bucketed loop-count identities written out).
- [ ] Engine-level coverage expectations are separate from identity: an engine-parity row (engine-parity-e2e) asserts that both engines reach both return behaviours of Loopy under a stated budget, not that they report equal path counts.
- [ ] Proof at close: the trace-level test fails on current main for the concolic call site (paste the assertion output) and passes after the change, and forced (uncached) `task e2e` output.

## Suggested approach

If str-qwua7.29 has landed, put the canonical function in its leaf module. If not, put it in `explorer.rs` next to `path_hash` and let str-qwua7.29 move it. Either way, this issue changes semantics and call sites, and str-qwua7.29 owns module location.

## Out of scope

- Moving `hash_branch_path` or other leaf types out of `orchestrator.rs` (str-qwua7.29).
- `--max-iterations` and `max_executions` semantics (explore-budget-semantics).
- Unifying the `orchestrator::ExploreConfig` construction (str-qwua7.6.2; see qwua7-6-2-scan-observe-config-literals).
- Splitting `explore_with_oracle` (str-qwua7.6).

## Metadata

- Priority: P2
- Type: task
- Labels: audit-2026-09-22, architecture, parity, explorer, orchestrator
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: str-qwua7.29 (open, owns relocation), str-qwua7.6.2, str-inct, explore-budget-semantics, engine-parity-e2e, float-probe-paths-uncounted
- Source findings: core-07 (draft shatter-code/17)
