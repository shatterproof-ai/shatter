# Bundle: shatter-test-hygiene (repo: shatter)

Audit 2026-09-22. Final issue drafts for the bucket **shatter-test-hygiene**. Theme: test-suite reliability and hygiene (snapshot helpers, pinned inputs, /tmp leaks, tier sprawl, stale failfiles, fuzz policy vs reality). Target tracker: bd in /home/ketan/project/shatter (prefix str). Parent epic: "Epic: Audit 2026-09-22 findings". Nothing has been filed.

Evidence was checked on 2026-09-23 against `origin/main` 70465921. Among code files, audit HEAD 56c86168 differs from main in `shatter-core/Taskfile.yml`, `shatter-core/src/scan_orchestrator.rs` and `shatter-core/src/cache.rs` (str-8q1b4), plus audit-only files; line numbers in these drafts are for main unless stated. Revised 2026-09-23 after the Codex cross-check (see REVISION.md): 01, 04 and 05 were split (new 10, 11, 12), titles shortened to the AGENTS.md under-50-character convention, and acceptance criteria tightened.

## Maintainer decisions (2026-09-23)

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix, and fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64). Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot-diff command and the unused Snapshot writer path. spec-diff is the regression tool. SPEC/README/QUICKSTART are updated. The `diff` name becomes free, and whether to reuse it is str-81xiw's call. The shatter-agents `shatter diff --staged` docs are corrected.
- **D3 Concolic positioning:** measure first. P1 controlled default-vs-concolic benchmark; P1 fix for concolic early termination; a follow-up decision issue blocked by both. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import. Tracker sync moves to a Dolt remote, with a first step that checks whether the stale-JSONL import has been clobbering DB state. AGENTS.md drops `bd sync`. str-qwua7.28 is superseded. bento beads-issue-flow gets matching guidance. No hook-timeout env var and no bypass guidance.
- **D5 Git identity:** the leaked `[user]` section was already removed. Drafts cover a `.mailmap` (test@example.com mapped to Ketan Gangatirkar <33678+ketang@users.noreply.github.com>), a git-state check (local identity override, example.com email, core.bare=true, hooksPath override), and a `.git/config` before/after snapshot in test_git_fixture_isolation.py.
- **D6 Filing:** the maintainer runs one filer script after reconciliation and the Codex cross-check. No agent files anything.

None of D1-D6 changes this bucket's issues directly. `snapshot-test-helpers` explicitly excludes the D2 Snapshot writer path.

## Contents

| # | Slug | Kind | P | Title | Blocked by |
|---|---|---|---|---|---|
| 01 | snapshot-test-helpers | new | P2 | Core snapshot tests pass on missing file | - |
| 02 | pin-examples-repo | new | P2 | Examples repo test input unpinned | - |
| 03 | tests-leak-tmp-dirs | new | P2 | Tests leak dirs into shared /tmp | - |
| 04 | ts-handlers-test-timeouts | new | P3 | TS handlers.test timeouts: diagnose | - |
| 05 | collapse-test-tiers | new | P3 | Gate tier sprawl and body duplication | task-list-json-poisons-checksums, task-sources-cover-real-inputs |
| 06 | rapid-failfile-purge | new | P3 | Go rapid failfile still tracked | - |
| 07 | rapid-failfile-reopen-note | reopen-note (str-qwua7.4) | P3 | str-qwua7.4 failfile purge incomplete | - |
| 08 | broad-run-gate-duplicates | new | P3 | Broad-run gates duplicated, unscheduled | - |
| 09 | fuzz-policy-vs-reality | new | P3 | Fuzzing policy vs reality drift | - |
| 10 | cli-output-snapshots | new | P3 | CLI explore/scan output snapshots | snapshot-test-helpers, pin-examples-repo |
| 11 | e2e-once-in-pre-completion | new | P3 | pre-completion-e2e runs E2E twice | - |
| 12 | ts-handlers-timeout-fix | new | P3 | TS handlers.test load timeouts: fix | ts-handlers-test-timeouts |

---

<!-- file: 01-snapshot-test-helpers.md -->

---
slug: snapshot-test-helpers
kind: new
title: "Core snapshot tests pass on missing file"
priority: P2
type: task
labels: [testing, report, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Core snapshot tests pass on missing file

## Problem

The shatter-core snapshot tests pass when a snapshot file is missing: the helper writes the current output and returns. Deleting a snapshot, or renaming a test so it looks for a new path, turns the check into a no-op that still reports success. Four test files each define their own `assert_snapshot`. Two of them collapse all whitespace before comparing, which hides layout regressions in the HTML reports and in `outcome.md`, where line structure is the output.

`CLAUDE.md:13` says "Regression snapshots are checked into the repo and verified in CI". That cannot hold while a missing snapshot passes.

This issue only fixes the existing shatter-core snapshot helpers. New CLI output snapshots are a separate issue (`cli-output-snapshots`), so this correctness fix does not wait on that work.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23); the files are identical at audit HEAD 56c86168.

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
- Whitespace collapsing: `html_snapshots.rs:36` `normalize_ws` is used at :72-73. `outcome_md_snapshots.rs:24` copies it ("same helper as html_snapshots.rs") and uses it at :54-55, so each markdown snapshot is compared with every newline collapsed to one space. `run_markdown_ordering_snapshots.rs` and `source_set_summary_snapshots.rs` compare byte-exact.
- There are 4 separate `fn assert_snapshot` definitions. `shatter-core/tests/snapshots/` holds exactly 8 files: `explore_fn.html`, `explore_page.html`, `scan_report.html`, three `outcome_md_*.md`, `run_markdown_ordering.md` and `source_set_summary.md`.
- For contrast, shatter-cli already has a correct exact-match convention: `shatter-cli/tests/hide_exec_flags_help.rs:47-68` compares full `spec-diff --help` and `doctor --help` output against checked-in fixtures in `shatter-cli/tests/fixtures/help/` with `assert_eq!`, fails if a fixture is missing, and documents regeneration in the file header (lines 5-7).
- `insta` is not a dependency of any workspace crate (`grep -n insta Cargo.toml */Cargo.toml` returns nothing).
- Audit finding tests-ci-08 (verified).

## Acceptance criteria

- [ ] One shared helper (either `insta`, or a small helper in a shared test-support module following the `hide_exec_flags_help.rs` pattern) replaces all four `assert_snapshot` functions, and the four local copies are deleted. `grep -rn "fn assert_snapshot" shatter-core/tests` returns at most the one shared definition.
- [ ] A missing snapshot fails the test in every mode that CI and `task` gates use. Writing or updating snapshots is only possible through an explicit opt-in (e.g. `INSTA_UPDATE=always`/`cargo insta review`, or an `UPDATE_SNAPSHOTS=1` env var), never implicitly.
- [ ] Proof at close (red, then green): delete `shatter-core/tests/snapshots/outcome_md_*.md` (one file), run `CI=1 cargo test -p shatter-core --test outcome_md_snapshots` and paste the failing output. Also run it without `CI` set and paste that output, which must also fail. Restore the file and paste the passing run.
- [ ] Markdown snapshots (`outcome_md_*`, `run_markdown_ordering`, `source_set_summary`) are compared byte-exact, with no whitespace normalization. Proof: change one newline in a checked-in `outcome_md_*.md` fixture, show the test fails, then revert.
- [ ] HTML comparison does not erase meaningful whitespace. Either compare HTML byte-exact (preferred if the renderer output is deterministic), or use a normalizer that only touches whitespace proven insignificant. If a normalizer is kept, it has unit tests showing that:
  - `<span>Hello</span> <span>world</span>` and `<span>Hello</span><span>world</span>` normalize to different strings;
  - whitespace and newlines inside `<pre>`, `<code>` and `<textarea>` are preserved exactly;
  - text-node whitespace between words is preserved.
- [ ] Regenerated snapshot files are reviewed in the diff: for the byte-exact markdown and HTML files, the close comment states whether content changed apart from whitespace, and any non-whitespace change is explained.
- [ ] `.claude/skills/rust-conventions` gains a one-paragraph snapshot convention: use the shared helper, never self-create, regenerate only via the explicit opt-in.
- [ ] `task affected` passes, and its `Gates selected` output is recorded in the close comment.

## Suggested approach

Byte-exact comparison is the simplest rule and removes the need for a normalizer. The HTML renderers are Askama templates, so their output should already be deterministic; try byte-exact first and add normalization only for a demonstrated source of noise. If `insta` is adopted, add it as a dev-dependency of shatter-core only; shatter-cli can adopt it in `cli-output-snapshots`.

## Out of scope

- New CLI output snapshots (`cli-output-snapshots`).
- The retired `shatter diff` / Snapshot writer path (maintainer decision D2, handled in retire-snapshot-diff). This issue is only about test snapshots.
- Rewriting report renderers, or unrelated refactors in the touched test files.
- The CI-hollowness problem itself (shatter-gates-integrity bucket).

## Priority / type / labels

P2 · task · testing, report, audit · Size S

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Blocks: `cli-output-snapshots` (reuses the shared helper convention).

---

<!-- file: 02-pin-examples-repo.md -->

---
slug: pin-examples-repo
kind: new
title: "Examples repo test input unpinned"
priority: P2
type: task
labels: [testing, examples, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Examples repo test input unpinned

## Problem

Unit, E2E, smoke, TS and walkthrough tests read `SHATTER_EXAMPLES_DIR`, a checkout of `shatterproof-ai/examples` that `scripts/examples_checkout.py` resets to `origin/main` whenever it is more than 10 minutes old. No commit is pinned, and no Task fingerprint covers the examples content. As a result:

- the same shatter commit can pass or fail depending on when the gate ran;
- a change to the examples repo can break shatter's gates with no shatter commit to bisect;
- a cached Task result can be served for a gate whose inputs (the examples) have changed, because the only fingerprinted input is the script itself.

Pinning only the canonical checkout is not enough. The per-consumer snapshot that tests actually read is published by cloning the canonical checkout's `main` branch, not its HEAD commit, so the snapshot content must be pinned too.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- `scripts/examples_checkout.py:19` `DEFAULT_REPO_URL = "https://github.com/shatterproof-ai/examples.git"`, `:20` `DEFAULT_BRANCH = "main"`, `:21` `DEFAULT_DIR = <tempdir>/shatter-examples-main`, `:48` `REFRESH_WINDOW_SECONDS = 600`.
- `scripts/examples_checkout.py:130-139` (`_refresh_checkout_locked`): returns early if refreshed within the window; otherwise `fetch origin main`, `checkout main`, `reset --hard origin/main`, `clean -fdx`.
- `scripts/examples_checkout.py:188-233` (`_snapshot_shared_checkout_locked`): the snapshot cache directory is keyed by the canonical checkout's `rev-parse HEAD` (:191, :197), but the snapshot itself is made with `git clone --local --no-hardlinks --branch main <checkout>` (:213-226, `--branch` at :220). If the canonical checkout's HEAD is detached at a pin while its local `main` branch points elsewhere, the snapshot published under the pinned SHA's directory contains `main`'s content. Detaching HEAD alone therefore does not pin what consumers read.
- A direct fallback bypasses the script: `shatter-rust/src/executor.rs:11444` (test code) uses `std::env::temp_dir().join("shatter-examples-main")` when `SHATTER_EXAMPLES_DIR` is unset.
- Consumers of `SHATTER_EXAMPLES_DIR` include `Taskfile.yml` (tasks at :76, :131, :159, :606, :625), `shatter-core/Taskfile.yml` (test-ignored/-fast), `shatter-ts/Taskfile.yml`, `shatter-core/tests/e2e_concolic{,_go,_rust}.rs`, `e2e_llm_oracle.rs`, `bench_frontier_ranking.rs`, `tests/support/rust_frontend_harness.rs`, `shatter-ts/src/{executor,handlers}.test.ts`, `shatter-rust/src/executor.rs` and `scripts/perf_runner.py`.
- The Task `sources:` lists contain `scripts/examples_checkout.py` (e.g. `Taskfile.yml:128,156,598`), but nothing that changes when the examples content changes. `test-standard` has no `sources:` of its own; the checksum-cached unit is the internal `workspace-test` task.
- There is no lock or pin file (`git ls-files | grep -i examples` shows no SHA file).
- Related closed issues: str-35vtk.4 (locking for the shared checkout), str-w5ry (refresh crash) and str-91yri (PR auth). None pins a SHA.
- Audit finding tests-ci-09 (verified).

## Acceptance criteria

- [ ] A tracked lock file (e.g. `examples.lock` holding a full 40-char SHA) pins the examples commit.
- [ ] The canonical checkout is brought to exactly that SHA (fetch the SHA if missing, then `checkout --detach <sha>`); nothing resets to a moving branch. The 10-minute refresh window either goes away or only governs whether a fetch is attempted, never which commit is checked out.
- [ ] The published snapshot is created from the pinned SHA, not from a branch name (e.g. clone then `checkout --detach <sha>`, or `git worktree`/`archive` of the SHA), and the snapshot cache is keyed by the pinned SHA.
- [ ] Before returning a path, the script verifies that the returned snapshot's `git rev-parse HEAD` equals the pinned SHA and that `git status --porcelain` in it is empty; on mismatch it fails loudly rather than returning.
- [ ] Unit tests in `scripts/test_walkthrough_examples_checkout.py` (or a sibling test file), using a local bare repo as origin, cover:
  - fresh clone: returned snapshot HEAD == pin;
  - recently refreshed canonical checkout (inside the refresh window) whose local `main` has moved: returned snapshot HEAD == pin, not `main`;
  - two callers with different pins in the same process/cache dir: each gets a snapshot at its own pin;
  - an already-published snapshot directory whose HEAD does not match its key: rejected.
  Proof at close: show at least the "moved local main" test failing against the current script (red), then passing after the fix (green).
- [ ] The `shatter-rust/src/executor.rs:11444` fallback either goes through the pinned checkout or fails with a message telling the developer to set `SHATTER_EXAMPLES_DIR` via the script.
- [ ] Every checksum-cached Task that exports `SHATTER_EXAMPLES_DIR` lists the lock file in its `sources:`. Proof at close: on a scratch branch, change the SHA in the lock and show `task --status workspace-test` (and the other affected cached tasks) exiting non-zero (not up to date), where it exited zero before the change.
- [ ] A short documented bump procedure (README or `docs/`): edit the lock, open a PR, and let the normal gates run. Bumping the pin is an ordinary gated change.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Keep the existing shared-checkout lock and snapshot publication machinery (`SNAPSHOT_CACHE_DIR`, str-35vtk.4). Replace the branch target with the pinned SHA in both the refresh and the snapshot clone. Optionally add a `task examples-bump` helper that writes the current `origin/main` SHA into the lock.

## Out of scope

- Moving the examples back in-tree.
- Changing which examples tests use.
- General Task `sources:` coverage (task-sources-cover-real-inputs, shatter-gates-integrity bucket).

## Priority / type / labels

P2 · task · testing, examples, audit · Size M

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Blocks: `cli-output-snapshots`.
- Related: str-35vtk.4, str-w5ry, str-91yri (closed); `task-sources-cover-real-inputs`.

---

<!-- file: 03-tests-leak-tmp-dirs.md -->

---
slug: tests-leak-tmp-dirs
kind: new
title: "Tests leak dirs into shared /tmp"
priority: P2
type: bug
labels: [tests, tempdir, rust-frontend, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Tests leak dirs into shared /tmp

## Problem

Test runs on the shared multi-agent machine keep adding entries under `/tmp` (300+ `shatter-crate-bridge-*` harness dirs, ~7 GB, on 2026-09-23), and the growth is accelerating. Shared `/tmp` state also couples concurrent runs to each other: the same class of coupling caused the `discover_configs` flake that blocked landings on 2026-09-19 (str-dl2pj). Agents have been working around it by hand (`rm -rf /tmp/.shatter` 9 times in 3 sessions, and `SHATTER_ALLOW_HOST_WRITES=1` 20 times) instead of fixing the class.

The entries are not all the same kind of problem. Three classes need different fixes:

1. **Intentional caches placed in a shared location.** The bin-only and crate-bridge harness dirs are deliberately retained, content-keyed build caches. Retaining them is by design; the problem is that, when `SHATTER_HARNESS_CACHE` is unset (as in most test runs), they land directly in the shared `temp_dir()` with no owner, bound or cleanup.
2. **Per-run scratch that is not removed on the success path.** Fixed or pid-keyed test paths that nothing deletes.
3. **Per-run scratch removed only on success.** Cleanup that is skipped when a test panics or fails.

## Evidence

Line numbers are for `origin/main` 70465921 (2026-09-23); `shatter-rust/src/executor.rs` is identical at audit HEAD.

- `shatter-rust/src/executor.rs:1067` `harness_cache_root()` returns `None` unless `SHATTER_HARNESS_CACHE` is set. Production fallbacks when it is unset:
  - `:856` `temp_dir().join("shatter-bin-only-{key:016x}")`: bin-only harness cache (class 1).
  - `:3247` `temp_dir().join("shatter-crate-bridge-{key:016x}")`: crate-bridge harness cache (class 1).
  - `:1119` `make_harness_dir()` → `temp_dir().join("shatter-harness-{id}")`: per-subprocess harness dir, documented (:1105-1108) as removed by `PersistentHarnessManager::close_all()` (removals around :5851-:6085). Whether it survives a test panic is unverified.
- Test-only paths:
  - `:1091-1102` `make_request_scratch()` is `#[cfg(test)]`; it falls back to `temp_dir().join("shatter-exec-{id}")` when `SHATTER_HARNESS_SCRATCH` is unset. (An earlier draft wrongly listed it among production fallbacks.)
  - The `mod tests` block (starts at :7608) has 33 `std::env::temp_dir()` call sites (e.g. :10523 `shatter-test-exec-count`, :10548, :10630, :10779, :10809, :11171, :11248-:11337, :11444, :11580-:11811, :12140-:13342). Some already clean up on success; each needs classifying.
  - Generated harness code writes console capture files under `std::env::temp_dir()` (`:2525`, `:2704`, `let __capture_dir = std::env::temp_dir();`). Whether those files are removed is unverified.
- `SHATTER_HARNESS_CACHE` is process-global. Several tests set it with `std::env::set_var` under `ENV_LOCK` (`:11170-11177`, `:11200-11213`, `:11227-11230`, `:11246-11258`, `:11285`, `:11305-11321`), while other tests reach `harness_cache_root()` through production code paths without taking `ENV_LOCK`. Under `cargo test` (threads in one process) a per-test `set_var` can therefore be observed by an unrelated concurrent test. Setting more env vars per test would widen that race.
- `shatter-core/src/scan_orchestrator.rs` stub-frontend flag files under `temp_dir()`: `shatter-id-mismatch-injected-{pid}`, `shatter-dead-after-handshake-{pid}` and `shatter-execute-exits-twice-{pid}` (main :9670/:9803/:9915; audit HEAD :9489/:9622/:9734).
- Counts on 2026-09-23 (`ls -d /tmp/<prefix>* | wc -l`): `shatter-crate-bridge-*` 318 (170 distinct keys, `du -sch` 7.1 GB), `shatter-id-mismatch-injected-*` 87, `shatter-execute-exits-twice-*` 87, `shatter-dead-after-handshake-*` 87, `shatter-bin-only-*` 52, `shatter-gauntlet.*` 19. At audit time (2026-09-22) there were 127 crate-bridge dirs (6.0 GB). Crate-bridge dirs by mtime: 09-19 30, 09-20 40, 09-21 88, 09-22 57.
- Audit finding sessions-08 (verified, P2).

## Acceptance criteria

- [ ] Every `temp_dir()` use in shatter-rust `executor.rs` (production and `mod tests`), the generated-harness capture dir, and the three scan_orchestrator flag files is classified in the issue as class 1, 2 or 3 (or "cleaned correctly, including on panic"), with the chosen fix per entry.
- [ ] Test isolation does not rely on mutating process-global env per test. Tests get their scratch/cache root by injection (a function parameter or struct field threaded to the code under test, e.g. `*_in(root: &Path)` variants), or by running in a separate process (a subprocess helper, or a gate that runs this crate under nextest's process-per-test model and documents that requirement). Any remaining `set_var` of `SHATTER_HARNESS_CACHE`/`SHATTER_HARNESS_SCRATCH` in tests is either removed or every reader of that variable in tests is covered by the same lock.
- [ ] Class 2/3 paths use `tempfile::TempDir` (or an injected root inside one), so they are removed on success and on panic.
- [ ] Class 1: decide explicitly whether the production fallbacks (`:856`, `:3247`, `:1119`) keep using `temp_dir()` when no cache root is configured, or default to a per-user cache dir (e.g. `$XDG_CACHE_HOME/shatter`) with a size or age bound. Record the decision in the issue. Test runs must not populate the shared fallback either way: tests that exercise these caches pass an injected per-test root.
- [ ] A leak check runs as part of the Rust test gates: each gate runs with a gate-private `TMPDIR` and fails if anything remains in it at exit (or, if the gate cannot own `TMPDIR`, a before/after diff of `/tmp/shatter-*` scoped to that gate). Proof at close (red, then green): show the check failing on a scratch commit that adds a deliberately leaking test, then passing on the final branch.
- [ ] Concurrency proof at close: run `cargo test -p shatter-rust` with default test threads twice in parallel (two processes, same machine, different `TMPDIR`s) and paste that both pass and both leak checks report zero entries. Also run it once under `cargo nextest run -p shatter-rust` and paste the result.
- [ ] Proof for shatter-core: run `cargo test -p shatter-core --lib scan_orchestrator` under a private `TMPDIR` and paste the leak-check output showing zero remaining entries.
- [ ] str-dl2pj is linked as related, with a comment that this issue covers the leak class and dl2pj stays scoped to the `discover_configs` walk-up boundary.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Start with the three scan_orchestrator flag files (small and mechanical; pass the path to the stub via an arg or a per-test env on the child process only, never `set_var` in the parent). For shatter-rust, add root-taking variants of `harness_cache_root`-dependent helpers so tests can inject a `TempDir` without touching global env; `harness_scratch_root()` shows the current env-based shape to replace. Cleaning the existing `/tmp` backlog is a one-off manual step for the operator, not part of the fix.

## Out of scope

- The `discover_configs` walk-up boundary itself (str-dl2pj).
- Gauntlet temp dirs already handled by str-jeen.58/str-jeen.64.
- Changing harness caching semantics for real users beyond the class-1 fallback decision above.

## Priority / type / labels

P2 · bug · tests, tempdir, rust-frontend, audit · Size L

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related (not duplicate): str-dl2pj (open, P1; config discovery boundary; verified open with `bd show` on 2026-09-23). Prior related fixes: str-ri1z, str-jeen.64, str-jeen.58 (closed).

---

<!-- file: 04-ts-handlers-test-timeouts.md -->

---
slug: ts-handlers-test-timeouts
kind: new
title: "TS handlers.test timeouts: diagnose"
priority: P3
type: task
labels: [typescript, tests, flake, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS handlers.test timeouts: diagnose

## Problem

During the audit's `check-unit` run, with the machine heavily loaded, `shatter-ts/src/handlers.test.ts` took 679 s and 10 distinct tests hit the 30 s jest timeout. An isolated rerun at lower load passed 90/90 in 26 s. `analyzer.test.ts` took 427 s in the same run but passed. The shared machine regularly runs oversubscribed, so timeouts tuned on an idle machine make the TS unit gate unreliable.

The cause is not known. This issue is diagnosis only: build a reproducible recipe, find where the time goes, and recommend a fix. The fix itself is `ts-handlers-timeout-fix`, which this issue blocks.

## Evidence

The audit gate logs (`audits/2026-09-22/gates/*.log`) are gitignored (`.gitignore:6 *.log`) and are not retrievable from any commit, so the relevant excerpts are reproduced here.

- Invocation (from the log header): `env PROPTEST_CASES=256 SHATTER_FUZZ_CASES=1000 SHATTER_FAST_CHECK_NUM_RUNS=default bash scripts/gate-wrapper.sh check task check-unit`, started 2026-09-22T12:52:01-05:00, load average at start `23.69 88.69 113.87` (1/5/15 min) on 32 cores. The TS suite is run by `shatter-ts/Taskfile.yml:53`/`:72` as `SHATTER_EXAMPLES_DIR="$examples_root" npm test -- --runInBand`, so jest already uses a single worker.
- Result lines from `check-unit.log`:
  ```
  FAIL src/handlers.test.ts (679.399 s)
  Test Suites: 1 failed, 20 passed, 21 total
  Tests:       10 failed, 968 passed, 978 total
  ```
  Each failure reports `Exceeded timeout of 30000 ms for a test.` The 10 failing tests:
  - `handleRequest › analyze › returns function_not_found error for missing func…`
  - `handleRequest › analyze › returns all functions when no function name speci…`
  - `handleRequest › instrument › returns instrumentation_failed for missing fun…`
  - `handleRequest › invocation adapter hooks › returns not_supported when adapt…`
  - `handleRequest › invocation adapter hooks › clears cachedAnalyses on shutdow…`
  - `handleRequest › async function execution › executes async function and retu…`
  - `handleRequest › async function execution › executes async function that rej…`
  - `handleRequest › missing browser global classification (str-jeen.30) › …` (3 tests)
- Rerun log (`cd shatter-ts && SHATTER_EXAMPLES_DIR=<examples snapshot 49984f4b> npx jest --runInBand src/handlers.test.ts`, started 2026-09-22T13:43:26-05:00 at load `31.20 35.55 56.58`, ended at load `29.35 34.66 55.72`): `Tests: 90 passed, 90 total`, `Time: 26.11 s, estimated 680 s`, wall 28 s.
- `shatter-ts/jest.config.js:5` sets `testTimeout: 30000`.
- `handlers.test.ts:171-185`: a top-level `beforeAll` already warms one worker thread (handshake plus an analyze call "to fully load the TypeScript compiler (~2-3s cold start)"). Whether a ts-morph Project is rebuilt per test is not verified. Load average alone is not a reproducible condition: it does not say what the competing load was.
- Audit finding gates-05 (verified "partially": the root cause is speculative, P3).

## Acceptance criteria

- [ ] A reproducible load recipe is written into the issue: the exact competing workload (e.g. `stress-ng --cpu <N> --timeout <T>` with N stated relative to `nproc`, and/or a named concurrent build command), the machine's core count, and the exact test command. Running the recipe reproduces at least one `Exceeded timeout` in `handlers.test.ts` in at least 2 of 3 attempts; the attempt outputs are pasted. If no recipe reproduces the failure after a documented good-faith attempt (at least three recipes tried), the issue records that, and `ts-handlers-timeout-fix` is closed as not reproducible with a link.
- [ ] Under the recipe, at least one timing-out test is profiled (e.g. `--cpu-prof`, jest `--logHeapUsage`, or timestamps around worker startup, Project construction, type checking and request queueing), and the issue records where the time goes, with numbers.
- [ ] The issue records a recommended fix for `ts-handlers-timeout-fix` (e.g. shared fixture, reduced per-test work, a load-aware gate budget, or a documented timeout change) and why the profile supports it.

## Out of scope

- Implementing the fix (`ts-handlers-timeout-fix`).
- The general gate concurrency and load budget on the shared machine (docs/perf).

## Priority / type / labels

P3 · task · typescript, tests, flake, audit · Size S

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Blocks: `ts-handlers-timeout-fix`.

---

<!-- file: 05-collapse-test-tiers.md -->

---
slug: collapse-test-tiers
kind: new
title: "Gate tier sprawl and body duplication"
priority: P3
type: task
labels: [quality-gates, taskfile, refactor, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [task-list-json-poisons-checksums, task-sources-cover-real-inputs]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Gate tier sprawl and body duplication

## Problem

Gate tiers have piled up, one per efficiency issue, and nobody prunes them. Agents and humans have to choose among about 20 overlapping entry points. Four pairs of Task bodies are copy-pasted only so that each copy gets its own go-task checksum identity, and a wiring test exists just to keep the copies in sync.

This issue has two deliverables that share one design decision (which tiers exist determines which cache identities are needed), so they stay together; the design is agreed before any Taskfile change. The E2E double-run that an earlier draft bundled here is now `e2e-once-in-pre-completion`, which is unblocked and independent.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- Root `Taskfile.yml` test/gate entry points: `test` (:87), `test-quick` (:95), `test-standard` (:103), `check-fast` (:188), `check` (:491), `affected` (:511), `pre-completion` (:667), `pre-completion-e2e` (:673), `e2e`/`e2e-ts`/`e2e-go`/`e2e-rust` (:577-640), `smoke` (:648), `walkthrough`/`walkthrough-cold` (:680-696), `gauntlet`/`gauntlet-cold` (:701-717), `golden-test` (:312), `parity` (:245), `conformance` (:225), `broad-run-corpus` (:722), `broad-run-validation` (:897), `drift-patrol` (:278). Most also have a `*-governed` twin.
- Duplicated bodies that exist only for cache identity:
  - `Taskfile.yml:134-139` above `workspace-test-quick`: "Keep this body in sync with workspace-test above. Its distinct checksum identity protects the reduced property/fuzz budget. Task does not fingerprint caller-provided environment variables, so using workspace-test here could let a quick result satisfy test-standard later."
  - `shatter-cli/Taskfile.yml:33` `test` / `test-fast` ("Keep this body in sync with test above").
  - `shatter-core/Taskfile.yml:37` `test-ignored` / `:71` `test-ignored-fast` (comment at :68-70).
  - `shatter-ts/Taskfile.yml:38` / `:55` `test` / `test-fast`.
  - Parity is enforced by `scripts/test_test_tier_wiring.py`.
- `Taskfile.yml:504-506`: nested task calls do not propagate `task --force` into stages, so "forced" outer runs can still serve cached stages.
- Related open issue str-nl1g (P2, verified open 2026-09-23) proposes yet another tier (a seconds-level live-path tier). That pulls the other way and should be reconciled here.
- Audit finding tests-ci-13 (verified, lowered to P3: the cost is maintenance, not incorrect behaviour).

## Acceptance criteria

- [ ] A proposed tier set (for example dev / affected / check / release, with what each covers) is written into this issue and agreed by the maintainer (recorded as a comment) before any Taskfile change. It says what happens to each current entry point (kept, aliased or deleted) and how str-nl1g fits.
- [ ] Before relying on any alternative cache-identity mechanism, a small recorded experiment shows how go-task names checksum state (task name vs `label:`, including templated labels) and that two variants with different budgets get distinct, non-interchangeable up-to-date status: run variant A, then show `task --status` for variant B still reports not up to date.
- [ ] Cache identity for fast-budget variants no longer relies on copy-pasted bodies. `scripts/test_test_tier_wiring.py`'s body-parity checks are deleted or reduced to what still applies, and a replacement test asserts the property the duplication protected (a quick-budget result cannot satisfy the standard-budget task).
- [ ] Every entry point marked "deleted" in the agreed set is gone, and every "aliased" one delegates to its target. `task --list` output before and after is pasted.
- [ ] The CLAUDE.md Test Tiers table, `/pre-completion` skill and `task affected` selection are updated to the new set in the same change.
- [ ] `task check` passes on the final branch with each stage actually executed: force each stage directly (`task --force check-static`, `task --force check-unit`, `task --force check-integration`, per the `Taskfile.yml:504-506` note) and attach the logs.

## Suggested approach

Do this after the checksum-poisoning fix (str-qwua7.3, via task-list-json-poisons-checksums) and task-sources-cover-real-inputs. Until then, cache behaviour is too unreliable to judge which tiers are redundant.

## Out of scope

- The E2E double-run (`e2e-once-in-pre-completion`).
- Documentation accuracy of the tier table beyond updating it to the new set (test-tier-docs-overstate-coverage, shatter-docs bucket).
- The broad-run duplicate gates (broad-run-gate-duplicates).
- CI workflow restructuring.

## Priority / type / labels

P3 · task (refactor) · quality-gates, taskfile, refactor, audit · Size M

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: `task-list-json-poisons-checksums` (this slug is a note on existing str-qwua7.3, so the real blocker is str-qwua7.3, verified open 2026-09-23) and `task-sources-cover-real-inputs`.
- Related: str-nl1g (open), str-35vtk (tier epic), `e2e-once-in-pre-completion`, `test-tier-docs-overstate-coverage`, `broad-run-gate-duplicates`.

---

<!-- file: 06-rapid-failfile-purge.md -->

---
slug: rapid-failfile-purge
kind: new
title: "Go rapid failfile still tracked"
priority: P3
type: chore
labels: [go, testing, cleanup, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go rapid failfile still tracked

## Problem

Follow-up to str-qwua7.4 (closed), whose rapid-failfile purge was left incomplete.

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
title: "str-qwua7.4 failfile purge incomplete"
priority: P3
type: note
labels: [go, testing, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.4
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# str-qwua7.4 failfile purge incomplete

Target: **str-qwua7.4** (tracker state verified with `bd show` on 2026-09-23: CLOSED) (closed 2026-09-22 at 16794cef, "Fix TestPlanParam_HTTPRequestBodyInvariants (mined literal vs generic seed) and purge rapid failfiles"). Do not reopen it. Post the comment below, which points to the new issue.

## Comment text

> Audit 2026-09-22 (findings tests-ci-15, prior-23): the "purge rapid failfiles" half of this issue was only done for `planner/`.
>
> - A March rapid failfile is still tracked on main:
>   `shatter-go/instrument/testdata/rapid/TestPropertyExecTimeoutAlwaysPositive/TestPropertyExecTimeoutAlwaysPositive-20260306134644-2197485.fail` (added in f0a58576, 2026-03-06). `git ls-tree -r --name-only origin/main | grep '\.fail$'` still lists it at 70465921.
> - `shatter-go/.gitignore` ignores only `planner/testdata/rapid/**/*.fail`. The acceptance text asked for `testdata/rapid/**/*.fail` to be removed from git and ignored, i.e. the whole class.
>
> The close reason lists the gates that ran but does not re-check this acceptance bullet. The remaining work is tracked in **<rapid-failfile-purge id>** ("Go rapid failfile still tracked"). The TestPlanParam fix in this issue is not affected.

(Filer: replace `<rapid-failfile-purge id>` with the id assigned to slug `rapid-failfile-purge`.)

---

<!-- file: 08-broad-run-gate-duplicates.md -->

---
slug: broad-run-gate-duplicates
kind: new
title: "Broad-run gates duplicated, unscheduled"
priority: P3
type: chore
labels: [quality-gates, cleanup, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Broad-run gates duplicated, unscheduled

## Problem

Two Task gates cover the same purpose, validating shatter against a Kapow-derived broad-run failure-class corpus, with different driver scripts, different corpora and different assertion sets. They are overlapping, not identical, so deleting either one without porting its assertions would silently drop regression coverage. Neither is wired into `check`, `affected`, `ci.yml` or any scheduled workflow, so the corpus can rot unnoticed and nobody knows which one is authoritative. One of them (`broad-run-corpus`) has not been touched since 2026-05-13.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- `Taskfile.yml:722-738` `broad-run-corpus` ("Run the broad-run validation corpus gate (Kapow failure-class fixtures)"). Its sources are `tests/fixtures/broad-run-corpus/**/*` and `tests/scripts/broad_run_validation.py`, and it runs `python3 tests/scripts/broad_run_validation.py`. Last commit touching either input: 630e8ccb, 2026-05-13.
- `Taskfile.yml:897-917` `broad-run-validation` ("Broad-run validation corpus gate (str-jeen.14). Documented local check; not in CI."). Its sources are `tests/broad-run-corpus/**/*`, `scripts/broad_run_validation_gate.py` and four `examples/go/*` dirs, and it runs `python3 scripts/broad_run_validation_gate.py --corpus tests/broad-run-corpus/manifest.yaml -v`. `broad-run-validation-tests` at `:919` runs `python3 -m unittest scripts.test_broad_run_validation_gate`. Last commit touching the corpus or driver: a14370ef, 2026-05-02.
- The two drivers check different invariants:
  - `tests/scripts/broad_run_validation.py` (docstring :2-22): (1) denominator integrity, `completed + failed + skipped + unsupported == attempted` and `attempted >= min_attempted` (`assert_denominator`, :124); (2) every referenced `file_path` exists (`assert_artifacts_exist`, :188); (3) failure-class presence via pinned stderr regex or `failed[].reason` (`assert_failure_reason_present`, :196); (4) stale-source detection: scan with a transient file, delete it, rescan, assert no artifact references the deleted path (`run_stale_source_phase`, :278).
  - `scripts/broad_run_validation_gate.py` (docstring :2-23, `assert_fixture` from :292): per-fixture min/max/range thresholds on report counts (`compare_min`/`compare_max`/`compare_range`), `run_must_succeed`, `artifact_paths_must_resolve`, `no_target_reasons` checks, and `tighten_when:` ratchet notes; its corpus has its own fixture set (e.g. `stale-source-go`, `ts-browser-globals`, `mixed-rust-frontend`) that does not match the older corpus's (`dangling-artifacts`, `no-target-categories`, `rust-unavailable`, `source-churn`, `ts`).
  Neither driver's assertions are a superset of the other's.
- `docs/validation/broad-run-corpus.md` documents only `tests/broad-run-corpus/` + `task broad-run-validation`.
- `grep -n broad Taskfile.yml .github/workflows/*.yml` finds no reference from `check`, `affected`, `ci.yml`, `drift-patrol.yml` or `perf-ci.yml`.
- Audit finding tests-ci-16 (verified, P3). str-jeen.14 (closed) created the corpus.

## Acceptance criteria

- [ ] Before any deletion, the issue contains an assertion-by-assertion comparison table: each assertion and each fixture of both drivers, mapped to where the survivor covers it (existing check, ported check, or ported fixture) or marked "dropped".
- [ ] Every "dropped" row has explicit maintainer approval recorded as an issue comment. Without approval, the assertion is ported. In particular, denominator integrity and stale-source disappearance (older driver) and threshold ratchets (newer driver) are not dropped by default.
- [ ] One gate is kept (probably `broad-run-validation`, the documented one), with the ported assertions and fixtures. Only then are the other gate's Task entry, driver script and corpus directory deleted.
- [ ] Proof that ported assertions can fail (red then green): for each ported assertion class, show the survivor failing on a scratch change that breaks it (e.g. a corrupted fixture report or a manifest threshold set impossibly high), then passing on the final branch.
- [ ] The survivor is either scheduled (nightly or weekly workflow, or added to drift-patrol's cadence) or explicitly documented as manual-only, with the reason, in `docs/validation/broad-run-corpus.md` and the CLAUDE.md tier table.
- [ ] If scheduled, proof at close: the URL of a green scheduled or `workflow_dispatch` run. If manual-only, proof at close: a forced local run log (`task broad-run-validation --force`) showing it executes and passes on current main.
- [ ] `broad-run-validation-tests` stays wired to whatever gate runs the Python meta tests.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Build the comparison table first; do not assume the newer corpus absorbed the older one (their assertion sets differ, see Evidence). If a schedule is chosen, drift-patrol's weekly workflow is the cheapest home (see `docs/DRIFT-PATROL.md`).

## Out of scope

- Adding new failure-class fixtures.
- The general tier collapse (`collapse-test-tiers`), although the deletion here reduces that list.

## Priority / type / labels

P3 · chore · quality-gates, cleanup, audit · Size M

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
title: "Fuzzing policy vs reality drift"
priority: P3
type: task
labels: [testing, fuzzing, docs, formal-methods, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Fuzzing policy vs reality drift

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

The maintainer picks exactly one of the three policies below, and the choice is recorded as an issue comment before implementation. Each option is internally consistent: what the policy says is run is exactly what a gate or scheduled job runs.

**Option A: coverage-guided fuzzing for Go and Rust**
- [ ] A scheduled (weekly) workflow, or a drift-patrol step, runs each Go `Fuzz*` target with a bounded `-fuzztime` (e.g. 60 s per target), and a `cargo-fuzz` crate covering at least the protocol `Request`/`Response` and `SymExpr`/`TypeInfo` deserializers (nightly toolchain pinned for that job only).
- [ ] New crashers are committed as `testdata/fuzz/<Target>/` (Go) or corpus/regression files (Rust), so they become regression seeds.
- [ ] The policy docs say Go and Rust coverage-guided fuzzing run on that schedule, and name the job.

**Option B: coverage-guided fuzzing for Go only**
- [ ] A scheduled (weekly) workflow, or a drift-patrol step, runs each Go `Fuzz*` target with a bounded `-fuzztime`; crashers are committed as `testdata/fuzz/<Target>/` seeds.
- [ ] The policy docs say: Go uses `testing.F` targets, run as seed-corpus regression tests in `go test` and as coverage-guided fuzzers in the named scheduled job; Rust byte-level fuzzing is proptest in `tests/fuzz_deserialization.rs` driven by `SHATTER_FUZZ_CASES`, and `cargo-fuzz` is explicitly not used.

**Option C: no coverage-guided fuzzing**
- [ ] The policy docs say: Go `testing.F` targets run only as seed-corpus regression tests in `go test`; Rust byte-level fuzzing is proptest in `tests/fuzz_deserialization.rs`; coverage-guided fuzzing (`go test -fuzz`, `cargo-fuzz`) is explicitly not run.

**For every option**
- [ ] `.claude/skills/formal-methods-policy/SKILL.md` (table at :14, "Native Fuzzing" at :34-38, :63), `shatter-core/CLAUDE.md` (:28, :50-54) and `shatter-go/CLAUDE.md` (:34) state the chosen policy and nothing contradicting it.
- [ ] The `*_fuzz_test.go` naming claim is corrected to `fuzz_test.go`, or the files are renamed to match.
- [ ] A drift check (e.g. in `scripts/drift-patrol.py`) validates affirmative execution claims: for each fuzz mechanism the policy says is run (`-fuzz`/`-fuzztime`, `cargo fuzz`), it fails unless some Task, script or workflow invokes it. Mechanisms the policy explicitly describes as not used are allowed to be named. The check has a unit test with a fixture policy that claims an un-invoked mechanism (fails) and one that names it as not used (passes).
- [ ] For options A and B, proof at close: the URL of a green scheduled or `workflow_dispatch` run showing each target's fuzz duration. For option C, proof at close: the drift check output on the final branch.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Option B is likely the best cost/benefit: it turns the 22 existing Go targets into real fuzzers with a small weekly job and avoids a nightly Rust toolchain. Choose A only if coverage-guided Rust fuzzing is worth a nightly toolchain in one scheduled job.

## Out of scope

- The engine's own input fuzzer (`shatter-core/src/fuzzer.rs`) and the hybrid-fuzzing design docs. Those are product features, not test policy.
- proptest/fast-check/rapid property-test coverage policy beyond the fuzzing rows.

## Priority / type / labels

P3 · task · testing, fuzzing, docs, formal-methods, audit · Size S (C) / M (A, B)

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related: str-df9g, str-l02k, str-aslo (all closed).

---

<!-- file: 10-cli-output-snapshots.md -->

---
slug: cli-output-snapshots
kind: new
title: "CLI explore/scan output snapshots"
priority: P3
type: task
labels: [testing, cli, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [snapshot-test-helpers, pin-examples-repo]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# CLI explore/scan output snapshots

## Problem

No test pins the human-facing terminal output of `shatter explore`, `shatter scan` or the top-level `shatter --help`. Regressions in that output are caught only by manual walkthrough review. Split out of `snapshot-test-helpers` so the helper correctness fix is not held up by this new coverage.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- Full-output snapshots already exist for two subcommand help screens: `shatter-cli/tests/hide_exec_flags_help.rs:47-68` compares `spec-diff --help` and `doctor --help` exactly against `shatter-cli/tests/fixtures/help/*.txt` (regeneration command in the file header, lines 5-7). No equivalent exists for top-level `shatter --help`, `explore` output or `scan` output.
- `shatter-cli/tests/json_stdout_contract.rs` checks JSON structure only.
- `standalone/ts/01-arithmetic.*` and `standalone/go/01-arithmetic.*` exist in the external examples repo, which is not pinned until `pin-examples-repo` lands. Snapshots over unpinned inputs would be flaky by construction.
- Audit finding tests-ci-08 (the CLI-snapshot part, verified).

## Acceptance criteria

- [ ] Top-level `shatter --help` gets a full-output fixture using the existing `tests/fixtures/help/` convention (or the shared helper chosen in `snapshot-test-helpers`, if that moved the convention).
- [ ] New snapshots cover the terminal (non-JSON) output of `shatter explore` and `shatter scan` on the pinned `01-arithmetic` TS and Go examples.
- [ ] Redaction is explicit and minimal: absolute paths are made relative, and durations, timestamps and PIDs are replaced by fixed tokens. Each redaction rule is listed in the test file with the reason. No other normalization (in particular no whitespace collapsing).
- [ ] A missing fixture fails the test (same rule as `snapshot-test-helpers`). Proof at close: delete one new fixture, paste the failing run, restore, paste the passing run.
- [ ] Determinism proof: run the new tests 3 times in a row, and once with a different `TMPDIR` and working directory, and paste that all runs pass.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Reuse the `run_help`/`fixture` shape from `hide_exec_flags_help.rs` for `--help`. For explore/scan, use the shared helper plus a small redaction function. Keep the explored function set small so the snapshots stay readable.

## Out of scope

- Snapshotting every subcommand's help (only top-level is new here).
- JSON output contracts (already covered by `json_stdout_contract.rs`).

## Priority / type / labels

P3 · task · testing, cli, audit · Size S

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: `snapshot-test-helpers` (shared helper convention), `pin-examples-repo` (stable example inputs).

---

<!-- file: 11-e2e-once-in-pre-completion.md -->

---
slug: e2e-once-in-pre-completion
kind: new
title: "pre-completion-e2e runs E2E twice"
priority: P3
type: task
labels: [quality-gates, taskfile, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# pre-completion-e2e runs E2E twice

## Problem

`pre-completion-e2e` runs the three shatter-core E2E concolic suites twice when caches are cold (the common case after an edit): once inside `check` via `core:test-ignored --run-ignored all`, and again in `task e2e`. The e2e subtasks are checksum-cached, so the second run is skipped only when their sources are unchanged since their last run. This issue was split out of `collapse-test-tiers` so that the cheap dedupe is not held behind that issue's blockers and maintainer tier decision. It owns the "E2E runs twice in pre-completion-e2e" item that other buckets (test-tier-docs-overstate-coverage, affected-gates-routing) point at.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- `Taskfile.yml:673-678` `pre-completion-e2e` runs `task: check`, then `task: e2e`, then `git status --short`.
- `check` → `check-governed` (:500) → `check-integration` (:564), which runs `core:test-ignored` (:569).
- `shatter-core/Taskfile.yml:63` (`test-ignored`, nextest path): `cargo nextest run -p shatter-core --run-ignored all -E 'not binary(bench_frontier_ranking)'`; `:65` is the `cargo test -- --include-ignored` fallback. The filter excludes only `bench_frontier_ranking`, so `e2e_concolic`, `e2e_concolic_go` and `e2e_concolic_rust` run here.
- `task e2e` (:577) → `e2e-ts`/`e2e-go`/`e2e-rust`, which run `cargo test --test e2e_concolic{,_go,_rust} -- --include-ignored` (:609, :628, :646). `task e2e` took 193 s in the audit's gate run.
- `Taskfile.yml:504-506`: nested task calls do not propagate `task --force` into the stages. `scripts/gate-wrapper.sh` records outer gate names, not individual test binaries. So neither `--force` on the outer task nor the gate-wrapper log can prove how many times a binary ran.
- Audit finding gates-07 (the E2E duplication part, verified).

## Acceptance criteria

- [ ] Each of `e2e_concolic`, `e2e_concolic_go` and `e2e_concolic_rust` is executed exactly once per `task pre-completion-e2e` run. Either exclude those binaries from `core:test-ignored` (and `core:test-ignored-fast`, if the same reasoning applies to `check-fast`) and keep them in `task e2e`, or drop `e2e` from `pre-completion-e2e`. Whichever is chosen, `task check` alone must still run the E2E suites or the CLAUDE.md tier table must say it does not.
- [ ] `task e2e` remains usable standalone (CLAUDE.md "E2E gate").
- [ ] Proof at close, with caches demonstrably cold:
  1. Invalidate the checksum-cached tasks involved by a method that is shown to work: e.g. touch a file listed in the `sources:` of `core:test-ignored`, `e2e-ts`, `e2e-go` and `e2e-rust` on a scratch commit, then paste `task --status core:test-ignored e2e-ts e2e-go e2e-rust` exiting non-zero (not up to date) before the run. (Deleting `.task/checksum` is not sufficient on its own; see str-qwua7.3.)
  2. Run `task pre-completion-e2e` capturing the full output to a file, and paste the test-runner lines showing each `e2e_concolic*` binary started once (e.g. nextest `PASS [...] shatter-core::e2e_concolic ...` lines, or cargo's `Running tests/e2e_concolic*.rs` lines, with a `grep -c` per binary).
  3. Paste the same count from a run on the pre-fix commit, showing 2 per binary (red then green).
- [ ] If `core:test-ignored`'s filter changes, a comment at the filter says where the E2E suites run instead, and `scripts/test_test_tier_wiring.py` (or a new wiring test) asserts the exclusion so it cannot silently regress.
- [ ] The CLAUDE.md Test Tiers table is updated if what `check` covers changes.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Out of scope

- Redesigning the tier set or cache identity (`collapse-test-tiers`).
- Documentation accuracy of the tier table beyond this change (test-tier-docs-overstate-coverage).

## Priority / type / labels

P3 · task · quality-gates, taskfile, audit · Size S

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related: `collapse-test-tiers`, str-qwua7.3 (checksum invalidation).

---

<!-- file: 12-ts-handlers-timeout-fix.md -->

---
slug: ts-handlers-timeout-fix
kind: new
title: "TS handlers.test load timeouts: fix"
priority: P3
type: bug
labels: [typescript, tests, flake, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [ts-handlers-test-timeouts]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS handlers.test load timeouts: fix

## Problem

`shatter-ts/src/handlers.test.ts` hits the 30 s jest timeout (10 tests) when the shared machine is heavily loaded, failing the TS unit gate. The diagnosis issue `ts-handlers-test-timeouts` produces a reproducible load recipe, a profile and a recommended fix. This issue implements that fix. Its acceptance is outcome-based: it does not prescribe a particular mechanism.

## Evidence

See `ts-handlers-test-timeouts` for the log excerpts (679 s suite, 10 `Exceeded timeout of 30000 ms` failures at load ~114 on 32 cores; 26 s clean rerun at load ~30). `shatter-ts/jest.config.js:5` sets `testTimeout: 30000`; `shatter-ts/Taskfile.yml:53`/`:72` already run jest with `--runInBand`. Audit finding gates-05.

## Acceptance criteria

- [ ] The fix is the one recommended by the diagnosis, or the issue explains why a different one was chosen.
- [ ] Under the diagnosis issue's load recipe, the handlers suite passes with no timeouts in 3 of 3 consecutive runs. Paste the suite time and load average of each run, plus the same recipe's failing output on the pre-fix commit (red then green).
- [ ] Without load, the suite passes and its time is not worse than before the fix by more than 20% (paste before/after times).
- [ ] Per-test timeouts are not raised piecemeal. If a timeout change is part of the fix, it is set once (jest config or the TS Task entry) and documented in `docs/perf/gate-budgets.md` with the measured basis.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Out of scope

- Other TS test files, unless the diagnosis showed the same cause.
- Machine-wide gate concurrency policy.

## Priority / type / labels

P3 · bug · typescript, tests, flake, audit · Size S

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: `ts-handlers-test-timeouts` (diagnosis).
