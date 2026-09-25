---
slug: fast-hermetic-precommit
kind: new
title: "Pre-commit hook runs the full shatter-core/shatter-cli test suites on every commit: make it fast (<=30 s warm) and hermetic, with a measured budget"
priority: P1
type: task
labels: [git-hooks, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Pre-commit hook runs the full shatter-core/shatter-cli test suites on every commit: make it fast and hermetic

## Problem

On every commit that stages Rust changes, the pre-commit hook runs `cargo test` for all of shatter-core and/or shatter-cli (3,345 core lib tests plus the integration test binaries), then clippy. The pre-push hook then runs `task affected` (or `task check` for main), and the land verifier runs gates again on the merge preview. Pre-commit has no latency budget and no measurement. str-npdt (closed) added the pre-commit test run without a latency target, and the gate-dedup epic str-35vtk never covered pre-commit. Pre-commit failures unrelated to the diff (an unbuilt TS dist in fresh worktrees, ambient `/tmp/.shatter` config, Go build timeouts under load) came right before most of the commits that skipped hooks in recent sessions.

The goal is a pre-commit hook fast and deterministic enough that nobody needs to skip it. This issue must **not** add or document any way to bypass hooks (maintainer decision D4, 2026-09-23). The beads post-checkout hook stall is handled by the D4 beads issues (`beads-jsonl-import-clobber-check`, `beads-retire-jsonl-import-dolt-remote`) and is out of scope here.

## Evidence (re-verified 2026-09-23; the cited scripts are identical on main `70465921` and the audit snapshot)

- `scripts/precommit-rust.sh:32-37` runs `cargo test -p shatter-core` and/or `-p shatter-cli` at `:33`, then `cargo clippy ... -D warnings` at `:34`. It also runs `cargo test` plus clippy in `shatter-rust` (`:36`) and `shatter-rust-runtime` (`:37`) when those crates change. The E2E suites are all `#[ignore]`d, so they do not run here; the cost is the lib and integration tests.
- `scripts/setup-hooks.sh:92-94` installs `precommit-rust.sh` as the pre-commit body. It calls cargo directly, outside `scripts/gate-wrapper.sh`'s machine-wide semaphore. The pre-push body (`:96-190`) runs `task affected` or `task check`, and both of those Taskfile entries already run through `gate-wrapper.sh` (`Taskfile.yml:498`, `:514`). Only pre-commit is ungoverned.
- `shatter-cli/build.rs` runs `npm install`/`npm run bundle` for shatter-ts (`:113-117`) and `go build` for shatter-go (`:196-204`), with per-file `rerun-if-changed`. Any `cargo check`, `clippy` or `test` of shatter-cli therefore does frontend build work on a cold target dir, or when frontend sources changed. Replacing `cargo test` with `cargo check`/`clippy` removes the test cost, not the build-script cost.
- Session measurements since 2026-09-04 (audit finding sessions-04): a commit with hooks takes a median of 60 s (p90 124 s) against 2 s without hooks.
- Hook failures unrelated to the diff:
  - 9f13ca23, 2026-09-19T14:09: `TypeScript frontend not built: .../shatter-ts/dist/main.js does not exist` in a fresh worktree (a test that needs a prebuilt dist).
  - 87606e10, 2026-09-19T15:58: `discover_configs` tests failed on an ambient `/tmp/.shatter` (str-dl2pj).
  - b6375e4e: Go build timeout under a sustained load average of about 100-170.

## Definitions used below

- **Warm:** the worktree's cargo target dir already holds a build of the parent commit (produced by running the new pre-commit command once on `HEAD`), and no frontend source changed. On warm runs `build.rs` must not re-run npm or go.
- **Cold:** a fresh linked worktree with an empty target dir. `build.rs` frontend bundling is expected and allowed here; what is not allowed is a dependency on artifacts that some *other* command had to build first (for example a prebuilt `shatter-ts/dist/main.js`), on ambient config, or on an examples checkout.

## Acceptance criteria

- [ ] `precommit-rust.sh` runs no `cargo test` and no `cargo nextest`, only `cargo clippy -D warnings` (which subsumes `cargo check`) on the staged packages, plus optionally `cargo fmt --check` limited to staged files. A test in `meta` asserts the script contains no `cargo test`/`cargo nextest` invocation; it fails on today's script.
- [ ] The hook depends on no prebuilt artifacts, ambient config, or examples checkout. Proof: in a cold worktree with `HOME` and `TMPDIR` pointed at empty temp dirs, stage a one-line shatter-cli change and commit; the hook passes. Record the wall time (not held to the 30 s budget).
- [ ] Measured budget on warm runs: record wall time before and after for 5 commits (a core-only change, a cli-only change, a shatter-rust-only change, a frontend-only change, a docs-only change) on warm worktrees, with the load average at start. The after-median is at most 30 s. The close reason includes the exact commands used to prime and measure, so the numbers can be reproduced.
- [ ] Pre-commit's cargo invocation runs under `scripts/gate-wrapper.sh` (with its own label, for example `precommit`) or bento's `run-heavy`, so it takes a machine-wide slot like the pre-push gates. A `meta` test asserts this.
- [ ] The per-hook budget (pre-commit ≤30 s warm, and the existing pre-push behaviour) is documented in CONTRIBUTING.md and AGENTS.md. It is a budget, not bypass instructions.
- [ ] The tests removed from pre-commit remain covered on pre-push: for each crate `precommit-rust.sh` used to test, `select_gates` in `scripts/affected-gates.py` selects that crate's test leaf for a path in it. A `meta` assertion (or existing cases in `scripts/test_affected_gates.py`, cited in the close reason) proves it.

## Suggested approach

Replace the `cargo test` lines in `precommit-rust.sh` with `cargo clippy -p <staged pkgs> -- -D warnings` wrapped in `gate-wrapper.sh precommit`. Leave tests to pre-push's `task affected`. Keep `rerun-if-changed` precise so warm runs skip the frontend build. str-jttrf (closed, 2026-09-12) fixed the `GIT_DIR` leak for hook-run tests; with tests out of pre-commit, that class of failure disappears from this hook.

## Out of scope

- Pre-push receipt reuse (skipping a re-run for a tree a verifier already passed). That is str-35vtk.25 (shadow check only), str-35vtk.26 (evidence threshold) and str-35vtk.9 (batch landing). Reuse may not be enabled before str-35vtk.26's threshold is met, so it is not part of this issue.
- Any documented or scripted way to skip hooks (D4).
- The beads post-checkout JSONL import stall (the D4 beads issues in the shatter-tracker-and-beads bucket).
- Redesigning the landing verifier (str-qwua7.55, str-35vtk.24).
- Unrelated refactors in the touched files.

## Metadata

- Priority: P1. Type: task. Size: M.
- Labels: git-hooks, quality-gates, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: none.
- Related: str-35vtk.25, str-35vtk.26, str-npdt, str-jttrf (closed), str-dl2pj.
- Source findings: sessions-04. Draft shatter-code/83.
