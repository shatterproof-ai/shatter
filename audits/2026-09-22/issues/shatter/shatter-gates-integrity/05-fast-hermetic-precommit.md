---
slug: fast-hermetic-precommit
kind: new
title: "Pre-commit hook runs the full shatter-core/shatter-cli test suite (incl. E2E) on every commit: make it fast (<=30 s) and hermetic, and stop re-gating the same tree 3-4 times"
priority: P1
type: task
labels: [git-hooks, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Pre-commit hook runs the full shatter-core/shatter-cli test suite on every commit: make it fast and hermetic

## Problem

On every commit the pre-commit hook runs `cargo test` for all of shatter-core and/or shatter-cli, which is 3,345 core lib tests plus the integration and E2E suites, and then clippy. The pre-push hook then runs `task affected`, and the land verifier and the pre-push `task check` on main run the gates again. The same tree is gated three or four times. Hook cost has no budget and no measurement. str-npdt (closed) added the pre-commit test run without a latency target, and the gate-dedup epic str-35vtk never covered pre-commit. Hook failures unrelated to the diff (an unbuilt TS dist in fresh worktrees, ambient `/tmp/.shatter` config, Go build timeouts under load) came right before most of the commits that skipped hooks in recent sessions.

The goal is a hook that is fast and deterministic enough that nobody needs to skip it. This issue must **not** add or document any way to bypass hooks (maintainer decision D4, 2026-09-23). The separate beads post-checkout hook stall is handled by the D4 beads issues (`beads-jsonl-import-clobber-check`, `beads-retire-jsonl-import-dolt-remote`) and is out of scope here.

## Evidence (re-verified 2026-09-23 against the audit snapshot, unchanged on main)

- `scripts/precommit-rust.sh:28-37` runs `cargo test -p shatter-core` and/or `-p shatter-cli` (the full suite, integration tests included) at `:33`, then `cargo clippy ... -D warnings` at `:34`. It also runs `cargo test` plus clippy in `shatter-rust` (`:36`) and `shatter-rust-runtime` (`:37`) when those crates change.
- `scripts/setup-hooks.sh:92-94` installs `precommit-rust.sh` as the pre-commit body. The pre-push body (`:96-190`) runs `task affected`, or `task check` for main, directly.
- Neither hook nor `precommit-rust.sh` goes through bento's `run-heavy` load regulator (see bento `docs/specs/2026-06-18-run-heavy-load-regulation-design.md`) or `scripts/gate-wrapper.sh`'s machine-wide semaphore. A grep for `run-heavy` in `scripts/` finds nothing.
- Session measurements since 2026-09-04 (audit finding sessions-04): a commit with hooks takes a median of 60 s (p90 124 s) against 2 s without hooks. A push with hooks takes a median of 80 s in the foreground and 308 s in the background, with a maximum of 728 s. The land verifier then re-runs the gates on the merge preview (land.py median about 780 s).
- Hook failures unrelated to the diff:
  - 9f13ca23, 2026-09-19T14:09: `TypeScript frontend not built: .../shatter-ts/dist/main.js does not exist` in a fresh worktree.
  - 87606e10, 2026-09-19T15:58: `discover_configs` tests failed on an ambient `/tmp/.shatter` (str-dl2pj).
  - b6375e4e: Go build timeout under a sustained load average of about 100-170.

## Acceptance criteria

- [ ] pre-commit runs only fast, hermetic checks on the staged crates (`cargo check` and/or `cargo clippy -D warnings`, and optionally `cargo fmt --check` on staged files). It runs no tests that need built frontends, examples checkouts or ambient config. Target: 30 s or less on a warm cache.
- [ ] Measured proof: the close reason records wall time for 5 representative commits (a core-only change, a cli-only change, a frontend-only change, a docs-only change, and a cold new worktree), before and after. The median is at most 30 s and none of them needs a built frontend.
- [ ] pre-push accepts a fresh verifier or affected receipt for the exact same tree (the mechanism tracked by str-35vtk.24/.25) instead of re-running the gate. If .24/.25 have not landed, this item is linked as blocked on them and the rest of this issue can close without it.
- [ ] Hook-invoked gates run under `run-heavy` or `gate-wrapper.sh`, so they respect the machine-wide slot.
- [ ] The per-hook budget (pre-commit ≤30 s, and the pre-push expectation) is documented in CONTRIBUTING.md and AGENTS.md. It is a budget, not bypass instructions.
- [ ] A test in `meta` asserts that `precommit-rust.sh` invokes no `cargo test` and no `cargo nextest`. It fails on today's script.

## Suggested approach

Replace `cargo test` in `precommit-rust.sh` with `cargo check`/`clippy` on the staged packages, and move tests to pre-push, where `task affected` already covers them. Wrap the pre-push `task` call in `run-heavy`. Wire receipt reuse once str-35vtk.24/.25 provide it. Coordinate with str-jttrf, which covers hook-run tests that leak `GIT_DIR`. Most of that fragility disappears once pre-commit stops running tests.

## Out of scope

- Any documented or scripted way to skip hooks (D4).
- The beads post-checkout JSONL import stall (the D4 beads issues in the shatter-tracker-and-beads bucket).
- Redesigning the landing verifier (`verifier-per-language-evidence`, str-qwua7.55).
- Unrelated refactors in the touched files.

## Metadata

- Priority: P1. Type: task. Size: M.
- Labels: git-hooks, quality-gates, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: none. The receipt-reuse criterion depends on str-35vtk.24/.25.
- Related: str-35vtk.24, str-35vtk.25, str-jttrf, str-npdt, str-dl2pj.
- Source findings: sessions-04. Draft shatter-code/83.
