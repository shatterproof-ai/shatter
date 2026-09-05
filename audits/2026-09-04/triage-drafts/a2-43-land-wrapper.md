---
repo: shatter
type: feature
priority: 2
labels: agents, landing, git
existing: none
---
# Triage: build a repo-local `land` wrapper now, or wait for the bento land-work fixes (bento-rdtn children)?

Triage: build a repo-local `land` wrapper now, or wait for the bento land-work fixes (bento-rdtn children)? Proposed default: **wait; re-evaluate after bento-rdtn closes.** This issue holds the evidence and the design so the re-evaluation is quick.

## Problem
Landing is the most repeated manual procedure in 252 sessions: agents type the same ~10-step sequence of bento scripts by hand, get one step wrong, and thrash. `land-work-prepare.py` fails 32% of the time and `verify-lease.py` 17%, mostly from ordering/state mistakes (dirty tree, lease moved, run from the primary checkout on `main`); create-preview was called 195 times against 90 cleanups, matching the five orphaned `/tmp/land-work-preview-*` worktrees. The same failure modes are filed against bento (preview cleanup, killed verifier, diverged primary, executed-vs-cached), and the repo already suffers from having three documented landing procedures — a fourth, repo-local one must become the *only* one or it adds to the problem.

## Current code facts
- Scripts (bento 2.2.6, `~/.claude/plugins/cache/bento/bento/2.2.6/skills/`): `launch-work/scripts/launch-work-verify.py`, `land-work/scripts/{land-work-prepare,land-work-create-preview,land-work-run-verifier,land-work-verify-lease,land-work-verify-landing,land-work-root-hygiene}.py`; project verifier manifest `.agent-plugins/bento/bento/land-work/verifier.json` → `scripts/land_work_verifier.sh`. The plugin cache is not repo-owned; the only repo-owned hook point is `verifier.json` and the AGENTS.md/skill text.
- Retrospective (session-retro.md §1, §3): exact 3-gram `git fetch → create-preview → run-verifier` 49×; `land-work-prepare.py` 18/57 failures, `land-work-verify-lease.py` 11/66, `launch-work-verify.py` 6/22; `land_work_flake` keyword in 16 sessions; 8 `fatal: this operation must be run in a work tree` since 2026-08-23 (primary checkout bare, p1-01).
- `scripts/land_work_verifier.sh` header claims "same gates ci.yml uses" but runs `test-standard + parity + conformance`; CI runs `task check`; no `timeout` in `verifier.json`; verifier output not captured on kill (memory `project_land_work_verifier_first_run_flake`).
- Memory `feedback_primary_checkout_diverged_local_main` documents a hand-rolled fourth landing path (merge inside the preview).
- bento-side items from this audit (bento-rdtn children): always-clean previews + verifier logs, native diverged-primary handling, executed-vs-cached assertion, launch-work/superpowers precedence note.

## Options
1. **Wait (proposed default)**: let bento-rdtn land the cleanup/log/diverged-primary/executed-vs-cached fixes in `land-work` itself; meanwhile fix the repo-owned pieces only — `land_work_verifier.sh` runs `task check` (or says what it runs), `verifier.json` gets a `timeout`, AGENTS.md's manual landing script is deleted (a2-38a) so `bento:land-work` is the single procedure. Re-evaluate when bento-rdtn closes: if prepare/lease failure rates are still >10% in the next retrospective, build option 2.
2. **Build now**: `scripts/land.sh <issue> <branch>` chaining prepare → `git fetch` → create-preview → run-verifier (tee to `<preview>/verifier.log`, timeout) → verify-lease → merge (explicit merge commit, from the preview when the primary is bare/diverged) → push → verify-landing → root-hygiene → cleanup with a `trap`; refuses on primary/`main` or dirty tree with an exact fix; exits non-zero when the verifier ran zero checks; `scripts/test_land.sh` dry-runs diverged-primary and dirty-tree paths. Pinned to bento 2.2.6 script paths, so it must be revalidated on every bento upgrade — the main cost of this option.

## Acceptance checks
- Decision recorded with the retrospective numbers that justified it.
- Either branch: exactly one documented landing procedure remains in the repo (AGENTS.md points at it); `land_work_verifier.sh`'s claim matches what it runs; `verifier.json` has a timeout.

## Scope
In: decision + the repo-owned verifier/manifest/doc pieces. Out: bento script changes (bento-rdtn).

## Size
small (wait) / medium (build).

## Provenance
Audit 2026-09-04, section 11, action item 43; evidence audits/2026-09-04/session-retro.md §1, §3 anti-pattern 3, §4; agent-system.md §2 items 5/6, rec 3.
