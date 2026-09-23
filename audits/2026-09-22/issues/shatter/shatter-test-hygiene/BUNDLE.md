# Bundle: shatter-test-hygiene (repo: shatter)

Audit 2026-09-22. Final issue drafts for the bucket **shatter-test-hygiene**. Theme: test-suite reliability and hygiene (snapshot helpers, pinned inputs, /tmp leaks, tier sprawl, stale failfiles, fuzz policy vs reality). Target tracker: bd in /home/ketan/project/shatter (prefix str). Parent epic: "Epic: Audit 2026-09-22 findings". Nothing has been filed.

Evidence was checked on 2026-09-23 against `origin/main` 70465921. Audit HEAD 56c86168 differs from main only in the files `shatter-core/Taskfile.yml` and `shatter-core/src/scan_orchestrator.rs`, whose line numbers were updated.

## Maintainer decisions (2026-09-23)

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix, and fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64). Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot-diff command and the unused Snapshot writer path. spec-diff is the regression tool. SPEC/README/QUICKSTART are updated. The `diff` name becomes free, and whether to reuse it is str-81xiw's call. The shatter-agents `shatter diff --staged` docs are corrected.
- **D3 Concolic positioning:** measure first. P1 controlled default-vs-concolic benchmark; P1 fix for concolic early termination; a follow-up decision issue blocked by both. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import. Tracker sync moves to a Dolt remote, with a first step that checks whether the stale-JSONL import has been clobbering DB state. AGENTS.md drops `bd sync`. str-qwua7.28 is superseded. bento beads-issue-flow gets matching guidance. No hook-timeout env var and no bypass guidance.
- **D5 Git identity:** the leaked `[user]` section was already removed. Drafts cover a `.mailmap` (test@example.com mapped to Ketan Gangatirkar <33678+ketang@users.noreply.github.com>), a git-state check (local identity override, example.com email, core.bare=true, hooksPath override), and a `.git/config` before/after snapshot in test_git_fixture_isolation.py.
- **D6 Filing:** the maintainer runs one filer script after reconciliation and the Codex cross-check. No agent files anything.

None of D1-D6 changes this bucket's issues directly. `snapshot-test-helpers` explicitly excludes the D2 Snapshot writer path.

## Contents

| # | Slug | Kind | P | Title |
|---|---|---|---|---|
| 01 | snapshot-test-helpers | new | P2 | Snapshot tests self-create missing snapshots and pass; four copy-pasted helpers; whitespace-collapsed markdown; no CLI output snapshots |
| 02 | pin-examples-repo | new | P2 | Test inputs come from an unpinned external examples repo (origin/main, refreshed every 10 min) |
| 03 | tests-leak-tmp-dirs | new | P2 | Tests leak per-run directories into shared /tmp (300+ crate-bridge harness dirs, ~7 GB); give tests filesystem isolation and a leak check |
| 04 | ts-handlers-test-timeouts | new | P3 | shatter-ts handlers.test.ts: 10 tests exceed the 30 s jest timeout under machine load |
| 05 | collapse-test-tiers | new | P3 | Collapse ~20 overlapping gate tiers; stop duplicating task bodies for checksum identity; E2E runs twice in pre-completion-e2e |
| 06 | rapid-failfile-purge | new | P3 | Purge the still-tracked March rapid failfile and ignore rapid failfiles repo-wide (str-qwua7.4 left incomplete) |
| 07 | rapid-failfile-reopen-note | reopen-note (str-qwua7.4) | P3 | Comment on closed str-qwua7.4: rapid failfile purge incomplete |
| 08 | broad-run-gate-duplicates | new | P3 | Two duplicate broad-run validation gates (different scripts and corpora); neither in check, affected, CI or any schedule |
| 09 | fuzz-policy-vs-reality | new | P3 | formal-methods-policy prescribes cargo-fuzz and Go native fuzzing; reality is proptest byte-fuzz and seed-corpus-only Go Fuzz targets that nothing mutates |

---

<!-- file: 01-snapshot-test-helpers.md -->

---
slug: snapshot-test-helpers
kind: new
title: "Snapshot tests self-create missing snapshots and pass; four copy-pasted helpers; whitespace-collapsed markdown; no CLI output snapshots"
priority: P2
type: task
labels: [testing, report, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Snapshot tests self-create missing snapshots and pass; four copy-pasted helpers; whitespace-collapsed markdown; no CLI output snapshots

## Problem

The shatter-core snapshot tests pass when a snapshot file is missing. If the file does not exist, the helper writes the current output and returns. Deleting a snapshot, or renaming a test so that it looks for a new path, turns the check into a no-op that still reports success. Four test files each define their own `assert_snapshot`. Two of them collapse all whitespace before comparing. That hides layout regressions in the HTML reports and also in `outcome.md`, where line structure is the output. No test pins the terminal output of `shatter explore`/`shatter scan` or the top-level `--help`, so regressions in human-facing CLI output are caught only by manual walkthrough review.

`CLAUDE.md:13` says "Regression snapshots are checked into the repo and verified in CI". That statement cannot hold while a missing snapshot passes.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23); the files are identical to audit HEAD 56c86168.

- `shatter-core/tests/outcome_md_snapshots.rs:45-51`, `run_markdown_ordering_snapshots.rs:38-44` and `source_set_summary_snapshots.rs:32-38` all contain:
  ```rust
  fn assert_snapshot(path: &Path, actual: &str) {
      if !path.exists() {
          std::fs::create_dir_all(path.parent().unwrap()) ...;
          std::fs::write(path, actual) ...;
          return;
      }
  ```
- `shatter-core/tests/html_snapshots.rs:63-64` has the same pattern. Its module doc (lines 8-11) documents "First-run behaviour: ... writes the current output to disk and passes".
- Whitespace collapsing: `html_snapshots.rs:36` `normalize_ws` is used at :72-73. `outcome_md_snapshots.rs:24` copies it ("same helper as html_snapshots.rs") and uses it at :54-55, so a markdown snapshot is compared with every newline collapsed to one space. `run_markdown_ordering_snapshots.rs` and `source_set_summary_snapshots.rs` compare byte-exact.
- There are 4 separate `fn assert_snapshot` definitions. `shatter-core/tests/snapshots/` holds exactly 8 files: `explore_fn.html`, `explore_page.html`, `scan_report.html`, three `outcome_md_*.md`, `run_markdown_ordering.md` and `source_set_summary.md`.
- `shatter-cli/tests/` has no snapshot files. `json_stdout_contract.rs` checks JSON structure only, and `hide_exec_flags_help.rs` asserts a few substrings of `--help`.
- `insta` is not a dependency of any workspace crate (`grep -n insta Cargo.toml */Cargo.toml` returns nothing).
- Audit finding tests-ci-08 (verified).

## Acceptance criteria

- [ ] `insta` (or an equivalent single shared helper) replaces all four `assert_snapshot` functions, and the local helpers are deleted.
- [ ] A missing snapshot fails the test when `CI` is set (insta's default `INSTA_UPDATE=no` behaviour under CI). Local update is an explicit opt-in (`cargo insta review` / `INSTA_UPDATE=always`).
- [ ] Proof at close: delete one snapshot file, run `CI=1 cargo test -p shatter-core --test outcome_md_snapshots`, and paste the failing output into the issue. Restore the file and paste the passing run.
- [ ] Markdown snapshots (`outcome_md_*`, `run_markdown_ordering`, `source_set_summary`) are compared byte-exact, with no whitespace collapsing.
- [ ] HTML snapshots keep only structural normalization: insignificant inter-tag whitespace/indentation. Whitespace inside `<pre>`/`<code>` and text nodes is not collapsed.
- [ ] New normalized CLI snapshots in `shatter-cli/tests/` cover `explore` and `scan` terminal output on the `01-arithmetic` TS and Go examples, plus top-level `shatter --help`. Absolute paths are made relative, and durations/timestamps/PIDs are redacted, so the snapshots are deterministic across machines. Run each twice to show it is stable.
- [ ] `.claude/skills/rust-conventions` gains a one-paragraph snapshot convention (use the shared helper; never self-create).
- [ ] `task affected` passes, and its `Gates selected` output is recorded in the close comment.

## Suggested approach

Add `insta` as a dev-dependency of shatter-core and shatter-cli. Convert the existing 8 snapshot files by running the tests once with `INSTA_UPDATE=always` and checking that the regenerated content matches the old files, apart from whitespace in outcome_md. For HTML, apply a small normalizer that strips whitespace between tags before handing the text to insta. For the CLI snapshots, use `insta` filters (regex redactions) for paths and timings. Depending on `pin-examples-repo` is not required, but CLI snapshots over `01-arithmetic` will be more stable once the examples SHA is pinned.

## Out of scope

- The retired `shatter diff` / Snapshot writer path (maintainer decision D2, handled in retire-snapshot-diff). This issue is only about test snapshots.
- Rewriting report renderers, or unrelated refactors in the touched test files.
- The CI-hollowness problem itself (shatter-gates-integrity bucket).

## Priority / type / labels

P2 · task · testing, report, audit · Size M

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related: `pin-examples-repo` (stabilizes the CLI snapshot inputs).

---

<!-- file: 02-pin-examples-repo.md -->

---
slug: pin-examples-repo
kind: new
title: "Test inputs come from an unpinned external examples repo (origin/main, refreshed every 10 min)"
priority: P2
type: task
labels: [testing, examples, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Test inputs come from an unpinned external examples repo (origin/main, refreshed every 10 min)

## Problem

Unit, E2E, smoke, TS and walkthrough tests read `SHATTER_EXAMPLES_DIR`, a checkout of `shatterproof-ai/examples` that `scripts/examples_checkout.py` resets to `origin/main` whenever it is more than 10 minutes old. No commit is pinned, and no Task fingerprint covers the examples content. As a result:

- the same shatter commit can pass or fail depending on when the gate ran;
- a change to the examples repo can break shatter's gates with no shatter commit to bisect;
- a cached Task result can be served for a gate whose inputs (the examples) have changed, because the only fingerprinted input is the script itself.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- `scripts/examples_checkout.py:19` `DEFAULT_REPO_URL = "https://github.com/shatterproof-ai/examples.git"`, `:20` `DEFAULT_BRANCH = "main"`, `:21` `DEFAULT_DIR = <tempdir>/shatter-examples-main`, `:48` `REFRESH_WINDOW_SECONDS = 600`.
- `scripts/examples_checkout.py:134-137` (`_refresh_checkout_locked`): `fetch origin main`, `checkout main`, `reset --hard origin/main`, `clean -fdx`.
- Consumers of `SHATTER_EXAMPLES_DIR` include `Taskfile.yml` (tasks at :76, :131, :159, :606, :625), `shatter-core/Taskfile.yml` (test-ignored/-fast), `shatter-ts/Taskfile.yml`, `shatter-core/tests/e2e_concolic{,_go,_rust}.rs`, `e2e_llm_oracle.rs`, `bench_frontier_ranking.rs`, `tests/support/rust_frontend_harness.rs`, `shatter-ts/src/{executor,handlers}.test.ts`, `shatter-rust/src/executor.rs` and `scripts/perf_runner.py`.
- The Task `sources:` lists contain `scripts/examples_checkout.py` (e.g. `Taskfile.yml:128,156,598`), but nothing that changes when the examples content changes.
- There is no lock or pin file (`git ls-files | grep -i examples` shows no SHA file).
- Related closed issues: str-35vtk.4 (locking for the shared checkout), str-w5ry (refresh crash) and str-91yri (PR auth). None pins a SHA.
- Audit finding tests-ci-09 (verified).

## Acceptance criteria

- [ ] A tracked lock file (e.g. `examples.lock` holding a full 40-char SHA) pins the examples commit.
- [ ] `examples_checkout.py` checks out exactly that SHA: fetch the SHA if missing, then `checkout --detach <sha>`. It never resets to a moving branch. The 10-minute refresh window either goes away or only governs fetching.
- [ ] Every Task that exports `SHATTER_EXAMPLES_DIR` lists the lock file in its `sources:`. Proof at close: change the SHA in a scratch branch, run `task --dry test-standard` (or the equivalent status query) and show that the task is no longer up to date.
- [ ] A unit test in `scripts/test_walkthrough_examples_checkout.py` (or a sibling test) asserts that the checkout HEAD equals the lock SHA after a run.
- [ ] A short documented bump procedure (README or `docs/`): edit the lock, open a PR, and let the normal gates run. Bumping the pin is an ordinary gated change.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Keep the existing shared-checkout lock and per-consumer snapshot machinery (`SNAPSHOT_CACHE_DIR`, str-35vtk.4). Only replace the branch target with the pinned SHA, and key the snapshot cache on the SHA. Optionally add a `task examples-bump` helper that writes the current `origin/main` SHA into the lock.

## Out of scope

- Moving the examples back in-tree.
- Changing which examples tests use.
- General Task `sources:` coverage (task-sources-cover-real-inputs, shatter-gates-integrity bucket).

## Priority / type / labels

P2 · task · testing, examples, audit · Size S

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related: str-35vtk.4, str-w5ry, str-91yri (closed); `task-sources-cover-real-inputs`.

---

<!-- file: 03-tests-leak-tmp-dirs.md -->

---
slug: tests-leak-tmp-dirs
kind: new
title: "Tests leak per-run directories into shared /tmp (300+ crate-bridge harness dirs, ~7 GB); give tests filesystem isolation and a leak check"
priority: P2
type: bug
labels: [tests, tempdir, rust-frontend, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Tests leak per-run directories into shared /tmp (300+ crate-bridge harness dirs, ~7 GB); give tests filesystem isolation and a leak check

## Problem

When `SHATTER_HARNESS_CACHE` is unset, the Rust frontend's harness paths fall back to fixed or keyed names under `std::env::temp_dir()`. Several shatter-core stub-frontend tests also write flag files under `temp_dir()`. Nothing removes these entries, so every test run on the shared multi-agent machine adds to `/tmp`, and the growth is accelerating. Shared `/tmp` state also couples concurrent runs to each other: the same class of coupling caused the `discover_configs` flake that blocked landings on 2026-09-19 (str-dl2pj). Agents have been working around it by hand (`rm -rf /tmp/.shatter` 9 times in 3 sessions, and `SHATTER_ALLOW_HOST_WRITES=1` 20 times) instead of fixing the class.

## Evidence

Line numbers are for `origin/main` 70465921 (2026-09-23).

- `shatter-rust/src/executor.rs:1067` `harness_cache_root()` returns `None` unless `SHATTER_HARNESS_CACHE` is set. Fallbacks:
  - `:856` `std::env::temp_dir().join(format!("shatter-bin-only-{key:016x}"))`. This is the bin-only harness. (Verifier correction: :856 is bin-only, not crate-bridge.)
  - `:3247` `std::env::temp_dir().join(format!("shatter-crate-bridge-{key:016x}"))` is the crate-bridge harness.
  - `:1101` `shatter-exec-{id}` and `:1119` `shatter-harness-{id}` are similar fallbacks.
  - Test-only fixed paths are at `:10523` `shatter-test-exec-count`, `:10548`, `:10630` and `:10779` (`shatter-test-*`).
- `shatter-core/src/scan_orchestrator.rs` test flag files under `temp_dir()`: `:9670` `shatter-id-mismatch-injected-{pid}`, `:9803` `shatter-dead-after-handshake-{pid}` and `:9915` `shatter-execute-exits-twice-{pid}`. These were at :9489/:9622/:9734 at audit HEAD 56c86168.
- Counts on 2026-09-23 (`ls -d /tmp/<prefix>* | wc -l`): `shatter-crate-bridge-*` 318 (170 distinct keys, `du -sch` 7.1 GB), `shatter-id-mismatch-injected-*` 87, `shatter-execute-exits-twice-*` 87, `shatter-dead-after-handshake-*` 87, `shatter-bin-only-*` 52 and `shatter-gauntlet.*` 19. At audit time (2026-09-22) there were 127 crate-bridge dirs (6.0 GB), then 264. Crate-bridge dirs by mtime: 09-19 30, 09-20 40, 09-21 88, 09-22 57.
- Audit finding sessions-08 (verified, P2).

## Acceptance criteria

- [ ] No test writes to a shared `std::env::temp_dir()` path. Rust-frontend tests set `SHATTER_HARNESS_CACHE` (or an equivalent per-test root) to a `tempfile::TempDir`. The shatter-core stub-frontend flag files live in a per-test `TempDir` that is passed to the stub through env. The fixed `shatter-test-*` paths in `executor.rs` tests use `TempDir`.
- [ ] Decide explicitly whether the production fallbacks (`:856`, `:1101`, `:1119`, `:3247`) should keep using `temp_dir()` when no cache root is configured, or default to a per-user cache dir (e.g. `$XDG_CACHE_HOME/shatter`). Record the decision in the issue. If they stay, document the cleanup story.
- [ ] A test-hygiene check fails when a test run leaves new `/tmp/shatter-*` entries. For example, a wrapper that sets a private `TMPDIR` for the gate and asserts it is empty afterwards, or a before/after diff of `/tmp/shatter-*` in drift-patrol or CI.
- [ ] Proof at close: from a clean private `TMPDIR`, run `cargo test -p shatter-rust` and `cargo test -p shatter-core --lib scan_orchestrator` and paste the output of the leak check showing zero new entries. Also show the check failing on a deliberately leaking test before the fix, or on a scratch commit.
- [ ] str-dl2pj is linked as related. Note on it that this issue covers the leak class, and that dl2pj stays scoped to the `discover_configs` walk-up boundary.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Start with the three scan_orchestrator flag files (small and mechanical). Then thread a harness cache root through the shatter-rust test helpers; `harness_scratch_root()` (`SHATTER_HARNESS_SCRATCH`, `#[cfg(test)]`) already exists as a model. For the leak check, running each Rust test gate under a gate-private `TMPDIR` and asserting that it is empty at exit catches every leak class at once, including new ones, without listing prefixes. Cleaning the existing `/tmp` backlog is a one-off manual step for the operator, not part of the fix.

## Out of scope

- The `discover_configs` walk-up boundary itself (str-dl2pj).
- Gauntlet temp dirs already handled by str-jeen.58/str-jeen.64.
- Changing harness caching semantics for real users beyond the fallback decision above.

## Priority / type / labels

P2 · bug · tests, tempdir, rust-frontend, audit · Size M

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related (not duplicate): str-dl2pj (open, P1; config discovery boundary). Prior related fixes: str-ri1z, str-jeen.64, str-jeen.58 (closed).

---

<!-- file: 04-ts-handlers-test-timeouts.md -->

---
slug: ts-handlers-test-timeouts
kind: new
title: "shatter-ts handlers.test.ts: 10 tests exceed the 30 s jest timeout under machine load"
priority: P3
type: bug
labels: [typescript, tests, flake, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-ts handlers.test.ts: 10 tests exceed the 30 s jest timeout under machine load

## Problem

During the audit's `check-unit` run, with the machine at load average >100 on 32 cores, `shatter-ts/src/handlers.test.ts` took 679 s and 10 distinct tests hit the 30 s jest timeout. An isolated rerun at load ~30 passed 90/90 in 28 s. `analyzer.test.ts` took 427 s in the same run but passed. The shared machine regularly runs 3-6x oversubscribed, so wall-clock timeouts tuned on an idle machine make the TS unit gate unreliable.

## Evidence

- `shatter-ts/jest.config.js:5` sets `testTimeout: 30000`.
- `audits/2026-09-22/gates/check-unit.log:82` shows `FAIL src/handlers.test.ts (679.399 s)`. The log has 18 `Exceeded timeout` lines. The failing-test list at :83-235 (repeated in the summary at :281-433) names 10 distinct tests:
  - `analyze`: 2 tests (stack frames `handlers.test.ts:368`, `:406`, in `describe("analyze")` at :323)
  - `instrument`: 1 test (`:439`, in `describe("instrument")` at :422)
  - `invocation adapter hooks`: 2 tests (describe at :891)
  - `async function execution`: 2 tests (describe at :1069)
  - `missing browser global classification (str-jeen.30)`: 3 tests (describe at :1921)

  The draft said "analyze tests"; the failures span five describe blocks.
- `audits/2026-09-22/gates/ts-handlers-rerun.log` shows `exit=0 wall=28s ... load=29.35`, with jest reporting "Time: 26.11 s, estimated 680 s".
- `handlers.test.ts:171-185`: a top-level `beforeAll` already warms one worker thread (handshake plus an analyze call "to fully load the TypeScript compiler (~2-3s cold start)"). The earlier guess that "a full ts-morph Project is built per test" is not verified. The cost may instead be per-request worker or compile work, or plain CPU starvation.
- Audit finding gates-05 (verified "partially": the root cause is speculative, P3).

## Acceptance criteria

- [ ] Profile at least one of the timing-out tests under induced load (e.g. `stress-ng --cpu 64` or a parallel `cargo build` alongside) and record in this issue where the time goes: worker startup, ts-morph Project construction, type checking, or queueing.
- [ ] Based on that profile, restructure fixtures (e.g. share a Project or worker across a describe block) so the handlers suite completes with no timeouts at load ~100. Paste the measured suite time and load average into the issue.
- [ ] If a load-scaled timeout is still needed after that, it is implemented once (in the jest config, not per test) and documented in `docs/perf/gate-budgets.md` together with the known-load-flake note.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Confirm the cost first. Do not restructure fixtures until the profile shows per-test construction dominates. If the time is CPU starvation of the worker thread rather than redundant work, the right fix may be a jest `maxWorkers` cap for the TS gate under the shared machine budget rather than fixture changes.

## Out of scope

- The general gate concurrency and load budget on the shared machine (docs/perf).
- Other TS test files, unless the profile shows the same cause.

## Priority / type / labels

P3 · bug · typescript, tests, flake, audit · Size S

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.

---

<!-- file: 05-collapse-test-tiers.md -->

---
slug: collapse-test-tiers
kind: new
title: "Collapse ~20 overlapping gate tiers; stop duplicating task bodies for checksum identity; E2E runs twice in pre-completion-e2e"
priority: P3
type: task
labels: [quality-gates, taskfile, refactor, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [task-list-json-poisons-checksums, task-sources-cover-real-inputs]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Collapse ~20 overlapping gate tiers; stop duplicating task bodies for checksum identity; E2E runs twice in pre-completion-e2e

## Problem

Gate tiers have piled up, one per efficiency issue, and nobody prunes them. Agents and humans have to choose among about 20 overlapping entry points. Four pairs of Task bodies are copy-pasted only so that each copy gets its own go-task checksum identity, and a wiring test exists just to keep the copies in sync. `pre-completion-e2e` runs the shatter-core E2E suites twice: once inside `check` (via `core:test-ignored --run-ignored all`) and again in `e2e`.

This issue owns the "E2E runs twice in pre-completion-e2e" item. It was removed from test-tier-docs-overstate-coverage (docs-ui/21) so that it is filed only once.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- Root `Taskfile.yml` test/gate entry points: `test` (:87), `test-quick` (:95), `test-standard` (:103), `check-fast` (:188), `check` (:491), `affected` (:511), `pre-completion` (:667), `pre-completion-e2e` (:673), `e2e`/`e2e-ts`/`e2e-go`/`e2e-rust` (:577-640), `smoke` (:648), `walkthrough`/`walkthrough-cold` (:680-696), `gauntlet`/`gauntlet-cold` (:701-717), `golden-test` (:312), `parity` (:245), `conformance` (:225), `broad-run-corpus` (:722), `broad-run-validation` (:897), `drift-patrol` (:278). Most also have a `*-governed` twin.
- Duplicated bodies that exist only for cache identity:
  - `Taskfile.yml:134-139` above `workspace-test-quick`: "Keep this body in sync with workspace-test above. Its distinct checksum identity protects the reduced property/fuzz budget. Task does not fingerprint caller-provided environment variables, so using workspace-test here could let a quick result satisfy test-standard later."
  - `shatter-cli/Taskfile.yml:33` `test` / `test-fast` ("Keep this body in sync with test above").
  - `shatter-core/Taskfile.yml:35-36`, `:62-64` `test-ignored` / `test-ignored-fast`.
  - `shatter-ts/Taskfile.yml:38` / `:55` `test` / `test-fast`.
  - Parity is enforced by `scripts/test_test_tier_wiring.py`.
- E2E duplication: `Taskfile.yml:673-678` `pre-completion-e2e` runs `task: check` then `task: e2e`. `check` stage 3 (`check-integration`, :564) runs `core:test-ignored`, which on main is `cargo nextest run -p shatter-core --run-ignored all -E 'not binary(bench_frontier_ranking)'` (`shatter-core/Taskfile.yml:63`). That filter still includes `e2e_concolic.rs`, `e2e_concolic_go.rs` and `e2e_concolic_rust.rs`, which `task e2e` (193 s in the audit's gate run) then runs again.
- Related open issue str-nl1g proposes yet another tier (a seconds-level live-path tier). That pulls the other way and should be reconciled here.
- Audit findings tests-ci-13 (verified, lowered to P3: the cost is maintenance, not incorrect behaviour) and gates-07 (the E2E duplication part, verified).

## Acceptance criteria

- [ ] A proposed tier set (for example dev / affected / check / release, with what each covers) is written into this issue and agreed by the maintainer before any Taskfile change. It says what happens to each current entry point (kept, aliased or deleted) and how str-nl1g fits.
- [ ] Cache identity for fast-budget variants no longer relies on copy-pasted bodies. Before relying on it, verify how go-task names checksum state (task name vs `label:`, including templated labels) with a small experiment, and record the result here. After the change, `scripts/test_test_tier_wiring.py`'s body-parity checks are deleted or reduced to what still applies.
- [ ] The shatter-core E2E binaries run exactly once in `pre-completion-e2e`. Either drop `e2e` from `pre-completion-e2e` or exclude the `e2e_concolic*` binaries from `core:test-ignored`, keeping `task e2e` usable standalone as CLAUDE.md requires. Proof at close: a forced run (`task pre-completion-e2e --force`, or a cleared `.task/` checksum dir) whose gate-wrapper log shows each `e2e_concolic*` binary executed once.
- [ ] The CLAUDE.md Test Tiers table and `/pre-completion` skill are updated to the new tier set in the same change.
- [ ] `task check` passes after the collapse (forced execution, not a cached pass; see the gate-cache caveat in project memory), and its log is attached.

## Suggested approach

Do this after the checksum-poisoning fix (str-qwua7.3, via task-list-json-poisons-checksums) and task-sources-cover-real-inputs. Until then, cache behaviour is too unreliable to judge which tiers are redundant. Start with the E2E duplication, which is a one-line change and does not need the tier decision.

## Out of scope

- Documentation accuracy of the tier table beyond updating it to the new set (test-tier-docs-overstate-coverage, shatter-docs bucket).
- The broad-run duplicate gates (broad-run-gate-duplicates).
- CI workflow restructuring.

## Priority / type / labels

P3 · task (refactor) · quality-gates, taskfile, refactor, audit · Size M

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: `task-list-json-poisons-checksums` (this slug is a note on existing str-qwua7.3, so the real blocker is str-qwua7.3) and `task-sources-cover-real-inputs`.
- Related: str-nl1g (open), str-35vtk (tier epic), `test-tier-docs-overstate-coverage`, `broad-run-gate-duplicates`.

---

<!-- file: 06-rapid-failfile-purge.md -->

---
slug: rapid-failfile-purge
kind: new
title: "Purge the still-tracked March rapid failfile and ignore rapid failfiles repo-wide (str-qwua7.4 left incomplete)"
priority: P3
type: chore
labels: [go, testing, cleanup, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Purge the still-tracked March rapid failfile and ignore rapid failfiles repo-wide (str-qwua7.4 left incomplete)

## Problem

str-qwua7.4 (closed 2026-09-22) required that `testdata/rapid/**/*.fail` be removed from git and added to `shatter-go/.gitignore`. The fix only covered the `planner/` package named in that issue. A rapid failfile from March is still tracked under `instrument/`, and the ignore rule does not cover any other package. The next rapid failure in any package other than `planner/` will show up as an untracked file that is easy to commit by accident.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- `git ls-tree -r --name-only origin/main | grep '\.fail$'` shows:
  `shatter-go/instrument/testdata/rapid/TestPropertyExecTimeoutAlwaysPositive/TestPropertyExecTimeoutAlwaysPositive-20260306134644-2197485.fail`
  (added in f0a58576, 2026-03-06, "feat: add property-based testing across all components (str-ho1d)").
- `shatter-go/.gitignore` in full:
  ```
  # rapid property-test failure artifacts (regenerated locally on failure;
  # not meant to be committed — see str-qwua7.4)
  planner/testdata/rapid/**/*.fail
  ```
- str-qwua7.4's close reason lists the gates that ran but does not re-check the purge acceptance bullet.
- Audit findings tests-ci-15 and prior-23 (both verified, P3).

## Acceptance criteria

- [ ] The `instrument/...2197485.fail` file is removed with `git rm`.
- [ ] `shatter-go/.gitignore` uses `**/testdata/rapid/**/*.fail` in place of the `planner/`-only rule.
- [ ] Proof at close: paste the empty output of `git ls-files '*.fail'`, and the output of `git check-ignore -v shatter-go/protocol/testdata/rapid/X/X.fail` showing the new rule matches a non-planner path.
- [ ] Before deleting the failfile, confirm whether `TestPropertyExecTimeoutAlwaysPositive` (`shatter-go/instrument/property_test.go:86`) still passes. If the failfile encodes a real counterexample that still fails, file it separately instead of discarding the evidence.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Two-line change plus `git rm`. Run `go test ./instrument/ -run TestPropertyExecTimeoutAlwaysPositive -count=3` first.

## Out of scope

- Other rapid or property-test cleanup.
- The TestPlanParam fix that str-qwua7.4 also covered.

## Priority / type / labels

P3 · chore · go, testing, cleanup, audit · Size S

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related: str-qwua7.4 (closed; receives `rapid-failfile-reopen-note`).

---

<!-- file: 07-rapid-failfile-reopen-note.md -->

---
slug: rapid-failfile-reopen-note
kind: reopen-note
title: "Comment on closed str-qwua7.4: rapid failfile purge incomplete"
priority: P3
type: note
labels: [go, testing, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.4
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-qwua7.4: rapid failfile purge incomplete

Target: **str-qwua7.4** (closed 2026-09-22 at 16794cef, "Fix TestPlanParam_HTTPRequestBodyInvariants (mined literal vs generic seed) and purge rapid failfiles"). Do not reopen it. Post the comment below, which points to the new issue.

## Comment text

> Audit 2026-09-22 (findings tests-ci-15, prior-23): the "purge rapid failfiles" half of this issue was only done for `planner/`.
>
> - A March rapid failfile is still tracked on main:
>   `shatter-go/instrument/testdata/rapid/TestPropertyExecTimeoutAlwaysPositive/TestPropertyExecTimeoutAlwaysPositive-20260306134644-2197485.fail` (added in f0a58576, 2026-03-06). `git ls-tree -r --name-only origin/main | grep '\.fail$'` still lists it at 70465921.
> - `shatter-go/.gitignore` ignores only `planner/testdata/rapid/**/*.fail`. The acceptance text asked for `testdata/rapid/**/*.fail` to be removed from git and ignored, i.e. the whole class.
>
> The close reason lists the gates that ran but does not re-check this acceptance bullet. The remaining work is tracked in **<rapid-failfile-purge id>** ("Purge the still-tracked March rapid failfile and ignore rapid failfiles repo-wide"). The TestPlanParam fix in this issue is not affected.

(Filer: replace `<rapid-failfile-purge id>` with the id assigned to slug `rapid-failfile-purge`.)

---

<!-- file: 08-broad-run-gate-duplicates.md -->

---
slug: broad-run-gate-duplicates
kind: new
title: "Two duplicate broad-run validation gates (different scripts and corpora); neither in check, affected, CI or any schedule"
priority: P3
type: chore
labels: [quality-gates, cleanup, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Two duplicate broad-run validation gates (different scripts and corpora); neither in check, affected, CI or any schedule

## Problem

Two Task gates do the same job, validating shatter against the Kapow-derived broad-run failure-class corpus, with different driver scripts and different corpora. Neither is wired into `check`, `affected`, `ci.yml` or any scheduled workflow, so the corpus can rot unnoticed and nobody knows which one is authoritative. One of them (`broad-run-corpus`) has not been touched since 2026-05-13.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- `Taskfile.yml:722-738` `broad-run-corpus` ("Run the broad-run validation corpus gate (Kapow failure-class fixtures)"). Its sources are `tests/fixtures/broad-run-corpus/**/*` and `tests/scripts/broad_run_validation.py`, and it runs `python3 tests/scripts/broad_run_validation.py`. Last commit touching either input: 630e8ccb, 2026-05-13.
- `Taskfile.yml:897-917` `broad-run-validation` ("Broad-run validation corpus gate (str-jeen.14). Documented local check; not in CI."). Its sources are `tests/broad-run-corpus/**/*`, `scripts/broad_run_validation_gate.py` and four `examples/go/*` dirs, and it runs `python3 scripts/broad_run_validation_gate.py --corpus tests/broad-run-corpus/manifest.yaml -v`. `broad-run-validation-tests` at `:919` runs `python3 -m unittest scripts.test_broad_run_validation_gate`. Last commit touching the corpus or driver: a14370ef, 2026-05-02.
- `docs/validation/broad-run-corpus.md` documents only `tests/broad-run-corpus/` + `task broad-run-validation`.
- `grep -n broad Taskfile.yml .github/workflows/*.yml` finds no reference from `check`, `affected`, `ci.yml`, `drift-patrol.yml` or `perf-ci.yml`.
- Audit finding tests-ci-16 (verified, P3). str-jeen.14 (closed) created the corpus.

## Acceptance criteria

- [ ] One gate is kept (probably `broad-run-validation`, the documented one). The other gate's Task entry, driver script and corpus directory are deleted. Before deleting, any fixture that exists only in the deleted corpus is either ported to the survivor or listed in the issue as intentionally dropped.
- [ ] The survivor is either scheduled (nightly or weekly workflow, or added to drift-patrol's cadence) or explicitly documented as manual-only, with the reason, in `docs/validation/broad-run-corpus.md` and the CLAUDE.md tier table.
- [ ] If scheduled, proof at close: the URL of a green scheduled or `workflow_dispatch` run. If manual-only, proof at close: a forced local run log (`task broad-run-validation --force`) showing it executes and passes on current main.
- [ ] `broad-run-validation-tests` stays wired to whatever gate runs the Python meta tests.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Diff the two corpora's fixture lists first. The newer corpus is `tests/broad-run-corpus` with a manifest.yaml, so it has probably absorbed the older one. If a schedule is chosen, drift-patrol's weekly workflow is the cheapest home (see `docs/DRIFT-PATROL.md`).

## Out of scope

- Adding new failure-class fixtures.
- The general tier collapse (`collapse-test-tiers`), although the deletion here reduces that list.

## Priority / type / labels

P3 · chore · quality-gates, cleanup, audit · Size S

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related: str-jeen.14 (closed), `collapse-test-tiers`.

---

<!-- file: 09-fuzz-policy-vs-reality.md -->

---
slug: fuzz-policy-vs-reality
kind: new
title: "formal-methods-policy prescribes cargo-fuzz and Go native fuzzing; reality is proptest byte-fuzz and seed-corpus-only Go Fuzz targets that nothing mutates"
priority: P3
type: task
labels: [testing, fuzzing, docs, formal-methods, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# formal-methods-policy prescribes cargo-fuzz and Go native fuzzing; reality is proptest byte-fuzz and seed-corpus-only Go Fuzz targets that nothing mutates

## Problem

The `formal-methods-policy` skill, which agents load when they decide how to test new code, and the matching section of `shatter-core/CLAUDE.md` both tell agents that Rust deserialization boundaries use `cargo-fuzz` and that Go uses native `testing.F` fuzzing in `*_fuzz_test.go` files. The repo does neither in the sense the policy implies:

- There is no cargo-fuzz crate. Rust "fuzzing" is a proptest suite that feeds random byte vectors to serde entry points (`shatter-core/tests/fuzz_deserialization.rs`), with no coverage guidance.
- There are 22 Go `Fuzz*` targets, but no Taskfile task, script or workflow runs `go test -fuzz=`. Plain `go test` executes only their `f.Add` seed corpus, so they are regression tests, not fuzzers. There is no `testdata/fuzz/` corpus directory either.
- The files are named `fuzz_test.go`, not `*_fuzz_test.go` as the policy says.

Agents that follow the policy will either add a cargo-fuzz target that no gate runs, or believe coverage exists that does not. The policy has no drift check against the code.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- `.claude/skills/formal-methods-policy/SKILL.md:14`: "**Native fuzzing** (Go `testing.F`, `cargo-fuzz`) | Crash resistance at parsing boundaries". `:34-38`: "## Native Fuzzing ... **Go**: `testing.F` in `*_fuzz_test.go` ... **Rust**: `cargo-fuzz` for deserialization boundaries." `:63`: "`*_fuzz_test.go` for byte-level fuzzing".
- `shatter-core/CLAUDE.md:28,50-54` and `shatter-go/CLAUDE.md:34` say the same.
- `ls fuzz shatter-core/fuzz` returns "No such file or directory". `git ls-files | grep -i fuzz` lists only `shatter-core/tests/fuzz_deserialization.rs`, `shatter-core/src/fuzzer.rs` (the engine's input fuzzer, unrelated), `shatter-go/{instrument,protocol}/fuzz_test.go`, a protocol stub script and two 2026-04-15 hybrid-fuzzing plan/spec docs.
- `shatter-core/tests/fuzz_deserialization.rs:7-8`: "These use proptest (not cargo-fuzz) so they run in CI without nightly. For deeper coverage-guided fuzzing, consider adding cargo-fuzz targets later." Case count comes from `SHATTER_FUZZ_CASES` (`Taskfile.yml:144,192` set 32 for the fast tiers; `check` sets 1000 at :495).
- Go targets: `grep -c '^func Fuzz'` gives 8 in `shatter-go/instrument/fuzz_test.go` and 14 in `shatter-go/protocol/fuzz_test.go`. The audit area note said 10; the current count is 22.
- `grep -rln -- '-fuzz=\|-fuzztime\|cargo fuzz\|cargo-fuzz' Taskfile.yml */Taskfile.yml .github scripts` returns no matches.
- Tracker history:
  - **str-df9g** (closed 2026-03-07, reason "Closed") asked for "cargo-fuzz **or proptest bytes-based fuzzing**" for Request/SymExpr/TypeInfo/YAML spec parsing. 218e59c6 (2026-03-06, "test(core): add proptest fuzz targets for deserialization boundaries") delivered the proptest option. So str-df9g was **closed fixed under its own either/or acceptance**, not closed-unfixed. The policy text is what drifted.
  - **str-l02k** (closed 2026-03-06) and **str-aslo** (closed 2026-03-30) added the Go native fuzz targets. Neither added a job that runs them with `-fuzz`.
- Audit finding tests-ci-14 (verified, P3). Area evidence: `audits/2026-09-22/areas/tests-ci.md` T-14.

## Acceptance criteria

The maintainer picks option A or B, and the choice is recorded in the issue before implementation.

**Option A: make reality match the policy**
- [ ] A scheduled (weekly) workflow, or a drift-patrol step, runs each Go `Fuzz*` target with a bounded `-fuzztime` (e.g. 60 s per target). New crashers are committed as `testdata/fuzz/<Target>/` seed files, so they become regression seeds.
- [ ] Either a `cargo-fuzz` crate covering at least the protocol `Request`/`Response` and `SymExpr`/`TypeInfo` deserializers runs in the same scheduled job (nightly toolchain pinned for that job only), or the policy states that proptest byte-fuzzing is the Rust standard (see B).
- [ ] Proof at close: the URL of a green scheduled or `workflow_dispatch` run showing each target's fuzz duration.

**Option B: make the policy match reality**
- [ ] The SKILL.md table and the "Native Fuzzing" section, `shatter-core/CLAUDE.md` and `shatter-go/CLAUDE.md` describe what exists: Go `testing.F` targets in `fuzz_test.go` run as seed-corpus regression tests in `go test`, and Rust byte-level fuzzing is proptest in `tests/fuzz_deserialization.rs` driven by `SHATTER_FUZZ_CASES`. Coverage-guided fuzzing is named as not currently run.
- [ ] The `*_fuzz_test.go` naming claim is corrected to `fuzz_test.go`, or the files are renamed to match.

**Either option**
- [ ] A cheap drift check (for example in `scripts/drift-patrol.py`) fails when the policy names a fuzz mechanism (cargo-fuzz, `-fuzz`) that no Task or workflow invokes.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Option B plus a bounded Go `-fuzztime` step in the existing weekly drift-patrol workflow costs the least, and turns the 22 existing Go targets into real fuzzers. Add cargo-fuzz only if the maintainer wants coverage-guided Rust fuzzing enough to accept a nightly toolchain in one scheduled job.

## Out of scope

- The engine's own input fuzzer (`shatter-core/src/fuzzer.rs`) and the hybrid-fuzzing design docs. Those are product features, not test policy.
- proptest/fast-check/rapid property-test coverage policy beyond the fuzzing rows.

## Priority / type / labels

P3 · task · testing, fuzzing, docs, formal-methods, audit · Size S (B) / M (A)

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related: str-df9g, str-l02k, str-aslo (all closed).
