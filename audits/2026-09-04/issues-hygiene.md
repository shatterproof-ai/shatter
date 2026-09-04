# Shatter issue-tracker hygiene audit — 2026-09-03

Scope: read-only. Repo `/home/ketan/project/shatter`, tracker = Beads (`bd`, data in `.beads/`).

## 0. Data-source caveat (P1 — blocks the whole tracker workflow)

`bd` was **unusable for the entire audit**. Every invocation (`bd list --all --json`, `bd blocked`, `bd stats`, 4 attempts, 60–120 s timeouts) failed with:

```
Error: failed to open database: Dolt server unreachable at 127.0.0.1:0 and auto-start failed:
server started (PID …) but not accepting connections on port NNNNN: timeout after 10s
```

`.beads/dolt-server.log` tail: `database "dolt" is locked by another dolt process`.

Root cause (inspected, not fixed): an orphaned `/usr/local/bin/dolt sql-server -H 127.0.0.1 -P 43105` (PID 28938, uptime ≈49 h) holds the exclusive DB lock, but `.beads/dolt-server.port` / `.beads/dolt-server.pid` do **not exist**, so `bd` cannot discover it and tries to auto-start a second server that dies on the lock. Fix: `kill 28938` (or `bd dolt stop`) then let `bd` auto-start, or `bd dolt start`. Never hammered; moved on.

**All tracker numbers below come from the committed export `.beads/issues.jsonl`** (1653 rows, last committed 2026-08-30 18:57 -0500, commit `c90f709f`) — the same fallback drift-patrol uses. Two known lags of that export vs. the live DB:

- `.beads/last-touched` = `str-mpgg1`, which is **not in the JSONL** (the branch is already merged to local `main`). The export is at least one landing behind.
- `.beads/export-state.json` is dated 2026-07-05 / 1524 issues, while the JSONL has 1653 rows — export bookkeeping is itself stale.
- ~90 % of open/in_progress issues have `updated_at = 2026-07-06` (a mass-touch, probably a migration or bulk `bd sync`). "Untouched 58 d" therefore *understates* real staleness for those issues; many were last meaningfully touched in June.
- The JSONL export carries no dependency edges (only `dependency_count`/`dependent_count`), so `bd blocked` is approximated from counts + body text.

---

## Part A — Drift patrol

Command: `python3 scripts/drift-patrol.py` (run 2026-09-03 17:14 UTC). **Exit status: 1.**
Summary line: `2 failed, 2 pending, 0 skipped, 3 passed.`

| Check | Status |
|---|---|
| `protocol-registry` | PASS |
| `protocol-codegen` | PASS |
| `protocol-conformance` | PASS |
| `parity-expiry` | **FAIL** |
| `cli-surface-drift` | PENDING (str-wurp, open) |
| `docs-stories` | PENDING (str-u394l.3, open) |
| `tracker-hygiene` | **FAIL** — 13 stale in_progress, 2 orphaned children |

### FAIL: `parity-expiry` (verbatim)

```
$ /usr/bin/python3 scripts/validate-parity.py --warn-as-error-within-days 14
Loading parity matrix: /home/ketan/project/shatter/protocol/parity-matrix.yaml
Matrix: 10 commands, 32 complex_types, 10 allowed_divergences
Loading registry capabilities: /home/ketan/project/shatter/protocol/registry.yaml
Detecting capabilities: go
Detecting capabilities: rust
Detecting capabilities: typescript
Validating...
ERRORS (1 failure(s)):
  FAIL: allowed_divergences[rust-protocol-enum-vocabulary-narrower]: resolved on 2026-08-12 — grace period expires in 8 day(s); remove this entry from parity-matrix.yaml and protocol/PARITY.md now, before the hard deadline blocks an unrelated parity-touching branch (escalated by --warn-as-error-within-days=14)
Parity check FAILED.
```
Responsible issue: `str-5dx0` (itself one of the stale in_progress issues below). Remediation per report: remove the `rust-protocol-enum-vocabulary-narrower` entry from `protocol/parity-matrix.yaml` and `protocol/PARITY.md` before the hard deadline (~2026-09-11) turns `task parity` red on an unrelated branch.

### FAIL: `tracker-hygiene` (verbatim)

```
source: .beads/issues.jsonl (committed export)
-- in_progress with no update for >14d (13) --
str-wnso (59d) E2E rust-fe cache target drift
str-uk8y (59d) Single config-resolution entrypoint for per-target hints + policy
str-o09e (59d) Deterministic + detectable embedded-frontend builds (cargo not recompi
str-jyxr (59d) Rust input prefetch timeout
str-j5zp (59d) Add CI test/lint gate; fix dead pre-push hook script references
str-ck05 (59d) SPEC.md overhaul: command coverage, flag tables, removed features, sta
str-bphx (59d) Pickpackit seed diversity
str-aj0k (59d) loop_convergence_window is a dead config knob; LoopCoverageTracker is
str-5ijo (59d) Commit a Bash permission allowlist to .claude/settings.json (swarm wor
str-5dx0 (59d) Give parity divergence expiry warnings an audience (scheduled check be
str-4yc9w (59d) Param-decode failures misreported as completed: launcher deser errors
str-mhinv.1 (57d) Receiver field plan wire type
str-sff87 (55d) Project health audit 2026-07-10
-- open/in_progress children under a closed parent (2) --
str-hy9b.J3 [open] — parent str-hy9b (epic) is closed: Kapow re-run validation
str-hy9b.1 [open] — parent str-hy9b (epic) is closed: Go frontend redesign doc
```
Responsible issue: `str-u394l.1`. Remediation: finish or `bd update <id> --status open` each stale issue; reopen parent / re-parent / close each orphan.

### PENDING: `cli-surface-drift` (verbatim)

```
A mechanical CLI-surface drift gate (clap inventory vs SPEC.md §2 and gauntlet coverage) is not implemented yet.
This patrol slot activates when str-wurp lands; it is reported
on every run so the gap stays visible instead of being forgotten.
```
Responsible: `str-wurp` (open, P2, created 82 d ago, untouched 58 d).

### PENDING: `docs-stories` (verbatim)

```
A stories coverage gate (docs/stories does not exist yet) is not implemented yet.
This patrol slot activates when str-u394l.3 lands; it is reported
on every run so the gap stays visible instead of being forgotten.
```
Responsible: `str-u394l.3` (open, P2).

---

## Part B — Tracker health

Totals (JSONL): 1653 issues — closed 1538, open 96, in_progress 14, deferred 5.
Types: feature 546, task 535, bug 487, epic 60, event 9, chore 8, molecule 6, decision 2.

### B1. in_progress (14) — 13 stale [P1]

| id | P | last update | started | assignee | title |
|---|---|---|---|---|---|
| str-4yc9w | P1 | 58 d | 60 d | Test User | Param-decode failures misreported as completed |
| str-o09e | P1 | 58 d | 69 d | Test User | Deterministic + detectable embedded-frontend builds |
| str-ck05 | P1 | 58 d | 60 d | Test User | SPEC.md overhaul |
| str-j5zp | P1 | 58 d | 69 d | Test User | Add CI test/lint gate; fix dead pre-push hook refs |
| str-jyxr | P1 | 58 d | 84 d | Ketan | Rust input prefetch timeout |
| str-bphx | P1 | 58 d | 93 d | Ketan | Pickpackit seed diversity |
| str-mhinv.1 | P1 | 56 d | 56 d | Test User | Receiver field plan wire type |
| str-5dx0 | P2 | 58 d | 60 d | Test User | Parity expiry warnings audience (owns the failing patrol check) |
| str-5ijo | P2 | 58 d | 69 d | Test User | Bash permission allowlist in .claude/settings.json |
| str-aj0k | P2 | 58 d | 69 d | Test User | loop_convergence_window dead knob |
| str-wnso | P2 | 58 d | 69 d | **none** | E2E rust-fe cache target drift |
| str-sff87 | P2 | 54 d | 54 d | Test User | Project health audit 2026-07-10 |
| str-uk8y | P3 | 58 d | 69 d | **none** | Single config-resolution entrypoint |
| str-duens | P3 | 7 d | 7 d | Test User | input_gen.rs proptest verifier (worktree branch **already merged**, see B1b) |

Observations:
- 13/14 claims are dead: 54–93 days old, no notes. Two (`str-wnso`, `str-uk8y`) are in_progress with **no assignee** — a claim without an owner.
- Assignee `"Test User"` on 10 of them: the `bd` identity in agent sessions is not configured (`bd config user.name` or env), so claims can't be attributed to a session/agent. [P2]
- `str-sff87` "Project health audit 2026-07-10" is a report issue left in_progress 54 d — audits should be closed on delivery.
- Six of these are P1 and were started 60–93 d ago; whatever branch work existed is presumably lost or unmerged — see recommendation R2.

### B1b. Linked worktrees (8) [P1]

`main` (local `84941b37`) is **3 commits ahead of `origin/main`** (`c90f709f`) — the `str-mpgg1` merge (2026-09-02) has not been pushed. Per AGENTS.md "work is complete when changes are on main *and pushed*".

| worktree branch | bd issue (JSONL) | last commit | merged to main? | pushed? | notes |
|---|---|---|---|---|---|
| str-0z1im-negzero-roundtrip | str-0z1im open P3 bug | 2026-08-31 | NO (1 ahead, 10 behind) | branch pushed | ready to land; issue never claimed |
| str-6vl7p-redundant-canonicalize | str-6vl7p open P3 task | 2026-08-31 | NO (1 ahead, 10 behind) | branch pushed | ready to land; never claimed |
| str-8q1b4-resume-report-parity | str-8q1b4 open **P1** bug | 2026-08-31 | NO (4 ahead, 3 behind) | branch pushed | P1 fix sitting unmerged 3 d; never claimed |
| str-duens-tuple-proptest | str-duens **in_progress** P3 | 2026-08-26 (merge commit) | **YES** — 0 ahead, 115 behind | n/a | worktree is fully merged; 2 dirty files; issue still in_progress → **should be closed + worktree removed** |
| str-mpgg1-revert-hook-edits | **not in JSONL** | 2026-09-02 | YES (local main only) | main NOT pushed | landed but never pushed; export lags |
| str-na9db-dedupe-arity-constants | str-na9db open P3 task | 2026-08-30 | NO (1 ahead, 10 behind) | **branch NOT pushed** | unpushed local-only work — at risk |
| str-rmcrl-clippy-const-assert | str-rmcrl open P3 bug | 2026-08-31 | NO (1 ahead, 3 behind) | branch pushed | clippy -D warnings on main is red until this lands |
| str-vr7vq-shatter-init-gitignore | str-vr7vq open P3 chore | 2026-08-31 | NO (3 ahead, 10 behind) | branch pushed | ready to land |

None are stale by commit date (all ≤ 8 d), but **6 finished branches are parked unmerged** with their issues still `open` (never claimed), so the tracker shows no work in flight and `bd ready` would hand the same issues to another agent. `git cherry` confirms 0 patch-equivalent commits on main for any of the six — nothing was landed via a different route.

### B2. Blocked (approximation — `bd blocked` unavailable)

Open issues with `dependency_count > 0` (21): `str-35vtk.9/.10/.12/.13/.14/.15/.16/.19/.22–.27/.29/.31`, `str-mhinv.2/.3`, `str-3lsgf`, `str-81xiw.3/.4`. `str-35vtk.10` (WS-H closeout) has 13 dependencies.
Body-text "blocked by" references in `str-35vtk.*` use **plan-local placeholders** ("blocked by issue3", "issues8,13,14", "issue11") instead of `str-` ids — a fresh agent cannot resolve them without the original plan document. [P2]

### B3. Open (96): counts, overlaps, oldest

- Priority: P1 23 · P2 54 · P3 19 (no P0, no P4 open).
- Type: task 51 · bug 22 · feature 14 · epic 5 · chore 4.
- Assignee: 94 unassigned; 2 have an assignee but status `open` (`str-poylc` → Ketan, `str-gjsb2` → Test User) — half-claimed.
- Top labels: build 11, go 10, governance 7, git 6, concurrency 6, rust 6.
- Created vs closed per month: Jul 67/50, Aug 90/88 — roughly balanced; net open is not growing.

**Duplicates [P2]:**
- `str-0wxw` ≡ `str-hrg2` — identical bug ("axum handler generators re-seed FK chain per iteration"), same day (2026-06-16), near-identical body. Close one as duplicate. (23 closed issues already carry reason "Duplicate created by racing audit filing" — the racing-audit filing pattern is a known source.)
- Overlap: `str-wfd2` (P2 "resolve/synthesize structs/enums defined outside the analyzed file") vs `str-1fik` (P1 "cross-file/cross-crate struct synthesis") — same feature at two priorities; `str-1fik` has an **empty body**.
- Overlap: `str-35vtk.22` "Gate receipt schema and writer" vs `.23` "validator" — legitimately split, not dup.

**Oldest open:**
- `str-cl53` P2 (178 d, created 2026-03-08): "Test impact analysis" — body is one line pointing at `docs/plans/str-zwgc-test-impact-analysis.md`. Either schedule or defer.
- `str-hy9b.1` / `str-hy9b.J3` (138 d/137 d) — orphans under closed epic (see B6).
- Then a cluster of 84-d-old refactor tasks (`str-26ky` god-file split, `str-9ee5`, `str-inct`, `str-rf2v`) and 82-d governance tasks (`str-wurp`, `str-5b9f`, `str-2fjn`).

**High-priority open untouched > 30 d [P2]:** `str-1fik` (P1, empty body), `str-la75`, `str-j49xg` (P1 epic), `str-u394l` (P1 epic), `str-mhinv.2`, `str-mhinv.3`. P1 that nobody touches for two months is mis-prioritized.

### B4. Stats (`bd stats` unavailable; computed)

- Closed 1538 — by priority P0 6 · P1 606 · P2 697 · P3 195 · P4 34; by type feature 526 · task 477 · bug 460 · epic 55.
- 40 % of all closed issues are P1 and 23 open of 96 are P1 → P1 is the de-facto default, not "urgent". [P3]
- Close reasons: "Closed" 670, blank 90, "all steps complete" 29, "Duplicate created by racing audit filing" 23, "Completed under str-hy9b expedition" 22. ~50 % of closes carry no informative reason.
- Deferred (5): `str-rlbq` P1 telemetry epic, `str-1hlk.16`, `str-rlxf`, `str-w0d.3`, `str-34ib`.

### B5. Priority inversions [P2]

- `str-u394l` (P1 epic "Drift enforcement") has only P2 children (`.3`, `.4`) open → epic priority is not reflected in its remaining work; either the epic is P2 or the children are P1.
- `str-1fik` P1 vs `str-wfd2` P2 — same work, inconsistent priority (see dups).
- `str-8q1b4` P1 bug has a finished fix branch unmerged for 3 d while P3 branches from the same day are in the same state — landing order is not following priority.
- `str-rmcrl` is P3 but describes `cargo clippy -D warnings` **failing on main** — a red-main lint should be P1/P2.
- No child-more-urgent-than-parent cases found.

### B6. Epics

| epic | status | children | verdict |
|---|---|---|---|
| str-35vtk Build/test efficiency overhaul | open P1 | 20 open / 16 closed | healthy, active (upd 7 d) |
| str-mhinv Typed receiver construction plans | open P1 | 2 open, 1 in_progress (stale 56 d) | stalled |
| str-j49xg Adapter-owned executions empty coverage | open P1 | **0 dotted children** | body says "Child issues (filed alongside)" — they were never filed, or filed without the `.N` convention (untraceable). [P2] |
| str-u394l Drift enforcement | open P1 | 2 open / 2 closed | ok; both open children are patrol PENDING slots |
| str-81xiw Diff-scoped exploration | open P2 | 3 open / 1 closed | idle 58 d |
| str-hy9b Go frontend redesign | **closed** | 46 closed / **2 open** | orphaned children `.1`, `.J3` (drift-patrol FAIL) |
| str-1hlk Protocol contract source | closed | 21 closed / 1 deferred (`.16`) | acceptable if deferral is deliberate |
| str-w0d Concolic execution for TS | closed | 2 closed / 1 deferred (`.3`) | same |

No "all children closed but epic still open" case exists. `str-5b9f` ("Epic-close lifecycle rule: re-status open children when closing an epic", open 82 d) is exactly the missing rule that produced the `str-hy9b` orphans.

### B7. Issue quality sample (10 random open issues)

Criterion: could a fresh agent start from the body alone (goal, acceptance criteria, code pointers)?

| id | P/type | body | verdict |
|---|---|---|---|
| str-hrg2 | P2 bug | 735 ch; symptom + numbers, pointers to generators; no acceptance | **partial** — and a duplicate of str-0wxw |
| str-poylc | P2 task | 1.5 k; exact file:line, pattern to mirror, scope note | **yes** |
| str-j49xg | P1 epic | 4.2 k; symptom, code facts, per-frontend change list, acceptance, out-of-scope | **yes** (but its promised children don't exist) |
| str-35vtk.33 | P3 task | 823; exact script, exact functions, expected behaviour | **yes** |
| str-35vtk.29 | P2 task | 797; "blocked by issues17,18", "issue3 lease", "issue17 candidates" | **no** — depends on unresolvable plan-local ids |
| str-uz5m | P2 bug | 1.4 k; file:line range, mechanism, expected behaviour | **yes** |
| str-yd31w | P2 task | 1.2 k; directions (a)(b)(c) + acceptance for slice (b) | **yes** |
| str-a1ix6 | P2 task | 2.4 k; problem, what was ruled out, leads, acceptance | **yes** (model issue) |
| str-6trg | P2 task | 2.6 k; summary, code facts, acceptance checks, in/out scope | **yes** |
| str-bh9wu | P2 bug | 2.1 k; three harness modes, repro, fix direction, evidence lines | **yes** |

8/10 startable, 1 partial, 1 not. Population-wide: 58/96 open bodies mention acceptance criteria, 72/96 contain a code pointer, 4 have < 300 chars, **2 are empty** (`str-1fik` P1, `str-2zsy` P2). Quality is high where the `bento:issue-readiness-check` template was used; the failures are the empty bodies and the `str-35vtk.*` batch with placeholder cross-references.

---

## Part C — Closed-issue regression check

Closed: **1538** (P0 6 · P1 606 · P2 697 · P3 195 · P4 34). "All closed P0/P1" = 612 issues — too many to verify one-by-one, so the sample is **all 6 P0 + 30 random closed P1 (non-epic)**, plus **30 random closed bugs (P2–P4)** and **15 random closed P2 features** = **81 issues**. Seeds: 20260903 / 7 (reproducible from `.beads/issues.jsonl`). Verification: extract concrete artifacts from the body, grep HEAD (`84941b37`) for them; done by 4 parallel read-only agents.

| verdict | count | notes |
|---|---|---|
| PRESENT | 76 | includes 3 where the fix was a deliberate deletion (`str-grl5`, `str-tnpc`) or a documented supersession (`str-31j.1` → staging copy; `str-pe7`'s `--emit-tests` removed by closed `str-tlnt`) |
| MOVED | 2 | `str-ozjv` (constructor materializer split/renamed in `planner_consumer.rs`), `str-0wuw` (`ScanConfig.pool` → `pool_path`) — behaviour intact |
| REGRESSED | **0** | — |
| UNVERIFIABLE | 2 | `str-wepv.2` (plugin skill lives outside repo), `str-6fjl` (pickpackit-repo artifacts; closed as stale, superseded by open `str-poylc`) |

Regression rate in sample: 0/79 verifiable (0 %). Closed issues are, in general, trustworthy as a record of what is on HEAD. Two soft notes: `str-3ky9.11`'s recorded-mocks dir moved to `shatter-artifacts/recorded-mocks/` (legacy path still read) and `str-6jg`'s per-stratum mock breakdown exists only as a `stratum_excluded` source, not a separate table.

### C1. Closed P0 (all) + P1 sample

| id | P | type | artifact(s) checked | verdict | evidence |
|---|---|---|---|---|---|
| str-szcn3 | P0 | bug | `InstrumentableLineCount` on Go instrument response | PRESENT | shatter-go/protocol/handler.go:714; instrument/visitor.go:44; e2e_concolic_go.rs:354 |
| str-1y6q | P0 | bug | goroutine panic captured as failure | PRESENT | shatter-go/build/instrumented_overlay.go:454-469; protocol/outcome_test.go:680 |
| str-kyhy | P0 | task | `approximate_constants` / PI literals purged | PRESENT | no bare 3.14 in listed files; boundary_dict.rs:171 uses `consts::PI` |
| str-flqp | P0 | bug | float JSON accepted where `i64` expected | PRESENT | shatter-core/src/protocol.rs:645, test :2887 |
| str-iqnk | P0 | bug | Go frontend always emits `args: []` | PRESENT | shatter-go/protocol/handler.go:1660; types.go:544 |
| str-m5w | P0 | feature | concolic worklist loop in orchestrator.rs | PRESENT | shatter-core/src/orchestrator.rs:1, :241 |
| str-bnsw | P1 | bug | `check_frontend_availability` precheck | PRESENT | shatter-cli/src/helpers.rs:497; scan.rs:497; explore.rs:4353 |
| str-jdge | P1 | bug | `solver_guided_inputs` stat; user-seed starvation guard | PRESENT | report.rs:293; strategy.rs:607-658 |
| str-28ea.14 | P1 | feature | explore reuses single `PreparedTarget` | PRESENT | shatter-cli/src/commands/explore.rs:940, :4486 |
| str-0018 | P1 | bug | `prune_serde_json_value` (E0252); `SHATTER_RUNTIME_PATH` | PRESENT | shatter-rust/src/executor.rs:6944-7007, :1199 |
| str-2qwo | P1 | bug | native-replay branch-line accounting | PRESENT | shatter-core/src/pipeline.rs:553-592; tests :1150-1379 |
| str-bmh | P1 | task | fingerprint compare + spec merge | PRESENT | fingerprint.rs:33-410; spec.rs:912; scan_orchestrator.rs:1607 |
| str-p6s5 | P1 | bug | `expand_target_args` accepts `(*Type).Method` | PRESENT | shatter-cli/src/args.rs:2245, :2236 |
| str-wepv.2 | P1 | feature | plugin "init integration" skill | UNVERIFIABLE | retired with str-wepv epic; plugin lives outside this repo |
| str-si1a | P1 | bug | `skip_cargo_check()` default true; audit cache reuse | PRESENT | shatter-rust/src/executor.rs:1140-1158; helpers.rs:684 |
| str-rv0k | P1 | bug | Rust `handle_execute` `last_file` fallback | PRESENT | shatter-rust/src/handler.rs:370, :842, :970 |
| str-k9y5 | P1 | bug | `external_audit_mode` guard in explore | PRESENT | shatter-cli/src/commands/explore.rs:4181-4192 |
| str-1qd5i | P1 | task | adapter launcher propagates `branch_path`/`lines_executed` | PRESENT | shatter-go/protocol/adapter_launcher.go:66; adapter.go:120-126 |

### C2. Closed P1 sample (cont.) + closed bugs

| id | P | type | artifact(s) checked | verdict | evidence |
|---|---|---|---|---|---|
| str-44as | P1 | bug | `--timeout-total` enforcement test | PRESENT | scan_orchestrator.rs:9366; scan.rs:31,231,1187 |
| str-19pm.4 | P1 | feature | `--isolation none` shared pool | PRESENT | shatter-cli/src/args.rs:944-970 |
| str-28ea.1 | P1 | feature | `capture_side_effects` default false | PRESENT | args.rs:608-610 |
| str-ozjv | P1 | bug | `materialize_constructor_value`, `zero_value_for_type_hint` | MOVED | split into `materialize_execute_constructor_value` (planner_consumer.rs:416) / `materialize_stored_constructor_value` (:394); `zero_value_for_type_hint` :459 |
| str-jeen.63 | P1 | bug | "Production-ish source lines" vs `source_set` | PRESENT | report.rs:2210, :1619, :1702 |
| str-b22g | P1 | bug | pure `http.NotFound` policy | PRESENT | shatter-go/protocol/policy.go:414-416; policy_test.go:204 |
| str-1mc | P1 | task | shatter-go instrument package | PRESENT | shatter-go/instrument/{api,materialize,mcdc,flowwalk,executor}.go |
| str-7jgm.7 | P1 | task | single `task parity` entrypoint | PRESENT | Taskfile.yml:245-265; scripts/validate-parity.py |
| str-osr7 | P1 | feature | crate_bridge `&T` param deser (`owned_type_for_ref`) | PRESENT | shatter-rust/src/executor.rs:2296; tests :10394-10432 |
| str-ndb.4 | P1 | feature | `protocol/schemas/`, PROTOCOL.md, noop frontend | PRESENT | protocol/schemas/ (20 files); PROTOCOL.md; protocol/noop-frontend.sh |
| str-dtzq | P1 | bug | nested-struct map sentinel normalization | PRESENT | shatter-go/wrapper/wrapper.go:2410, :691, :912-930 |
| str-g7h7 | P1 | feature | `ReceiverRequiresConstruction` heuristic | PRESENT | shatter-go/protocol/receiver_construction.go:33; handler.go:2066 |
| str-do53 | P1 | feature | `convert_type_path` cross-file synthesis | PRESENT | shatter-rust/src/analyzer.rs:1263; test :3052 |
| str-pf51.5 | P1 | feature | harness storage outside project temp | PRESENT | shatter-core/src/harness_storage.rs:1-21; shatter-go/build/registry.go:14 |
| str-6fjl | P1 | bug | pickpackit `tags.rs` handlers / `WorkspaceWriter` | UNVERIFIABLE | artifacts live in pickpackit repo; closed as stale, superseded by open str-poylc |
| str-31j.1 | P1 | bug | Rust run must not dirty target checkout | PRESENT (superseded by staging copy, str-ja70) | shatter-rust/src/executor.rs:458-462, :3990-4045 |
| str-oc67 | P1 | bug | axum current-thread runtime + spawned-await test | PRESENT | executor.rs:6783-6801; test :12852 |
| str-w0d.1 | P1 | feature | `buildSymExpr` / `buildSymExprWithFlow` | PRESENT | shatter-ts/src/instrumentor.ts:1862, :874 |
| str-tnpc | P2 | bug | `loop_convergence_window` removal | PRESENT (deleted) | zero hits; loop_analysis.rs:100 retained |
| str-heegk | P3 | bug | wire-shim dedupe on `sanitizeMockName` | PRESENT | shatter-go/instrument/mocksubst.go:212; mockname_collision_test.go |
| str-wg4jo | P3 | bug | `PathSegment`, `coerce_slot_to_array`, `overlay_json_path` | PRESENT | shatter-core/src/orchestrator.rs:1205-1484 |
| str-6q1i | P2 | bug | `--resume off` must not create `./off` | PRESENT | shatter-cli/src/commands/scan.rs:93-95; tests :2203 |
| str-mpwp | P2 | bug | explore `--format` help no json | PRESENT | args.rs:620-622 |
| str-94cg | P2 | bug | no-match include diagnostic names scan root | PRESENT | scan.rs:573-611 |
| str-hjp5 | P2 | bug | analyze deser failure classification | PRESENT | shatter-core/src/frontend.rs:696-702 |
| str-w5jt9 | P2 | bug | `run_implicit_init` / `is_path_tracked` | PRESENT | shatter-cli/src/main.rs:43-52; init.rs:104; tests/implicit_init_gitignore_test.rs |
| str-jeen.83 | P2 | bug | `pool_to_candidate_inputs_for_callees` cap | PRESENT | input_gen.rs:3954-3972; test :6658 |
| str-40w2 | P2 | bug | `detects_go_project_via_go_mod` self-contained | PRESENT | shatter-core/src/project.rs:92-102 |
| str-jeen.82 | P2 | bug | `extractLiterals` const relevance gate | PRESENT | shatter-ts/src/analyzer.ts:2453-2532 |
| str-e12b | P2 | bug | axum path segment truncation | PRESENT | executor.rs:6593; test :8804 |

### C3. Closed bugs (cont.)

| id | P | type | artifact(s) checked | verdict | evidence |
|---|---|---|---|---|---|
| str-2zfz | P2 | bug | expression-string input seeding; `CompileCELMatcher` fixture | PRESENT | shatter-core/src/input_gen.rs:3815-3840; shatter-go/planner/compose.go:290 |
| str-4cqz | P2 | bug | Go `func(string) error` param stubbed | PRESENT | shatter-go/wrapper/wrapper.go:1885; wrapper_internal_test.go:1834 |
| str-ph77 | P2 | bug | `plan` field on prepare not instrument | PRESENT | protocol/registry.yaml:148; schemas/request.schema.json:126 |
| str-poyv | P2 | bug | serial scheduler Started overlap | PRESENT | scan_orchestrator.rs:12060 test |
| str-iylc | P2 | bug | wrapper imports for qualified types | PRESENT | shatter-go/wrapper/wrapper.go:2697 |
| str-grl5 | P2 | bug | `loop_convergence_window`/`LoopCoverageTracker` removed | PRESENT (deleted) | no hits in HEAD; only in stale `.claude/worktrees/str-umw3/` copy |
| str-o7a | P2 | bug | `wrapBranchCondition` `!!` coercion | PRESENT | shatter-ts/src/instrumentor.ts:1704-1734 |
| str-807vi | P2 | bug | `join_with_dynamic_watchdog` | PRESENT | scan_orchestrator.rs:5535; tests :6822-6908 |
| str-5zjc | P2 | bug | bounded Go workspace GC | PRESENT | shatter-go/workspace/gc.go:12-28; commands/workspace.rs:56 |
| str-s4cg | P4 | bug | `buildvcs_meta_test.go` audit header | PRESENT | shatter-go/buildvcs_meta_test.go:18-40 |
| str-bni0 | P2 | bug | no `.shatter-launchers/` on clean scan | PRESENT | shatter-cli/tests/clean_scan_no_project_writes.rs |
| str-2tza | P2 | bug | `-buildvcs=false` in overlay smoke test | PRESENT | shatter-go/overlay/overlay_test.go:227 |
| str-wcf3 | P2 | bug | `HARNESS_CACHE_VERSION` in `source_hash` | PRESENT | shatter-rust/src/executor.rs:758-767 |
| str-uoclg | P2 | bug | conformance `prepare_supported_rust` timeout | PRESENT | protocol/conformance/conformance_cases.yaml:167-171 |
| str-ispk | P3 | bug | "Test order:" line removed | PRESENT | scan_orchestrator.rs:7862, :8305 guards |
| str-76np | P2 | bug | `capture` field on Rust `Request` | PRESENT | shatter-rust/src/protocol.rs:637; handler.rs:88 |
| str-70m2 | P2 | bug | crate-bridge `bridge_source_hash` | PRESENT | shatter-rust/src/executor.rs:3268; tests :14422 |
| str-is5g | P2 | bug | Go `time.Duration` numeric JSON | PRESENT | shatter-core/src/input_gen.rs:161-166; wrapper_duration_test.go |

### C4. Closed P2 features

| id | P | type | artifact(s) checked | verdict | evidence |
|---|---|---|---|---|---|
| str-81xiw.1 | P2 | feature | `DiffHunkSet`, `FileDeleted`, `--unified=0` | PRESENT | shatter-core/src/scm.rs:189-284 |
| str-u4c | P2 | feature | `BatchSpec`/`parse_batch_spec`, `--batch` | PRESENT | core_sample.rs:201-272; shatter-cli/src/args.rs:817 |
| str-b2my.16 | P2 | feature | `ExploreState` resume API | PRESENT | shatter-core/src/orchestrator.rs:683-690, :2398 |
| str-0s76.3 | P2 | feature | `discover_setup_files()` | PRESENT | shatter-core/src/discovery.rs:490-532 |
| str-j4wb | P2 | feature | `.tsx` / `ScriptKind.TSX` / `jsx` option | PRESENT | discovery.rs:24; shatter-ts/src/instrumentor.ts:163; analyzer.ts:169 |
| str-pe7 | P2 | feature | `scan` subcommand + 15 flags | PRESENT (superseded) | all flags at shatter-cli/src/args.rs:684-800 except `--emit-tests`, which was **deliberately removed by closed str-tlnt** ("Drop --emit-tests export", commit 87cad05b); walkthrough moved to demo/walkthrough.yaml:27 |
| str-jeen.8 | P2 | feature | 6 coverage-budget gate fields + flags | PRESENT | shatter-core/src/config.rs:570-585; run.rs:1780 |
| str-0wuw | P2 | feature | `ScanConfig.pool`, `.shatter/seeds/pool.json`, `--seeds-dir`/`--no-seeds` | MOVED | field is now `pool_path: Option<PathBuf>` (scan_orchestrator.rs:203); flags at args.rs:531-923; behaviour intact |
| str-6jg | P2 | feature | `mocks_used` with `MockSource` variants | PRESENT | shatter-core/src/report.rs:494, :1452-1483 (per-stratum table not separate) |
| str-bo4z.7 | P2 | feature | scheduler state partitioned by coverage mode | PRESENT | scan_orchestrator.rs:259-262; batch_scheduler.rs:198 |
| str-3ky9.11 | P2 | feature | `--record`, `recorded_mocks` YAML | PRESENT (dir moved) | args.rs:549; recorded_mocks.rs:206; output dir now `shatter-artifacts/recorded-mocks/` with legacy path still read |
| str-ttu3 | P2 | feature | `harvest_from_exploration` from all input sources | PRESENT | interesting_pool.rs:722; explorer.rs:6496 guard test |
| str-fyw | P2 | feature | `analysis_cache.rs` lookup/store, `cache clear --analysis` | PRESENT | shatter-core/src/analysis_cache.rs:68-124; commands/cache.rs:14-30 |
| str-su5o.3 | P2 | feature | `PatternMatch` evidence (UUID/JWT/epoch) | PRESENT | shatter-core/src/nondeterminism.rs:26-392 |
| str-qsr3 | P2 | feature | `refine_boundaries`, `--refine-budget` | PRESENT | boundary_search.rs:319-337, test :741; orchestrator.rs:143 |

Side finding from C: a stale, unregistered checkout `.claude/worktrees/str-umw3/` (9 MB, dated 2026-04-10, gitignored, not in `git worktree list`; its issue closed 2026-04-11) still sits inside the repo and pollutes grep results (it was the only place `LoopCoverageTracker` still "exists"). Safe to delete. [P3]

---

## Part D — Findings summary (tagged)

**P1**
1. `bd` is down: orphaned `dolt sql-server` PID 28938 holds the DB lock; `.beads/dolt-server.{pid,port}` missing → every `bd` call fails after a 10 s auto-start timeout. Nothing in Part B could use the live DB. (§0)
2. 13 of 14 `in_progress` issues are dead claims (54–93 d, no notes; 6 are P1; 2 have no assignee). `bd ready` hides them from other agents. (§B1, drift-patrol FAIL)
3. Six finished worktree branches (`str-0z1im`, `-6vl7p`, `-8q1b4` [P1 bug], `-na9db` [unpushed], `-rmcrl`, `-vr7vq`) are unmerged with their issues still `open`/unclaimed; `str-duens` is merged but its issue is `in_progress` and the worktree lingers; local `main` is 3 commits ahead of `origin/main` (`str-mpgg1`, unpushed). (§B1b)
4. `parity-expiry` FAIL: `rust-protocol-enum-vocabulary-narrower` must be removed from `protocol/parity-matrix.yaml` + `protocol/PARITY.md` within ~8 days or `task parity` goes red on unrelated branches. (§A)

**P2**
5. Duplicate open bugs `str-0wxw` ≡ `str-hrg2`; overlapping `str-1fik` (P1, empty body) / `str-wfd2` (P2). (§B3)
6. Orphans under closed epic `str-hy9b` (`.1`, `.J3`) — the missing rule is itself an open 82-d issue `str-5b9f`. (§B6)
7. Epic `str-j49xg` (P1) promises three "filed alongside" children that don't exist. (§B6)
8. `str-35vtk.*` bodies cross-reference "issue3 / issue11 / issues17,18" placeholders instead of `str-` ids — unresolvable by a fresh agent. (§B2, §B7)
9. Empty bodies on open `str-1fik` (P1) and `str-2zsy`; one-line `str-cl53` (oldest open, 178 d). (§B7)
10. Priority hygiene: `str-u394l` P1 epic with only P2 children; `str-rmcrl` P3 for a red `clippy -D warnings` on main; 6 P1s untouched > 30 d. (§B5)
11. Assignee identity is "Test User" on 10 in_progress + 1 open issue — `bd` user identity not configured in agent sessions. (§B1)
12. Export bookkeeping stale: `export-state.json` 2026-07-05/1524 vs JSONL 1653 rows; `last-touched` names an issue absent from the export. (§0)

**P3**
13. ~50 % of closes carry no informative `close_reason` ("Closed"/blank). (§B4)
14. P1 is used as the default priority (40 % of closed, 24 % of open). (§B4)
15. Stale `.claude/worktrees/str-umw3/` checkout inside the repo. (§C)
16. Two PENDING patrol slots (`str-wurp`, `str-u394l.3`) have sat 58+ d untouched. (§A)

---

## Part E — Issue-ready process recommendations

Each is written so it can be filed as-is (`bd create … --label governance`).

**R1 — Restore bd; make the dolt lock self-healing.** *Goal:* `bd` works from a fresh shell. *Steps:* stop PID 28938 (`bd dolt stop` or `kill`), run `bd dolt start`, confirm `bd stats`. Then add a check to `scripts/drift-patrol.py` `tracker-hygiene` (or a `bd doctor` step in the SessionStart hook) that fails when a `dolt sql-server` process exists for `.beads/dolt` but `.beads/dolt-server.port` is absent, printing the PID. *Acceptance:* `bd list --json` succeeds; patrol reports the orphan condition when reproduced by deleting the port file.

**R2 — Triage the 13 stale in_progress claims (one sweep).** For each id in §B1: if a branch/worktree exists, land or park it; else `bd update <id> --status open` and drop the assignee. Close `str-sff87` (delivered audit). Then lower the patrol `--stale-days` from 14 to 7 for in_progress, since agent sessions are hours long, not weeks. *Acceptance:* `python3 scripts/drift-patrol.py --only tracker-hygiene` PASS.

**R3 — Land or park the six finished worktree branches; push main.** Order: `str-8q1b4` (P1) → `str-rmcrl` (unbreaks clippy on main) → the four P3s; push `str-na9db` first since it exists only locally. Close `str-duens`, remove its worktree. `git push origin main`. Use `bento:land-work` per branch. *Acceptance:* `git worktree list` shows only active work; `origin/main == main`.

**R4 — Claim-on-branch rule.** Make `bento:launch-work` (or the pre-edit hook) run `bd update <id> --claim` when it creates the `str-<id>-…` worktree, and `bento:land-work` close it. Today branches are cut without claims (6 of 8 worktrees). *Acceptance:* a patrol check "branch `str-X-*` exists but `str-X` is not in_progress/closed" reports zero.

**R5 — Fix parity-expiry now.** Remove `rust-protocol-enum-vocabulary-narrower` from `protocol/parity-matrix.yaml` and `protocol/PARITY.md`; run `task parity`. Then re-status `str-5dx0` (its patrol wiring already landed via `str-u394l.1`; if nothing remains, close it).

**R6 — Epic-close lifecycle (implement `str-5b9f`).** `bd close <epic>` must refuse (or warn and re-parent/close) when dotted children are open. Immediate: reopen `str-hy9b` or close/re-parent `.1` and `.J3`. Also file the three promised `str-j49xg.1–3` children or edit the epic body to say they weren't split.

**R7 — Issue-body readiness gate on batch filing.** `bento:issue-readiness-check` already exists; the misses are (a) empty bodies (`str-1fik`, `str-2zsy`) and (b) batch-filed plan children using plan-local ids (`str-35vtk.*`). Add to the check: body non-empty, and every `issue<N>` / `blocked by` token resolves to a `str-` id (rewrite placeholders after `bd create` returns ids). Fix the two empty bodies and the `str-35vtk.*` references now.

**R8 — Dedupe and re-prioritise.** Close `str-hrg2` as dup of `str-0wxw`; merge `str-wfd2` into `str-1fik` (keep one priority); raise `str-rmcrl` to P1/P2 or land it; either promote `str-u394l.3/.4` to P1 or demote the epic; move `str-cl53` to deferred or schedule it. Adopt "P1 = someone will pick this up this week" and audit the 23 open P1s against that.

**R9 — Configure bd identity in agent sessions.** Set `bd` user (config or `BD_USER`/`git config user.name`) in the SessionStart hook so claims read as a real person/agent, not "Test User". *Acceptance:* new claims show a distinct assignee.

**R10 — Require a close reason.** Land-work should pass `--reason` (commit SHA or "duplicate of str-X" / "won't do: …"). Cheap, and it made Part C's verification possible where present.

**R11 — Repo cruft.** Delete `.claude/worktrees/str-umw3/`; have `bento:closure` ignore-or-report anything under `.claude/worktrees/`.
