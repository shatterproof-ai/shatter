# Surface persistently red GitHub workflows to agents (Build and Release 0/200, Perf CI 0/13, no issue)

- Priority: P1
- Type: task
- Labels: agents,ci,github-actions,drift,landing
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new
- Source findings: agent-repo-03 (related L5: prior-02; bento side: tests-ci-03)
- Parent: 01 (epic)
- Blocked by: 02
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
Several main-branch workflows fail every run and nobody has filed an issue,
because landing and all agent checks look only at `ci.yml` and the local
verifier. Persistent red is invisible to agents.

## Current Code Facts
- `gh run list --workflow release.yml -L 200`: 168 failure, 32 cancelled,
  0 success. Run 35756993200: `x86_64-pc-windows-msvc` fails with
  `wrapper.h:1:10: fatal error: 'z3.h' file not found`;
  `aarch64-unknown-linux-gnu` fails in `cross build` at `openssl-sys`.
  The other targets succeed; the "Create continuous GitHub Release" job is
  skipped, so `gh release list` is empty.
- `perf-ci.yml`: 13/13 failed ("gauntlet-auto-warm failed on run 1 with exit
  code 1", reason not surfaced).
- `devcontainer.yml` and `docker-publish.yml` have not succeeded recently
  (19 and 7 failures respectively).
- `bd search` for windows / aarch64 / "release workflow" / perf-ci finds no
  open issue. str-lj7s (closed) created the 5-target release matrix.
- `scripts/drift-patrol.py` has no workflow-health check. AGENTS.md mentions
  `gh run` only in an rtk example (AGENTS.md:~581).
- str-qwua7.41 deleted `Cross.toml`/`cross/` claiming "no workflow uses
  cross"; `release.yml` uses `cross` at lines ~59/121/142.

## Acceptance Criteria
- New drift-patrol check `workflow-health`: for each workflow in
  `.github/workflows/` that runs on push to main or on schedule, query the last
  N (default 3) completed runs on main via `gh run list --json`; FAIL when all
  N failed, printing workflow name, last run URL and first failing job. SKIP
  (not PASS) when `gh` is unavailable or unauthenticated.
- Unit tests for the check using canned `gh` JSON.
- AGENTS.md landing section says: after pushing main, run
  `python3 scripts/drift-patrol.py --only workflow-health` (or equivalent) and
  file an issue for any new red workflow.
- Separate tracker issues exist (filed as part of this work, linked here) for:
  Windows Z3 release build, aarch64 openssl cross build, perf-ci warm-step
  failure, devcontainer and docker-publish failures — or those workflows/
  targets are disabled with a documented reason.

## Suggested Approach
Reuse the existing drift-patrol check structure (PASS/FAIL/SKIP/PENDING). Keep
the check read-only. File the per-workflow fix issues rather than fixing the
builds here.

## Out of Scope
Actually fixing the release/perf builds. A bento-side land-work CI poll (bento
tracker).
