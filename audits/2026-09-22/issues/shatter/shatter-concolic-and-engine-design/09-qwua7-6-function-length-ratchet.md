---
slug: qwua7-6-function-length-ratchet
kind: note-to-existing
title: "Note on str-qwua7.6: explore_with_oracle grew 1,307 -> 1,372 lines; add a function-length ratchet"
priority: P3
type: note
labels: [audit-2026-09-22, tech-debt, shatter-core]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.6
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.6: explore_with_oracle grew 1,307 -> 1,372 lines; add a function-length ratchet

**Target:** `str-qwua7.6` (open). Action: append the comment below. Do not create a new issue. Suggested priority for the target is unchanged. This note adds P3 scope.

## Comment text

Audit 2026-09-22 note (finding core-15; evidence `audits/2026-09-22/areas/core-engine.md`).

**Current sizes** (audit HEAD 56c86168; brace-matched from the `fn` line to the closing `}`):

- `orchestrator::explore_with_oracle` (`shatter-core/src/orchestrator.rs:2490`): **1,372 lines**, up from 1,307 at the 2026-09-04 audit.
- `scan_orchestrator::parallel_scan_with_progress` (`scan_orchestrator.rs:3806`): 1,346.
- `explorer::explore_function` (`explorer.rs:1012`): 965.
- `scan_orchestrator::run_layer_batched` (`scan_orchestrator.rs:2461`): 476.

The shrink-witness selection block is still duplicated between `explorer.rs:~1700-1740` and `orchestrator.rs:~3561-3600`; both call `hash_branch_path`.

Features keep landing in the god function while this epic's P1 children stay open.

**Proposed additions to this issue's acceptance:**

- [ ] Add a function-length ratchet to `task check-static`. A script records the current length of each listed function (at least the four above) in a checked-in baseline and fails when any of them grows. Shrinking updates the baseline. Proof: the script fails on a branch that adds a line to `explore_with_oracle`.
- [ ] After str-qwua7.6.1 (`select_witnesses`) lands, plan the phase extraction of `explore_with_oracle` (probe, main loop, refine, shrink) as child issues. str-qwua7.6.3 covers the CLI's `run_explore`, not this function.
