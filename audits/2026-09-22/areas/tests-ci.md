# Area review: test suite, CI and quality gates (tests-ci)

Audit 2026-09-22, branch `audit-2026-09-22` (base `16794cef`). Reviewer scope:
Taskfile tiers, checksum caching, `.github/workflows`, flakes, snapshots,
PBT adequacy, test runtime, and whether "green" means "tested".
Observation only; no source or tracker changes were made. No `task` command
that writes `.task/` was run in the shared audit worktree (see T-01 for why).

## Headline

**CI has not run the Rust, TypeScript, Go, Rust-frontend, parity or
conformance test suites since 2026-08-30, even though it has reported green on
every push since.** The cause is a `meta` unit test that runs
`task --list-all --json` in the repo root. That command makes Task write a
fresh checksum for every task that has `sources:`. After it runs, stage 2 and
stage 3 of `task check` report every test task as "is up to date". This is also
the root cause behind prior-audit issue **str-qwua7.3**, which is still open and
unexplained. The other workflows are no healthier. `Build and Release` has
failed on all 266 runs it has ever made, and no GitHub release exists, so
`install.sh` and `action.yml` have nothing to install. `Drift Patrol`,
`Perf CI`, `Devcontainer CI` and `Docker Image` are red on every scheduled or
PR run, and nothing in the agent system notices.

## Findings table

| id | P | level | title |
|---|---|---|---|
| T-01 | P1 | L4 | `task --list-all --json` in a meta test poisons Task checksums; `task check` in CI runs no test suites |
| T-02 | P1 | L5 | Build and Release has 0/266 successes; no GitHub release exists; installer cannot work |
| T-03 | P1 | AGENT | Scheduled workflows are red for weeks and no one notices; closures accepted partial CI evidence |
| T-04 | P1 | L4 | Task `sources:` do not list the real inputs, so gates skip after relevant edits |
| T-05 | P2 | L4 | affected-gates misroutes askama `.md` templates and core changes; never selects docs-smoke or walkthrough |
| T-06 | P2 | AGENT | Verifier still runs a different gate set from CI; issue closures cite gates that do not cover the changed language |
| T-07 | P2 | L4 | Local test runner and CI runner differ (nextest vs cargo test); the nextest `ci` profile is dead config |
| T-08 | P2 | L1 | Snapshot helpers create a missing snapshot and pass, and are copy-pasted four times |
| T-09 | P2 | L4 | The external examples repo floats on `origin/main`, so test results are not reproducible |
| T-10 | P2 | AGENT | About 220 script unit tests and three gate infrastructure scripts are wired into nothing |
| T-11 | P2 | L4 | Walkthrough, gauntlet and smoke never run in CI; the only CI path to the gauntlet (Perf CI) is red |
| T-12 | P2 | L1 | TS property tests are mostly JSON round-trips; instrumentor and analyzer have none |
| T-13 | P2 | L4 | Too many overlapping tiers, with bodies copied only to get separate cache keys |
| T-14 | P3 | L2 | Fuzzing policy says cargo-fuzz and Go fuzzing; reality is seed-only Go fuzz and no cargo-fuzz |
| T-15 | P3 | L1 | str-qwua7.4 was closed with a committed rapid failfile still tracked and a gitignore scoped too narrowly |
| T-16 | P3 | L1 | Two duplicate broad-run gates, neither in check or CI |
| T-17 | P3 | L2 | check-fast description says "pre-push"; the tier is missing from the CLAUDE.md table |
| T-18 | P3 | L1 | Workflows use actions on Node 20 (deprecated); runner label migration is pending |

---

## T-01 (P1, L4): `meta` poisons the Task checksum cache; CI's `task check` runs no test suites

**Evidence**

- `scripts/test_affected_gates.py:203-212`
  (`test_every_emitted_gate_is_a_real_task`) runs
  `subprocess.run(["task", "--list-all", "--json"], cwd=ROOT, ...)`. The `meta`
  task runs `python3 -m unittest scripts.test_affected_gates`
  (`Taskfile.yml:459`), and `meta` is in `check-static`, stage 1 of `task check`
  (`Taskfile.yml:535-546`).
- `task --list-all --json` reports `"up_to_date"` for each task. To compute
  that, Task (tested on v3.50; CI uses v3.53.1) **writes the current checksum
  to `.task/checksum/<task>`** without running the task. Minimal reproduction
  in scratch (`scratchpad/tasktest2`): a root Taskfile whose `meta` runs
  `task --list-all --json`, then `task: sub:test` (included with `dir:`,
  `sources: [src/*.txt]`):
  ```
  $ task check           # fresh tree, no .task/
  task: [meta] task --list-all --json >/dev/null
  task: Task "sub:test" is up to date        <- never executed, even once
  $ echo b > sub/src/a.txt; task check
  task: Task "sub:test" is up to date        <- source changed, still skipped
  ```
  With `--dry` (`task --list-all --json --dry`) or plain `task --list-all`,
  no checksum is written and `sub:test` runs (verified).
- CI log for the latest green CI run (job of run on 2026-09-22T16:52Z, fetched
  via `gh api .../actions/jobs/<id>/logs`): `task check` took **78 s**
  (16:53:47 to 16:55:05). Stage 1 ran (clippy, meta, schemas). Then:
  ```
  task: Task "rust-rt:test" is up to date
  task: Task "go:vet" is up to date
  task: Task "go:test" is up to date
  task: Task "ts:test" is up to date
  task: Task "cli:test" is up to date
  task: Task "rust-fe:test" is up to date
  task: Task "core:test-ignored" is up to date
  task: Task "conformance" is up to date
  task: Task "parity" is up to date
  ```
  The log has only 4 `test result:` lines, all from the CI-only
  `cargo test -p shatter-llm` step. `.task/` is gitignored
  (`.gitignore:157`) and is not in the `actions/cache` paths
  (`ci.yml:72-77`), so the checkout is fresh and the checksums can only come
  from the in-run `--list-all --json`.
- CI duration per day (`gh run list --workflow CI`): 6 to 11 min through
  2026-08-27. 23 to 31 min on 2026-08-29, the first `task check` runs after
  str-35vtk.21 replaced test-standard + parity + conformance with a single
  `task check`. **3 to 4 min every day since 2026-08-30.** The test that
  poisons the cache was added in `8ae14f13`/`b12e3054` (2026-08-29,
  str-35vtk.8). About 50 green CI runs since then executed no product test
  suites.
- Local effect: in `~/.cache/shatter/gate-times.csv`, successful `check` runs
  had a median of 286 s before 2026-08-29 (n=61) and 59 s after 2026-08-30
  (n=49). 37 of 135 `check` rows finished in under 15 s. Locally, `meta`
  re-runs whenever its `sources` change. Those include
  `shatter-ts/src/**/*.ts` and `shatter-core/tests/**/*.rs`
  (`Taskfile.yml:441,447`). So any TS change, or any core integration-test
  change, causes the landing `task check` to skip `ts:test`, `core:test-ignored`
  and the rest. In a fresh land-work preview worktree `meta` always runs.
- Prior audit: str-qwua7.3 ("Explain why stage-2/3 tasks report up to date
  right after .task/checksum is deleted") is **this mechanism**. The prior
  audit saw the symptom (`task-check-real2.log:293-302`) but not the cause.
  str-qwua7.2 (`task check-fresh`) does not exist yet (no `check-fresh` in
  `Taskfile.yml`).
- `scripts/test_ci_workflow_structure.py` asserts only the *shape* of the
  workflow (task check invoked once, no standalone parity). Nothing asserts
  that tests *executed*.

**Recommendation**

1. Change the test to `task --list-all --json --dry`, or parse
   `task --list-all` text or the Taskfile YAML. Add a meta regression test
   that snapshots `.task/checksum` before and after the `meta` body and fails
   if it changed.
2. In CI, bypass the cache entirely: set a CI-only `TASK_TEMP_DIR` to a fresh
   temporary directory per step, or run `task --force` per leaf, or implement
   str-qwua7.2's `check-fresh` and use it in CI.
3. Make `task check` fail when all leaves are cached. Have gate-wrapper grep
   the child output for `is up to date` and print an "executed vs cached"
   table. Under `CI=1`, treat any cached test leaf as failure.
4. Run the real suite once on current `main` before trusting anything since
   2026-08-30. The gates agent's forced run is the first real signal in three
   weeks.
5. Close str-qwua7.3 with this explanation. Report upstream to go-task that
   `--list --json` mutates checksum state; listing should be read-only.

**Agent root cause:** Gates are verified by exit status and workflow *shape*,
never by proof that work ran. Two individually reviewed changes (str-35vtk.8
added a test that lists tasks; str-35vtk.21 folded CI into `task check`)
combined into a hollow gate, and nothing measures runtime or test counts. The
cache semantics are documented only in private agent memory
(`project_shatter_gate_cache_and_bare_primary.md`), not in the repo.
str-qwua7.3 has sat as P1 "explain" with no investigation for 17 days.

## T-02 (P1, L5): Build and Release has never succeeded; there are no releases to install

**Evidence**

- `gh run list --workflow "Build and Release" --limit 1000`: **266 runs, 0
  successes** (oldest 2026-02-25, cancelled). It runs on every push to `main`
  (`release.yml` `on: push: branches: [main]`).
- Latest failure (run 35756993200):
  - `Build (x86_64-pc-windows-msvc)`: `z3-sys v0.10.7 ... wrapper.h:1:10: fatal error: 'z3.h' file not found`.
  - `Build (aarch64-unknown-linux-gnu)` via cross: `failed to run custom build command for openssl-sys v0.9.116`.
- `gh release list` returns nothing. `install.sh:67` resolves the latest build
  from `https://api.github.com/repos/${REPO}/releases`, and `install.sh:81`
  errors with "Could not determine latest build". `action.yml` installs from
  releases as well.
- Tracker: str-lj7s ("Replace release.yml: timestamped GitHub Releases on every
  main push") was closed with the reason "ab2a8515 landed on main". It did not
  verify a green release run. str-ibmq ("Z3 build avoids gh-release") is also
  closed. No open issue mentions windows, aarch64, openssl, z3.h or
  release.yml.
- The prior audit (2026-09-04) did not look at workflow run history.

**Recommendation**

- Fix or trim the matrix. On Windows, use z3's `static-link-z3` or `bundled`
  feature, or install Z3 through vcpkg or choco. For aarch64, find the
  dependency that pulls in `openssl-sys` and switch it to rustls, or vendor
  OpenSSL.
- Make the release job depend on a required subset so one bad target does not
  block every release, and publish the targets that did build.
- Add a CI smoke test that runs `install.sh` against the latest release. Do
  not close release work until a green run URL is cited.

**Agent root cause:** Issue closure means "merged", not "workflow observed
green". The landing flow never checks post-merge workflow runs, and red
workflows on `main` are not surfaced to agents.

## T-03 (P1, AGENT): Every non-CI workflow is red and invisible to the agent system

**Evidence** (`gh run list --limit 300`, 2026-08-01 to 2026-09-22):

| workflow | result | cause (from annotations or job logs) |
|---|---|---|
| Drift Patrol (weekly) | 7 failures, last success 2026-08-07 | `actions/setup-go` `go-version-file: go.mod` → "The specified go version file at: go.mod does not exist" (`drift-patrol.yml:82-85`). No root `go.mod` has ever existed (`git log --diff-filter=D -- go.mod` is empty). |
| Perf CI (weekly) | 8/8 failures | `gauntlet-auto-warm failed on run 1 with exit code 1`. The reason is suppressed (no gauntlet output in the log). Also: "Restore cache failed: ... go.sum". |
| Devcontainer CI | 8/8 failures | `postCreateCommand ... bash .devcontainer/post-create.sh` failed |
| Docker Image (PRs) | 7/7 failures | `cargo build --release -p shatter-cli` exit 101 in the Dockerfile |
| Build and Release | 266/266 failures | see T-02 |

- str-u394l.1 ("Scheduled drift patrol", P1) was closed on 2026-08-07. The
  closing evidence cites "CI Patrol self-test job -> pass". The drift-patrol
  job itself fails at `Setup Go` before the patrol runs.
  `docs/DRIFT-PATROL.md:70-71` warns that "a weekly scheduled run that is red
  from day one ... teaches everyone to ignore it". That is what happened.
- Agent guidance never mentions checking workflow runs. A grep of `AGENTS.md`,
  `CLAUDE.md`, `.claude/skills`, `docs/DRIFT-PATROL.md` and bento
  `land-work/SKILL.md` for `gh run`, "CI status" or "workflow run" finds only
  an rtk command-list example (`AGENTS.md:581`).
- The prior audit marked `perf-ci.yml` as "**Live**"
  (`design-foundation.md:172`) without checking its run history.

**Recommendation**

- Fix the four workflows (drift-patrol: `go-version-file: shatter-go/go.mod`;
  perf-ci: surface the gauntlet stderr; devcontainer and Docker: reproduce
  locally).
- Add a drift-patrol check or a SessionStart nudge: "any workflow whose last N
  runs on main are red". Use `gh run list --json` and file or refresh one bd
  issue per red workflow.
- In bento `land-work`, after pushing main, wait for or poll the CI run for
  the pushed SHA and report its conclusion. A red post-merge run should
  reopen or annotate the issue.
- Closure checklist: a workflow-touching issue needs a green run URL of the
  workflow itself, not of a sibling job.

**Agent root cause:** No feedback loop from GitHub Actions into the agent
system. Drift patrol, the one mechanism meant to catch drift, is itself
drifted, and it reports only through a web UI that no agent reads.

## T-04 (P1, L4): Task `sources:` omit real inputs, so gates skip after relevant edits

**Evidence** (all from `*/Taskfile.yml`):

- `cli:test` (`shatter-cli/Taskfile.yml:16-24`) lists only `src/**/*.rs`,
  `Cargo.toml` and core `src`. It omits:
  - `shatter-cli/tests/**` (18+ integration test files, e.g.
    `json_stdout_contract.rs`, `hide_exec_flags_help.rs`)
  - `shatter-cli/templates/*.md` (askama templates compiled in by
    `render.rs:17,40`)
  - `shatter-cli/build.rs`
  - `shatter-ts/src`, `shatter-go/**/*.go`: the CLI embeds both frontends via
    `build.rs` (`build_ts_frontend`, `build_go_frontend`), and CLI tests
    exercise them.

  `cli:build` and `cli:clippy` have the same gaps.
- `core:test-ignored` (`shatter-core/Taskfile.yml:37-45`) runs all three E2E
  suites against real frontend binaries (`--run-ignored all`), but lists only
  core `src` and `tests`. A change only to `shatter-ts/src`, `shatter-go` or
  `shatter-rust/src` leaves it "up to date", so the landing gate's E2E
  coverage skips exactly the frontend changes that CLAUDE.md says need E2E.
  It also omits `../scripts/examples_checkout.py`, which `core:test`
  includes.
- `workspace-test` (the `test-standard` tier, which includes the Go and Rust
  E2E suites per `Taskfile.yml:105-108`) has no frontend sources.
- `rust-fe:test` omits `shatter-rust/tests/**` (`codegen_parity.rs`) and the
  runtime crate. `ts:test` omits `jest.config.js`, although `meta` lists it
  (`Taskfile.yml:446`).
- The external examples repository is not fingerprinted anywhere (see T-09).

**Recommendation**

- Generate `sources:` from real dependency data, or at minimum add the missing
  globs.
- Add a `meta` test that, for each test leaf, asserts its `sources` cover the
  crate's `tests/`, `templates/`, `build.rs`, and the source trees of any
  embedded frontend.
- Better: stop relying on Task checksums for *test* leaves. Cargo, go test and
  jest already do incremental builds. Keep `sources:` only on build and
  codegen tasks where skipping is safe.

**Agent root cause:** No rule or test ties `sources:` to real inputs. Caching
was added for speed (str-35vtk) with no "must not skip on relevant change"
property test.

## T-05 (P2, L4): affected-gates misroutes files

**Evidence** (`python3 -c ... select_gates([...])` against `scripts/affected-gates.py`):

```
shatter-cli/templates/scan.md      -> ['docs']
README.md / QUICKSTART.md          -> ['docs']
shatter-core/src/report/html.rs    -> ['smoke','core:clippy','core:test']
shatter-ts/src/index.ts            -> ['smoke','ts:typecheck','ts:test','e2e-ts','parity','conformance']
```

- `_classify` (`affected-gates.py:125-127`) sends every `*.md` to `docs`
  before the `shatter-cli/` branch runs. The explicit
  `shatter-cli/templates/ -> gauntlet` rule (`:161`) can never match the real
  templates, which are `.md` files. A CLI output template change runs no
  build and no test.
- `docs-smoke` is not in `GATE_ORDER`. A README, QUICKSTART or SPEC change
  selects only `docs`. That gate prints `[skip]` for markdownlint, vale and
  lychee locally and in CI (CI log: all three `[skip] ... not installed`), so
  the affected gate for doc changes is effectively `test -f README.md`.
- `shatter-core/` changes never select `cli:test`, although `shatter-cli`
  depends on core and renders its reports. `html.rs` feeds CLI HTML reports.
- Frontend changes (`shatter-ts/`, `shatter-go/`) do not select `cli:test`
  (embedded frontends) or `walkthrough`.

**Recommendation:** Classify by crate before the `.md` rule. Add `docs-smoke`
to `GATE_ORDER` for README, QUICKSTART, SPEC and `docs/`. Add `cli:test` for
core and frontend paths. Add table-driven tests in `test_affected_gates.py`
for these exact paths.

**Agent root cause:** The selector's tests are derived from historical merges
(`test_affected_gates.py:180-201`), so they pin past behaviour instead of
dependency truth. No review step checks the crate dependency graph against
the rules.

## T-06 (P2, AGENT): The verifier's gates do not match CI, and closures cite irrelevant gates

**Evidence**

- `scripts/land_work_verifier.sh` has been unchanged since `d01a22db`
  (2026-08-05). It still claims to run "the same gates ci.yml uses" but runs
  `test-standard`, `parity` and `conformance`. `test-standard` is cargo
  workspace tests plus clippy only, with **no ts:test, go:test or
  rust-fe:test**. Every check's output goes to `>/dev/null 2>&1`, so failure
  evidence is discarded. (This confirms str-qwua7.55 still holds.)
- str-qwua7.4 was a **Go** rapid test fix. Its close reason cites "land.py
  verify step passed on full task test-standard", which runs no Go tests. The
  only real Go evidence was a manual "run 3x clean".
- The `/pre-completion` skill output table has "Rust tests / TypeScript tests
  / Go tests PASS" rows, but the skill never says that an "is up to date" line
  means the suite did not run.
- CLAUDE.md Completion Checklist item 2 ("Property tests adequate") has no
  matching check in `.claude/skills/pre-completion/SKILL.md` (grep for
  `propert` finds none).

**Recommendation:** Make the verifier run `task affected` for the landing
diff, plus `check` with fresh caches. Print each gate's executed or cached
status and the last 50 lines of output on failure. In `/pre-completion`, add
a "gate evidence covers every changed language" row and a PBT row.

**Agent root cause:** There are three definitions of "the gate" (verifier,
CI, pre-completion), and close reasons are free text that is never checked
against the diff.

## T-07 (P2, L4): Local and CI test runners differ; nextest `ci` profile is dead

**Evidence**

- Local Taskfiles use `cargo nextest run` when `cargo-nextest` is on PATH
  (`shatter-core/Taskfile.yml:28-33,55-60`). CI never installs nextest
  (`ci.yml` steps), so it falls back to `cargo test`.
- `.config/nextest.toml` defines `[profile.ci]` (`retries = 1`,
  `fail-fast = false`, `final-status-level = "flaky"`). Nothing selects it: no
  `NEXTEST_PROFILE` or `--profile ci` anywhere in the Taskfiles, workflows or
  scripts.
- The `rust-frontend-harness` test group (`max-threads = 1`), which works
  around the known rust-fe fixture-build flake (memory
  `project_shatter_rust_heavyweight_test_flakes.md`; str-0f6ze), only applies
  under nextest. CI's `cargo test` has no such serialization.
- The local default profile has `fail-fast = true`. A single flake hides every
  later failure. The prior audit had to run core with no fail-fast by hand.

**Recommendation:** Install nextest in CI (taiki-e/install-action) and run
with `--profile ci`. Use `fail-fast = false` in check. Record flaky retries as
a machine-readable artifact so flakes become tracked issues instead of
folklore in memory files.

**Agent root cause:** Flake knowledge lives in private memory files, not in
tracked issues or gate output.

## T-08 (P2, L1): Snapshot tests create a missing snapshot and pass

**Evidence**

- `shatter-core/tests/outcome_md_snapshots.rs:45-51`,
  `run_markdown_ordering_snapshots.rs:38-44` and
  `source_set_summary_snapshots.rs:32-38`: `if !path.exists() { write(actual); return; }`.
  A deleted or renamed snapshot passes silently in CI.
- There are four hand-rolled copies of `fn assert_snapshot` (grep count 4) and
  no `insta`. `html_snapshots.rs:36` compares after collapsing all whitespace,
  so indentation and newline regressions in HTML reports are invisible.
- Only 8 snapshot files exist (`shatter-core/tests/snapshots/`). Terminal
  output of `explore`/`scan`, `--help`, and the askama markdown templates
  have none. CLAUDE.md:13 says "Regression snapshots are checked into the repo
  and verified in CI", and CI currently verifies none of them (T-01).

**Recommendation:** Adopt `insta`, with a failing `INSTA_UPDATE=no` and
`CI=1` mode. Add snapshots for CLI terminal output (explore, scan, help),
normalized for paths and timings.

**Agent root cause:** No convention or skill for snapshot tests, so each
author reinvented the helper.

## T-09 (P2, L4): Test inputs float with the external examples repo

**Evidence:** `scripts/examples_checkout.py:19-20,136`:
`DEFAULT_REPO_URL = ".../shatterproof-ai/examples.git"`, `DEFAULT_BRANCH = "main"`,
`reset --hard origin/main`, refreshed every 600 s. Unit, E2E, smoke, TS and
walkthrough tests read from it via `SHATTER_EXAMPLES_DIR`. No commit is
pinned, and no Task fingerprint covers it.

**Recommendation:** Pin an examples commit in-repo (for example
`scripts/examples.lock`). Check out that SHA, include the lock file in the
relevant `sources:`, and bump it via a PR so example changes go through
gates.

**Agent root cause:** n/a (design choice with no recorded trade-off; no issue
found by `bd search "examples repo"`).

## T-10 (P2, AGENT): Tests and gate infrastructure wired into nothing

**Evidence:** Scripts whose module name is not referenced by any Taskfile,
workflow or demo script:
`scripts/test_build_cache_doctor.py`, `test_docs_smoke.py` (54 tests),
`test_gate_event_log.py` (25), `test_gate_pressure.py` (30),
`test_perf_compare.py`, `test_validate_parity.py` (24),
`test_validate_protocol_registry.py` (11), `demo/test_gauntlet_check_output.py`.
`test_broad_run_validation_gate` (27) and `test_kapow_refute_agent` run only
under non-gate tasks. `test_ci_workflow_structure.py` runs in CI only
(str-35vtk.35 open). All pass today (run individually during this review).
`scripts/gate-event-log.py`, `scripts/gate-pressure.py` and
`scripts/gate-receipt.py` (751 lines, `docs/perf/gate-receipt-v1.md`) are not
invoked by any gate, wrapper or workflow. They are str-35vtk infrastructure
that landed without integration.

**Recommendation:** Add a meta test that discovers `scripts/test_*.py` and
`demo/test_*.py` and fails on any that no gate runs, with an explicit
allowlist. Either wire gate-receipt, event-log and pressure into
gate-wrapper or the verifier, or delete them.

**Agent root cause:** Epic children close on "script + tests landed" with no
"is it reachable from a gate" acceptance check.

## T-11 (P2, L4): End-to-end user paths never run in CI

**Evidence:** `ci.yml` runs only `task check` and the shatter-llm steps.
`check` does not include `smoke`, `walkthrough`, `gauntlet`, `e2e` (beyond
`core:test-ignored`, which is hollow in CI per T-01). `docs-smoke` is in
check-static but was reported "up to date" in CI, so it did not run either. The only CI
workflow that runs the gauntlet is Perf CI, which fails on
`gauntlet-auto-warm` every week (T-03). str-qwua7.10 (demo gates must fail on
bad output) is still open.

**Recommendation:** Add a CI job, on main or nightly, that runs `task smoke`
and `task walkthrough` (bounded) and uploads their output as an artifact.
Pair it with str-qwua7.10's content checks.

**Agent root cause:** Demo gates are run by agent discretion only; no CI
backstop.

## T-12 (P2, L1): TypeScript property tests are mostly round-trips

**Evidence:** Only 7 of 21 TS test files use `fc.assert` or `fc.property`.
`shatter-ts/src/property.test.ts:717-896` is almost entirely "survives JSON
round-trip" properties. `instrumentor.ts` (2,363 lines, where the
`buildSymExpr*` parity concern lives) and `analyzer.ts` (2,584 lines) have
example tests only (105 cases in `instrumentor.test.ts`) and no property
tests. CLAUDE.md Completion Checklist item 2 says PBT must cover "core
invariants, not just serialization roundtrips". Prior issues str-qwua7.47 and
.48 cover core and Go only.

**Recommendation:** Add fast-check properties for the instrumentor. Examples:
for random boolean and arithmetic ASTs, `buildSymExpr` and
`buildSymExprWithFlow` agree on shared nodes; every emitted SymExpr validates
against the protocol schema; `Param` indices stay within the signature.

**Agent root cause:** The PBT checklist item has no enforcement point (see
T-06).

## T-13 (P2, L4): Tier proliferation with copied bodies

**Evidence:** Public tiers include test, test-quick, test-standard,
check-fast, check, affected, pre-completion, pre-completion-e2e, e2e,
e2e-{ts,go,rust}, smoke, walkthrough(-cold), gauntlet(-cold), golden-test,
parity, conformance, broad-run-corpus, broad-run-validation and drift-patrol.
Four body pairs are duplicated only so that Task gives them separate
checksum identities, because "Task does not fingerprint caller-provided
environment variables" (`Taskfile.yml:136-139`, core `:62-64`, cli `:33-34`,
ts `test` / `test-fast`). A wiring test then enforces that the copies stay
in sync.

**Recommendation:** Collapse to about four documented tiers (dev, affected,
check, release). If caching stays, derive the cache identity from a `label:`
that includes the budget variable, or from a generated budget file in
`sources:`, instead of copying bodies. (The `label:` approach is untested
here; verify it against Task's checksum file naming.)

**Agent root cause:** Each efficiency issue added a tier. No owner prunes
them.

## T-14 (P3, L2): The fuzzing policy is not reflected in the code

**Evidence:** The `formal-methods-policy` skill (lines 14, 34-38) prescribes
`cargo-fuzz` for Rust deserialization and Go `testing.F`. There is no
`fuzz/` crate anywhere. Rust uses proptest-based
`shatter-core/tests/fuzz_deserialization.rs`. The 10 Go `Fuzz*` targets
(`shatter-go/{instrument,protocol}/fuzz_test.go`) only run their seed corpus,
because no gate or workflow runs `go test -fuzz=`. The policy's
`*_fuzz_test.go` naming does not match the actual `fuzz_test.go`.

**Recommendation:** Either add a weekly bounded fuzz job (`-fuzztime=60s`
per target) or change the policy to say "seed-corpus regression +
proptest-based fuzzing".

**Agent root cause:** The skill is aspirational, with no drift check against
the code.

## T-15 (P3, L1): str-qwua7.4 closed with its acceptance criteria unmet

**Evidence:**
`shatter-go/instrument/testdata/rapid/TestPropertyExecTimeoutAlwaysPositive/...-20260306134644-2197485.fail`
is still tracked (`git ls-files`). `shatter-go/.gitignore:3` ignores only
`planner/testdata/rapid/**/*.fail`. The issue's acceptance said
"testdata/rapid/**/*.fail removed from git and added to shatter-go/.gitignore".

**Recommendation:** `git rm` the file and change the pattern to
`**/testdata/rapid/**/*.fail`.

**Agent root cause:** Acceptance checks are not re-verified at close. The
close reason lists gates, not acceptance bullets.

## T-16 (P3, L1): Duplicate broad-run gates, neither in check

**Evidence:** `broad-run-corpus` (`Taskfile.yml:722-738`) uses
`tests/scripts/broad_run_validation.py` and `tests/fixtures/broad-run-corpus/`
(last touched 2026-05-13). `broad-run-validation` (`:897-917`) uses
`scripts/broad_run_validation_gate.py` and `tests/broad-run-corpus/`.
Neither is in `check`, `affected` or CI.

**Recommendation:** Keep one, delete the other, and put the survivor in a
scheduled job.

**Agent root cause:** n/a (dead-code sweep gap).

## T-17 (P3, L2): check-fast is stale and undocumented

**Evidence:** `Taskfile.yml:189` says "Fast quality gate (pre-push: ...)".
The pre-push hook uses `affected` or `check` (`scripts/setup-hooks.sh:165-176`).
The CLAUDE.md tier table does not list `check-fast`, yet gate-times.csv shows
151 invocations.

**Recommendation:** Document it or remove it, and fix the description.

**Agent root cause:** n/a

## T-18 (P3, L1): Workflow action versions are aging

**Evidence:** Every run carries an annotation: "Node.js 20 is deprecated ...
actions/cache@v4, actions/checkout@v4, actions/setup-go@v5,
actions/setup-node@v4". There is also a notice that `ubuntu-latest` migrates
to Ubuntu 26 on 2026-10-19.

**Recommendation:** Bump the actions. Pin `ubuntu-24.04` until the Z3 and
libclang packages are verified on 26.

**Agent root cause:** Annotations appear only in the web UI (T-03).

---

## Status of prior-audit items in this area

| prior | status now |
|---|---|
| str-qwua7.2 check-fresh | Still absent. More urgent given T-01. |
| str-qwua7.3 unexplained "up to date" | **Root cause found (T-01).** |
| str-qwua7.4 Go rapid failure | Fixed. The failfile half of the issue is incomplete (T-15). |
| str-qwua7.10 demo gates sanity | Open. Still no CI backstop (T-11). |
| str-qwua7.46 doc linters | Still `[skip]` in CI (log lines "[skip] markdownlint-cli2/vale/lychee/actionlint/semgrep"). |
| str-qwua7.47/.48 PBT | Open. TS gap added (T-12). |
| str-qwua7.55 verifier honesty | Unchanged since 2026-08-05 (T-06). |
| str-qwua7.1 bare primary | `git -C ~/project/shatter config core.bare` now prints `false`. The issue is still open, so check whether it can be closed. |

## Positives worth keeping

- `scripts/gate-wrapper.sh` is careful engineering: a counting semaphore, a
  re-entrancy pass-through, a locked CSV append and graceful degradation. Its
  `~/.cache/shatter/gate-times.csv` telemetry (833 rows) made T-01 provable
  after the fact. Consider publishing a weekly summary from it (median
  runtime, failure rate per gate, share of all-cached runs).
- `affected-gates.py` fails safe: unknown paths fall back to `check`, and
  Taskfile, Cargo.toml and Cargo.lock always force `check`.
- The script-level unit test culture is strong: dozens of Python unit test
  modules, and structural wiring tests such as `test_test_tier_wiring`,
  `test_governed_task_graph` and `test_ci_workflow_structure`. They test
  shape, though, not effect.
- The known-answer E2E suites for all three frontends and the nextest test
  group for the rust-fe fixture builds are the right designs.
- Go's rapid/plain package partitioning (`scripts/go-test-tier.sh`) is a
  careful fix for rapid's flag registration.

## Commands used (selection)

- `gh run list --limit 300 --json workflowName,conclusion,createdAt,startedAt,updatedAt`
- `gh api repos/{owner}/{repo}/actions/jobs/<id>/logs` (CI and Perf CI logs), `.../check-runs/<id>/annotations`
- scratch Task reproduction in `$SCRATCH/tasktest{,2}` (v3.50.0)
- `python3` analysis of `~/.cache/shatter/gate-times.csv`
- `python3 -m unittest scripts.<module>` for the 10 unwired or under-wired test modules (all OK)
- `select_gates()` probes of `scripts/affected-gates.py`
