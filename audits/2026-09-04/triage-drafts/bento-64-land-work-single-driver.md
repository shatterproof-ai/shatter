---
repo: bento
tracker: beads
type: feature
priority: 2
labels: land-work
existing: none
---
# land-work: ship one orchestrating driver for the whole landing sequence

## Decision (2026-09-06)
Shatter's audit chose to wait for this rather than build a repo-local wrapper (shatter draft a2-43): the orchestration belongs in bento because every land-work consumer has the same problem.

## Problem
land-work is a skill that agents follow step by step: prepare, `git fetch`, create-preview, run-verifier, verify-lease, merge, push, verify-landing, cleanup. In 252 shatter sessions (Jul–Sep 2026) the exact `git fetch → create-preview → run-verifier` 3-gram appears 49 times, previews were created 195 times against 90 cleanups, `land-work-prepare.py` failed 32% of invocations and `verify-lease.py` 17%, and the three longest, most-corrected sessions were all landing recoveries. Fixing each failure mode (bento-rdtn.3–.6, .13) still leaves an agent issuing nine commands by hand and improvising when one fails.

## Current code facts
- `~/project/bento/catalog/skills/land-work/scripts/` holds the steps as separate scripts: `land-work-prepare.py`, `land-work-create-preview.py`, `land-work-run-verifier.py`, `land-work-verify-lease.py`, `land-work-verify-landing.py`; `SKILL.md` sequences them in prose.
- No script chains them; there is no trap/atexit cleanup spanning the sequence, so an interrupted agent leaves the preview registered.
- `land-work-run-verifier.py` has no `--timeout` and no log capture (bento-rdtn.4 adds the log).

## Acceptance checks
- A single `land-work/scripts/land.py <issue-or-branch>` (or equivalent) runs the full sequence, stops at the first failure with the step name and the raw output path, and always removes the preview via a trap/finally, including on SIGINT.
- Each step's outcome is printed as one line (step, status, seconds, executed-vs-cached for the verifier per bento-rdtn.6).
- SKILL.md tells agents to run the driver and falls back to the step list only when the driver is unavailable.
- Tests: happy path, verifier failure leaves no preview, SIGINT mid-merge leaves no preview and no lease.

## Scope
In: the driver, SKILL.md update, tests. Out: the individual failure-mode fixes (already filed as bento-rdtn.3–.6, .13); repo-local wrappers in consumers.

## Size
medium

## Provenance
Shatter audit 2026-09-04 (shatter repo, branch audit-2026-09-04, audits/2026-09-04/session-retro.md §3 anti-pattern 3; decision recorded 2026-09-06).
