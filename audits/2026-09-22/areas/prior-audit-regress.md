# Area: tracker health & regression check (AGENT, L5) — audit 2026-09-22

Reviewer scope: drift patrol, bd tracker health, verification that closed
issues' fixes are really in code, why str-qwua7 (audit 2026-09-04 epic) stalls.
Worktree: `audit-2026-09-22` @ `16794cef` (== origin/main). Observation only.

Binary used for live checks: `target/debug/shatter` built in this worktree at
16794cef (`run-heavy cargo build -p shatter-cli`, exit 0, ~20 min under load),
plus the pre-existing `shatter-rust/target/release/shatter-rust` put on PATH.
Scratch dirs for runs: `$SCRATCH/{ex,ex2,ex3,ex4,rs}` (session scratchpad).

---

## 1. Drift patrol

### 1a. Local run

`python3 scripts/drift-patrol.py` → **exit 1**, "1 failed, 2 pending, 1 skipped, 4 passed".

| Check | Status | Note |
|---|---|---|
| protocol-registry | PASS | |
| protocol-codegen | PASS | |
| protocol-conformance | SKIP | frontends not built in this worktree at run time |
| parity-expiry | PASS | (was FAIL on 09-03; fixed by str-ajjri) |
| cli-surface-drift | PENDING | str-wurp open since 2026-06-12 |
| docs-stories | PENDING | str-u394l.3 open since 2026-06-17; `docs/stories` still absent |
| tracker-hygiene | **FAIL** | 2 stale in_progress, 4 orphans |
| tracker-server | PASS | new check from str-qwua7.16 works |

tracker-hygiene verbatim:
```
-- in_progress with no update for >14d (2) --
str-8q1b4 (22d) Resumed scans produce reports not comparable to uninterrupted scans
str-mpgg1 (20d) Revert unauthorized beads-hook edits from str-35vtk.4
-- open/in_progress children under a closed parent (4) --
str-qwua7.56.1 [open] — parent str-qwua7.56 (bug) is closed
str-hy9b.J3 [open] — parent str-hy9b (epic) is closed
str-hy9b.1 [open] — parent str-hy9b (epic) is closed
str-qwua7.9.1 [open] — parent str-qwua7.9 (task) is closed
```

### 1b. The scheduled patrol has never run (NEW, P1)

`gh run list --workflow drift-patrol.yml`:
```
failure schedule 2026-09-21, 09-14, 09-07, 08-31, 08-24, 08-17, 08-10
success pull_request 2026-08-07 (str-u394l-1-drift-patrol)  <- self-test job only
```
`gh run view 35620815498`: `X The specified go version file at: go.mod does not exist`.
`.github/workflows/drift-patrol.yml:85` has `go-version-file: go.mod`; the file
is `shatter-go/go.mod` (ci.yml:53, perf-ci.yml:36, release.yml:131 all use the
correct path). The patrol job fails at setup in ~18 s, before
`scripts/drift-patrol.py` ever runs. Seven consecutive red weekly runs, no
tracker issue. str-u394l.1 ("Scheduled drift patrol", P1) was closed 2026-08-07
on local `task test-standard` + the PR run, which only executes the
`self-test` job (`if: github.event_name != 'pull_request'` guards the patrol
job) — i.e. closed before the trigger it delivers had ever executed once.
Contrast str-35vtk.21, whose body required "remains open until the first
resulting GitHub Actions CI run is green".

### 1c. Other permanently red workflows (NEW, P1)

| Workflow | Runs listed | Successes | Failing step |
|---|---|---|---|
| Build and Release (`release.yml`, every push to main) | 266 | **0** | Windows `Build shatter CLI (cargo)`; aarch64 `cross build` → `failed to run custom build command for openssl-sys v0.9.116` |
| Perf CI (`perf-ci.yml`, weekly) | 13 | **0** | `Run stable perf scenarios` → `gauntlet-auto-warm failed on run 1 with exit code 1` |
| Drift Patrol | 8 | 1 (PR self-test only) | setup-go (above) |

No open bd issue mentions release.yml / Build and Release / perf-ci failures
(search of all 1773 issues for `release.yml|build and release|perf-ci|openssl|windows-msvc`
finds only closed str-lj7s and open str-qwua7.42 (unrelated: dir merge)).
The aarch64 failure predates str-qwua7.41's deletion of `cross/` + `Cross.toml`
(run 33343284223, before 09-06, same failure), so .41 did not cause it, but
.41's claim "no workflow uses cross" was wrong — release.yml:121-160 uses
`cross` (it simply never needed the deleted Dockerfile).

---

## 2. Tracker health (bd)

`bd stats`: 1773 total; 177 open, 2 in_progress, 23 blocked, 1589 closed, 154 ready.
(bd was healthy and fast this time — 1.3 s — confirming str-qwua7.16's repair.)

### 2a. Stale claims
- **str-mpgg1** (P1 bug, in_progress since 09-01): its work *is landed* —
  `git merge-base --is-ancestor 84941b37 origin/main` → true (commits b5cd25ec,
  ccb6e30b, 84941b37). Landed-but-not-closed.
- **str-8q1b4** (P1 bug, in_progress since 08-28, assignee "Test"): branch
  `origin/str-8q1b4-resume-report-parity` is 4 ahead / **105 behind** main,
  last commit 2026-08-31, worktree still registered.
  This is the 3rd time this branch has been flagged (audit 09-04, str-qwua7.18 item 1).

### 2b. Obsolete open issues (premise already satisfied)
| Issue | Premise | Reality today |
|---|---|---|
| str-qwua7.1 (P1) | primary checkout `core.bare=true`; add a hygiene check | `/home/ketan/project/shatter/.git/config` `bare = false`; `git status` works there; bento `hooks/scripts/agent-env-doctor.py:669-680` ("check 5: bare primary checkout") and `land-work-prepare.py:113` now detect it |
| str-qwua7.12 (P1) | "file not found / unsupported ext / unknown fn / malformed spec / sandbox refusal all exit 1" | all five exit **2** at HEAD (live run below). `error_exit_code` (`shatter-cli/src/main.rs:1475-1481`, GateFailure→1 else 2) dates from `464e3c9e` 2026-08-26 — **before** the 09-04 audit base (`git merge-base --is-ancestor 464e3c9e e067979d` → true). The 09-04 finding came from a stale `target/release/shatter` |
| str-qwua7.18 (P1) | land six parked branches | 5 of 6 landed under their own ids (str-rmcrl 09-21, str-na9db 09-21, str-0z1im 09-19, str-6vl7p 09-21, str-duens 09-19); only str-8q1b4 left |
| str-qwua7.19 (P1) | push 3 local commits, prune previews | main == origin/main (`git rev-list --count origin/main..main` → 0); mpgg1 commits on origin/main; `git worktree list` → 4 entries |

Live exit-code evidence (scratch dir, `target/debug/shatter`):
```
spec-diff bad.json bad.json      -> exit=2
explore nope.ts:foo              -> exit=2
explore a.ts:doesNotExist        -> exit=2
explore a.ts:f (no sandbox/flag) -> exit=2
explore foo.txt:bar              -> exit=2
```
Blocking edges: str-qwua7.18 and .19 are `blocked_by str-qwua7.1` — a blocker
whose premise is gone, so four P1 items that are effectively done sit in (or
block) the ready queue: bd ready's top 20 contains .1 and the parent epic.

### 2c. Orphans and the follow-up-as-child pattern
str-hy9b.J3 and str-hy9b.1 (parent closed 2026-04-27) have been open for
five months. The prior audit flagged them (issues-hygiene.md:74-76), and
str-7nlk/str-fdy7/str-fg5e ("Tracker hygiene: resolve stale in_progress
issues (… str-hy9b …)") were all closed (two as racing-filing duplicates,
one blank), yet the orphans remain. str-5b9f "Epic-close lifecycle rule" has
been open since 06-12.
New orphans str-qwua7.9.1 and str-qwua7.56.1 were created by the standard
"file the review follow-up as a child of the issue you're closing" habit —
the landing workflow itself creates the drift that the patrol then flags.

### 2d. Duplicates / empty bodies / decisions not executed
str-qwua7.62 ("Tracker content sweep", decision recorded 2026-09-06, "All
proposed defaults approved") is purely mechanical and nothing in it has been done:
- str-hrg2 ≡ str-0wxw (title similarity 0.65, same day 2026-06-16) — both still open.
- str-1fik (P1 feature, **0-byte body**) and str-wfd2 (same work) — both open.
- str-2zsy (0-byte body), str-cl53 (51 bytes) — still open, not deferred.
- str-j49xg (P1 epic): CORRECTION (second pass) — it does have 3 children
  (str-1qd5i, str-26fhi closed; str-3eki5 open); the .62 decision to rewrite
  its body is still unexecuted.
- str-u394l.4 still P2 (decision: promote to P1).
Of the 13 maintainer decisions filed on 09-06 (qwua7.20.3, .52–.62), only
.56 has been executed.

### 2e. Priority inflation / inversions
48 open P1s (26% of open). Inversions: str-35vtk.10 (P1) blocked by
str-35vtk.29 (P2) and str-35vtk.31 (P2). Label mix of open issues:
`governance` 72, `quality-gates` 67 vs `cli` 21, `core` 15 — the backlog is
dominated by process work.

### 2f. Identity: all tracker and git activity attributed to a test fixture (NEW, P1)
- All 119 issues created since 2026-09-05: `created_by: Test`, `owner: test@example.com`.
- `git log --since=2026-09-05 main --format='%an <%ae>'` → 101 `Test <test@example.com>`,
  1 `Test User <test@example.com>`; **zero** real-author commits. Since
  2026-06-23: 467 "Test User" + 115 "Test" commits on main, pushed to
  `github.com/shatterproof-ai/shatter`.
- Source: the primary repo's own config, `/home/ketan/project/shatter/.git/config`:
  ```
  [user]
      name = Test
      email = test@example.com
  ```
  (`git config --show-origin user.email` → `file:.git/config`).
- Those exact identities are what test fixtures write:
  `scripts/test_target_dir_report_json.sh:27-28` and
  `scripts/test_cleanup_merged_remote_branches.sh:47-48,136-137` (`Test`),
  `shatter-cli/tests/implicit_init_gitignore_test.rs:51-52`,
  `scripts/test_walkthrough_examples_checkout.py:57-63` (`Test User`).
  str-jttrf/str-y0rcz document that these scripts ran under a leaked
  `GIT_DIR` from git hooks and wrote to the real repo (stray `init` commit,
  a `str-abc1.2` branch). Scratch repro: with `GIT_DIR=<real>/.git`,
  `git config user.name Test` run from another cwd writes the real repo's
  config (confirmed). The leak was fixed (scripts now `source
  scripts/git-sandbox-test-lib.sh`; `scm.rs:599-605` scrubs env) but the
  damage to `.git/config` was never reverted, so every commit and bd write
  since has been attributed to the fixture identity.
- str-qwua7.51 ("Configure bd identity in the SessionStart hook") diagnoses
  this as "agent sessions never configure a bd identity"; the actual root
  cause is the polluted repo-local `[user]` section. (Inference, high
  confidence: identity strings match fixture code exactly; I cannot prove
  which run wrote them.)

### 2g. A closure verified against a corrupted HEAD (NEW)
str-qwua7.14's close reason: "Not reproducible against current main
(e50fc399)". `e50fc399` is **not on main**: `git show e50fc399` → author
`Test`, message `init`, 2026-09-07, parent 84941b37, sweeping 20+ files
(`.claude/agents/*` deletions, `Cross.toml`, `README` added, scm.rs…) — the
exact "stray init commit" signature str-y0rcz describes. It survives only as
`recovery/shatter-main-20260912-11_4ty6m` (plus `recovery/shatter-index-…`,
2 commits not on main). So the "not reproducible" evidence was gathered on a
fixture-corrupted HEAD. I re-verified on real HEAD: `explore
01_arithmetic.rs:classify_number` → `100 iters, 4 paths, 3/3 branches` — the
deserialization failure is indeed gone, so the closure's outcome stands, but
the verification was invalid and nothing checks for it.
Side observation: the same run reports **54% (7/13 lines)** for a function
with every path covered, because the Rust frontend never sets
`instrumentable_line_count` (`shatter-rust/src/protocol.rs:814,1233,1268,1303`
all `None`; documented in `protocol/parity-matrix.yaml:854-871`). Go got this
fix in str-szcn3 (P0); there is no open issue for Rust.

---

## 3. Regression table

### 3a. The 13 closed children of str-qwua7

| Issue | Verdict | Evidence |
|---|---|---|
| .4 rapid planner dedupe | PRESENT (purge partial) | commits a098525d, 9dc2726e on main; `shatter-go/planner/testdata/rapid/` absent; `shatter-go/.gitignore:3` ignores only `planner/testdata/rapid/**/*.fail` — a March failfile is still tracked at `shatter-go/instrument/testdata/rapid/TestPropertyExecTimeoutAlwaysPositive/…-20260306134644-2197485.fail` (f0a58576). Close reason says the older duplicate branches were "deleted as superseded", but `origin/str-qwua7.4-testplan-http-body-fix` still exists (§6.2) |
| .7 registry validator | PRESENT (residual noise) | extractors read generated enums / `handler.rs` (`validate-protocol-registry.py:42,545,598`); run prints permanent warning `'get_invocation_plan' in registry but not found in shatter-rust (may be unimplemented)` although parity-matrix.yaml:175-181 marks it optional/Go-only |
| .8 QUICKSTART default-deny | PRESENT | QUICKSTART.md:79-89 (`--allow-host-writes`), SPEC.md:586, 610-612, changelog :1182, "Last updated: 2026-09-09" |
| .9 docs-smoke hardening | PRESENT | docs-smoke.yaml:32-38 real `explore`; docs-smoke.py:549,592,622 struct validators |
| .9.2 SPEC 5.5 snapshot | PRESENT | SPEC.md:868-869 `"version": 1`, `created_at` |
| .14 Rust walkthrough 0% | CLOSED-WITHOUT-FIX (outcome OK) | verified against stray `e50fc399`; symptom absent at HEAD (4 paths, 3/3 branches) |
| .15 hide exec-only flags | PARTIAL | `spec-diff --help` 48 lines, 0 hits for `allow-host-writes|--timeout-explore|--time-limit`; but `shatter help spec-diff` is 78 lines and still lists `--allow-host-writes` (:50) and `--set` (:55); `help doctor` also leaks. Implementation is a raw-argv intercept (`main.rs:70 maybe_print_non_executing_help`), so the clap `help <cmd>` path was missed |
| .16 bd/dolt patrol check | PRESENT | `tracker-server` PASS; bd responsive |
| .17 release 13 stale claims | PRESENT, recurred | 13 handled; 2 new stale claims now (§2a) |
| .27 delete .claude/agents | PRESENT | `.claude/agents` absent |
| .32 errcheck Unmarshal | PARTIAL | no `Unmarshal` in `shatter-go/.golangci.yml`; regression test `handler_test.go:831`; but acceptance "`task go:lint` passes" is false — `golangci-lint run ./...` exits 1 with 10 issues (second pass, §6.3) |
| .41 delete shatter-vs etc. | PRESENT | dirs absent; release aarch64 red before and after |
| .56 lifecycle exports | PRESENT (scan+run) | `discovery.rs:594-624`; used at `scan.rs:1960`, `run.rs:520`; dry-run gap is child .56.1 (orphan) |

Also open children that are **done** (should close): .1, .12, .18 (mostly), .19 (§2b).
Open children confirmed **still true** at HEAD: .5 (`orchestrator.rs` has 0 hits
for `capture_side_effects`; explorer.rs:169,1573,2784 has it), .11 (see below),
.13 (the 600-char rust-frontend hint printed twice: `STATUS
skipped_by_unavailable_frontend … hint=…` then `Error: … shatter-rust frontend
not found: <same hint>`), .39 (implicit init prints `Created …` lines to
**stdout** and `(detected language: unknown)` for a `.rs` / `.ts` target),
.2 (no `check-fresh` in Taskfile.yml), .51 (misdiagnosed, §2f), .62 (§2d).

.11 evidence: `shatter explore a.ts:f --allow-host-writes --spec-json > out.json`
→ exit 0, `out.json` begins `# Shatter Explore\n\n## \`f\` …` then the JSON;
`json.load` → `JSONDecodeError: Expecting value: line 1 column 1`.

### 3b. Sample of other recently closed P0/P1/bug issues (since 2026-08-15)

| Issue | Pri | Verdict | Evidence |
|---|---|---|---|
| str-szcn3 Go instrumentable_line_count | P0 | PRESENT | `shatter-go/protocol/handler.go:714` assigns it |
| str-ajjri expired parity divergence | P1 | PRESENT | no `rust-protocol-enum-vocabulary-narrower` in parity-matrix.yaml / PARITY.md; patrol parity-expiry PASS |
| str-y0rcz git test env leak | P1 | PRESENT (MOVED to str-noghq) | `scm.rs:599-605 scrub_repo_env` |
| str-jttrf GIT_DIR leak in hooks | P1 | PRESENT, damage not repaired | scripts source `git-sandbox-test-lib.sh`; `.git/config [user]` still fixture identity (§2f) |
| str-nfg4y spec-diff unmatched inputs | P1 | PRESENT | `spec_diff.rs:31-63,150 canonical_examples_comparable` |
| str-aj0k dead loop_convergence_window | P2 | PRESENT | 0 hits in shatter-core/src, shatter-cli/src |
| str-o09e deterministic embedded builds | P1 | PRESENT | `shatter-cli/build.rs:96-107,166` per-file rerun-if-changed |
| str-wyde8 stale doc comment | P3 | PRESENT | `report.rs:722-725` includes `interrupted` |
| str-k7cla CLI temp git helper env | P1 | PRESENT | `init.rs:362-371` removes 6 GIT_* vars |
| str-yyl9a core git callers env | P2 | PRESENT | core_sample.rs:394, test_runner.rs:634, test_prioritization.rs:203 via `scm::git_command` |
| str-gnagk analysis cache identity | P2 | PRESENT | commits 3dfcca56, f02cc0a1, 067be286 |
| str-ahqdb test/test-quick build race | P2 | PRESENT | Taskfile.yml:91, 99 `deps: [frontends-built…]` |
| str-zn94g passthrough mock counter | P2 | PRESENT | `instrument/executor.go:192` skips passthrough |
| str-qpttz MockOverride errors | P3 | PRESENT (sibling left) | `auto_mock.rs:324` manual Deserialize; `config.rs:119-121 CustomOpaqueType` still `#[serde(untagged)]` (same error-quality class) |
| str-heegk mock-name collision | P3 | PRESENT | 5dad4d72 / merge 73f06701 |
| str-jjt2h budget-exhausted rows | P2 | PRESENT | string only kept as legacy (`report.rs:138`) and asserted absent (`:3894`) |
| str-leozr scan --scope | P2 | PRESENT | `scan --help` lists `--scope` |
| str-0m0vn --seed seeds exploration | P2 | PRESENT | `scan --help` text |
| str-35vtk.21 CI runs full landing gate | P1 | PRESENT | CI green on every main push 09-21..09-22 |
| str-u394l.1 scheduled drift patrol | P1 | **CLOSED-BUT-BROKEN** | §1b |
| str-rmcrl / str-na9db / str-0z1im / str-6vl7p / str-duens | P3 | PRESENT (landing SHAs in close reasons, ancestors of main) | |
| str-qwua7.14 | P1 | CLOSED-WITHOUT-FIX (verified on corrupted HEAD) | §2g |
| str-6y1u4 Go planner orchestrator | P1 | CLOSED-WITHOUT-FIX (env: stale untracked launchers) | acceptable, reasoned |
| str-mpgg1 | P1 | LANDED-NOT-CLOSED | §2a |

No REGRESSED item found among the sampled closures. Close-reason hygiene: 17
of 49 closures since 09-04 have an empty or literal "Closed" reason (e.g.
str-qwua7.8, .9, .15, .56, str-ajjri); the evidence usually lives in bd
NOTES instead (str-qwua7.15 notes carry full gate evidence), so this is
cosmetic but makes the `close_reason` field useless for scripted audit.

---

## 4. Why str-qwua7 stalls

- **Throughput:** 13/63 children closed in 17 days; all 13 were S-sized or
  mechanical. None of the P1 structural items (.5, .6.x, .2, .3, .10, .11,
  .13) has a comment, branch, or commit. `bd` comment_count = 0 on all 50
  open children; 34 still carry their 2026-09-05 filing timestamp.
- **Competing intake:** 119 new issues created since 09-05 (62 of them the
  audit itself); a new P2 epic str-hjrnp (Jev frontier-ranking benchmark) was
  created and closed on 09-21 while P1 audit bugs sat ready. The 7 P1 bugs
  filed 2026-09-13 (str-t854z, str-gr41w, str-wlban, str-0znjr, str-hds8u,
  str-dl2pj, str-2zgjq) sort above every qwua7 item in `bd ready`.
- **Premise rot pollutes the queue:** .1, .12, .18, .19 are done-but-open,
  and .12 was filed from stale-binary evidence. An agent that picks them up
  first burns a session discovering that.
- **Tracker-only tasks have no workflow:** .62 (dedupe/fill bodies) and the
  follow-up of .17 need no branch, so `bento:launch-work`/`land-work` don't
  fit, and nothing in AGENTS.md or the swarm skill schedules tracker-only work.
- **Flat, mixed epic:** 62 children spanning product bugs, refactors, docs,
  agent-system and tracker chores, with no waves or `bd swarm` ordering and
  two nested epics (.6, .20, .21). Nothing expresses "fix the product P1s
  first".
- **Decisions recorded, not executed:** 12 of 13 maintainer decisions
  (2026-09-06) remain open with no owner.

---

## 5. Additional L5 observations made while verifying (new, outside the audit list)

1. **TS ternary branches are analyzed but never instrumented.** `export
   function h(x: number) { return x > 1 ? 1 : 0; }` → random: `100 iters, 1
   paths, 0/1 branches` "Class 1 — returns 0"; `--concolic` (fresh dir):
   `21 iters, 1 paths, 0/1 branches`. The markdown still says **100% coverage
   (1/1 lines)**. `shatter-ts/src/analyzer.ts:1384-1398` emits a `ternary`
   branch; `shatter-ts/src/instrumentor.ts` has no ConditionalExpression
   branch probe (only :2260 builds a ternary for mock wrapping). The `if`
   form of the same function finds both paths. No open issue mentions
   ternary/conditional expression.
2. **Explore markdown path count contradicts the batch line.** Fresh dir,
   `explore b.ts:g` (if/return): `[batch 1/1] g: 100 iters, 2 paths, 1/1
   branches` but the report says `**0 path(s)** · **100%** coverage` and
   `Summary: 0 path(s)`; `--spec` for the same run shows "Behavioral
   classes: 2". The concolic run of the ternary prints "1 path(s)", so the
   random-explorer path count is the odd one out.
3. **Switching explorer mode silently reuses the previous mode's result.**
   In the same dir, a random `explore a.ts:f` followed by `explore a.ts:f
   --concolic` prints `[info] Resumed 1 function(s) from prior explore
   artifacts` and then labels the output `Explorer: concolic (Z3-backed)` —
   concolic never ran.

---

## 6. Second pass (2026-09-22 ~13:00, same HEAD 16794cef)

### 6.1 Drift patrol re-run: conformance now FAIL, and it is a harness flake amplifier (NEW)

`python3 scripts/drift-patrol.py` → exit 1, "2 failed, 2 pending, 0 skipped, 4 passed"
(frontends are now built in the worktree, so `protocol-conformance` ran instead of SKIP):

```
FAIL: go / execute_outcome_shape_go -- no response (timeout or crash)
FAIL: go / adapter_http_nethttp_go_analyze -- no response (timeout or crash)
FAIL: go / analyze_runtime_value_go -- no response (timeout or crash)
FAIL: go / planner_runtime_value_go -- no response (timeout or crash)
FAIL: go / shutdown -- id: expected 99, got 20
FAIL: go / shutdown -- status: expected 'shutdown_ack', got 'error'
```
Re-runs minutes later: `conformance_harness.py -f go` → "All checks passed" (2.3 s);
full harness → "42 checks, All checks passed". `uptime` load average at the
time: 66 / 103 / 116 (many audit reviewers building).

Root cause of the cascade: `protocol/conformance/conformance_harness.py:88-107`
`send()` returns `None` on a 30 s `select` timeout (`COMMAND_TIMEOUT_S = 30`,
`:31`) but keeps using the same frontend process (`:610-615` just `continue`s).
The late response to case N is then read as the reply to case N+1 — hence
`shutdown` receiving id 20 instead of 99. One slow response under load turns
into 5-7 failures with misleading messages. The harness neither matches
response `id` to request before accepting, nor restarts the frontend after a
timeout.

### 6.2 Landing leaves remote/worktree debris that the rules say must be removed (NEW)

- `git branch -r` → 67 remote branches; `git branch -r --merged origin/main` →
  39 (incl. HEAD/main) fully merged but never deleted (e.g.
  `origin/str-hjrnp.1..4-*` landed 09-21/22, `origin/str-2tyfk-lint-errcheck`,
  `origin/str-szcn3-*`). AGENTS.md:125 and :270 make
  `scripts/cleanup-merged-remote-branches.sh` "mandatory, not an optional
  periodic chore" at every landing.
- Four unmerged remote branches `origin/str-qwua7.{4-testplan-http-body-fix,
  7-protocol-registry-validate,16-restore-bd-dolt,17-stale-claims-cleanup}`
  are 102 commits ahead of main and all contain the stray fixture commit
  `e50fc399 "init"` (author Test, deletes `.claude/agents/*`, adds `README`,
  411-line `.beads/issues.jsonl` rewrite). Their work was ported onto clean
  bases before landing, but the contaminated originals remain published and
  a future "land the parked branch" sweep (like str-qwua7.18) could merge them.
  str-qwua7.4's close reason claims the older duplicate branches were
  "both deleted as superseded" — `git ls-remote origin 'refs/heads/str-qwua7*'`
  still lists `str-qwua7.4-testplan-http-body-fix` (8b498e21).
- `git worktree list` shows `/tmp/land-work-preview-a5l9ycto (detached HEAD)`
  — the exact debris str-qwua7.19 asked to prune has recurred.

### 6.3 `bd sync` no longer exists; the committed tracker snapshot is frozen (NEW, P1)

- `bd version` → `1.1.0`; `bd sync --help` → `Error: unknown command "sync" for "bd"`.
- AGENTS.md mandates it 9 times (`:125` landing step 5, `:293`, `:340`,
  `:347-369` "Beads Sync Cadence"), and `.claude/skills/audit/SKILL.md:385`
  delegates beads persistence to it.
- Effect: `git log -1 -- .beads/issues.jsonl` → `134dd616 2026-09-07`. The live
  DB (`bd export`) has 1774 issues; the committed file has 1733 — **41 issues
  missing** (earliest created 2026-09-08, including the seven P1 bugs filed
  09-13: str-0znjr, str-2zgjq, …) and **19 status mismatches**.
- Consumers of the stale file: CI drift-patrol `tracker-hygiene` (documented
  to use the committed export where bd is absent — docs/DRIFT-PATROL.md) and
  `scripts/cleanup-merged-remote-branches.sh` in-progress protection
  (AGENTS.md:274-276). str-ly5bz (P3, open) already worries about the JSONL
  being "not refreshed while in_progress"; the reality is it is not refreshed
  at all.
- str-qwua7.51's evidence cites bd v0.63.3 flags; the toolchain moved to 1.1.0
  underneath the open audit items without anyone re-validating AGENTS.md's
  bd quick reference.

### 6.4 Go lint is red on main and no gate runs it (NEW; str-2tyfk closed with residuals)

`cd shatter-go && golangci-lint run ./...` → exit 1, 10 issues:
```
protocol/handler.go:1325:32: nilness: tautological condition: nil == nil (govet)
instrument/property_test.go:544:18: SA5011: possible nil pointer dereference (staticcheck)
protocol/analyzer.go:592:6: func analyzeFunc is unused (unused)
protocol/analyzer.go:799:6: func extractParams is unused
protocol/analyzer.go:1518:6: func mapTypeInfo is unused
protocol/analyzer.go:1534:6: func structTypeInfo is unused
protocol/handler.go:1964:19: func (*Handler).lookupAnalyzedByTargetID is unused
protocol/prepared_launcher.go:474:6 / :506:6: toWrapperConstructors(/Params) unused
```
str-2tyfk ("task lint fails on main…") listed exactly the handler.go:1325
tautology and the unused helpers in its body, was closed on a "targeted check
… 0 errcheck findings in the 4 changed files", empty close_reason. str-qwua7.32
acceptance "`task go:lint` passes" is likewise unmet. Why it lingers:
`shatter-go/Taskfile.yml:61-65` makes `lint` conditional
(`preconditions: command -v golangci-lint … (optional)`), and no root
Taskfile stage or CI workflow invokes `go:lint` (root Taskfile only wires
`go:vet`, lines 186, 554).

### 6.5 Generated Go mock code ignores Unmarshal errors and embeds JSON in a raw string (NEW, P3, medium confidence)

`shatter-go/instrument/executor.go:226-231` emits
``json.Unmarshal([]byte(`%s`), &vals)`` with `retValsJSON` from
`json.Marshal(mock.ReturnValues)`, and `:299` emits
`json.Unmarshal(retvals[idx], &retVal)` — both unchecked in the generated
harness (the spirit of str-qwua7.32, invisible to golangci because it is a
string template). `json.Marshal` does not escape a backtick, so a mock
return value containing `` ` `` would terminate the raw string literal and
produce uncompilable harness code. Not reproduced by execution in this pass.

### 6.6 Tracker numbers (second pass)
`bd stats`: 1774 total, 178 open, 2 in_progress, 23 blocked, 1589 closed,
155 ready. Open by priority: P1 48, P2 111, P3 23, P4 3; 37 open issues are
older than 90 days, median open age 20 d. Priority inversions: only
str-35vtk.10 (P1) ← str-35vtk.29/.31 (P2). Open epics: str-qwua7 62 children /
50 open; str-35vtk 36 / 18; str-u394l 4 / 2; str-mhinv 3 / 2.
