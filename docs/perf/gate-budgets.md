# Gate Timing Baselines and Budgets

Baseline captured 2026-07-10/11 on the primary checkout at commit e3614d3f
(WSL2, 32 cores, 47 GB RAM), pre-efficiency-plan (epic str-35vtk). Budgets are
1.5× the baseline median and will be recomputed from post-change medians when
WS-H (str-35vtk.10) closes. Loadavg column is the 1-minute load at run start —
idleness could not be guaranteed, so treat entries with high start-load as
upper bounds.

Measurement recipe: one `task build` warm-up, then per gate the run counts
below. go-task `sources:` fingerprinting makes an unchanged-tree re-run a
no-op (~0.1s); rows marked *forced* used `task --force` to measure real work,
which is what an agent pays after editing code. Raw CSV: see the WS0 issue
(str-35vtk.1) close notes.

## Baseline results

| Gate | Runs | Baseline median (s) | Budget (s) | Notes |
|---|---|---|---|---|
| build (incremental, warm) | 1 | 37.9 | 57 | primary checkout warm-up |
| build (cold fresh worktree) | 1 | 102.3 | 153 | 582 crates + npm install; creates ~12.5 GB of target/node_modules |
| build (warm no-op rebuild) | 1 | 3.7 | 6 | |
| smoke (first run of day) | 1 | 15.3 | 23 | examples-checkout network refresh dominates |
| smoke (forced, warm) | 2 | 8.2 | 12 | |
| smoke (fingerprint no-op) | 2 | 2.3 | 3.5 | |
| test-quick (forced) | 2 | 91.1 | 137 | real cost of the inner-loop tier today |
| test-standard (forced) | 2 | 81.8 | 123 | ran right after test-quick; shares warm build |
| parity (forced) | 2 | 5.5 | 8 | unforced no-op is 0.1s |
| conformance (forced) | 1 | 9.1 | 14 | |
| e2e | 1 | 77.0 | 116 | includes test-time `go build` (removed in WS-B) |
| check | 1 | 620.8 | 931 | **exit 201 — see failure note** |
| walkthrough | 1 | 115.7 | 174 | full cold compile by design (changes in WS-D) |
| gauntlet | 1 | 410.1 | 615 | **exit 201 — see failure note**; full cold compile by design |

## Failure note: `check` and `gauntlet` failed under self-induced load

Both failures are load-induced flakes, not code regressions, and are themselves
baseline evidence for WS-C (resource governance) and WS-M (test mechanics):

- `task check` runs its ~16 dep subtasks in parallel; 1-minute loadavg went
  from 2.3 to **66 within a minute and peaked at 105** on 32 cores. Under that
  load, `rust-fe:test` (shatter-rust executor tests, which spawn nested
  `cargo build`s of fixture crates) hit 13× `CompilationFailed("build timed
  out")`, and one Go suite hit its 10m0s timeout. A single full check on an
  otherwise idle machine saturates the box past the point where its own
  subtests' nested builds meet their deadlines.
- `task gauntlet` (running while loadavg was ~105 in the tail of the same
  window) lost exactly one frontend request to a 30s observe timeout
  (`negotiate_language`).

Implication recorded for WS-H: post-change acceptance for `check` is not just
"faster" but "passes reliably, bounded load".

## Concurrency headline (pending — deliberately not run unattended)

WS0 calls for 4 worktrees running `task check` simultaneously (aggregate wall,
peak load, responsiveness). Given that a single `check` already drives loadavg
to 105 and flakes its own subtests, running 4× would render the machine
unusable for ~30–60 minutes, so it was **not run unattended**. The scenario is
scripted and ready; run it at a convenient time and append the results here.
The post-WS-C re-run of the same scenario is the before/after comparison that
matters.

## Test inventories (no-silent-loss reference)

Snapshots in `docs/perf/inventories/` (captured at the same commit):
rust-shatter-core 3,346 tests; rust-shatter-cli 558; rust-shatter-rust 1,078;
rust-shatter-rust-runtime 25; go 1,039 test funcs; ts 18 jest suites
(file-level; jest names enumerate per-run). WS-M diffs against these after any
tiering change.

## Other observations feeding the workstreams

- Fingerprint no-ops mean an agent who runs a gate twice without edits pays
  ~0s the second time — but every real edit invalidates the fingerprint, so
  forced numbers are the honest per-issue cost.
- Cold worktree cost is dominated by CPU (≈50+ core-minutes of dep compiles,
  identical across worktrees — the sccache case) and disk (~12.5 GB per
  worktree × ~24 worktrees), not wall time on an idle box.
- `test-quick` (91s forced) currently includes the 23 non-`#[ignore]`d
  node-subprocess TS e2e tests (WS-M moves them to the e2e tier).
