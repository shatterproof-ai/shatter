# Shatter Build/Test Efficiency Plan

Tracker: epic str-35vtk (children str-35vtk.1 – str-35vtk.10). This file is a
mirror of the user-approved plan (2026-07-10, two planning sessions merged);
**the child issue bodies are authoritative** for per-workstream scope and
acceptance. Decision record: merging shatter-rust into the root cargo
workspace was considered and rejected — generated fixture crates path-dep on
shatter-rust-runtime as an independent workspace, and a shared root lockfile
would rebuild-couple concurrent agents to frontend churn. sccache + CI-cache
coverage bridge the two build graphs instead.

## Context

The build+test cycle has become prohibitive: every worktree rebuilds ~11.4 GB of artifacts cold (root `target/` 6.4G, `shatter-rust/target/` 4.7G, `node_modules` 243M), there is **zero** build caching infrastructure (no `[profile]` overrides, no `.cargo/config.toml`, no sccache, no fast linker), demo scripts deliberately compile from scratch into throwaway `mktemp` target dirs, gates redundantly rebuild the three frontends, and the required `pre-completion` gate runs the full ~16-subtask `check` fan-out for every issue regardless of what changed. With N concurrent agents this thrashes the machine.

**User decisions already made:**
- All five themes approved (caching, gate dedup, change-aware gating, resource governance, measurement).
- **Swarm-level CI model:** worker agents run diff-scoped gates only; the lead agent batches multiple landings into ONE full `check` per batch. Full check remains the backstop at landing time.
- Demos become **warm-by-default** with a `--cold` flag preserving today's hermetic behavior.

Host facts: WSL2, 32 cores, 47 GB RAM. Not installed yet: sccache, mold, lld, cargo-nextest. z3-sys links **system** libz3 dynamically (z3 itself never rebuilt; cost is dep compilation + linking).

## Sequencing overview

0. Baseline timing capture (before anything lands)
1. WS-A: host + build config (profiles, mold, sccache, node seeding) — one coordinated cold rebuild
2. WS-B: Taskfile/gate dedup (zero policy risk)
3. WS-C: resource governance + timing wrapper
4. WS-D: demo warm-by-default
5. WS-E: diff-scoped gating (affected-gates)
6. WS-F: batch landing protocol — **land E and F together or in one swarm wave** (diff-scoped workers without the lead batch-check backstop would briefly weaken coverage)
7. WS-G: nextest (optional, needs dedicated validation)
8. WS-H: anti-regression closeout

## Step 0 — Baseline (do first)

Run each heavyweight gate 3× on an idle machine; record median wall times in new `docs/perf/gate-budgets.md` (gate, baseline, budget = 1.5× baseline). Gates: `test-quick`, `test-standard`, `check`, `e2e`, `smoke`, `walkthrough`, `gauntlet`, `parity`, `conformance`, plus cold-worktree first build.

## WS-A: Compilation caching & sharing (highest leverage)

### A1. Cargo profile tuning (XS effort, HIGH impact)
Add to all three workspace roots — `Cargo.toml`, `shatter-rust/Cargo.toml`, `shatter-rust-runtime/Cargo.toml`:
```toml
[profile.dev]
debug = "line-tables-only"
split-debuginfo = "unpacked"

[profile.dev.package."*"]
debug = false
```
No separate test profile (would double artifacts; `cargo test` shares dev). Follow-up (separate issue, needs e2e+conformance run): have `shatter-rust/src/executor.rs` emit the same profile block into generated fixture-harness Cargo.tomls.

### A2. mold linker — per-machine, NOT checked in (S, HIGH)
`~/.cargo/config.toml` (covers all worktrees; repo config would break machines/CI lacking mold):
```toml
[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
```
Host install: `sudo apt-get install mold` (user runs; fallback lld). Document host setup in AGENTS.md.

### A3. sccache shared across worktrees (M, HIGH)
**Rejected alternative:** shared `CARGO_TARGET_DIR` — cargo's target-dir flock serializes all agents and cross-branch fingerprint thrash makes it worse than useless.
- Install sccache; add to `~/.cargo/config.toml`: `[build] rustc-wrapper = "<abs path>/sccache"`.
- Env in shell profile: `SCCACHE_DIR=$HOME/.cache/sccache`, `SCCACHE_CACHE_SIZE="40G"`.
- Covers root workspace + shatter-rust + shatter-rust-runtime + examples + runtime fixture-harness builds automatically.
- Keep `CARGO_INCREMENTAL` untouched (deps compile non-incrementally → cache fine; workspace members stay incremental).
- Document `sccache --stop-server` recovery for wedged daemon.

Land A1–A3 together: single coordinated fingerprint-invalidating cold rebuild per worktree.

### A4. node_modules seeding (S, MED)
Keep npm. New `scripts/seed-worktree.sh`: if canonical checkout's `package-lock.json` matches, `cp -al` (hardlink clone, ~1s) `shatter-ts/node_modules` into the new worktree; else `npm ci`. Document in AGENTS.md worktree-creation flow. Safe: npm unpacks fresh files, never rewrites in place.

### A5. CI cache fixes (S, HIGH for CI)
`.github/workflows/ci.yml`: extend cargo cache `path` to include `shatter-rust/target` and `shatter-rust-runtime/target`; key on all lockfiles: `ci-cargo-${{ hashFiles('Cargo.lock', 'shatter-rust/Cargo.lock', 'shatter-rust-runtime/Cargo.lock') }}` (verify locks committed; else key on Cargo.tomls). Defer sccache-in-CI unless this proves insufficient (GHA 10 GB cache limit). A1 shrinks cached dirs substantially.

## WS-B: Gate build dedup (zero policy risk)

- **Taskfile.yml cleanup:** delete the dead first `parity` definition (line ~94); `check` currently runs `parity` twice (dep line ~273 AND cmd line ~279) — keep one.
- **Split `e2e`** into `e2e-ts` / `e2e-go` / `e2e-rust`; `e2e` aliases all three.
- **Go double-build fix:** `shatter-go/Taskfile.yml` `build` runs `go build ./...` then `go build -o bin/shatter-go .` — drop the first (vet/test cover package compilation).
- **Stop go-build-at-test-time:** `shatter-core/tests/e2e_concolic_go.rs` (~line 76) and `e2e_concolic.rs` (~line 1950): honor new `SHATTER_GO_FRONTEND_BIN` env, keep existing build-into-tempdir fallback for bare `cargo test`. Root Taskfile `e2e*`: add `go:build` dep and export `SHATTER_GO_FRONTEND_BIN`.
- Frontend prebuilt reuse otherwise already works via task deps; no relocation needed.
- **Do NOT merge shatter-rust into the root workspace** (exclusion is load-bearing: fixture crates path-dep on shatter-rust-runtime; lockfile coupling would hurt concurrent agents). sccache + CI cache bridge the graphs. Record as a decision note.

## WS-C: Resource governance + measurement

- **Repo `.cargo/config.toml`** (checked in — coexists with machine `~/.cargo/config.toml`; cargo merges, repo wins per-key and it sets different keys):
  ```toml
  [build]
  jobs = 8
  [env]
  RUST_TEST_THREADS = { value = "4", force = false }
  ```
  CI workflow overrides via `CARGO_BUILD_JOBS`. Root Taskfile top-level `env:`: `GOFLAGS: "-p=4"`, `GOMAXPROCS: "8"` (inherited by included taskfiles).
- **New `scripts/gate-wrapper.sh`** — machine-wide counting semaphore (2 slots via `flock -n` on `/tmp/shatter-gate-slot-{1,2}`, block on slot 1 when full) + `nice -n 10 ionice -c2 -n7` + timing emit. Re-entrancy guard: `SHATTER_GATE_LOCK_HELD=1` → passthrough (Task deps nest, e.g. `check` → `conformance`).
- Wrap only heavyweight tasks: `check`, `e2e*`, `conformance`, `parity`, `gauntlet`, `walkthrough`, `core:test*`. Light tasks unwrapped.
- **Timing capture:** wrapper appends CSV to `~/.cache/shatter/gate-times.csv`: timestamp, worktree, gate, wall-secs, exit-code, 1-min loadavg, slot count. Truncate to last 10k lines.
- AGENTS.md: documented cap — max 2 concurrent heavyweight suites machine-wide; don't bypass the wrapper with bare `cargo test --test e2e_*` when a task exists.

## WS-D: Demo warm-by-default + `--cold`

`demo/walkthrough.sh` (~lines 19–26), `demo/gauntlet.sh` (~lines 25–50 + cleanup trap). The mktemp cost is in **runtime fixture-harness builds** (Rust frontend cargo builds of generated crates, cold GOCACHE), not building shatter itself.
- `--cold`: today's behavior verbatim (mktemp CARGO_TARGET_DIR/GOCACHE/XDG_CACHE_HOME, rm -rf trap).
- Warm default: stable per-user caches outside the repo — `WARM_ROOT="${SHATTER_DEMO_CACHE:-$HOME/.cache/shatter-demo}"`; `SHATTER_CACHE_DIR`, `SHATTER_HARNESS_CACHE` (activates `standalone_target_dir()` persistence in `shatter-rust/src/executor.rs`), stable `$WARM_ROOT/cargo-target`; GOCACHE/XDG inherited from host. Keep `SHATTER_ARTIFACT_DIR` mktemp per run.
- Hermetic-validation role moves explicitly to `--cold` (document in task desc + AGENTS.md; release validation uses `--cold`).
- Validation before landing: `/walkthrough-review` + full gauntlet warm + one `--cold` pass.

## WS-E: Diff-scoped gating for worker agents

### E1. `scripts/affected-gates.py` + `scripts/test_affected_gates.py` + `task affected`
- `git diff --name-only $(git merge-base <base> HEAD)..HEAD` → ordered rule table → union of gates. **Any unmatched path → emit full `check`** (fail-safe by construction).
- Mapping (crate-level granularity only; never file-level inside shatter-core):

| Changed paths | Gates |
|---|---|
| `*.md`, `docs/**` only | `docs` |
| workflows/semgrep/install.sh/release scripts/shatter-go-tool | `meta` |
| `shatter-core/**` | `core:test`, `core:clippy`, `smoke`, `e2e` (all 3) |
| `shatter-cli/**` | `cli:test`, `cli:clippy`, `smoke`, `e2e` (all 3); + `gauntlet` if command-surface files changed |
| `shatter-ts/**` | `ts:test`, `ts:typecheck`, `smoke`, `e2e-ts`, `parity`, `conformance` (always — conservative) |
| `shatter-go/**` | `go:test`, `go:vet`, `smoke`, `e2e-go`, `parity`, `conformance` |
| `shatter-rust/**` | `rust-fe:test`, `rust-fe:clippy`, `e2e-rust`, `parity`, `conformance` |
| `shatter-rust-runtime/**` | `rust-rt:test`, `rust-rt:clippy`, `e2e-rust` |
| `protocol/**` | `schemas`, `parity`, `conformance`, `ts:test`, `go:test`, `rust-fe:test` |
| `demo/walkthrough*` / walkthrough examples | `walkthrough` |
| `demo/gauntlet*`, allowlist | `gauntlet` |
| `examples/**`, `examples_checkout.py` | `smoke`, `e2e` (all) |
| `.beads/**`, `.claude/**` | none |
| Taskfiles, Cargo.toml/locks, unmatched `scripts/**`, anything else | `check` (fail-safe) |

- Append `smoke` whenever any code gate emitted. Unit tests cover the mapping + fail-safe + "every emitted gate is a real task" assertion.
- E2E never dropped for core/cli/frontend changes — only narrowed to the touched frontend (guards this repo's parallel-code-path hazard).

### E2. Guidance rewrite
- **CLAUDE.md:** add "Affected" tier row (`task affected`) to the tier table; reframe `check` as batch-landing/CI/fail-safe; Completion Checklist item 1 → "`task affected` passes"; note solo agents merging to main themselves still run full `check`.
- **`/pre-completion` skill:** Phases 1–2 → run `affected-gates.py --base origin/main`, print list, execute via task through the wrapper; delete unconditional `task e2e`; add a **"Gates selected"** row to the output table (lets the lead audit under-selection); add "in a swarm, full check runs at batch landing".
- Audit `/check-all` and `/bugfix` skills for contradicting "run task check" language; align.

## WS-F: Batch-landing protocol (lead agent)

Files: AGENTS.md (new "Batch Landing (lead agent)" section), `.claude/swarm-config.md` (rewrite Quality Gates — this is what `bento:swarm` reads), new `.claude/skills/batch-land/SKILL.md`.

1. **Batching:** pushed, reviewed, non-overlapping branches; ≤5 per batch or ~30-min window. Branch whose gates hit the `check` fail-safe lands in a batch of 1–2 (cheap attribution).
2. **Serial integration:** lead only, one landing at a time, sequential `--no-ff` merges in the primary checkout/integration worktree; nothing pushed until green (preserves existing serial-merge + ancestry doctrine and the shared-checkout/beads-DB constraint).
3. Record each branch's `affected-gates` output pre-merge; after all merges run ONE `task check` + union of conditional gates (E2E frontends, walkthrough, gauntlet) via gate-wrapper.
4. **Pass:** push once → verify each branch by ancestry → batch `bd close` → delete branches.
5. **Fail — attribution ladder:** (a) failing gate appears in exactly one branch's list → prime suspect; (b) re-merge all except suspect, re-run only the failing gate; (c) ambiguous → linear removal in reverse merge order (N≤5 beats bisect); (d) rebuild integration without offender, land the rest (re-run failing gate + one final `check` on reduced batch — the skill must make this step explicit), reopen offending issue with gate output, reassign.
6. Worst case: 1 full check + ≤N single-gate re-runs — still far below N full checks.

## WS-G: cargo-nextest (optional, MED)

- New `.config/nextest.toml` with a `max-threads = 1` test-group covering ALL consumers of `tests/support/rust_frontend_harness.rs` — **mandatory**: nextest is process-per-test, so the in-process tokio Mutex serializing fixture-crate builds stops working; without the test-group the known flakiness returns worse.
- `shatter-core`/`shatter-cli` Taskfiles: `command -v cargo-nextest` → `cargo nextest run` else `cargo test` fallback (tooling probe already lists it as optional). Ignored tier: `--run-ignored all`.
- nextest skips doctests — check for doc examples; append `cargo test --doc` in the nextest branch if any exist.
- Validate with repeated `task check-fast` under load before landing.

## WS-H: Anti-regression closeout

- `docs/perf/gate-budgets.md`: baselines (step 0) + post-change medians + budgets (1.5×).
- Extend repo `/audit` skill: "gate timing" section reading `~/.cache/shatter/gate-times.csv`, compare recent medians vs budgets, file beads issue on sustained breach.
- Optional soft stderr warning in gate-wrapper at >1.5× budget (never a failure). **No CI wall-time gate** — hopelessly noisy on a contended box.

## Critical files

- `Cargo.toml`, `shatter-rust/Cargo.toml`, `shatter-rust-runtime/Cargo.toml` — profiles
- `~/.cargo/config.toml` (machine) — mold + sccache; repo `.cargo/config.toml` — job caps
- `Taskfile.yml` — parity dedup, e2e split, `affected` task, wrapper wiring, env caps
- `shatter-go/Taskfile.yml` — double-build fix
- `shatter-core/tests/e2e_concolic_go.rs`, `e2e_concolic.rs` — `SHATTER_GO_FRONTEND_BIN`
- `demo/walkthrough.sh`, `demo/gauntlet.sh` — warm/`--cold`
- `.github/workflows/ci.yml` — cache paths/keys
- New: `scripts/affected-gates.py` (+tests), `scripts/gate-wrapper.sh`, `scripts/seed-worktree.sh`, `.config/nextest.toml`, `.claude/skills/batch-land/SKILL.md`, `docs/perf/gate-budgets.md`
- Updated guidance: `CLAUDE.md`, `AGENTS.md`, `.claude/swarm-config.md`, `.claude/skills/pre-completion/SKILL.md`, `/check-all`, `/bugfix`, `/audit` skills

## Verification

1. **Before/after timings** per gate from step-0 baseline vs. post-WS-A/B medians (the deliverable proving impact).
2. **Cold-worktree test:** create a fresh worktree post-sccache; first build should be mostly cache hits + link time; `sccache --show-stats` hit rate.
3. **Behavioral gates:** after WS-B e2e changes → `task e2e` + `task conformance`; after WS-D → `/walkthrough-review` + gauntlet warm + one `--cold` pass; after WS-G → repeated `task check-fast` under load.
4. **Gating correctness:** `scripts/test_affected_gates.py` (mapping, fail-safe, gate-name validity); dry-run `task affected` on several recent real commits and eyeball selections.
5. **Concurrency test:** run 3 simultaneous heavyweight gates from different worktrees; confirm semaphore holds 2, machine stays responsive.
6. **Batch protocol dry-run:** simulate a 3-branch batch with one intentionally broken branch; confirm attribution ladder isolates it and the reduced batch lands.

## Amendments (merged from ~/tmp/shatter-build-test-efficiency-plan.md, a parallel planning session — verified 2026-07-10)

Additional verified facts: `.config/nextest.toml` already exists (slow-timeouts + ci profile, unused — WS-G becomes "wire it in", not "create it"); TS `shatter-core/tests/e2e_concolic.rs` has NO `#[ignore]` so its 23 node-subprocess tests run even in `test-quick`; `go test ./...` runs without `-short` though ~18 compile-heavy e2e files carry `testing.Short()` guards; ~64 of 68 core proptest blocks run at 256-case default, fuzz_deserialization at 1000; `check` includes `core:test-ignored` (`--include-ignored` = superset containing the e2e suites) so the landing gate already runs e2e; a pre-push hook runs `task check` on push to main (serialized landing point already exists); `scripts/precommit-rust.sh` already implements staged-files→affected-crates gating; `examples_checkout.py` refresh (network fetch + `git reset --hard` on shared /tmp/shatter-examples-main) has NO lock; `demo/gauntlet.sh` writes fixed `/tmp/shatter-spec*.json`/`/tmp/shatter-observe.json` paths colliding across concurrent runs; beads hooks run with a 300s timeout ceiling and `backup.git-push: true` does network I/O inside git hooks; ~24 worktrees carry stale multi-GB target dirs (disk GC opportunity). User decisions recorded there and adopted: **PBT case tiering approved** (reduced cases inner tiers, full at landing+CI); **full suite exactly once per landing**; quality reduction is not — anything removed from a gate must be shown covered elsewhere on the landing path.

Plan deltas:
- **WS0**: also measure the concurrency headline (4 worktrees × `task check`: wall, loadavg, responsiveness), cold + warm fresh-worktree builds, and snapshot test inventories (`cargo nextest list`, jest `--listTests`, go test lists) for later no-silent-loss diffs.
- **WS-A**: add one-time disk GC of stale worktree target dirs (bento:closure candidates) + size report; verify ordinary go build/test uses default shared GOCACHE.
- **WS-B**: add conformance-vs-parity golden-test overlap audit (remove only provably identical coverage); re-stage `check` fail-fast (cheap static → unit → expensive integration); verify+document the `core:test-ignored` ⊇ e2e superset claim.
- **New WS-M (test mechanics, same coverage less CPU)**: wire nextest into core/cli/rust-fe/rust-rt test tasks (thread budgets in existing .config/nextest.toml; doctest check); add `#[ignore]` to `e2e_concolic.rs` tests (landing's `--include-ignored` + `e2e` task still run them); `go:test-short` fast path for inner loops (full `go test` stays at landing); PBT case tiering via PROPTEST_CASES=32 / fast-check numRuns / rapid checks in fast tiers, full counts exported at `check` + CI; verify with test-inventory diffs.
- **New WS-CS (concurrency safety)**: gauntlet fixed /tmp paths → its mktemp dir; flock + freshness window for examples_checkout.py; BEADS_HOOK_TIMEOUT=30 (hooks degrade gracefully); investigate moving backup.git-push off the hook path (verify bd semantics first); sweep demo/scripts for remaining fixed paths.
- **WS-C**: slot count becomes `SHATTER_HEAVY_SLOTS` (default max(1, nproc/8) = 4 on this box); per-agent budgets via overridable env (SHATTER_BUILD_JOBS=4, SHATTER_TEST_THREADS=4, SHATTER_GO_PROCS=4).
- **WS-E**: within shatter-core/cli, e2e triggers on pipeline path patterns (solver/instrumentor `buildSymExpr*`/explorer/orchestrator/execute-response protocol/CLI wiring — the CLAUDE.md e2e-gate list) instead of whole-crate, justified because the landing gate always runs e2e (superset); reuse the precommit-rust.sh pattern; pre-push hook on branch pushes becomes change-scoped `check-fast` (landing gate backstops).
- **WS-F**: batch landing composes with the existing pre-push hook: the lead runs the batch `check` explicitly, then pushes `--no-verify` (documented; matches the existing hook-corrupts-worktree workaround); the hook remains for solo/non-batch pushes.
- **WS-G**: folded into WS-M (config exists; no longer conditional).
- **Later phase**: TIA revival (docs/plans/str-zwgc-test-impact-analysis.md, tracker str-cl53) after these land and re-baseline.
- Demos stay warm-by-default with `--cold` (this session's explicit decision; the parallel plan didn't address demo cache reuse).

## Notes / constraints

- Host steps (apt install mold, cargo install sccache, `~/.cargo/config.toml`, shell-profile env) are outside the repo and need user-run commands (sudo).
- All repo edits happen on a feature branch in a linked worktree per house rules (bento:launch-work).
- WS-E and WS-F land together (or within one swarm wave) to avoid a coverage gap.
