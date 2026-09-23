---
slug: merge-push-observability
kind: new
title: "land.py merge_push: split into timed sub-steps and capture pre-push hook output"
priority: P3
type: feature
labels: [audit, land-work, observability]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [land-py-invocation-progress-log]
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land.py merge_push: split into timed sub-steps and capture pre-push hook output

## Problem

`merge_push` is one opaque step. In shatter it took 258.6, 259.8, 298.7, 309.2, 498.6, 594.4, 614.9, 678.8 and 1107.8 s. Most of that time is the consumer repo's pre-push hook: for `refs/heads/main`, shatter's pre-push runs the full gate (`task check`), straight after the verifier ran its own gates. land.py captures the git output but never shows the hook output, and records only the total time. Agents cannot tell whether the step is making progress or stuck, and they cannot see that the time is going into the repo's own gates.

## Evidence

Re-verified 2026-09-23 at bento origin/main b1bb787 (unchanged since 1c0c1e6). File: `catalog/skills/land-work/scripts/land.py`.

- `:309-313`: the whole `_merge_and_push()` call is timed as one `_record("merge_push", ...)`.
- `_merge_in_primary` (`:166-214`) runs fast-forward sync, `git merge --no-ff`, a tree check and `git push`. `_push_from_preview` (`:216-~271`) runs `git commit`, `git push`, then `git fetch` and `git merge --ff-only` to sync the primary. Every call goes through `git(...)` with captured output. Push stderr, which carries the hook output, appears only inside a `StepFailure` message on failure and is never shown on success.
- Shatter `.git/hooks/pre-push` sets `SHATTER_GATE_RANK=2` for `refs/heads/main` (lines 70-71; audit verifier note on bento-18), which selects the full gate.

## Acceptance criteria

- [ ] The step line reports sub-durations, for example `merge_push: passed (620s; merge 3s, push 610s, sync 7s)`, and the final JSON has `merge_push_substeps: [{name, seconds, status}]`. The sub-steps cover whichever route ran (merge-in-primary or push-from-preview).
- [ ] Push stdout and stderr, including hook output, stream to the landing progress log from `land-py-invocation-progress-log` as they arrive, not only after the push ends. On failure, the last N lines appear in the JSON as `push_output_tail`.
- [ ] Optional verifier.json flag `push_hook_runs_gates: true`. When it is set, land.py prints `pre-push hook is running repo gates (this can take minutes)` before pushing, so the time is explained up front.
- [ ] Tests: a fake remote with a pre-push hook that prints a marker and sleeps. Assert that the sub-durations are present and add up to roughly the total, that the marker appears in the progress log, and that on a failing hook it appears in `push_output_tail`.
- [ ] Proof at close: test names plus passing output. "Merged" is not sufficient.

## Out of scope

- Whether consumer repos should keep double gating (verifier plus pre-push). That is the consumer's decision (shatter str-qwua7.55).
- Admission control for the push step (bento-dyp7; see `dyp7-admission-control`).

## Dependencies

- Blocked by: `land-py-invocation-progress-log` (the progress log the hook output streams into).
- Related: bento-rdtn.14 (closed; one line per step), `git-hook-latency-visibility` (bucket bento-guards-doctor-tracker; hook timing in other scripts).

Priority: P3 · Type: feature · Labels: audit, land-work, observability · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/20, bento-18
