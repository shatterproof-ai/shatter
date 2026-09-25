# land.py merge_push: split into timed sub-steps and capture pre-push hook stderr

- Filing action: new issue
- Priority: P3
- Type: feature
- Labels: audit, land-work
- Parent: epic
- Links: related bento-rdtn.14
- Source findings: bento-18

---BODY---
## Problem

`merge_push` is one opaque step. In shatter it took 258.6, 259.8, 298.7, 309.2, 498.6, 594.4, 614.9, 678.8 and 1107.8 s. Most of that is the repo's pre-push hook: shatter's pre-push runs the full `task check` for refs/heads/main, right after the verifier ran its own gates. land.py captures the git output but never shows the hook output, so agents cannot tell whether the step is stuck.

## Current code facts

- `catalog/skills/land-work/scripts/land.py` about lines 190-262: the merge/commit/push/sync sequence is reported as a single `merge_push` step line with its total time.

## Acceptance criteria

- The step line reports commit, push and sync sub-durations, for example `merge_push: passed (620s; commit 3s, push 610s, sync 7s)`. The final JSON includes them.
- Push stderr, including hook output, is appended to the landing log (see the land.py progress-log issue), and its last N lines appear in the JSON on failure.
- Optional verifier.json flag `push_hook_runs_gates: true`, so land.py can print "pre-push hook running repo gates" before pushing.

## Out of scope

- Whether consumer repos should keep double gating. That is the consumer's decision (shatter str-qwua7.55).
