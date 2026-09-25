# land.py reports verifier.log as output_path after deleting it with the preview

- Filing action: new issue (follow-up to closed bento-rdtn.4 + bento-rdtn.14; rdtn.4's goal is not met on the land.py path)
- Priority: P1
- Type: bug
- Labels: audit, land-work
- Parent: epic
- Links: related bento-rdtn.4, related bento-rdtn.14
- Source findings: bento-02

---BODY---
## Problem

When the verifier fails under `land.py`, the reported `output_path` points into the preview worktree, and `land.py` has already deleted that worktree. So the agent cannot see why the verifier failed. In shatter it had to recreate a preview by hand and rerun the gates.

## Evidence

Shatter session 9f13ca23 had three verify failures: 2026-09-19 15:38, 09-20 23:49 and 09-20 23:55. Each final JSON reports `output_path=/tmp/land-work-preview-XXXX/.land-work/verifier.log`, which is reported after `cleanup: passed` and no longer exists. Previews seen include g663hhnu, jus6t3ug and mz7t9g27.

## Current code facts (bento @ 1c0c1e6, source under catalog/skills/land-work/scripts/)

- `land-work-run-verifier.py` defaults `--log` to `<candidate>/.land-work/verifier.log` (about line 351). It already computes `diagnostics.verifier_log_tail` (about line 138).
- `land.py` calls run-verifier without `--log`. At line 126 it raises `StepFailure(step, message, output_path=payload.get("verifier_log"))`.
- `land.py`'s failure path calls `cleanup_preview()` (about line 367), which removes the preview, before it emits the JSON (`exc.output_path` at line 374).
- `land.py` drops `verifier_log_tail`.

## Acceptance criteria

- After a failed verify step, `output_path` in land.py's final JSON names a file that exists.
- The failure JSON includes the last N lines of the log, using the existing `verifier_log_tail`.
- A regression test makes the verifier fail and asserts both points above.

## Suggested approach

Pass `--log $XDG_STATE_HOME/bento/land-work/<repo>/<branch>-<timestamp>.log` (falling back to `~/.local/state/...`) from land.py. Propagate `verifier_log_tail` into the StepFailure payload.

## Out of scope

- Shatter's own verifier discarding gate output. That is a shatter issue (str-qwua7.55). The verifier-contract issue in this epic covers the bento-side contract.
