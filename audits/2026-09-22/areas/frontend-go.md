# Area review: shatter-go + shatter-go-tool (2026-09-22)

Scope: `shatter-go/` (~30k non-test lines, 18 packages plus the nested `harness/` module) and `shatter-go-tool/` (394 lines). Levels L1, L2, L4, with some L5/AGENT findings the evidence led to.
Base: audit worktree `audit-2026-09-22` @ 16794cef.
Prior audit: `audit-2026-09-04/audits/2026-09-04/frontends-protocol.md` (Go sections A3, B-SymExpr), filed as str-qwua7.*.

## Method

- Read `shatter-go/CLAUDE.md` (54,640 bytes, unchanged since the prior audit) and checked its claims against the code.
- Ran `golangci-lint run ./...` using the repo's `.golangci.yml`. The machine load average was about 100–200, so the 3-minute config timeout ran out and results are **partial**: 10 issues were reported before the timeout.
- Ran `deadcode ./...` (golang.org/x/tools, whole-program reachability from `main`) and `gofmt -l`.
- Built the frontend to the scratchpad and drove it over stdio (handshake/analyze/instrument/execute/shutdown). Ran `shatter explore` on small known-answer Go files.
- Built a **relocated** frontend (from a copy of the source that was then moved away) to simulate an installed binary.
- Ran `go get -tool` exactly as documented in `docs/distribution.md`.
- Checked the tracker (`bd list --all`, `bd show`) for duplicates and for dangling references.
- Not done: full test suites (a separate gates agent runs them). The hot-loop response-size experiment timed out twice at load average 208, so it is inconclusive and **not** reported as a finding.

## Headline findings

| # | P | L | Finding |
|---|---|---|---|
| go-01 | P1 | L5 | The Go frontend binary needs its source tree at the absolute path it was compiled at (`runtime.Caller(0)`). A relocated or installed binary fails every uncached execute with `build_failed`. |
| go-02 | P1 | L5/L2 | The documented `go get -tool github.com/shatterproof-ai/shatter/go-tool/cmd/shatter@…` path cannot resolve: the module lives in `shatter-go-tool/`, not `go-tool/`. str-fl9g.2 was closed on this acceptance check anyway. |
| go-03 | P1 | L1/L5 | Rune/char literals become `const str` in both SymExpr builders, so `c == 'x'` is unsatisfiable. The analyzer also mangles string escapes and quote characters (`strings.Trim`). |
| go-04 | P2 | L4 | The runtime (instrument) constraint builder never uses its flow map. `instrument/flow.go` and `flowwalk.go` (308 lines) are unreachable, even though CLAUDE.md names them as the `ite` mechanism. There are four SymExpr builders in total. |
| go-05 | P2 | L1/AGENT | golangci-lint is configured but no gate or CI job runs it. The partial run reported 10 issues (7 unused functions, 1 tautology, 2 SA5011). 10 files are not gofmt-clean. The go-conventions skill calls lint "Tool-Verified". str-qwua7.32 was closed with "task go:lint passes" as an acceptance check. |
| go-06 | P2 | L1/L4 | Large amount of test-only or unreachable production code. `reconstruct/` has no importers, but str-qwua7.48 asks for property tests *for it*. |
| go-07 | P2 | L4/security | `config.findConfigFile` walks all the way up to `/`. A stray `/tmp/.shatter/config.yaml` breaks `TestLoad_MissingFile_ReturnsZeroFile` (root cause of str-k7czv). Any ancestor config can grant `policy.allow`. |
| go-08 | P2 | L2/L4 | The CLI exports `SHATTER_BUILD_TIMEOUT` and `SHATTER_HARNESS_RELEASE`, but the Go frontend reads neither. `--build-timeout` does nothing for Go, and Go's `go build` calls have no timeout at all. |
| go-09 | P2 | L2 | Stale, false, or dangling claims in `shatter-go/CLAUDE.md` and code comments (preflight, str-8v66/str-ruw0, ResolveMockSpecs, SHATTER_HARNESS_CACHE, the 5s timeout, flow.go). |
| go-10 | P2 | L1 | shatter-go-tool: non-atomic extract means a partial binary gets cached as executable; there are no HTTP timeouts; it calls the GitHub API on every run; tests cover only argument parsing. |
| go-11 | P3 | L2 | The cgo "detect and refuse" claim in docs/go-frontend-scope-limits.md only covers signatures, not calls to `C.*` in the function body. |
| go-12 | P3 | L1 | Line-record calls are emitted for synthetic line-0 statements, so every execution reports `lines_executed: [0,0,…]`. |
| go-13 | P2 | L4 | Property coverage is misdirected. `wrapper` (3.2k-line code generator) has no "generated source always parses/compiles" property, and `setup/` has no tests. |
| go-14 | P3 | L2 | Prior-audit items still true: the false "does not emit preflight_failed" claims (str-qwua7.34), the stale frontend-parity skill (str-qwua7.24), a 54 KB CLAUDE.md (str-qwua7.25), and the unfiled P3 tidy items (`_ = anchorImport`, `_ = fresh`, unchecked lock-file PID writes). |
| go-15 | P2 | L6 | Cross-area (CLI): explore's resume ignores a change of explorer mode or iteration budget. "3/3 branches" is reported while an arm was never taken. "Initialized Shatter project at " prints an empty path. |

---

## go-01 [P1, L5] The frontend binary depends on its build-time source path

`instrument/executor.go:112-133`:

```go
func ensureHarnessRuntimeDir() (string, error) {
	harnessRuntimeOnce.Do(func() {
		_, currentFile, _, ok := runtime.Caller(0)
		...
		moduleDir := filepath.Clean(filepath.Join(filepath.Dir(currentFile), "..", "harness"))
		...
		if _, err := os.Stat(filepath.Join(absModuleDir, "go.mod")); err != nil {
			harnessRuntimeErr = fmt.Errorf("stat harness runtime go.mod: %w", err)
```

Production callers: `build/instrumented_overlay.go:162` and `:819`, reached through `instrument.EnsureHarnessRuntimeDir` (`api.go:16`). The API doc comment says it "materializes the shared harness runtime module". It does not: nothing is embedded or written. The nested module `shatter-go/harness/` (`module shatter-harness`) is wired into the launcher go.mod through a `replace` (`launcher/launcher.go:528`).

How the binaries are built:
- `shatter-cli/build.rs:163-205` runs `go build` from `../shatter-go` and embeds only the binary.
- `.github/workflows/release.yml` ("Build shatter-go frontend": `go build -o ../staging/$GO_BINARY .`) does the same without `-trimpath`.
- So the path baked into every release is the CI runner's checkout path.

Reproduction (relocated build):

```
cp -r shatter-go $S/gocopy && (cd $S/gocopy && go build -buildvcs=false -o $S/shatter-go-reloc .)
mv $S/gocopy $S/gocopy.moved
printf handshake/analyze/execute(Double,[5])/shutdown | $S/shatter-go-reloc
```

Response for id 3:

```
status: error, code: instrumentation_failed,
outcome: {status: build_failed, short_reason: 'go build failed during harness compilation',
  thrown_error.message: 'build failed: build: harness runtime: stat harness runtime go.mod:
  stat .../scratchpad/gocopy/harness/go.mod: no such file or directory'}
```

A first probe with an already-cached launcher (`lit.go:Classify`) still succeeded. So the failure shows up on the first uncached target, and dev machines with a warm cache or an in-place checkout never see it. The outcome is also mislabelled: no `go build` ran, but the reason says "go build failed during harness compilation".

Tracker: no issue found (`bd list --all | grep -i harness` shows only closed str-3f2o and str-b7zh, which are unrelated).

Recommendation:
- `//go:embed` the `harness/` module files (go.mod plus runtime.go).
- Materialize them once into `<workspace>/harness-runtime/<content-hash>/` and return that path.
- Add a test that builds the frontend with `-trimpath` into a temp dir, deletes nothing but runs from outside the repo, and executes a fresh target.
- Add a release-workflow smoke that runs `shatter explore` on an example from a directory without the source tree.
- Classify a missing harness runtime as an `infrastructure` error, not `build_failed`.

Agent root cause: every gate (unit, e2e, walkthrough, gauntlet) runs from inside the checkout, so no installed-binary test exists. The build.rs comment ("Absent … in an installed binary, in which case the staleness check is skipped") shows the installed case was considered for `doctor` but never for execution.

## go-02 [P1, L5] The documented Go tool install path does not resolve

`docs/distribution.md:59-71`:

```
go get -tool github.com/shatterproof-ai/shatter/go-tool/cmd/shatter@continuous-…
```

`shatter-go-tool/go.mod`: `module github.com/shatterproof-ai/shatter/go-tool`. The directory is `shatter-go-tool/`, and the repo has no root go.mod. The Go module system therefore looks for `go-tool/` at the repo root.

Run in a fresh temp module (network, `GOPROXY=direct`):

```
go: module github.com/shatterproof-ai/shatter@main found (v0.0.0-20260922164214-16794cef9e10),
    but does not contain package github.com/shatterproof-ai/shatter/go-tool/cmd/shatter
```

`git log -- shatter-go-tool` shows a single commit (8a667221, 2026-05-14), and there has never been a `go-tool/go.mod`. Even with the directory renamed, a non-root module needs `go-tool/`-prefixed tags to resolve `@continuous-…`.

str-fl9g.2 ("Go tool wrapper", P1) was closed "9dc76c85 landed on main". Its acceptance criterion was "A temp Go module can run go get -tool for the wrapper at a continuous tag and then go tool shatter --version". That check could never have passed. The Renovate regex in distribution.md:110 targets the same path.

Recommendation:
- Rename the directory to `go-tool/` (or change the module path to `…/shatter/shatter-go-tool` and update the docs and Renovate regex).
- Have release.yml also push a `go-tool/<tag>` tag, or document pseudo-version usage.
- Add a CI job that runs the documented `go get -tool …@<sha>` plus `go tool shatter --shatter-wrapper-help` in a temp module.

Agent root cause: the issue was closed on "landed on main" without executing the acceptance command. `task meta` only runs `go test ./...` inside `shatter-go-tool` (Taskfile.yml:467), which cannot catch module-path/directory drift.

## go-03 [P1, L1/L5] Literal handling in the SymExpr builders

Code:
- `protocol/analyzer.go:2352-2355`: `case token.STRING, token.CHAR: val := strings.Trim(lit.Value, "`\"'")`. This does not unescape, and it strips quote characters that are part of the value.
- `instrument/symextract.go:141-152` (the runtime builder) correctly uses `strconv.Unquote` for STRING, but also maps `token.CHAR` to `{kind: const, type: "str"}`.
- In Go a rune literal is an untyped rune constant (int32). Comparing a `rune`/`byte` parameter against a str const is ill-typed for the solver.

Known-answer probe (`lit.go`):

```go
func Classify(s string, c rune) int {
	if s == "a\tb" { return 1 }
	if s == "'q'"  { return 2 }
	if c == 'x'    { return 3 }
	return 0
}
```

Analyze output (branches[].condition right-hand sides):
- `{"type":"str","value":"a\\tb"}` — a literal backslash and t.
- `{"type":"str","value":"q"}` — the quotes were stripped from the value `'q'`.
- `{"type":"str","value":"x"}` — the int param is compared with a string.

Runtime `branch_path` constraint for line 10: `{"kind":"const","type":"str","value":"x"}` (from the explore artifact).

`shatter explore lit.go:Classify --allow-host-writes`:
- Default explorer, 60 iterations: 3 paths, 6/7 lines. `return 3` was never reached.
- `--concolic`, 200 iterations (stopped after 24): 2 paths, 5/7 lines. Both `return 1` and `return 3` were missed.
- Both runs reported `3/3 branches`.

The concolic miss of `"a\tb"` may come from the core's solver or string encoding rather than the analyzer constant, because the runtime constraint carries the correct tab. That attribution is **not verified**. The rune miss is definitively a Go frontend bug in both builders.

Prior audit: noted the analyzer `strings.Trim` in passing under str-qwua7.35 (P2, blocked by the SymExpr spec). The rune and char mistyping is **new**. Tracker: str-4j9 (closed) covered the rune generators, not constraints.

Recommendation, as a small fix before the str-qwua7.35 unification:
- Use `strconv.Unquote` in `litSymExpr`.
- Emit CHAR literals as `{type:"int", value:<codepoint>}` in both builders.
- Add known-answer E2E cases for escaped strings and rune comparison to `e2e_concolic_go.rs`.
- Add a rapid property: for a random printable or escaped Go string literal `s`, `litSymExpr(parse(strconv.Quote(s))).Value == s`.

Agent root cause: the E2E known-answer set has no string-escape or rune fixtures, and the prior audit's observation was folded into a P2 refactor epic blocked by a spec, so a user-visible correctness bug waited behind design work.

## go-04 [P2, L4] Four SymExpr builders; the runtime one never uses flow

The builders:
1. `protocol/analyzer.go:2171` `buildSymExpr` — static.
2. `protocol/analyzer.go:2201` `buildSymExprWithFlow` — static, flow-aware, with its own `analyzerFlowMap` and walker at 1963-2110.
3. `instrument/symextract.go:40/48` `exprToSymExpr(WithFlow)` over a private `symExpr` — runtime.
4. `protocol/loop_body_states.go:294` `buildGoLoopSnapshotSymExpr`. It resolves **params before flow** (`resolveGoLoopSnapshName`), while (2) resolves flow before params. It handles only Ident/BasicLit/Binary/`-` Unary/Paren, and it leaves stale flow values after unsupported compound assignments such as `%=` or `<<=` (visitGoLoopSnapshotAssign:236-247).

In production, instrument always calls `extractConstraint` with no flow (`visitor.go:334,350,440`, `mcdc.go:81,93`). `extractConstraintWithFlow`, `walkStmtsForFlow`, `applyIfToFlow`, `snapshot`, `mergeFlowMaps` and `symExprsEqual` are all reported by `deadcode` as unreachable:

```
instrument/flow.go:13:6: unreachable func: snapshot
instrument/flow.go:40:6: unreachable func: mergeFlowMaps
instrument/flowwalk.go:22:6: unreachable func: walkStmtsForFlow
instrument/flowwalk.go:124:6: unreachable func: applyIfToFlow
...
```

History: str-1hlk.17.1/.17.2 built the flow machinery in `instrument/`, then 34eb92c8 ("Hook flow-map into Go analyzer body walk") re-implemented it in `protocol/` and left the instrument copy only exercised by tests. As a result, analyze reports `ite` conditions while the runtime `branch_path` constraint for the same branch (`label > 0`) is `unknown`, so the solver cannot negate it.

`shatter-go/CLAUDE.md:52-62` ("Ite SymExpr Parity Contract") lists `instrument/flow.go` and `flowwalk.go` as steps 1–2 of the mechanism. That is false for runtime.

Prior audit covered (1)–(3) at a high level (str-qwua7.35). **New:** builder (4), the fact that the instrument flow code is dead rather than merely duplicated, and the CLAUDE.md claim.

Recommendation, folded into str-qwua7.35:
- Delete `instrument/flow*.go`.
- Move SymExpr plus one flow-aware builder into a leaf package (e.g. `shatter-go/symexpr`) consumed by analyzer, instrument and loop snapshots.
- Pass the flow map into `transformIfStmt` so runtime constraints for flow-tracked locals match static ones.
- Add a rapid property: static condition == runtime constraint for generated if-chains.

## go-05 [P2, L1/AGENT] Lint is configured and claimed but never run; the tree is not lint- or gofmt-clean

- `.golangci.yml:3-4` says "Runs via: task go:test". False: `go:test` runs `scripts/go-test-tier.sh`. `go:lint` (shatter-go/Taskfile.yml) exists but appears in no dependency list. The root `Taskfile.yml` `lint:` (line 184-186) depends on `go:vet` only, `check-unit` (548-554) runs `go:test` and `go:vet`, and ci.yml runs `task check`.
- `.claude/skills/go-conventions/SKILL.md` "Tool-Verified Rules": "`golangci-lint run` must pass".
- `.claude/skills/check-go/SKILL.md` runs only `go test` and `go vet`.
- Partial `golangci-lint run ./...` (v2.12.2, hit the config's 3m timeout under load):

```
protocol/handler.go:1325:32: nilness: tautological condition: nil == nil (govet)
instrument/property_test.go:544:18: SA5011: possible nil pointer dereference (staticcheck)
protocol/analyzer.go:592:6: func analyzeFunc is unused (unused)
protocol/analyzer.go:799:6: func extractParams is unused (unused)
protocol/analyzer.go:1518:6: func mapTypeInfo is unused (unused)
protocol/analyzer.go:1534:6: func structTypeInfo is unused (unused)
protocol/handler.go:1964:19: func (*Handler).lookupAnalyzedByTargetID is unused (unused)
protocol/prepared_launcher.go:474:6: func toWrapperConstructors is unused (unused)
protocol/prepared_launcher.go:506:6: func toWrapperConstructorParams is unused (unused)
10 issues ... level=error msg="Timeout exceeded"
```

- `handler.go:1325` `if preparedExec == nil && err == nil`: `err` is provably nil there. The dead conjunct hides which error the author meant to check.
- `gofmt -l` lists 10 files, 9 of them non-testdata: `instrument/symextract.go` (misaligned struct), `protocol/analysis_cache.go`, `instrument/mockfingerprint_test.go`, `instrument/overlay_test.go`, `launcher/launcher_buildvcs_test.go`, `protocol/analysis_cache_handler_test.go`, `protocol/generated_enums_test.go`, `protocol/invocation_plan_test.go`, and `protocol/property_test.go`.
- str-qwua7.32 (closed 2026-09-07) acceptance: "The encoding/json.Unmarshal line is removed; task go:lint passes." The Unmarshal part was done well (dae25798, with inline `//nolint` reasons at handler.go:1668/1672 and a regression test at handler_test.go:825). But `analyzeFunc`, `lookupAnalyzedByTargetID` and `toWrapperConstructors` already existed at dae25798 (`git grep` at that commit), so "go:lint passes" was not true when the issue was closed.
- Also `shatter-go/Taskfile.yml` `build:` comment: "CI … runs build via parity/conformance deps but never go:vet/go:test". Stale since check-unit (str-35vtk.21).

Recommendation:
- Add `go:lint` to `check-static`, with `golangci-lint` required under `CI=1`, a timeout of at least 10m or `--concurrency`, and a `gofmt -l` check (`task go:fmt-check`).
- Fix the 10 issues and the 9 unformatted files.
- Add `unused` and `deadcode` (see go-06) so dead code cannot re-accumulate.
- Update check-go to include lint, and fix the `.golangci.yml` header.

Agent root cause: skills state rules as "tool-verified" without a gate enforcing them, and issue closure does not require pasting the acceptance command's output.

## go-06 [P2, L1/L4] Test-only and unreachable production code

`deadcode ./...` (excludes tests) reports 55 unreachable functions. Selected:
- `reconstruct/` (Value, Inputs, toInt64, errorString) — the **entire package**, 136 lines. No importer anywhere (`grep -rn 'shatter-go/reconstruct'` finds only CLAUDE.md:300, which itself says "historical, no current callers"). The Taskfile keeps a special `go build ./...` just to compile it.
- `loader/legal_anchor.go` (102 lines: LegalAnchor, LauncherPackagePath, …).
- `launcher/session.go` `OpenSession` (the file is 316 lines; used by 6 test files only).
- `planner/aggregate.go` `PlanAggregate` (289 lines), `planner/classify.go` `Classify`, `planner/plan.go` `ResolveMockSpecs`. CLAUDE.md:273 says "The planner still emits … via planner.ResolveMockSpecs", which is false in production.
- `workspace/run.go` (NewRun, Run.*, generateRunID; 188 lines), `workspace/gc.go` `PlanGC`.
- `wrapper.BuildWrapperTargets` (production uses `…ForSource`), `wrapper.applyRuntimeValueBindings`.
- `protocol.AnalyzeFile`, `NewHandler`, `NewHandlerWithLogLevel`, `Handler.Registry`, plus the 7 `unused` items above.
- `instrument/flow.go` and `flowwalk.go` (go-04).

Prior audit rec 14 and str-qwua7.48 ask for **new rapid tests for `reconstruct`**. That would invest in dead code.

Recommendation:
- Delete `reconstruct/` and re-scope str-qwua7.48 to drop it.
- For each remaining test-only API, either delete it or move it to `_test.go` / `internal/testutil`.
- Add a `deadcode -test=false` allowlist check to `task meta` so new unreachable production code fails the gate.

Agent root cause: no dead-code gate, and audit recommendations were filed without checking reachability.

## go-07 [P2, L4] Unbounded upward config discovery makes tests non-hermetic and lets ancestor configs grant policy

`config/loader.go:228-249` `findConfigFile` walks from the file's directory to `/` with no module-root or VCS-root stop.

Observed: `/tmp/.shatter/config.yaml` exists (2026-09-22 11:40, "Generated by `shatter init`", probably an implicit init by some run with cwd `/tmp`). `config/loader_test.go:656-669` `TestLoad_MissingFile_ReturnsZeroFile` uses `t.TempDir()` under `/tmp`, so `config.Load` finds that stray file and the test fails. This is exactly str-k7czv (open, P3, "Confirmed pre-existing … reproduces on a clean main checkout"), whose root cause is not yet identified in the tracker. (The reproduction run was killed by `timeout` under load before the test ran, so the causal link is inferred from the code path plus the existing file. Confidence: medium-high.)

Security angle: `functions.<glob>.policy.allow` (CLAUDE.md:227-236) grants side-effect classes. A `.shatter/config.yaml` in `$HOME`, `/tmp` or any shared ancestor silently widens the safety policy for every project beneath it. Mocks and receivers from a foreign config would also be applied.

Recommendation:
- Stop the walk at the nearest `go.mod`/`go.work`/`.git` boundary (mirroring whatever the Rust CLI does; align both).
- Log the resolved config path at INFO.
- Make the test hermetic by passing an explicit stop dir.
- Update str-k7czv with this root cause.

Agent root cause: implicit init (str-qwua7.58 keeps it) can create `.shatter/` in arbitrary cwd, such as `/tmp`, and nothing bounds discovery.

## go-08 [P2, L2/L4] `--build-timeout` is a no-op for Go; Go builds have no timeout

- The CLI exports `SHATTER_BUILD_TIMEOUT` (default 30) and optionally `SHATTER_HARNESS_RELEASE` to every frontend (`shatter-cli/src/helpers.rs:745-760`). `--help` says: "Build timeout in seconds for compiling instrumented code in the frontend. Default: 30s".
- `grep -rn 'BUILD_TIMEOUT\|HARNESS_RELEASE' shatter-go` returns nothing. The only reader is `shatter-rust/src/executor.rs:2967`, whose default is 120s, not 30s.
- Go's `exec.Command("go", buildArgs...)` (`launcher/launcher.go:342`, `setup/loader.go:63`) has no context or deadline. A hung build is only bounded by the core's per-request timeout, which kills the whole session.
- Environment variables the Go frontend actually reads: SHATTER_EXEC_TIMEOUT, SHATTER_MCDC, SHATTER_SANDBOX_*, SHATTER_LOG_LEVEL, SHATTER_HOST_WRITE_DIR, SHATTER_GO_WORKSPACE_ROOT, SHATTER_FRONTEND_FINGERPRINT, SHATTER_DISABLE_ANALYSIS_CACHE (plus test-only ones).

Recommendation:
- Honour `SHATTER_BUILD_TIMEOUT` with `exec.CommandContext` in both build sites, and classify expiry as `timed_out` with a build-phase reason.
- Add both variables to `parity-matrix.yaml` (for example an `env_contract` section) and test that every CLI-exported variable is read by each frontend or explicitly declared ignored.
- This feeds str-qwua7.20.2 (the environment-variable table).

## go-09 [P2, L2] Stale, false and dangling claims in shatter-go docs and comments

| Location | Claim | Reality |
|---|---|---|
| CLAUDE.md:206; constants.go:16-19; types.go:584-590 | "The Go frontend does not currently emit either" (`preflight_failed`); divergence `error-code-preflight-failed-typescript-only` | Emitted at handler.go:394. The divergence ID is absent from parity-matrix.yaml (0 matches). Prior audit; str-qwua7.34 still open. |
| CLAUDE.md:52-62 | ite built by `instrument/flow.go` + `flowwalk.go` | Unreachable; the analyzer has its own copy (go-04). |
| CLAUDE.md:273 | planner emits MockSpec via `planner.ResolveMockSpecs` | Unreachable in production (deadcode). |
| CLAUDE.md:288, 292 | "`str-8v66` (blocked by str-ruw0) still tracks…"; "adding mock substitution in str-8v66" | `bd show str-8v66` / `str-ruw0` → "no issue found". Mock substitution already shipped (str-c8djq). |
| CLAUDE.md:318 | handler without workspace falls back to "legacy `SHATTER_HARNESS_CACHE`-based cache hierarchy" | No Go source references SHATTER_HARNESS_CACHE. |
| CLAUDE.md:202; api.go:5-7 | "5s default" | True only standalone. The CLI always sets SHATTER_EXEC_TIMEOUT=10 (helpers.rs:751, CLI_EXEC_TIMEOUT_DEFAULT_SECS=10). |
| CLAUDE.md:210 | outcome statuses: completed/build_failed/runtime_failed/timed_out/unsupported | Omits `skipped_by_policy` and `preflight_failed`, which Go emits. |
| CLAUDE.md:300 | reconstruct "historical, no current callers" | Accurate, but the right action is deletion (go-06). |
| instrument/api.go:16 | EnsureHarnessRuntimeDir "materializes" the module | It only stats the source path (go-01). |
| .golangci.yml:4 | "Runs via: task go:test" | Nothing runs it (go-05). |
| shatter-go/Taskfile.yml build comment | CI "never go:vet/go:test" | check-unit runs both. |

Recommendation:
- Fix each entry.
- Extend drift-patrol (str-qwua7.34's proposed check) to resolve every `str-xxxx` ID mentioned in `shatter-go/**/*.{go,md}` against bd and every divergence ID against parity-matrix.yaml.
- Pair with the str-qwua7.25 CLAUDE.md slimming: most of the 54 KB is per-issue history (str-c8djq, str-djcv2, str-hr40t, …) that belongs in `docs/frontends/go.md`.

## go-10 [P2, L1] shatter-go-tool robustness

`shatter-go-tool/cmd/shatter/main.go`:
- `:340-348` `extractBinary` opens `targetBinary` directly with mode 0755 and copies into it. An interrupted download or extract, or two concurrent first runs, leaves a truncated executable. The next run sees `isExecutable(targetBinary)` (`:160`) and execs the corrupt cached file forever. The SHA is checked on the archive, not on the extracted file.
- `:267`, `:279`: `http.DefaultClient` / `http.Get` have no timeout, so a stalled GitHub connection hangs `go tool shatter` indefinitely.
- `:141-147`: when `SHATTER_BUILD` is unset, every invocation calls `api.github.com/repos/…/releases` **before** checking the cache. Unauthenticated users are limited to 60/h, so CI loops and repeated local runs get rate-limited.
- `downloadFile` does not use `GITHUB_TOKEN` (only `getJSON` does).
- `main_test.go` is 39 lines covering only `parseArgs`.

Recommendation:
- Extract to a temp file in `targetDir`, verify a per-binary hash if the manifest carries one, `chmod`, then `os.Rename`, following the pattern already used at `launcher/launcher.go:336-350`.
- Use an `http.Client{Timeout: …}`.
- Cache the resolved "latest continuous" tag for a short TTL, or prefer the newest cached build when offline.
- Add httptest-backed tests for manifest selection, checksum mismatch, and partial-extract recovery.

## go-11 [P3, L2] cgo refusal is signature-only

`docs/go-frontend-scope-limits.md` ("cgo … Permanent non-goal"): "The Go frontend will detect and refuse cgo-bearing functions at analysis time".

`protocol/discovered_target.go:100-120` `fnHasCGoDep` inspects only parameter and result types for package `C`. A function calling `C.foo()` in its body with Go-typed signature is not flagged. (Not reproduced; this is code reading only.)

Recommendation: walk the body for selector expressions whose `types.PkgName` imports `"C"`, or mark the whole file when it imports `"C"`. Otherwise reword the doc.

## go-12 [P3, L1] Phantom line-0 records

`instrument/visitor.go:130-139`: `makeLineRecordCall(line)` is appended for every statement, including the synthetic `call_enter`/`call_exit` statements whose line resolves to 0. Only the denominator is guarded (`if line > 0`).

Observed in the explore artifact: `"lines_executed": [0, 0, 4, 7, 10, 13]`. Every execution records two useless entries under a mutex, and consumers must filter 0.

Recommendation: skip `makeLineRecordCall` when `line <= 0`. Add an assertion in `visitor_test.go` that no emitted record has line 0.

## go-13 [P2, L4] Property coverage is misdirected

Per-package census (non-test lines / test files / rapid files):

| package | lines | test files | rapid files |
|---|---|---|---|
| wrapper | 3219 | 7 | 1 (error_sentinel only) |
| launcher | 1114 | 7 | 0 |
| config | 524 | 1 | 0 |
| sandbox | 468 | 1 | 0 |
| setup | 158 | **0** | 0 |
| reconstruct | 136 | 1 | 0 (dead) |
| planner | 3891 | 13 | 12 |
| protocol | 12045 | 35 | 6 |

`wrapper.GenerateWrapper` and `launcher.GenerateLauncherMain`/`GenerateHarnessLauncherMain` emit Go source by string building. Example tests call `parser.ParseFile`, but no property generates random `WrapperTarget` param/receiver/generic shapes and asserts that the output parses and gofmt-formats (and, in a slow tier, compiles). That generator is where a malformed-harness regression would originate.

Recommendation: re-scope str-qwua7.48 to rapid properties for (a) wrapper and launcher generator output validity, and (b) the config key-matching invariants (anchored beats fallback, and determinism under map iteration — the str-cl19s tie-break is exactly a randomized-iteration hazard). Add a basic `setup/loader_test.go`. Drop reconstruct.

## go-14 [P3] Prior-audit items re-verified (still true)

- `_ = anchorImport` (launcher/launcher.go:391), `_ = fresh` (build/builder.go:277), and `_, _ = fmt.Fprintf(lockFile, pid)` (launcher.go:616, builder.go:188). `lockIsStale` reads the PID back, and on a write failure it falls back to a ModTime timeout. Prior audit rec 13 was **not filed** (no matching str-qwua7 child).
- `timing/collector.go:77,84` panics. These are now mitigated by `safeDispatch` recover (handler.go ~220), which turns them into `internal_error` rather than a crash; downgrade to P3 or close.
- The frontend-parity skill still says Go `thrown_error` ✗ (line 37) and "Go lacks data flow tracking" (line 59). str-qwua7.24 is open.
- `protocol` package concentration: handler.go 2577, analyzer.go 2774, wrapper.go 2949 lines. The prior P2 was **not filed**.

Positive re-verification: str-qwua7.32 Unmarshal handling is genuinely fixed (with a regression test). `launcher` builds to a temp path and renames atomically (launcher.go:336-350). `safeDispatch` preserves request/response ID pairing on panic. `bestEffortRequestID` recovers IDs from malformed JSON.

## go-15 [P2, L6] Cross-area observations (CLI/core) found while probing Go

1. **Resume ignores configuration changes.** A second `shatter explore lit.go:Classify --concolic --max-iterations 200`, run after a default-mode run, printed `[info] [resumed] Classify: 3 branches, 13.6s (prior run)` and did not explore. The mode and budget change were silently ignored, and the report still said "Explorer: concolic (Z3-backed)".
2. **Branch metric overstates coverage.** `[batch 1/1] Classify: 24 iters, 2 paths, 3/3 branches` while two of four arms were never taken. The concolic run stopped at 24 of 200 iterations, possibly because the metric counts branch *sites* observed rather than arms. This is suspected, not verified.
3. **Empty path in the init message.** Implicit init stdout: `Initialized Shatter project at ` (empty path), plus the "Created …" lines on stdout (str-qwua7.39 covers the stream but not the empty path).

Recommendation: key resume on (explorer mode, iteration/time budget, frontend fingerprint) and print why a resume happened. Report arm coverage (`taken/total arms`) and use it for the early stop. Print the absolute init path.

---

## Agent-system observations (for the synthesis)

- **Acceptance checks are not executed at closure.** str-fl9g.2 (go get -tool) and str-qwua7.32 (go:lint passes) were both closed with criteria that were false at close. Recommendation (bento land-work / beads-issue-flow): require the literal acceptance command and its output in the close reason, and have land-work refuse closure when an acceptance block names a command that was not run in the landing transcript.
- **"Tool-verified" rules without tools.** go-conventions promises golangci-lint enforcement that no gate performs. Recommendation: add a drift-patrol check that every tool named in a `*-conventions` skill's "Tool-Verified Rules" appears in the `task check` dependency graph.
- **All gates run inside the checkout.** No gate exercises an installed or relocated binary (go-01), so the classic "works on my machine" class is invisible.
- **Audit follow-through.** Prior P3 items and one P2 (protocol package split) from 2026-09-04 were never filed, and one filed item (str-qwua7.48) targets dead code. Recommendation: the audit skill should verify reachability (`deadcode`/`cargo udeps`/`ts-prune`) before recommending tests for a module, and file or explicitly waive every numbered recommendation.

## Grades

- L1 code quality: **C+**. The code is generally careful: rich comments, panic recovery, atomic launcher builds, thoughtful mock identity. It is dragged down by an unrun linter, about 55 unreachable functions including whole packages, gofmt drift, and literal mistyping.
- L2 docs/code agreement: **C**. CLAUDE.md is detailed, but carries at least 10 false or dangling claims, several of which are load-bearing (preflight, ite mechanism, build timeout).
- L4 design: **C**. There are four divergent SymExpr builders, a source-path dependency that breaks distribution, and unbounded config discovery. The runtimeval symbolic registry (str-ijtww) and the single call-site walker (str-n0rtz) are good single-sourcing examples worth copying.
