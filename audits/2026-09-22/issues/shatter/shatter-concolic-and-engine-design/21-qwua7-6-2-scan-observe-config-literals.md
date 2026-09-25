---
slug: qwua7-6-2-scan-observe-config-literals
kind: note-to-existing
title: "Note on str-qwua7.6.2: scan and observe also hand-build orchestrator::ExploreConfig, and the three literals already diverge"
priority: P1
type: note
labels: [audit-2026-09-22, architecture, parity, orchestrator]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.6.2
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.6.2: scan and observe also hand-build orchestrator::ExploreConfig, and the three literals already diverge

**Target:** `str-qwua7.6.2` (open P1, "Unify explorer::ExploreConfig and orchestrator::ExploreConfig behind a shared base"; verified with `bd show` on 2026-09-23). Action: append the comment below. Do not create a new issue. Priority unchanged.

## Comment text

Audit 2026-09-22 note (finding core-14; evidence `audits/2026-09-22/areas/core-engine.md`).

This issue names only the CLI explore translation (`explore.rs:5138-5171`). Two more non-test sites hand-build `orchestrator::ExploreConfig`:

- `shatter-cli/src/commands/explore.rs:5138-5169`
- `shatter-core/src/scan_orchestrator.rs:3080-3102`
- `shatter-cli/src/commands/observe.rs:107-130`

(`pipeline_orchestrator.rs:1342` is inside `mod tests`.)

They already disagree:

| Field | explore | scan | observe |
|---|---|---|---|
| `seed` | None | `explore_config.seed` | None |
| `refine_budget` | set | None | None |
| `default_execute_plan` | None | threaded | None |
| `mcdc` | flag | false | false |
| `fuzz` | resolved | default | default |
| `mocks` / `mock_params` | from config | from config | empty |
| `solver_timeout_ms` | from flag | from flag | None |
| `plateau_threshold` | 20 or 60 | 20 | 20 |

**Proposed additions to this issue's acceptance:**

- [ ] The shared constructor (`From<&ExploreConfig>` or equivalent) is used by explore, scan and observe. `grep -n "orchestrator::ExploreConfig {"` over non-test code in `shatter-cli/src` and `shatter-core/src` finds no struct literal.
- [ ] Each field in the table above either comes from the shared base or is set by a named, per-command override with a comment giving the reason. Unit tests assert that the shared fields round-trip for all three commands.
- [ ] The `max_executions` derivation is out of scope here; explore-budget-semantics (audit 2026-09-22) owns it. Coordinate so the constructor calls that issue's single budget function.
