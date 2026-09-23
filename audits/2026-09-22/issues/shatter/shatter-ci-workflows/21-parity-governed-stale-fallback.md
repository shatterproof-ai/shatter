---
slug: parity-governed-stale-fallback
kind: new
title: "parity-governed keeps a dead 'pending str-7jgm.2' fallback that would turn a deleted validate-parity.py into a silent skip"
priority: P3
type: chore
labels: [quality-gates, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# parity-governed keeps a dead 'pending str-7jgm.2' fallback that would turn a deleted validate-parity.py into a silent skip

## Problem

The `parity-governed` task runs `scripts/validate-parity.py` only if the file exists. Otherwise it prints `[skip] validate-parity.py not present (pending str-7jgm.2)` and carries on. The script exists and str-7jgm.2 is closed, so the fallback is dead code. Its only remaining effect is that deleting or renaming the script would turn the parity check into a silent skip instead of a gate failure.

This was split out of `nextest-ci-profile-and-stale-parity-fallback`, because it is unrelated to the test-runner work.

## Evidence

Re-verified 2026-09-23 against the audit worktree (main 70465921 plus audit files):

- `Taskfile.yml:265-276` (`parity-governed`):

  ```sh
  if [ -f scripts/validate-parity.py ]; then python3 scripts/validate-parity.py;
  else echo "[skip] validate-parity.py not present (pending str-7jgm.2)"; fi
  ```

- `scripts/validate-parity.py` exists. `bd show str-7jgm.2` shows it CLOSED.

## Acceptance criteria

- [ ] The `if`/`else` is replaced by an unconditional `python3 scripts/validate-parity.py`.
- [ ] Proof that a missing script now fails the gate: on a scratch branch, rename the script and run `task parity --force` (or clear its checksum). The run exits non-zero. Revert, rerun, and it passes with validate-parity's output visible. Paste both outputs in the close reason.
- [ ] A grep for `pending str-` in `Taskfile.yml` and `taskfiles/` finds no other fallback that references a closed issue. List any that are found; they are in scope here if trivial.

## Out of scope

- The nextest and CI runner work (`nextest-ci-profile-and-stale-parity-fallback`).
- The content of the parity checks.

## Dependencies

- None.
- Related: str-7jgm.2 (closed).

Priority: P3 · Type: chore · Labels: quality-gates, parity, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: gates-08 (split from nextest-ci-profile-and-stale-parity-fallback in revision)
