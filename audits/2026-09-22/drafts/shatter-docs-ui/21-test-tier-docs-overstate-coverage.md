# CLAUDE.md test-tier table overstates coverage; check-fast description is stale; e2e runs twice in pre-completion-e2e

- Priority: P3
- Type: task
- Labels: docs,quality-gates,taskfile,agents
- Tracker action: new issue
- Related: str-qwua7.2, str-qwua7.3, str-35vtk.25. Blocked in part by the hollow-CI fix: CLAUDE.md's CI claims can only be restored once `task check` actually runs tests in CI (L4 finding gates-01/tests-ci-01).
- Source findings: audit 2026-09-22 gates-07 (partially confirmed; P3), tests-ci-17 (confirmed)

<!-- body -->
## Problem
- CLAUDE.md's Test Tiers table labels Standard (`task test-standard`) "Before committing". `test-standard` (`Taskfile.yml:103-111`) runs frontends-built, workspace-clippy and `cargo test --workspace` (core, cli, llm) only, with no TS, Go or rust-fe unit tests.
- CLAUDE.md says snapshots are "verified in CI by .github/workflows/ci.yml (runs the full `task check`…)". While the task checksum-caching bug stands, CI's `task check` executes no test leaves.
- `pre-completion-e2e` (`Taskfile.yml:673-678`) runs `check` and then `e2e`. `core:test-ignored` (`--run-ignored all`) already runs the `e2e_concolic*.rs` binaries, so E2E runs twice (about 193 s).
- `check-fast` (`Taskfile.yml:188-203`) is described as "Fast quality gate (pre-push: …)", but `scripts/setup-hooks.sh:165-176` selects `affected` or `check` for pre-push, never `check-fast`. `check-fast` does not appear in CLAUDE.md, only in AGENTS.md:540.

## Acceptance criteria
- The tier table gains a "Covers" column (crates/frontends and test kinds). `scripts/test_test_tier_wiring.py` validates it against the Taskfile deps graph.
- E2E runs once in pre-completion-e2e: either drop `e2e` there, or exclude the e2e binaries from `core:test-ignored`.
- `check-fast` is either documented in CLAUDE.md with an accurate description, or removed. Reconcile str-35vtk.25's reference to check-fast.
- The CI/snapshot sentence is corrected, or restored once CI runs tests.
