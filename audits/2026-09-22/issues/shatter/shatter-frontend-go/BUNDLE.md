# Audit 2026-09-22 issue drafts: bucket `shatter-frontend-go`

- Repo: shatter (tracker: bd in /home/ketan/project/shatter, prefix `str`)
- Parent epic: "Epic: Audit 2026-09-22 findings"
- Theme: Go frontend and go-tool wrapper: relocatable runtime, module path, rune literals, config discovery, build timeout, dead code, divergences.
- Nothing here is filed by agents (D6). Placeholders such as `<id of go-tool-module-path>` are for the filer to substitute.
- All evidence re-verified against the audit worktree at commit 56c86168 on 2026-09-23 unless marked otherwise.

## Maintainer decisions (2026-09-23), overriding the report and old drafts

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix; fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64), do not drop them. Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot-diff command and the unused Snapshot writer path; spec-diff is the regression tool. Update SPEC/README/QUICKSTART. The `diff` name becomes free; whether str-81xiw takes it is left to str-81xiw. Correct the shatter-agents plugin's `shatter diff --staged` docs.
- **D3 Concolic positioning:** measure first. P1 controlled default-vs-concolic benchmark (fixed seeds, fresh artifacts, examples corpus + one downstream project), reported per release; P1 fix concolic early termination (~21-35 iterations). A follow-up decision issue (blocked by both) re-decides "concolic-first" positioning. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import in shatter; move tracker sync to a Dolt remote; first verify whether importing the stale JSONL has clobbered newer DB state; AGENTS.md drops `bd sync`; str-qwua7.28 superseded; bento beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT fix, no hook-bypass guidance.
- **D5 Git identity:** leaked [user] section already removed 2026-09-23. Draft a .mailmap (test@example.com "Test"/"Test User" -> Ketan Gangatirkar <33678+ketang@users.noreply.github.com>, no history rewrite), a drift-patrol/setup-hooks git-state check (local identity override, example.com email, core.bare=true, hooksPath override; possibly as a note on str-qwua7.1), and test_git_fixture_isolation.py snapshotting .git/config around fixture entrypoints.
- **D6 Filing:** after reconciliation and Codex cross-check, the maintainer runs one filer script. Nothing is filed by agents.

None of D1-D6 changes this bucket's scope. D1 touches `go-harness-runtime-embed` (its release smoke must not cause any matrix leg to be dropped, and closes on a green release-run URL).

## Contents

| file | kind | priority | existing | blocked_by | title |
|---|---|---|---|---|---|
| 01-go-harness-runtime-embed.md | new | P1 | - | [] | Go frontend finds its harness runtime via the compile-time source path: installed or relocated binaries fail every uncached execute |
| 02-go-tool-module-path.md | new | P1 | - | [] | Documented `go get -tool github.com/shatterproof-ai/shatter/go-tool/cmd/shatter` cannot resolve: module path says go-tool/ but the directory is shatter-go-tool/ |
| 03-go-tool-reopen-note.md | reopen-note | P1 | str-fl9g.2 | [go-tool-module-path] | NOTE on str-fl9g.2: documented `go get -tool .../go-tool/cmd/shatter` never resolved; fix tracked in go-tool-module-path |
| 04-go-rune-and-escape-literals.md | new | P1 | - | [] | Go rune/char literals are typed as string constants in both SymExpr builders; analyzer strings.Trim mangles escapes and quotes |
| 05-qwua7-35-four-go-builders.md | note-to-existing | P2 | str-qwua7.35 | [] | NOTE on str-qwua7.35: four Go SymExpr builders; instrument/flow*.go is unreachable although CLAUDE.md names it as the ite mechanism |
| 06-go-config-discovery-unbounded.md | new | P2 | - | [] | Go config loader walks up to / (stray /tmp/.shatter breaks tests; ancestor configs can widen policy.allow) and warns on the `defaults` key that `shatter init` writes |
| 07-go-dead-code-and-property-targets.md | new | P2 | - | [] | ~57 unreachable shatter-go functions incl. the whole reconstruct package; add a deadcode gate and re-scope str-qwua7.48 property tests to live generators |
| 08-go-tool-wrapper-robustness.md | new | P2 | - | [go-tool-module-path] | shatter-go-tool: non-atomic extract can cache a truncated binary; no HTTP timeouts; GitHub API call on every run; tests cover only arg parsing |
| 09-go-build-timeout-ignored.md | new | P2 | - | [] | Go frontend ignores --build-timeout / SHATTER_BUILD_TIMEOUT (go build has no timeout) and SHATTER_HARNESS_RELEASE |
| 10-go-connection-failures-divergence.md | new | P2 | - | [] | Go frontend never emits connection_failures or runtime_crypto_boundaries, so LiveFirst fallback never fires for Go; no divergence recorded |
| 11-go-explore-warmup-gate.md | new | P3 | - | [] | Explore lacks the cold-build warmup gate that scan has; many-target Go explores on a cold cache can time out en masse |
| 12-go-small-correctness-tidy.md | new | P3 | - | [] | Go frontend small fixes: line-0 records on every execution, generated mock code ignores Unmarshal errors and embeds JSON in backtick raw strings, dead assignments, unchecked lock-PID writes |
| 13-go-cgo-refusal-covers-bodies.md | new | P3 | - | [] | Go cgo "detect and refuse" is signature-only, and its only consumer (planner.Classify) is unreachable: verify, then refuse body C.* calls or narrow the claim |

---

<!-- file: 01-go-harness-runtime-embed.md -->
---
slug: go-harness-runtime-embed
kind: new
title: "Go frontend finds its harness runtime via the compile-time source path: installed or relocated binaries fail every uncached execute"
priority: P1
type: bug
labels: [go-frontend, distribution, install, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go frontend finds its harness runtime via the compile-time source path: installed or relocated binaries fail every uncached execute

## Problem

`ensureHarnessRuntimeDir` locates the nested harness module (`shatter-go/harness/go.mod` + `runtime.go`, `module shatter-harness`) relative to the source file path that `runtime.Caller(0)` recorded when the frontend binary was compiled. Nothing is embedded. Release binaries are built on CI runners, so the baked-in path is the runner's checkout path, and on a user's machine every Go execute that needs a fresh harness build fails.

The failure is also mislabelled: it surfaces as outcome `build_failed` with reason "go build failed during harness compilation", although no `go build` ran. Targets whose launcher is already cached still work, which is why dev machines (in-place checkout, warm cache) never see it.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go/instrument/executor.go:112-133` `ensureHarnessRuntimeDir`: `runtime.Caller(0)` at :114, joins `<srcdir>/../harness`, and stats `go.mod` at :126, returning `stat harness runtime go.mod: ...` on failure.
- Production callers: `shatter-go/build/instrumented_overlay.go:162` and `:819`, via `instrument.EnsureHarnessRuntimeDir` (`shatter-go/instrument/api.go:16-18`). The doc comment at api.go:16 says it "materializes the shared harness runtime module"; it only stats the source path.
- No `//go:embed` exists in non-test shatter-go code (`grep -rn "go:embed" shatter-go --include=*.go | grep -v _test` is empty).
- `.github/workflows/release.yml:175` builds with `go build -o "../staging/$GO_BINARY" .` (no embed, no `-trimpath`). `shatter-cli/build.rs:196-204` runs `go build` from `../shatter-go` and embeds only the resulting binary.
- Audit reproduction (finding frontend-go-01, areas/frontend-go.md go-01):
  ```
  cp -r shatter-go $S/gocopy && (cd $S/gocopy && go build -buildvcs=false -o $S/shatter-go-reloc .)
  mv $S/gocopy $S/gocopy.moved
  # drive handshake / analyze / execute(Double,[5]) / shutdown over stdio
  ```
  Response: `status: error, code: instrumentation_failed, outcome: {status: build_failed, short_reason: 'go build failed during harness compilation', thrown_error.message: 'build failed: build: harness runtime: stat harness runtime go.mod: stat .../gocopy/harness/go.mod: no such file or directory'}`. A target with an already-cached launcher succeeded in the same session.
- No existing issue covers this (str-o650 "Precompiled harness template library", closed, is related background only).

## Acceptance criteria

- [ ] `shatter-go/harness/go.mod` and `runtime.go` are `//go:embed`ed into the frontend and materialized once (atomic temp-dir + rename) into `<workspace>/harness-runtime/<content-hash>/`; `EnsureHarnessRuntimeDir` returns that path. `runtime.Caller` is no longer used to find the module.
- [ ] A missing or unmaterializable runtime is reported as an infrastructure error (not outcome `build_failed` and not "go build failed").
- [ ] Regression test, failing before and passing after the fix: build the frontend with `-trimpath` into a temp dir, run it with a cwd outside the repo (and with the source copy moved away, as in the repro), and execute a target with a cold harness cache; it returns the function's value. Record the failing run's output in the close note.
- [ ] Release smoke: `release.yml` gains a step that runs `shatter explore` on a Go example from a directory that contains no shatter source tree, using the staged artifact. Proof at close: the URL of a green release-workflow run (a `workflow_dispatch` run is fine) in which this step executed. Per maintainer decision D1 the matrix keeps Windows and aarch64; the smoke must run at least on x86_64 Linux and must not be the reason any matrix leg is dropped.
- [ ] `cargo test --test e2e_concolic_go` passes; `task affected` passes with its `Gates selected` output recorded.
- [ ] `shatter-go/CLAUDE.md` describes the embed/materialize mechanism instead of the source-path lookup.

## Suggested approach

Add an `embed.go` in `shatter-go/harness` (or a small leaf package that the harness module directory can be embedded from, since `//go:embed` cannot reach `..`) exposing an `embed.FS`; hash its contents; write to `<workspace>/harness-runtime/<hash>/` under a temp name and `os.Rename`, mirroring the atomic pattern in `shatter-go/launcher/launcher.go:336-350`. Keep the `replace shatter-harness => <dir>` wiring in the generated launcher go.mod (`launcher/launcher.go:528`) pointed at the materialized dir.

## Out of scope

- Precompiled harness templates (str-o650 territory).
- Other release-matrix failures (tracked by the audit's release-workflow issue; D1 requires those to be fixed, not dropped).

## Dependencies

- Blocked by: none.
- Related: the audit's release-workflow-never-green issue (the green run URL required above needs a working release workflow); str-o650.

## Size

M

## References

- Finding frontend-go-01 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-01). Old draft: `drafts/shatter-code/50-go-harness-runtime-compile-path.md`.

---

<!-- file: 02-go-tool-module-path.md -->
---
slug: go-tool-module-path
kind: new
title: "Documented `go get -tool github.com/shatterproof-ai/shatter/go-tool/cmd/shatter` cannot resolve: module path says go-tool/ but the directory is shatter-go-tool/"
priority: P1
type: bug
labels: [go-tool, distribution, install, docs, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Documented `go get -tool github.com/shatterproof-ai/shatter/go-tool/cmd/shatter` cannot resolve: module path says go-tool/ but the directory is shatter-go-tool/

## Problem

The Go tool wrapper's module path does not match its directory, so the install command in `docs/distribution.md` fails for every user. The repo has no root `go.mod`, so the Go module system resolves `github.com/shatterproof-ai/shatter/go-tool` to a `go-tool/` directory at the repo root, which has never existed. Even after a rename, a nested module needs `go-tool/`-prefixed tags for `@continuous-...` versions to resolve.

str-fl9g.2 ("Go tool wrapper", P1) was closed "9dc76c85 landed on main" with exactly this command as its acceptance check; that check could never have passed.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go-tool/go.mod:1`: `module github.com/shatterproof-ai/shatter/go-tool`. `ls go-tool` and `ls go.mod` at the repo root: no such file. `git log --all -- go-tool/go.mod` is empty.
- `docs/distribution.md:64`: `go get -tool github.com/shatterproof-ai/shatter/go-tool/cmd/shatter@continuous-20260512-1735-abc123def456`. The Renovate regex at `docs/distribution.md:110` targets the same path.
- `Taskfile.yml:467`: `task meta` only runs `cd shatter-go-tool && go test ./...`, which cannot detect module-path/directory drift.
- `release.yml` pushes no `go-tool/<tag>` tags.
- Audit run in a fresh temp module (network, `GOPROXY=direct`):
  ```
  go: module github.com/shatterproof-ai/shatter@main found (v0.0.0-20260922164214-16794cef9e10),
      but does not contain package github.com/shatterproof-ai/shatter/go-tool/cmd/shatter
  ```

## Acceptance criteria

- [ ] The module path and directory agree: either the directory is renamed to `go-tool/`, or the module path, docs and Renovate regex all change to `.../shatter/shatter-go-tool`. Every reference (Taskfile `meta`, CI, `docs/distribution.md`, Renovate regex, README/QUICKSTART if they mention it) is updated in the same change.
- [ ] Versions resolve: `release.yml` pushes a `<module-dir>/<tag>` tag alongside each continuous/release tag, or the docs switch to pseudo-versions and say so.
- [ ] A CI job (in `task check` or the release workflow) creates a temp module and runs the exact documented `go get -tool ...@<ref>` followed by `go tool shatter --shatter-wrapper-help`. Proof at close: the URL of a green CI run in which that job executed, plus the job's output pasted into the close note.
- [ ] str-fl9g.2 carries a comment linking this issue as the fix (see the companion reopen-note draft `go-tool-reopen-note`).

## Suggested approach

Renaming the directory to `go-tool/` is the smallest change that keeps the published path in the docs. Add the nested-module tag push to `release.yml` next to the existing tag creation.

## Out of scope

- Wrapper robustness (atomic extract, HTTP timeouts, API caching): `go-tool-wrapper-robustness`.

## Dependencies

- Blocked by: none.
- Blocks: `go-tool-wrapper-robustness` (paths move).
- Related: str-fl9g.2 (closed; acceptance never passed), str-wnyzy (closed; CI Setup Go step and root go.mod).

## Size

S

## References

- Finding frontend-go-02 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-02). Old draft: `drafts/shatter-code/51-go-tool-module-path.md`.

---

<!-- file: 03-go-tool-reopen-note.md -->
---
slug: go-tool-reopen-note
kind: reopen-note
title: "NOTE on str-fl9g.2: documented `go get -tool .../go-tool/cmd/shatter` never resolved; fix tracked in go-tool-module-path"
priority: P1
type: note
labels: [go-tool, distribution, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [go-tool-module-path]
existing_id: str-fl9g.2
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# NOTE on str-fl9g.2 (closed): the acceptance command never resolved

Target: `str-fl9g.2` ("Go tool wrapper", closed, P1).

Action: add the comment below with `bd comments add str-fl9g.2`. Leave str-fl9g.2 closed; the work is carried by the new issue filed from `go-tool-module-path` (blocked_by lists it so the filer can substitute its real id into the comment). Do not re-close anything on the basis of this note.

## Comment text

> **Audit 2026-09-22 (finding frontend-go-02):** this issue was closed on "9dc76c85 landed on main", but its acceptance check ("a temp Go module can run `go get -tool` for the wrapper at a continuous tag and then `go tool shatter --version`") could never pass.
>
> - `shatter-go-tool/go.mod:1` declares `module github.com/shatterproof-ai/shatter/go-tool`, but the module lives in `shatter-go-tool/`; there is no `go-tool/` directory and no root `go.mod`, so the Go toolchain looks for `go-tool/` at the repo root.
> - `docs/distribution.md:64` (and the Renovate regex at :110) document `go get -tool github.com/shatterproof-ai/shatter/go-tool/cmd/shatter@...`.
> - Running it in a fresh temp module with `GOPROXY=direct` gives: `module github.com/shatterproof-ai/shatter@main found (v0.0.0-20260922164214-16794cef9e10), but does not contain package github.com/shatterproof-ai/shatter/go-tool/cmd/shatter`.
> - `release.yml` also pushes no `go-tool/<tag>` tags, which a nested module needs for `@continuous-...` to resolve.
>
> The fix (module path/directory alignment, nested-module tags, and a CI job that runs the documented command) is tracked in **<id of go-tool-module-path>**. Evidence: `audits/2026-09-22/areas/frontend-go.md` go-02.

---

<!-- file: 04-go-rune-and-escape-literals.md -->
---
slug: go-rune-and-escape-literals
kind: new
title: "Go rune/char literals are typed as string constants in both SymExpr builders; analyzer strings.Trim mangles escapes and quotes"
priority: P1
type: bug
labels: [go-frontend, solver, concolic, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go rune/char literals are typed as string constants in both SymExpr builders; analyzer strings.Trim mangles escapes and quotes

## Problem

In Go a rune literal (`'x'`) is an untyped rune constant (int32). Both Go SymExpr builders emit it as `{kind: const, type: "str"}`, so a comparison `c == 'x'` on a `rune`/`byte` parameter becomes `{param c} == {const str "x"}`: ill-typed for the solver and unreachable in both explorer modes.

Separately, the static analyzer converts STRING and CHAR literals with `strings.Trim(lit.Value, "`\"'")` instead of `strconv.Unquote`. That leaves escapes unconverted (`"a\tb"` becomes the four characters `a\tb`) and strips quote characters that belong to the value (`"'q'"` becomes `q`).

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go/protocol/analyzer.go:2351-2353` (static builder `litSymExpr` path): `case token.STRING, token.CHAR: val := strings.Trim(lit.Value, "`\"'")` then `Type: "str"`.
- The same `strings.Trim` pattern also appears as an Unquote-failure fallback at `analyzer.go:770`, `:2689`, `:2701`, `:2745` (literal harvesting); those are fallbacks after `strconv.Unquote` and are lower risk, but should be checked in the same change.
- `shatter-go/instrument/symextract.go:139-150` (runtime builder): STRING uses `strconv.Unquote` correctly, but `case token.CHAR:` at :145 also returns `&symExpr{Kind: "const", Type: "str", Value: s}`.
- `shatter-core/tests/e2e_concolic_go.rs` has no rune or string-escape known-answer case (`grep -n "rune\|'x'" ` finds none).
- Audit probe (finding frontend-go-03):
  ```go
  func Classify(s string, c rune) int {
      if s == "a\tb" { return 1 }
      if s == "'q'"  { return 2 }
      if c == 'x'    { return 3 }
      return 0
  }
  ```
  Analyze right-hand sides: `{"type":"str","value":"a\\tb"}`, `{"type":"str","value":"q"}`, `{"type":"str","value":"x"}`; runtime `branch_path` constraint for the rune compare: `{"kind":"const","type":"str","value":"x"}`. `shatter explore lit.go:Classify` default mode, 60 iterations: 6/7 lines, `return 3` never reached. `--concolic`, 200 iterations (stopped at 24): 5/7 lines, `return 1` and `return 3` missed. Both reported "3/3 branches".
- Unverified: why concolic also missed `"a\tb"`; the runtime constraint carries the correct tab, so the miss may be in the core's string encoding. Treat as a separate question (see Out of scope).

## Acceptance criteria

- [ ] CHAR literals emit `{kind: const, type: "int", value: <codepoint>}` in both the analyzer builder and the instrument builder.
- [ ] The analyzer uses `strconv.Unquote` for STRING and CHAR literals (falling back to `unknown`, not to a trimmed raw string, on error).
- [ ] rapid property in shatter-go: for a random printable-or-escaped string `s`, the analyzer's literal SymExpr for the parsed literal `strconv.Quote(s)` has `Value == s`; and for a random rune `r`, the literal SymExpr for `strconv.QuoteRune(r)` is `{type:int, value:int(r)}` in both builders.
- [ ] `shatter-core/tests/e2e_concolic_go.rs` gains known-answer cases (fixture under `examples/go/`) for an escaped-string compare, a quote-containing string compare, and rune and byte compares; each expected arm is reached. The new cases fail on current main and pass on the branch; record the failing output in the close note.
- [ ] `cargo test --test e2e_concolic_go` passes; `task affected` passes with `Gates selected` recorded.

## Suggested approach

Small targeted fix in the two literal switch arms plus the tests. Do not wait for builder unification; that stays in str-qwua7.35 (see the `qwua7-35-four-go-builders` note). If the `"a\tb"` concolic miss persists after the analyzer fix, file it against the core string encoding with the probe above.

## Out of scope

- Unifying the four Go SymExpr builders (str-qwua7.35) and the SymExpr construction spec (str-qwua7.37).
- The branch metric reporting "3/3 branches" while arms are missed (engine-correctness bucket).
- Root-causing the concolic `"a\tb"` miss if it survives this fix.

## Dependencies

- Blocked by: none.
- Related: str-qwua7.35 (builder unification), str-qwua7.37 (SymExpr spec).

## Size

S

## References

- Finding frontend-go-03 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-03). Old draft: `drafts/shatter-code/52-go-rune-and-escape-literals.md`.

---

<!-- file: 05-qwua7-35-four-go-builders.md -->
---
slug: qwua7-35-four-go-builders
kind: note-to-existing
title: "NOTE on str-qwua7.35: four Go SymExpr builders; instrument/flow*.go is unreachable although CLAUDE.md names it as the ite mechanism"
priority: P2
type: note
labels: [go-frontend, symexpr, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.35
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# NOTE on str-qwua7.35: four Go SymExpr builders

Target: `str-qwua7.35` ("shatter-go: unify analyzer and instrument SymExpr builders over one shared type", open, P2).

Action: add the comment below with `bd comments add str-qwua7.35`. Priority stays P2. No new issue.

## Comment text

> **Audit 2026-09-22 note (finding frontend-go-04).** Re-verified at commit 56c86168. The scope of this issue is larger than two builders:
>
> **There are four Go SymExpr builders**
> 1. `shatter-go/protocol/analyzer.go:2171` `buildSymExpr` (static).
> 2. `shatter-go/protocol/analyzer.go:2201` `buildSymExprWithFlow` (static, flow-aware), with its own `analyzerFlowMap` (`analyzer.go:1963`) and walker; re-implemented in `protocol/` by commit 34eb92c8.
> 3. `shatter-go/instrument/symextract.go` `exprToSymExpr(WithFlow)` over a private `symExpr` (runtime).
> 4. `shatter-go/protocol/loop_body_states.go:294` `buildGoLoopSnapshotSymExpr`. It resolves params before flow (the analyzer does the reverse), handles only Ident/BasicLit/Binary/unary `-`/Paren, and keeps stale flow values after unsupported compound assignments such as `%=` or `<<=` (`visitGoLoopSnapshotAssign`, loop_body_states.go:227). Whether the ordering difference changes results is unverified.
>
> **The runtime builder never uses flow.** Instrument always calls `extractConstraint` with no flow map: `instrument/visitor.go:334, 350, 440` and `instrument/mcdc.go:81, 93`. `deadcode ./...` in shatter-go reports every function in `instrument/flow.go` (`snapshot`, `mergeFlowMaps`, `symExprsEqual`) and `instrument/flowwalk.go` (`walkStmtsForFlow`, `applyStmtToFlow`, `applyAssignToFlow`, `applyDeclToFlow`, `applyIfToFlow`, `flowLHSName`) as unreachable. Consequence: analyze reports `ite` conditions for flow-tracked locals while the runtime `branch_path` constraint for the same branch is `unknown`, so the solver cannot negate it.
>
> **Docs are wrong.** `shatter-go/CLAUDE.md:57-58` ("Ite SymExpr Parity Contract") lists `instrument/flow.go` and `instrument/flowwalk.go` as steps 1-2 of the ite mechanism.
>
> **Proposed additions to this issue's acceptance criteria**
> - Delete `instrument/flow.go` and `instrument/flowwalk.go`; keep one flow-aware builder with a param-resolver hook in a leaf package (e.g. `shatter-go/symexpr`) used by the analyzer, instrument and loop snapshots.
> - Thread the flow map into instrument's `transformIfStmt` so runtime constraints for flow-tracked locals match the static ones.
> - rapid property: for generated if-chains over params and flow-tracked locals, the static branch condition equals the runtime constraint.
> - Fix the CLAUDE.md ite section.
>
> The rune/escape literal bug (CHAR typed as `str`, analyzer `strings.Trim`) is filed separately as a small fix ahead of this refactor: **<id of go-rune-and-escape-literals>**. The dead-code sweep (**<id of go-dead-code-and-property-targets>**) leaves `instrument/flow*.go` to this issue.

---

<!-- file: 06-go-config-discovery-unbounded.md -->
---
slug: go-config-discovery-unbounded
kind: new
title: "Go config loader walks up to / (stray /tmp/.shatter breaks tests; ancestor configs can widen policy.allow) and warns on the `defaults` key that `shatter init` writes"
priority: P2
type: bug
labels: [go-frontend, config, security, tests, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go config loader walks up to / (stray /tmp/.shatter breaks tests; ancestor configs can widen policy.allow) and warns on the `defaults` key that `shatter init` writes

## Problem

Two Go-frontend config bugs:

1. **Unbounded discovery.** `findConfigFile` searches every ancestor directory up to the filesystem root for `.shatter/config.yaml`. Any config above the project is loaded. A stray `/tmp/.shatter/config.yaml` (left by an implicit `shatter init` run with cwd `/tmp`) makes `TestLoad_MissingFile_ReturnsZeroFile` fail for everyone whose `t.TempDir()` is under `/tmp`; this is the root cause of open str-k7czv. Worse, an ancestor config's `functions.<glob>.policy.allow` (and its mocks/receivers) silently applies to every project beneath it, widening the side-effect safety policy.
2. **Init-generated key rejected.** `knownTopLevelKeys` contains only `functions` and `go_runtime_values`, so the top-level `defaults:` that `shatter init` writes produces an "ignoring unknown top-level key" warning in every Go run.

This is the Go counterpart of open str-dl2pj (P1: core `discover_configs` walks past shared system dirs such as `/tmp` with no boundary). The two should end up with the same boundary rule.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go/config/loader.go:228-249` `findConfigFile`: loops `dir = filepath.Dir(dir)` until `parent == dir` (the root); no module, workspace or VCS stop.
- `shatter-go/config/loader.go:414-417` `knownTopLevelKeys = {functions, go_runtime_values}`. (`defaults` exists only in `knownFunctionKeys` at :420, the per-function key set.)
- `/tmp/.shatter/config.yaml` still exists on the audit host; its header reads "Generated by `shatter init`" and it has a top-level `defaults:`.
- `cd shatter-go && go test ./config/ -run TestLoad_MissingFile_ReturnsZeroFile -count=1` fails (audit gates log `audits/2026-09-22/gates/go-test-rerun.log`, reproduced 2/2):
  `loader_test.go:668: expected empty File, got {... Warnings:[config /tmp/.shatter/config.yaml: ignoring unknown top-level key "defaults"]}`. The test is at `config/loader_test.go:656`. Setting `TMPDIR` to another dir under `/tmp` does not help; `TMPDIR=/var/tmp/...` passes.
- str-k7czv (open, P3) records this failure with no root cause. str-dl2pj (open, P1) is the core/CLI equivalent. str-uk8y (single config-resolution entrypoint) and str-rs822 (CLI scan walk-up duplication) are related.

## Acceptance criteria

- [ ] `findConfigFile` stops at a documented boundary: the nearest directory containing `go.mod`, `go.work` or `.git` (or the rule str-dl2pj adopts for core; the two must match and the chosen rule is written in `shatter-go/CLAUDE.md` and the config docs).
- [ ] The resolved config path (or "none found, stopped at <dir>") is logged at debug level.
- [ ] `TestLoad_MissingFile_ReturnsZeroFile` is hermetic (explicit stop dir or its own boundary marker) and passes with a `/tmp/.shatter/config.yaml` present. Proof at close: run it once with a planted `/tmp/.shatter/config.yaml` on main (fails) and on the branch (passes); paste both outputs.
- [ ] New test: a config in an ancestor above a project's VCS root, granting `policy.allow`, is not applied to that project.
- [ ] `defaults` and every other top-level key `shatter init` writes are accepted without warning. New cross-frontend test: generate `shatter init` output and load it in each frontend's config loader, asserting zero warnings.
- [ ] When this lands, str-k7czv is closed as a duplicate with a comment pointing here; str-dl2pj gets a comment linking this issue.
- [ ] `task affected` passes with `Gates selected` recorded.

## Suggested approach

Add a stop-boundary check to the loop in `findConfigFile` (return "" when the current dir holds a boundary marker and has no config), add `defaults` to `knownTopLevelKeys`, and derive the known-key list from the init template if practical so it cannot drift again. Coordinate the boundary rule with whoever takes str-dl2pj.

## Out of scope

- Finding which process ran `shatter init` in `/tmp` and changing implicit-init behaviour (str-qwua7.58).
- The core/CLI discovery fix itself (str-dl2pj).

## Dependencies

- Blocked by: none.
- Related: str-k7czv (duplicate; close when this lands), str-dl2pj (core counterpart), str-uk8y, str-rs822, str-qwua7.58.

## Size

S-M

## References

- Findings gates-03 and frontend-go-07 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-07). Old draft: `drafts/shatter-code/05-go-config-discovery-unbounded.md`.

---

<!-- file: 07-go-dead-code-and-property-targets.md -->
---
slug: go-dead-code-and-property-targets
kind: new
title: "~57 unreachable shatter-go functions incl. the whole reconstruct package; add a deadcode gate and re-scope str-qwua7.48 property tests to live generators"
priority: P2
type: chore
labels: [go-frontend, cleanup, testing, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# ~57 unreachable shatter-go functions incl. the whole reconstruct package; add a deadcode gate and re-scope str-qwua7.48 property tests to live generators

## Problem

`deadcode ./...` reports 57 unreachable production functions in shatter-go, including the entire `reconstruct` package, which has no importers. Nothing gates new dead code. Meanwhile open str-qwua7.48 asks for new rapid property tests *for* `reconstruct` (dead) while the live code generators that build Go source by string concatenation (wrapper, launcher) and the config matcher have no property tests, and `setup/` has no tests at all.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `cd shatter-go && deadcode ./...` prints 57 lines. Selected:
  - `reconstruct/reconstruct.go:18 Value`, `:113 Inputs`, `:122 toInt64`, `:136 errorString.Error`. `grep -rn 'shatter-go/reconstruct"' --include=*.go shatter-go` finds no importer; `shatter-go/Taskfile.yml:13` keeps a `go build ./...` partly to compile such packages; `shatter-go/CLAUDE.md` calls it "historical, no current callers".
  - `loader/legal_anchor.go:21 LegalAnchor`, `:65 LauncherPackagePath`, `:93`, `:97`.
  - `launcher/session.go:136 OpenSession` (used only by tests).
  - `planner/aggregate.go:51 PlanAggregate`, `planner/classify.go:42 Classify`, `planner/plan.go:129 ResolveMockSpecs` (`shatter-go/CLAUDE.md:273` claims "The planner still emits ... via `planner.ResolveMockSpecs`").
  - `workspace/run.go:59 Workspace.NewRun`, `wrapper/wrapper.go:1548 BuildWrapperTargets`, `protocol/analyzer.go:189 AnalyzeFile`, `protocol/handler.go:99 NewHandler`.
  - `instrument/flow.go` and `instrument/flowwalk.go` (9 functions): owned by str-qwua7.35, see below.
- Property-test census (non-test lines / rapid files): wrapper 3219 / 1 (error_sentinel only); launcher 1114 / 0; config 524 / 0; setup 158 / no test files; reconstruct 136 / 0 (dead).
- `wrapper.GenerateWrapper` (`wrapper/wrapper.go:290`) and `launcher.GenerateLauncherMain` / `GenerateHarnessLauncherMain` (`launcher/launcher.go:666, 716`) emit Go source by string building; no property asserts the output parses.

## Acceptance criteria

- [ ] Each function in the deadcode report is deleted, or moved to `_test.go` / `internal/testutil` if tests need it, case by case. `reconstruct/` is deleted. `shatter-go/CLAUDE.md` claims about removed code (e.g. `ResolveMockSpecs` at :273, reconstruct) are corrected.
- [ ] Exceptions: `instrument/flow*.go` stays until str-qwua7.35 removes it; `planner/classify.go Classify` is not deleted until `go-cgo-refusal-covers-bodies` decides whether cgo refusal is wired through it. Both are listed in the allowlist with the owning issue id.
- [ ] A `deadcode` check with a checked-in allowlist runs in `task meta` (or `check-static`) and fails on new unreachable production functions. Proof at close: the gate output from a forced (non-cached) run, and a demonstration that adding an unused exported function makes it fail.
- [ ] str-qwua7.48 is re-scoped by the comment below (filer posts it; the implementer of str-qwua7.48 does the tests, not this issue).
- [ ] `go test ./...` in shatter-go, `cargo test --test e2e_concolic_go`, and `task affected` pass (`Gates selected` recorded).

## Comment for str-qwua7.48 (post with `bd comments add str-qwua7.48`)

> **Audit 2026-09-22 (findings frontend-go-06, frontend-go-11):** `reconstruct/` is unreachable production code (no importers; `deadcode ./...` lists all of it) and is being deleted by **<id of go-dead-code-and-property-targets>**. Please re-scope this issue to live code: (a) rapid properties that `wrapper.GenerateWrapper` and `launcher.GenerateLauncherMain`/`GenerateHarnessLauncherMain` output parses (`go/parser`) and is gofmt-stable for random param/receiver/generic shapes (a compile check can go in a slow tier); (b) config `MatchTarget` determinism under map iteration and "anchored beats fallback" (the str-cl19s tie-break); (c) a basic `setup/loader_test.go`. Drop reconstruct from the acceptance criteria.

## Suggested approach

Run `deadcode ./...`, walk the list package by package, and keep each deletion in its own commit so reviewers can check test-only users. Wire the check as a small script that diffs `deadcode` output against `shatter-go/.deadcode-allow`.

## Out of scope

- Writing the str-qwua7.48 property tests.
- golangci-lint gating and gofmt drift (the audit's separate Go-lint issue).
- The four-builder unification (str-qwua7.35).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.48 (re-scoped by this issue's comment), str-qwua7.35, `go-cgo-refusal-covers-bodies` (decides `planner.Classify`).

## Size

M

## References

- Findings frontend-go-06 and frontend-go-11 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-06, go-13). Old draft: `drafts/shatter-code/54-go-dead-code-and-property-targets.md`.

---

<!-- file: 08-go-tool-wrapper-robustness.md -->
---
slug: go-tool-wrapper-robustness
kind: new
title: "shatter-go-tool: non-atomic extract can cache a truncated binary; no HTTP timeouts; GitHub API call on every run; tests cover only arg parsing"
priority: P2
type: bug
labels: [go-tool, distribution, installer, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [go-tool-module-path]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-go-tool: non-atomic extract can cache a truncated binary; no HTTP timeouts; GitHub API call on every run; tests cover only arg parsing

## Problem

The `go tool shatter` wrapper downloads and caches the shatter release binary. It can permanently cache a partial binary, can hang forever on a stalled connection, and hits the GitHub API on every invocation.

- **Non-atomic extract.** The binary is extracted straight into its final cache path with mode 0755. An interrupted extract or two concurrent first runs leave a truncated executable; the cache check only tests mode bits, so every later run execs the corrupt file. The SHA-256 check covers the archive, not the extracted binary.
- **No timeouts.** Both HTTP calls use the default client with no timeout.
- **API call before cache.** With `SHATTER_BUILD` unset, the latest continuous build is resolved via `api.github.com` before the cache is consulted, on every run; unauthenticated users get 60 requests/hour, so CI loops get rate-limited.
- **Token only on the API call.** The API request sends `GITHUB_TOKEN`; the asset download does not (matters for private repos or authenticated rate limits).
- **Tests.** `main_test.go` covers only `parseArgs`.

## Evidence

Re-verified against the audit worktree at commit 56c86168 (`shatter-go-tool/cmd/shatter/main.go`, 355 lines):

- `:315 extractBinary`; `:340` `os.OpenFile(targetBinary, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o755)` then copies directly.
- `:352 isExecutable` checks mode bits only; used as the cache hit test at `:160`.
- `:142` `latestContinuousBuild(repo)` runs before the `:160` cache check; `:195` issues the API request.
- `:256 getJSON` sets the `Authorization` header from `GITHUB_TOKEN` at `:263` and calls `http.DefaultClient.Do` at `:267` (no timeout).
- `:278 downloadFile` calls `http.Get(url)` at `:279` (no timeout, no token).
- `shatter-go-tool/cmd/shatter/main_test.go`: 39 lines, `parseArgs` only.
- Reference pattern for atomic writes: `shatter-go/launcher/launcher.go:336-350` builds to a temp path and `os.Rename`s (`:350`).

## Acceptance criteria

- [ ] Extraction writes to a temp file in the target directory, is fsynced and chmodded, then `os.Rename`d into place. If the release manifest carries a per-binary hash, it is verified before the rename.
- [ ] Both HTTP calls use an `http.Client` with connect and overall timeouts; `downloadFile` sends `GITHUB_TOKEN` when set.
- [ ] The resolved latest-continuous tag is cached with a TTL; when the API is unreachable or rate-limited, the newest cached build is used with a warning.
- [ ] `httptest`-backed tests, each failing on current main where applicable and passing on the branch: manifest selection, checksum mismatch rejected, a truncated cached binary from an interrupted extract is not executed and is replaced, API unavailable falls back to cache, a stalled server times out.
- [ ] `task meta` (which runs `go test ./...` in the wrapper module) passes; `task affected` `Gates selected` recorded.

## Suggested approach

Mirror the launcher's temp-then-rename; wrap the client in a small struct so tests can point it at `httptest.NewServer`. Store the TTL-cached tag in a small JSON file next to the cached binaries.

## Out of scope

- The module path/directory mismatch (`go-tool-module-path`), which lands first and may move this file.

## Dependencies

- Blocked by: `go-tool-module-path` (directory/module rename touches the same files).
- Related: str-fl9g.2 (closed; original wrapper).

## Size

M

## References

- Finding frontend-go-10 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-10). Verifier correction applied: the API request does read `GITHUB_TOKEN`; only the download ignores it. Old draft: `drafts/shatter-code/55-go-tool-wrapper-robustness.md`.

---

<!-- file: 09-go-build-timeout-ignored.md -->
---
slug: go-build-timeout-ignored
kind: new
title: "Go frontend ignores --build-timeout / SHATTER_BUILD_TIMEOUT (go build has no timeout) and SHATTER_HARNESS_RELEASE"
priority: P2
type: bug
labels: [go-frontend, timeouts, parity, cli, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go frontend ignores --build-timeout / SHATTER_BUILD_TIMEOUT (go build has no timeout) and SHATTER_HARNESS_RELEASE

## Problem

The CLI exports `SHATTER_BUILD_TIMEOUT` (from `--build-timeout`, default 30) and, when release harnesses are requested, `SHATTER_HARNESS_RELEASE=1` to every frontend. `--help` promises "Build timeout in seconds for compiling instrumented code in the frontend. Default: 30s". The Go frontend reads neither variable, and its `go build` invocations have no context or deadline. A hung Go build is bounded only by the core's per-request timeout, which kills the whole frontend session instead of failing the one target with a build-phase timeout.

The Go frontend used to honour it: `instrument/executor.go` had a `buildTimeout()` reading `SHATTER_BUILD_TIMEOUT` (str-9smo extracted its default into a const). It was removed with the legacy direct-call harness in commit 9c39ad94 (str-kzxt, "trim legacy direct-call harness from executor.go"), and the replacement build path (`build.Builder` + `launcher`) never re-added it.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-cli/src/helpers.rs:750-760` pushes `SHATTER_EXEC_TIMEOUT`, `SHATTER_BUILD_TIMEOUT`, and (if `release`) `SHATTER_HARNESS_RELEASE=1` into the frontend env. `shatter-cli/src/args.rs:574-577` defines `--build-timeout` with the help text above.
- `grep -rn "BUILD_TIMEOUT\|HARNESS_RELEASE" --include=*.go shatter-go` returns nothing.
- Go build sites: `shatter-go/launcher/launcher.go:342` `exec.Command("go", buildArgs...)` and `shatter-go/setup/loader.go:63` `exec.Command("go", "build", ...)`; no `exec.CommandContext` anywhere in those files.
- Only other reader: `shatter-rust/src/executor.rs:2967`, `:3135`, `:5404` read `SHATTER_BUILD_TIMEOUT` (fallback default 120s); `executor.rs:1020-1022` reads `SHATTER_HARNESS_RELEASE`. The CLI's own `build_frontend.rs:384`, `:553` also reads `SHATTER_HARNESS_RELEASE`, so ignoring it in Go may be intentional (verifier note).
- `git log -S SHATTER_BUILD_TIMEOUT -- shatter-go` shows the reader removed in 9c39ad94.

## Acceptance criteria

- [ ] Both Go build sites run under `exec.CommandContext` with a deadline from `SHATTER_BUILD_TIMEOUT` (seconds; same parsing/default rules as the Rust frontend, documented), and the process group is killed on expiry.
- [ ] Expiry is reported for that target as outcome `timed_out` with a build-phase reason that names `--build-timeout`; the session stays usable for the next request.
- [ ] Test, failing on current main and passing on the branch: with a fake `go` on `PATH` (or a toolexec shim) that sleeps past a 1-2 s `SHATTER_BUILD_TIMEOUT`, the build is killed within the timeout plus a small margin and the outcome is `timed_out`. Record the failing run in the close note.
- [ ] `SHATTER_HARNESS_RELEASE` is either honoured by the Go frontend (e.g. build flags that matter for Go) or declared Rust-only: `shatter-go/CLAUDE.md`, the `--help` text for the flag that sets it, and `protocol/parity-matrix.yaml` say so.
- [ ] The per-frontend env contract for both variables is recorded in `protocol/parity-matrix.yaml` (or wherever str-qwua7.20.2 puts the env table), and `task parity` passes.
- [ ] `cargo test --test e2e_concolic_go` and `task affected` pass (`Gates selected` recorded).

## Suggested approach

Add a `buildTimeout()` helper in a shared Go package (the launcher already owns the build), thread a `context.Context` into the launcher build and `setup/loader.go`, and use `cmd.SysProcAttr` with `Setpgid` plus a group kill so child `compile`/`link` processes die too. For the env contract, a small table test that every `SHATTER_*` variable the CLI exports is either read by each frontend or listed as declared-ignored would catch the next drift.

## Out of scope

- The Rust timeout-budget inversion (request timeout not larger than build timeout); tracked by the audit's Rust/timeout issue.
- Documenting every `SHATTER_*` variable (str-qwua7.20.2).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.20.2 (env-var table), str-9smo (closed; original Go build-timeout const), str-kzxt (closed; removal).

## Size

S-M

## References

- Finding frontend-go-08 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-08). No earlier draft.

---

<!-- file: 10-go-connection-failures-divergence.md -->
---
slug: go-connection-failures-divergence
kind: new
title: "Go frontend never emits connection_failures or runtime_crypto_boundaries, so LiveFirst fallback never fires for Go; no divergence recorded"
priority: P2
type: bug
labels: [go-frontend, parity, protocol, mocking, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go frontend never emits connection_failures or runtime_crypto_boundaries, so LiveFirst fallback never fires for Go; no divergence recorded

## Problem

The core's LiveFirst mock policy (try the live dependency, fall back to a mock when it is unreachable) is driven solely by `connection_failures` in the execute result. The Go frontend's `Response` type has neither `connection_failures` nor `runtime_crypto_boundaries`, and no Go code produces them, so for Go targets LiveFirst can never fall back and runtime crypto boundary splitting never happens. The parity matrix records this gap only for Rust; its Rust entry says vaguely that "TypeScript and Go emit the subset each supports", which hides the Go gap.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `git grep -n -i "connection_failures\|ConnectionFailures\|runtime_crypto_boundaries\|RuntimeCryptoBoundaries" -- shatter-go ':!*_test.go'` returns nothing.
- `shatter-go/protocol/types.go:206-267` `type Response struct` has no such fields.
- `shatter-core/src/explorer.rs:891` `update_live_first_states` is driven only by `result.connection_failures`; called at `explorer.rs:1602` (random explorer) and `:2545`, and at `shatter-core/src/orchestrator.rs:3185` (concolic).
- `protocol/parity-matrix.yaml:1172-1190` `rust-execute-response-fields-partial` is the only record; there is no Go entry.
- Related tracker state: str-2fjn (open) notes the missing Go type fields only as registry-vs-types drift; str-924ca (open) is the Rust counterpart; str-3ky9.9.2 is the TS reference implementation (dial / connection-refused classification).

## Acceptance criteria

Implement (a), which is preferred, or record (b):

- [ ] (a) Go mocks/adapters detect connection failures (dial errors, connection refused, DNS failure on outbound calls; mirroring the TS classification) and report them in `connection_failures`; the Go `Response` carries both fields with the core's shapes. A Go E2E or conformance case with an unreachable live dependency under LiveFirst shows the fallback to a mock on the next iteration; the case fails on current main and passes on the branch.
- [ ] (b) Otherwise, a `go-execute-response-fields-partial` divergence entry in `protocol/parity-matrix.yaml` naming both fields and the LiveFirst consequence, with its own tracking issue, mirrored in `shatter-go/CLAUDE.md` per the matrix rules; the Rust entry's "TypeScript and Go emit the subset each supports" sentence is corrected.
- [ ] Either way: `task parity` and `task conformance` pass, and `task affected` passes with `Gates selected` recorded.

## Suggested approach

Look at how the TS frontend classifies connection failures (str-3ky9.9.2) and at where the Go frontend intercepts outbound calls for side-effect capture; add the classification there. `runtime_crypto_boundaries` can be declared as a divergence even if connection failures are implemented.

## Out of scope

- The Rust counterpart (str-924ca).
- The registry-vs-implementation cross-validation (str-2fjn).

## Dependencies

- Blocked by: none.
- Related: str-2fjn, str-924ca, str-3ky9.9.2.

## Size

M for (a), S for (b)

## References

- Finding protocol-parity-11 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/protocol-parity.md`). Old draft: `drafts/shatter-docs-ui/27-go-connection-failures-divergence.md`.

---

<!-- file: 11-go-explore-warmup-gate.md -->
---
slug: go-explore-warmup-gate
kind: new
title: "Explore lacks the cold-build warmup gate that scan has; many-target Go explores on a cold cache can time out en masse"
priority: P3
type: bug
labels: [go-frontend, explore, parity, performance, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Explore lacks the cold-build warmup gate that scan has; many-target Go explores on a cold cache can time out en masse

## Problem

str-tbk9e fixed a cold-cache thundering herd for **scan** by adding `BuildWarmupGate`: the first harness build of a run goes alone, then the other workers fan out once the build cache is warm. **Explore** never got the gate. A multi-target Go explore at default parallelism starts all workers on a cold `GOCACHE`, each triggers a full build at once, and under load they all hit the 30 s request timeout.

This is a parity gap between the scan and explore paths (see the "parallel parity" rule in the root CLAUDE.md). The user-facing failure was seen only on a heavily loaded host, so it is P3 until it is reproduced on a quiet one.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `grep -rn "BuildWarmupGate\|warmup_gate" shatter-core/src shatter-cli/src` finds hits only in `shatter-core/src/scan_orchestrator.rs` (struct at `:2138`, created at `:3870`, entered at `:3300`); none on the explore path (`shatter-cli/src/commands/explore.rs`, `shatter-core/src/explorer.rs`, `orchestrator.rs`).
- str-tbk9e (closed, P1) close reason says it serialises the first build "of a scan".
- Audit run (finding goals-12, `audits/2026-09-22/goals-runs/go-all-default.err`), host load average 150-200: `Spawned 1 frontend session(s) for 18 target(s) (16 parallel worker(s))`, then `Error: explore: all 18 attempted target(s) failed (build_failed=0, runtime_failed=0, timed_out=43)`, each `request timed out after 30s`. The same explore with `-w 2 --request-timeout 180` completed. The verifier downgraded to P3: the load confounds the timeouts and it was not reproduced on a quiet host.

## Acceptance criteria

- [ ] Explore uses the same warmup gate as scan (move `BuildWarmupGate` to a shared module and use it in both the random explorer and concolic orchestrator paths), or explore caps Go parallelism until the first harness build completes.
- [ ] Test: an E2E (Go) that explores several targets across multiple Go files with an empty `GOCACHE` and default parallelism succeeds. Run it on a quiet host (load average below the core count) and record the load and result in the close note; also record whether the pre-fix binary fails the same test on that host.
- [ ] `cargo test --test e2e_concolic_go` and `task affected` pass (`Gates selected` recorded).

## Suggested approach

Lift `BuildWarmupGate` and `WarmupLeaderGuard` out of `scan_orchestrator.rs` into a shared module; have the explore worker pool `enter()` the gate before a target's first execute. Check both explorer paths (random and concolic) per the parallel-parity rule.

## Out of scope

- Request/build timeout budgeting (tracked separately by the audit's timeout-budget issue).

## Dependencies

- Blocked by: none.
- Related: str-tbk9e (closed; scan-only fix).

## Size

S

## References

- Finding goals-12 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/goals.md`). Old draft: `drafts/shatter-code/81-go-explore-warmup-gate.md`.

---

<!-- file: 12-go-small-correctness-tidy.md -->
---
slug: go-small-correctness-tidy
kind: new
title: "Go frontend small fixes: line-0 records on every execution, generated mock code ignores Unmarshal errors and embeds JSON in backtick raw strings, dead assignments, unchecked lock-PID writes"
priority: P3
type: bug
labels: [go-frontend, cleanup, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go frontend small fixes: line-0 records on every execution, generated mock code ignores Unmarshal errors and embeds JSON in backtick raw strings, dead assignments, unchecked lock-PID writes

## Problem

Grouped small Go-frontend defects, each independently fixable:

1. **Phantom line-0 records.** The instrumenter emits a line-record call for every statement, including the synthetic `call_enter`/`call_exit` statements it prepends, whose position resolves to line 0. Only the denominator is guarded, so every execution reports `lines_executed: [0, 0, ...]` and consumers must filter.
2. **Generated mock code.** The mock harness generator embeds the mock return-value JSON inside a Go raw string (backticks). `json.Marshal` does not escape a backtick, so a mock value containing one breaks compilation of the harness. The generated code also ignores `json.Unmarshal` errors, so a malformed value silently becomes a zero value.
3. **Dead assignments** that hide intent: `_ = anchorImport`, `_ = fresh`.
4. **Unchecked lock-file PID writes**: `_, _ = fmt.Fprintf(lockFile, ...)`. `lockIsStale` reads the PID back and falls back to a ModTime timeout when it cannot, so a failed write silently degrades stale-lock detection.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go/instrument/visitor.go:131-139`: `newList = append(newList, makeLineRecordCall(line))` at :132 is unconditional; the `if line > 0` guard at :137 covers only `instrumentableLines`, and the comment there says synthetic statements resolve to line 0. Audit explore artifact: `"lines_executed": [0, 0, 4, 7, 10, 13]`.
- `shatter-go/instrument/executor.go:225-231`: `retValsJSON, _ := json.Marshal(...)`, then line 231 emits the Go source ``json.Unmarshal([]byte(`<json>`), &vals)`` via `fmt.Fprintf` (JSON inside a backtick raw string; the Unmarshal result is unchecked); `:299`: `b.WriteString("\t\tjson.Unmarshal(retvals[idx], &retVal)\n")` (unchecked). Backtick breakage is by reasoning, not executed.
- `shatter-go/launcher/launcher.go:391` `_ = anchorImport`; `shatter-go/build/builder.go:277` `_ = fresh`.
- `shatter-go/build/builder.go:188` and `shatter-go/launcher/launcher.go:616`: `_, _ = fmt.Fprintf(lockFile, "%d\n", os.Getpid())`.
- Prior-audit rec 13 (2026-09-04) named items 3-4 and was never filed. str-qo1.12 (closed) fixed a different line-coverage gap; str-qwua7.32 (closed) covered errcheck on frontend code, not generated code.

## Acceptance criteria

- [ ] No line-record call is emitted for `line <= 0`; a `visitor_test.go` assertion checks that no emitted record has line 0 (fails before, passes after).
- [ ] Generated mock code embeds the JSON via `strconv.Quote` (interpreted string literal) and checks both `json.Unmarshal` results (panic with a clear message or report through the harness error channel). A test generates, compiles and runs a mock whose return value contains a backtick; it fails before the fix and passes after.
- [ ] `_ = anchorImport` and `_ = fresh` are removed (or the variables are used for what they were meant for).
- [ ] Lock-file PID write errors are handled (returned or logged, with the existing ModTime fallback kept).
- [ ] `go test ./...` in shatter-go, `cargo test --test e2e_concolic_go`, and `task affected` pass (`Gates selected` recorded).

## Suggested approach

Four small commits, one per item.

## Out of scope

- Splitting the large `protocol` package (prior audit P2; not filed here).
- golangci-lint gating (the audit's Go-lint issue).

## Dependencies

- Blocked by: none.
- Related: str-qo1.12, str-qwua7.32.

## Size

S

## References

- Findings frontend-go-14, frontend-go-15, prior-22 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-12, go-14 and `areas/prior-audit-regress.md`). Old draft: `drafts/shatter-code/56-go-small-correctness-tidy.md`.

---

<!-- file: 13-go-cgo-refusal-covers-bodies.md -->
---
slug: go-cgo-refusal-covers-bodies
kind: new
title: "Go cgo \"detect and refuse\" is signature-only, and its only consumer (planner.Classify) is unreachable: verify, then refuse body C.* calls or narrow the claim"
priority: P3
type: bug
labels: [go-frontend, docs, scope-limits, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go cgo "detect and refuse" is signature-only, and its only consumer (planner.Classify) is unreachable: verify, then refuse body C.* calls or narrow the claim

## Problem

`docs/go-frontend-scope-limits.md` says cgo is a permanent non-goal and that "The Go frontend will detect and refuse cgo-bearing functions at analysis time rather than attempt a partial model", because Z3 cannot reason across the C ABI and analysing around cgo calls would produce unsound tests or misleading coverage.

The detection only inspects parameter and result types for the `C` pseudo-package. A function with a Go-typed signature that calls `C.foo()` in its body is not flagged. Re-verification for this draft found a second gap: the flag it sets (`HasCGoDep`) is read only by `planner.Classify`, which `deadcode` reports as unreachable, and the Go frontend never emits the `cgo_dependency` unsatisfied-requirement kind in production code. So it is possible that no cgo refusal happens at all, even for signature-level cgo. That is inferred from code reading and has not been checked by execution.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `docs/go-frontend-scope-limits.md:36-40` (cgo section): the "detect and refuse" claim.
- `shatter-go/protocol/discovered_target.go:100-125` `fnHasCGoDep` checks only `fn.Type.Params` and `fn.Type.Results`; it sets `HasCGoDep` at `:201`.
- `HasCGoDep` has one non-test reader: `shatter-go/planner/classify.go:54` inside `Classify` (`:42`). `cd shatter-go && deadcode ./...` lists `planner/classify.go:42:6: unreachable func: Classify`.
- `shatter-go/protocol/invocation_plan.go:198` declares `UnsatisfiedRequirementKindCGODependency = "cgo_dependency"`; no non-test code emits it. The core accepts it (`shatter-core/src/protocol.rs:313`) and `protocol/parity-matrix.yaml:756` lists it among `UnsatisfiedRequirementKind` variants the Go frontend emits.
- Audit finding frontend-go-13 (code reading; body-call case not reproduced). str-hy9b.H5 (closed) wrote the scope-limits doc.

## Acceptance criteria

- [ ] First, record the actual behaviour: a probe file with (i) a function whose parameter type is `C.int` and (ii) a Go-typed function whose body calls `C.puts`, run through `shatter analyze` and `shatter explore`; paste the outcomes in the issue.
- [ ] Then either:
  - (a) the Go frontend refuses both shapes: body selectors resolving to the `"C"` package are detected (via `types.Info.Uses` → `*types.PkgName` with path `"C"`, or by flagging every function in a file that imports `"C"`), and the refusal actually reaches the output as `cgo_dependency` (skipped/unsupported with that reason), with tests for both shapes that fail before and pass after; or
  - (b) the claim is narrowed: `docs/go-frontend-scope-limits.md`, `shatter-go/CLAUDE.md` and the `parity-matrix.yaml` notes state exactly what is detected and what happens to undetected cgo functions.
- [ ] If (a) re-uses `planner.Classify`, it is wired into the production path (and removed from the deadcode allowlist in `go-dead-code-and-property-targets`); if not, that issue is told `Classify` can be deleted.
- [ ] `task parity` passes if the matrix changes; `task affected` passes with `Gates selected` recorded.

## Suggested approach

(a) is preferred because the doc's rationale (unsound tests) applies to body calls as much as to signatures. The file-level "imports C" rule is simplest and conservative. Put the refusal where other unsupported-target reasons are produced in the live path, not in the dead planner code, unless the planner is being revived for other reasons.

## Out of scope

- Any cgo support or modelling.

## Dependencies

- Blocked by: none.
- Related: str-hy9b.H5 (closed; wrote the doc), `go-dead-code-and-property-targets` (holds `planner.Classify` on its allowlist until this decides).

## Size

S

## References

- Finding frontend-go-13 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-11). The unreachable-consumer observation was added while re-verifying for this draft. No earlier draft.
