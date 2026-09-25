# Audit 2026-09-22 issue drafts: bucket `shatter-frontend-go`

- Repo: shatter (tracker: bd in /home/ketan/project/shatter, prefix `str`)
- Parent epic: "Epic: Audit 2026-09-22 findings"
- Theme: Go frontend and go-tool wrapper: relocatable runtime, module path, rune literals, config discovery, build timeout, dead code, divergences.
- Nothing here is filed by agents (D6). Placeholders such as `<id of go-tool-module-path>` are for the filer to substitute.
- All evidence re-verified against the audit worktree at commit 56c86168 on 2026-09-23 unless marked otherwise.
- Revised 2026-09-23 after the Codex cross-check (`crosscheck/shatter-frontend-go.codex.md`); see REVISION.md. Five new drafts (14-18) come from splits; no slug removed.

## Maintainer decisions (2026-09-23), overriding the report and old drafts

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix; fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64), do not drop them. Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot-diff command and the unused Snapshot writer path; spec-diff is the regression tool. Update SPEC/README/QUICKSTART. The `diff` name becomes free; whether str-81xiw takes it is left to str-81xiw. Correct the shatter-agents plugin's `shatter diff --staged` docs.
- **D3 Concolic positioning:** measure first. P1 controlled default-vs-concolic benchmark (fixed seeds, fresh artifacts, examples corpus + one downstream project), reported per release; P1 fix concolic early termination (~21-35 iterations). A follow-up decision issue (blocked by both) re-decides "concolic-first" positioning. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import in shatter; move tracker sync to a Dolt remote; first verify whether importing the stale JSONL has clobbered newer DB state; AGENTS.md drops `bd sync`; str-qwua7.28 superseded; bento beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT fix, no hook-bypass guidance.
- **D5 Git identity:** leaked [user] section already removed 2026-09-23. Draft a .mailmap (test@example.com "Test"/"Test User" -> Ketan Gangatirkar <33678+ketang@users.noreply.github.com>, no history rewrite), a drift-patrol/setup-hooks git-state check (local identity override, example.com email, core.bare=true, hooksPath override; possibly as a note on str-qwua7.1), and test_git_fixture_isolation.py snapshotting .git/config around fixture entrypoints.
- **D6 Filing:** after reconciliation and Codex cross-check, the maintainer runs one filer script. Nothing is filed by agents.

None of D1-D6 changes this bucket's scope. D1 touches `go-harness-runtime-embed` (must keep the Windows release leg building) and `go-release-relocation-smoke` (closes on a green release-run URL with no matrix leg dropped). D1 also shapes `go-build-timeout-ignored` (Windows process-tree kill).

Cross-bucket blockers: `go-build-timeout-ignored` <- `timeout-budget-invariant` (shatter-frontend-rust); `go-release-relocation-smoke` <- `release-publish-and-install-smoke` (shatter-ci-workflows).

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
| 09-go-build-timeout-ignored.md | new | P2 | - | [timeout-budget-invariant] | Go frontend ignores --build-timeout / SHATTER_BUILD_TIMEOUT (go build has no timeout) and SHATTER_HARNESS_RELEASE |
| 10-go-connection-failures-divergence.md | new | P2 | - | [] | Record the Go execute-response gap in the parity matrix: Go never emits connection_failures or runtime_crypto_boundaries, so LiveFirst fallback never fires for Go |
| 11-go-explore-warmup-gate.md | new | P3 | - | [] | Explore lacks the cold-build warmup gate that scan has; many-target Go explores on a cold cache can time out en masse |
| 12-go-small-correctness-tidy.md | new | P3 | - | [] | Go frontend housekeeping: dead `_ = anchorImport` / `_ = fresh` assignments, and unchecked lock-file PID writes that silently weaken stale-lock detection |
| 13-go-cgo-refusal-covers-bodies.md | new | P3 | - | [] | Go cgo "detect and refuse" is signature-only and its only consumer (planner.Classify) is unreachable: refuse every function in a file that imports "C", on the live path |
| 14-go-release-relocation-smoke.md | new | P1 | - | [go-harness-runtime-embed, release-publish-and-install-smoke] | Release smoke must prove the shipped Go frontend works where its build checkout does not exist (fresh path, no shatter-go/ source, cold caches) |
| 15-go-concolic-escaped-string-miss.md | new | P2 | - | [go-rune-and-escape-literals] | Concolic Go explore misses `s == "a\tb"` although the runtime constraint carries the real tab: find where the escaped value is lost and fix it |
| 16-go-connection-failures-impl.md | new | P3 | - | [] | Go frontend: detect outbound connection failures during execute and emit connection_failures so LiveFirst can fall back to mocks |
| 17-go-line-zero-records.md | new | P3 | - | [] | Go instrumenter records line 0 on every execution (synthetic call_enter/call_exit statements), so lines_executed always contains phantom zeros |
| 18-go-mock-codegen-json.md | new | P3 | - | [] | Generated Go mock harness embeds mock JSON in a backtick raw string (a backtick in a mock value breaks the build) and ignores json.Unmarshal errors |

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

This issue fixes the lookup and proves it locally, in the normal test gates. Proving it in the release pipeline is a separate issue, `go-release-relocation-smoke`, because that proof depends on the release workflow being green (D1).

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go/instrument/executor.go:112-133` `ensureHarnessRuntimeDir`: `runtime.Caller(0)` at :114, joins `<srcdir>/../harness`, and stats `go.mod` at :126, returning `stat harness runtime go.mod: ...` on failure.
- Production callers: `shatter-go/build/instrumented_overlay.go:162` and `:819`, via `instrument.EnsureHarnessRuntimeDir` (`shatter-go/instrument/api.go:16-18`). The doc comment at api.go:16 says it "materializes the shared harness runtime module"; it only stats the source path.
- No `//go:embed` exists in non-test shatter-go code (`grep -rn "go:embed" shatter-go --include=*.go | grep -v _test` is empty).
- **Module boundary.** `shatter-go/harness/` is its own module (`shatter-go/harness/go.mod`: `module shatter-harness`), and `shatter-go/go.mod` neither requires nor replaces it. `//go:embed` cannot match files that belong to another module, so no package in the `shatter-go` module can embed `harness/go.mod` and `harness/runtime.go` where they are. The fix needs a copy or generated mirror of those files inside the `shatter-go` module (with a drift check), or a restructuring of the harness module.
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

- [ ] **Outcome.** The Go frontend binary carries the harness runtime module (`go.mod` + `runtime.go`) inside itself and materializes it once, atomically (temp dir + rename), into `<workspace>/harness-runtime/<content-hash>/`. `EnsureHarnessRuntimeDir` returns that path. `runtime.Caller` is no longer used to find the module. The mechanism respects the module boundary above: either an embedded mirror inside the `shatter-go` module that is generated or copied from `shatter-go/harness/`, or a restructured harness module. Record which in `shatter-go/CLAUDE.md`.
- [ ] **Drift check.** If a mirror is used, a test (in `go test ./...` for shatter-go, so it runs in `task check`) fails when the mirror's bytes differ from `shatter-go/harness/go.mod` / `runtime.go`. Show it failing once by editing `harness/runtime.go` on the branch without regenerating, and paste that output into the close note.
- [ ] A missing or unmaterializable runtime is reported as an infrastructure error (not outcome `build_failed`, and not "go build failed").
- [ ] **Relocation regression test**, failing on current main and passing on the branch, which cannot pass by finding the compiled-in path:
  - build the frontend with `-trimpath` from a temporary copy of `shatter-go/`, then **delete** that copy (not just move it), so the compiled-in source path no longer exists anywhere;
  - run the binary with a cwd outside the repo, with `SHATTER_GO_WORKSPACE_ROOT` set to a fresh empty temp dir (so the launcher and `GOCACHE` caches are cold; `workspace.GoEnv` pins `GOCACHE` under the workspace);
  - drive handshake / analyze / execute on a small target and assert the function's return value.
  Paste the failing run's output from main into the close note.
- [ ] `task e2e-go` passes (it runs `cargo test --test e2e_concolic_go -- --include-ignored`; a plain `cargo test --test e2e_concolic_go` skips every case because they are all `#[ignore]`). Paste into the close note the cargo summary line showing `0 ignored`. If the gate reports a cache hit, run the cargo command directly and paste that instead.
- [ ] `task affected` passes with its `Gates selected` output recorded.
- [ ] `shatter-go/CLAUDE.md` describes the embed/materialize mechanism instead of the source-path lookup.

## Suggested approach

Add a small leaf package inside the `shatter-go` module (e.g. `shatter-go/harnessembed`) holding a generated copy of the two files plus `//go:embed`, with a `go:generate` step that copies them from `../harness`; the drift test compares bytes. Hash the embedded contents; write to `<workspace>/harness-runtime/<hash>/` under a temp name and `os.Rename`, mirroring the atomic pattern in `shatter-go/launcher/launcher.go:336-350`. Keep the `replace shatter-harness => <dir>` wiring in the generated launcher go.mod (`launcher/launcher.go:528`) pointed at the materialized dir. Make sure the Windows release target still builds (D1 keeps it): use `filepath` everywhere.

## Out of scope

- The release-pipeline proof: `go-release-relocation-smoke`.
- Precompiled harness templates (str-o650 territory).
- Other release-matrix failures (`release-windows-z3-build`, `release-aarch64-openssl-cross`; D1 requires those to be fixed, not dropped).

## Dependencies

- Blocked by: none.
- Blocks: `go-release-relocation-smoke`.
- Related: str-o650.

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

The Go tool wrapper's module path does not match its directory, so the install command in `docs/distribution.md` fails for every user. The repo has no root `go.mod`, so the Go module system resolves `github.com/shatterproof-ai/shatter/go-tool` to a `go-tool/` directory at the repo root, which has never existed.

str-fl9g.2 ("Go tool wrapper", P1) was closed "9dc76c85 landed on main" with exactly this command as its acceptance check; that check could never have passed.

No tag changes are needed. `continuous-*` tags are not semantic versions, so `@continuous-...` is a revision query: Go resolves the revision in the repository and gives the nested module a pseudo-version. Directory-prefixed tags (`go-tool/vX.Y.Z`) are needed only for semantic-version tags of a nested module ([Go module reference, version queries](https://go.dev/ref/mod#version-queries); [VCS version tags for subdirectory modules](https://go.dev/ref/mod#vcs-version)).

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go-tool/go.mod:1`: `module github.com/shatterproof-ai/shatter/go-tool`. `ls go-tool` and `ls go.mod` at the repo root: no such file. `git log --all -- go-tool/go.mod` is empty.
- `docs/distribution.md:64`: `go get -tool github.com/shatterproof-ai/shatter/go-tool/cmd/shatter@continuous-20260512-1735-abc123def456`. The Renovate regex at `docs/distribution.md:110` targets the same path.
- `Taskfile.yml:467`: `task meta` only runs `cd shatter-go-tool && go test ./...`, which cannot detect module-path/directory drift.
- Audit run in a fresh temp module (network, `GOPROXY=direct`):
  ```
  go: module github.com/shatterproof-ai/shatter@main found (v0.0.0-20260922164214-16794cef9e10),
      but does not contain package github.com/shatterproof-ai/shatter/go-tool/cmd/shatter
  ```
  The error is a path error (no such package in the repo), not a version-resolution error.

## Acceptance criteria

- [ ] The module path and directory agree: either the directory is renamed to `go-tool/`, or the module path, docs and Renovate regex all change to `.../shatter/shatter-go-tool`. Every reference (Taskfile `meta`, CI, `docs/distribution.md`, Renovate regex, README/QUICKSTART if they mention it) is updated in the same change.
- [ ] **Pre-merge check (gates the PR).** A test in `task meta` resolves the documented module path against the local checkout, with no network: a temp module with `require <documented path> v0.0.0` plus `replace <documented path> => <repo>/<module dir>`, then `go build <documented path>/cmd/shatter`, and a check that the `module` line of the wrapper's `go.mod` equals the path in `docs/distribution.md` and the Renovate regex. It fails on current main (show the failing output in the close note) and passes on the branch.
- [ ] **Post-merge check (proves the documented command).** A job that runs on push to main (the release workflow or a small workflow of its own) creates a temp module and runs the exact documented `go get -tool <path>/cmd/shatter@<the continuous tag of that run, or the pushed commit SHA>` with `GOPROXY=direct`, then `go tool shatter --shatter-wrapper-help`. Proof at close: the URL of a green run of that job after the fix landed, with its output pasted into the close note.
- [ ] `docs/distribution.md` states that `@continuous-...` resolves to a pseudo-version, and does not ask for prefixed tags.
- [ ] str-fl9g.2 carries a comment linking this issue as the fix (see the companion reopen-note draft `go-tool-reopen-note`).
- [ ] `task affected` passes with `Gates selected` recorded.

## Suggested approach

Renaming the directory to `go-tool/` is the smallest change that keeps the published path in the docs. The post-merge job can use the commit SHA instead of the continuous tag if the release workflow is not yet green, so this issue does not wait on the release fixes.

## Out of scope

- Wrapper robustness (atomic extract, HTTP timeouts, API caching): `go-tool-wrapper-robustness`.
- Semantic-version tagging of the nested module (not needed for `continuous-*` revisions).

## Dependencies

- Blocked by: none.
- Blocks: `go-tool-wrapper-robustness` (paths move).
- Related: str-fl9g.2 (closed; acceptance never passed), str-wnyzy (closed; CI Setup Go step and root go.mod).

## Size

S

## References

- Finding frontend-go-02 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-02). Old draft: `drafts/shatter-code/51-go-tool-module-path.md`. Revised after the Codex cross-check: the earlier requirement to push `go-tool/<tag>` tags was wrong and is removed.

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
>
> The `continuous-*` tags themselves are fine: they are revision queries, which Go resolves to pseudo-versions for a nested module without directory-prefixed tags. Only the path is wrong.
>
> The fix (module path/directory alignment, a pre-merge local-resolution check, and a post-merge job that runs the documented command) is tracked in **<id of go-tool-module-path>**. Evidence: `audits/2026-09-22/areas/frontend-go.md` go-02.

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

This issue fixes literal decoding and typing. The concolic engine also missed the escaped-string arm although the runtime constraint was already correct; that is a separate, undiagnosed defect tracked in `go-concolic-escaped-string-miss`, and its E2E reach is not part of this issue's acceptance.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go/protocol/analyzer.go:2351-2353` (static builder `litSymExpr` path): `case token.STRING, token.CHAR: val := strings.Trim(lit.Value, "`\"'")` then `Type: "str"`.
- The same `strings.Trim` pattern also appears as an Unquote-failure fallback at `analyzer.go:770`, `:2689`, `:2701`, `:2745` (literal harvesting); those are fallbacks after `strconv.Unquote` and are lower risk, but should be checked in the same change.
- `shatter-go/instrument/symextract.go:139-150` (runtime builder): STRING uses `strconv.Unquote` correctly, but `case token.CHAR:` at :145 also returns `&symExpr{Kind: "const", Type: "str", Value: s}`.
- Parameter typing: `shatter-go/protocol/analyzer.go:1555-1561` (`typeInfoFromAST`; `basicTypeInfo` has the same rule) maps `rune`/`int32` to `TypeInfo{Kind: "int"}`, so `{type:int}` constants are well-typed against a `rune` param. `byte`/`uint8` map to `{Kind: "complex", ComplexKind: "go_byte"}`; whether the solver gives `go_byte` params an Int sort is not verified here (see the byte AC).
- `shatter-core/tests/e2e_concolic_go.rs` has no rune or string-escape known-answer case (`grep -n "rune\|'x'"` finds none). Every case in that file is `#[ignore]` and runs only with `--include-ignored` (`task e2e-go`, `Taskfile.yml:611-628`).
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

## Acceptance criteria

- [ ] CHAR literals emit `{kind: const, type: "int", value: <codepoint>}` in both the analyzer builder and the instrument builder.
- [ ] The analyzer uses `strconv.Unquote` for STRING and CHAR literals (falling back to `unknown`, not to a trimmed raw string, on error). The four fallback sites listed above are either switched to `unknown` or justified in a code comment.
- [ ] rapid property in shatter-go: for a random printable-or-escaped string `s`, the analyzer's literal SymExpr for the parsed literal `strconv.Quote(s)` has `Value == s`; and for a random rune `r`, the literal SymExpr for `strconv.QuoteRune(r)` is `{type:int, value:int(r)}` in both builders.
- [ ] Analyzer-level known-answer test on the probe above: the analyze response's right-hand sides are `"a\tb"` (with a real tab), `"'q'"`, and `{type:int, value:120}`. Fails on main, passes on the branch.
- [ ] `shatter-core/tests/e2e_concolic_go.rs` gains known-answer cases (fixture under `examples/go/`) for a quote-containing string compare, a rune compare and a byte compare; each expected arm is reached. The new cases fail on current main and pass on the branch; record the failing output in the close note. If the byte case fails only because `go_byte` params are not Int-sorted in the solver, fix that mapping in this issue (it is the same defect from the solver's side).
- [ ] `task e2e-go` passes (it runs `cargo test --test e2e_concolic_go -- --include-ignored`; plain `cargo test --test e2e_concolic_go` skips every case). Paste the cargo summary line showing `0 ignored` and the three new test names in the output. If the gate reports a cache hit, run the cargo command directly and paste that.
- [ ] `task affected` passes with `Gates selected` recorded.

## Suggested approach

Small targeted fix in the two literal switch arms plus the tests. Do not wait for builder unification; that stays in str-qwua7.35 (see the `qwua7-35-four-go-builders` note).

## Out of scope

- The concolic miss of the escaped-string arm (`go-concolic-escaped-string-miss`).
- Unifying the four Go SymExpr builders (str-qwua7.35) and the SymExpr construction spec (str-qwua7.37).
- The branch metric reporting "3/3 branches" while arms are missed (engine-correctness bucket).

## Dependencies

- Blocked by: none.
- Blocks: `go-concolic-escaped-string-miss`.
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

A boundary rule has to handle three layouts that exist today:

- **Nested modules inside a repo.** Most `examples/go/*` directories are their own Go modules (`examples/go/multi-file-service/go.mod`, `examples/go/configured-receiver/go.mod`, ...) with no `.shatter/` of their own, and they rely on the repo-root `.shatter/config.yaml`. A "nearest go.mod" boundary would silently drop that config.
- **Linked worktrees**, where `.git` is a file, not a directory.
- **Standalone files outside any module or repo.** `shatter-go/loader/loader.go:107` `LoadFile` supports markerless standalone Go files by synthesizing a module. A file in `/tmp/x/f.go` has no marker above it, so a marker-based rule alone would still walk `/tmp` → `/`.

This is the Go counterpart of open str-dl2pj (P1: core `discover_configs` walks past shared system dirs such as `/tmp` with no boundary; its body lists git root, `$HOME` and device boundary as candidates without choosing). This issue chooses the rule for Go, and str-dl2pj is asked to adopt the same rule; this issue does not wait for it.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go/config/loader.go:228-249` `findConfigFile`: loops `dir = filepath.Dir(dir)` until `parent == dir` (the root); no module, workspace or VCS stop. Callers: `Load` (`:199`) and the exported `FindConfigFile` (`:223`, used for mtime caching, str-c8djq).
- `shatter-go/config/loader.go:414-417` `knownTopLevelKeys = {functions, go_runtime_values}`. (`defaults` exists only in `knownFunctionKeys` at :420, the per-function key set.)
- Repo root has `.shatter/config.yaml`; `examples/go/.shatter` does not exist; `ls examples/go/*/go.mod` lists more than ten nested modules.
- `/tmp/.shatter/config.yaml` still exists on the audit host; its header reads "Generated by `shatter init`" and it has a top-level `defaults:`.
- `cd shatter-go && go test ./config/ -run TestLoad_MissingFile_ReturnsZeroFile -count=1` fails (audit gates log `audits/2026-09-22/gates/go-test-rerun.log`, reproduced 2/2):
  `loader_test.go:668: expected empty File, got {... Warnings:[config /tmp/.shatter/config.yaml: ignoring unknown top-level key "defaults"]}`. The test is at `config/loader_test.go:656`. Setting `TMPDIR` to another dir under `/tmp` does not help; `TMPDIR=/var/tmp/...` passes.
- Tracker (checked with `bd show` on 2026-09-23): str-k7czv (open, P3) records this failure with no root cause; str-dl2pj (open, P1) is the core/CLI equivalent. str-uk8y (single config-resolution entrypoint) and str-rs822 (CLI scan walk-up duplication) are related.

## Acceptance criteria

- [ ] `findConfigFile` uses this rule, written in `shatter-go/CLAUDE.md` and the config docs:
  1. Walk up from the target file's directory. Stop after checking the nearest ancestor that contains `.git` (directory **or** file). Nested `go.mod`/`go.work` do not stop the walk inside a repo.
  2. If no `.git` ancestor exists, stop after checking the nearest ancestor containing `go.work`, else `go.mod`.
  3. If there is none of those either (a markerless standalone file), check only the file's own directory.
  4. Never check `/`, and never check a directory above `$HOME` when the start dir is inside `$HOME`.
- [ ] Tests, each in `config/loader_test.go`, each failing on current main where applicable:
  - a config at a VCS root is found from a file in a nested module below it (models `examples/go/*`), including when `.git` is a file;
  - a config above the VCS root that grants `policy.allow` is **not** applied to a project below it;
  - a markerless standalone file in a temp dir with a planted `.shatter/config.yaml` in the temp dir's parent is not affected by it; a config in the file's own directory is found;
  - a module without VCS finds a config at its `go.mod` directory and ignores one above it.
- [ ] The resolved config path, or "none found, stopped at <dir>", is logged at debug level.
- [ ] `TestLoad_MissingFile_ReturnsZeroFile` passes with a `/tmp/.shatter/config.yaml` present. Proof at close: run it once with a planted `/tmp/.shatter/config.yaml` on main (fails) and on the branch (passes); paste both outputs. The fix must come from the rule, not from adding a marker to that test only.
- [ ] `defaults` and every other top-level key `shatter init` writes are accepted without warning. New test: generate `shatter init` output and load it in the Go config loader, asserting zero warnings. (Other frontends' loaders are out of scope here.)
- [ ] The E2E Go suites still find the repo-root config for `examples/go/*`: `task e2e-go` passes (it runs `cargo test --test e2e_concolic_go -- --include-ignored`; paste the summary line showing `0 ignored`).
- [ ] When this lands, str-k7czv is closed as a duplicate with a comment pointing here; str-dl2pj gets a comment stating the rule adopted here and asking core to match it.
- [ ] `task affected` passes with `Gates selected` recorded.

## Suggested approach

Add a `stopDir(start string) string` helper computed once per lookup, then bound the existing loop by it. Add `defaults` to `knownTopLevelKeys`, and derive the known-key list from the init template if practical so it cannot drift again.

## Out of scope

- Finding which process ran `shatter init` in `/tmp` and changing implicit-init behaviour (str-qwua7.58).
- The core/CLI discovery fix itself (str-dl2pj).
- Warning-free loading of init output in the TS and Rust frontends.

## Dependencies

- Blocked by: none.
- Related: str-k7czv (duplicate; close when this lands), str-dl2pj (core counterpart; should adopt this rule), str-uk8y, str-rs822, str-qwua7.58.

## Size

M

## References

- Findings gates-03 and frontend-go-07 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-07). Old draft: `drafts/shatter-code/05-go-config-discovery-unbounded.md`. Revised after cross-check: boundary rule now covers nested modules (same-runtime review) and markerless standalone files (Codex).

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

`deadcode ./...` reports ~57 unreachable production functions (57 at 56c86168; the count moves with every commit) in shatter-go, including the entire `reconstruct` package, which has no importers. Nothing gates new dead code. Meanwhile open str-qwua7.48 asks for new rapid property tests *for* `reconstruct` (dead) while the live code generators that build Go source by string concatenation (wrapper, launcher) and the config matcher have no property tests, and `setup/` has no tests at all.

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

- [ ] Each function in the deadcode report taken at branch start (paste that report, with its commit SHA, into the issue) is deleted, or moved to `_test.go` / `internal/testutil` if tests need it, case by case. `reconstruct/` is deleted. `shatter-go/CLAUDE.md` claims about removed code (e.g. `ResolveMockSpecs` at :273, reconstruct) are corrected.
- [ ] Exceptions: `instrument/flow*.go` stays until str-qwua7.35 removes it; `planner/classify.go Classify` is not deleted until `go-cgo-refusal-covers-bodies` decides whether cgo refusal is wired through it. Both are listed in the allowlist with the owning issue id.
- [ ] A `deadcode` check with a checked-in allowlist runs in `task meta` (or `check-static`) and fails on new unreachable production functions. Proof at close: the gate output from a forced (non-cached) run, and a demonstration that adding an unused exported function makes it fail.
- [ ] str-qwua7.48 is re-scoped by the comment below (filer posts it; the implementer of str-qwua7.48 does the tests, not this issue).
- [ ] `go test ./...` in shatter-go passes. `task e2e-go` passes (it runs `cargo test --test e2e_concolic_go -- --include-ignored`; plain `cargo test --test e2e_concolic_go` skips every case because all are `#[ignore]`); paste the cargo summary line showing `0 ignored`, or run the cargo command directly if the gate reports a cache hit. `task affected` passes (`Gates selected` recorded).

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

- **Non-atomic extract, and a cache check that trusts anything executable.** The binary is extracted straight into its final cache path with mode 0755. An interrupted extract or two concurrent first runs leave a truncated executable. The cache check only tests mode bits, so every later run execs the corrupt file. The SHA-256 check covers the archive, not the extracted binary. Making extraction atomic prevents new corrupt entries but does not repair ones already in users' caches, because the mode-bit check accepts them before any extraction runs.
- **No timeouts.** Both HTTP calls use the default client with no timeout.
- **API call before cache.** With `SHATTER_BUILD` unset, the latest continuous build is resolved via `api.github.com` before the cache is consulted, on every run; unauthenticated users get 60 requests/hour, so CI loops get rate-limited.
- **Token only on the API call.** The API request sends `GITHUB_TOKEN`; the asset download does not. Adding it naively would be unsafe: the download URL comes from the release manifest (`asset.URL`), so it could point at any host.
- **Tests.** `main_test.go` covers only `parseArgs`.

## Evidence

Re-verified against the audit worktree at commit 56c86168 (`shatter-go-tool/cmd/shatter/main.go`, 355 lines):

- `:315 extractBinary`; `:340` `os.OpenFile(targetBinary, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o755)` then copies directly.
- `:352 isExecutable` checks mode bits only; it is the cache-hit test at `:160` (cache dir `os.UserCacheDir()/shatter/binaries/<build>/<platform>`, `:154-158`).
- `:142` `latestContinuousBuild(repo)` runs before the `:160` cache check; `:195` issues the API request.
- `:256 getJSON` sets the `Authorization` header from `GITHUB_TOKEN` at `:263` and calls `http.DefaultClient.Do` at `:267` (no timeout).
- `:278 downloadFile` calls `http.Get(url)` at `:279` (no timeout, no token). Its URL is `selected.URL` (`:180`) from the manifest's `asset.url` field (`:36-41`). `release.yml:262` writes `https://github.com/{repo}/releases/download/{tag}/{name}` there, which GitHub redirects to a separate download host.
- `shatter-go-tool/cmd/shatter/main_test.go`: 39 lines, `parseArgs` only.
- Reference pattern for atomic writes: `shatter-go/launcher/launcher.go:336-350` builds to a temp path and `os.Rename`s (`:350`).

## Acceptance criteria

- [ ] **Atomic extract.** Extraction writes to a temp file in the target directory, is fsynced and chmodded, then `os.Rename`d into place.
- [ ] **Cache validation and migration.** The wrapper writes a sidecar (e.g. `<binary>.sha256`) after the rename, holding the SHA-256 of the extracted binary (and the manifest's per-binary hash when the manifest carries one). A cache hit requires the sidecar to exist and match the binary's current hash. Entries with no sidecar (everything cached by current releases) or a mismatching one are treated as untrusted: deleted and re-downloaded, with a one-line notice. This is documented in `docs/distribution.md`.
- [ ] **Timeouts.** Both HTTP calls use an `http.Client` with connect and overall timeouts.
- [ ] **Token scoping.** `GITHUB_TOKEN` is sent only to an allowlist of origins (`https://api.github.com` and `https://github.com`), on the initial request and on redirects to an allowlisted origin; never to any other host, including redirect targets such as the release-asset download host. Plain `http://` URLs never receive it.
- [ ] **Latest-tag cache.** The resolved latest-continuous tag is cached with a TTL; when the API is unreachable or rate-limited, the newest cached build is used with a warning.
- [ ] `httptest`-backed tests, each failing on current main where applicable and passing on the branch:
  - manifest selection; archive checksum mismatch rejected;
  - a pre-existing truncated executable in the cache with no sidecar (as current releases leave it) is not executed and is replaced;
  - a cached binary whose sidecar hash does not match is not executed and is replaced;
  - API unavailable falls back to the cached tag;
  - a stalled server times out within the configured bound;
  - with `GITHUB_TOKEN` set, a manifest asset URL on a non-allowlisted host receives no `Authorization` header, and a redirect from an allowlisted test origin to a non-allowlisted one does not carry it (make the allowlist injectable so the test server can stand in for github.com).
- [ ] `task meta` (which runs `go test ./...` in the wrapper module) passes; `task affected` passes with `Gates selected` recorded.

## Suggested approach

Mirror the launcher's temp-then-rename. Wrap the client in a small struct holding the timeouts and the token allowlist, with a `CheckRedirect` that removes `Authorization` for non-allowlisted hosts (Go already drops it on cross-domain redirects, but the initial request to a manifest-supplied host is the gap). Store the TTL-cached tag in a small JSON file next to the cached binaries.

## Out of scope

- The module path/directory mismatch (`go-tool-module-path`), which lands first and may move this file.
- Private-repo asset downloads through the API asset endpoint.

## Dependencies

- Blocked by: `go-tool-module-path` (directory/module rename touches the same files).
- Related: str-fl9g.2 (closed; original wrapper).

## Size

M

## References

- Finding frontend-go-10 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-10). Verifier correction applied: the API request does read `GITHUB_TOKEN`; only the download ignores it. Old draft: `drafts/shatter-code/55-go-tool-wrapper-robustness.md`. Revised after the Codex cross-check: cache validation/migration for existing corrupt entries, and origin-scoped token.

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
blocked_by: [timeout-budget-invariant]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go frontend ignores --build-timeout / SHATTER_BUILD_TIMEOUT (go build has no timeout) and SHATTER_HARNESS_RELEASE

## Problem

The CLI exports `SHATTER_BUILD_TIMEOUT` (from `--build-timeout`, default 30) and, when release harnesses are requested, `SHATTER_HARNESS_RELEASE=1` to every frontend. `--help` promises "Build timeout in seconds for compiling instrumented code in the frontend. Default: 30s". The Go frontend reads neither variable, and its `go build` invocations have no context or deadline. A hung Go build is bounded only by the core's per-request timeout, which taints the whole frontend session instead of failing the one target with a build-phase timeout.

Honouring the build timeout in Go is not enough on its own. With defaults, the request timeout (30 s) equals the build timeout (30 s), and the request timer starts first, so the core's timer fires first and taints the session before a Go-side build timeout can be reported. That ordering is a CLI-wide deadline problem owned by `timeout-budget-invariant` (request timeout for build-capable requests must exceed build + exec timeout plus a margin). This issue is blocked by it, and relies on that invariant to make "fails one target, session survives" hold under default flags.

The Go frontend used to honour the variable: `instrument/executor.go` had a `buildTimeout()` reading `SHATTER_BUILD_TIMEOUT` (str-9smo extracted its default into a const). It was removed with the legacy direct-call harness in commit 9c39ad94 ("trim legacy direct-call harness from executor.go"), and the replacement build path (`build.Builder` + `launcher`) never re-added it.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-cli/src/helpers.rs:750-760` pushes `SHATTER_EXEC_TIMEOUT`, `SHATTER_BUILD_TIMEOUT`, and (if `release`) `SHATTER_HARNESS_RELEASE=1` into the frontend env. `shatter-cli/src/args.rs:561-563` (`--request-timeout`, default 30) and `:574-577` (`--build-timeout`, default 30, with the help text above).
- `shatter-core/src/frontend.rs:329-342`: the request is wrapped in `tokio::time::timeout(request_timeout, ...)`; on expiry the frontend is marked `tainted` and later calls fail fast.
- `grep -rn "BUILD_TIMEOUT\|HARNESS_RELEASE" --include=*.go shatter-go` returns nothing.
- Go build sites: `shatter-go/launcher/launcher.go:342` `exec.Command("go", buildArgs...)` and `shatter-go/setup/loader.go:63` `exec.Command("go", "build", ...)`; no `exec.CommandContext` anywhere in those files.
- Only other reader: `shatter-rust/src/executor.rs:2967`, `:3135`, `:5404` read `SHATTER_BUILD_TIMEOUT` (fallback default 120s); `executor.rs:1020-1022` reads `SHATTER_HARNESS_RELEASE`. The CLI's own `build_frontend.rs:384`, `:553` also reads `SHATTER_HARNESS_RELEASE`, so ignoring it in Go may be intentional (verifier note).
- `git log -S SHATTER_BUILD_TIMEOUT -- shatter-go` shows the reader removed in 9c39ad94.
- D1 keeps `x86_64-pc-windows-msvc` in the release matrix, and that leg builds the Go frontend, so any process-group code must compile and work on Windows.

## Acceptance criteria

- [ ] Both Go build sites run under `exec.CommandContext` with a deadline from `SHATTER_BUILD_TIMEOUT` (seconds; same parsing/default rules as the Rust frontend, documented).
- [ ] On expiry the whole build process tree is killed, per platform: on Unix, a new process group (`Setpgid`) killed as a group; on Windows, a Job Object (or an equivalent tree kill) so child `compile`/`link` processes die too. The platform code lives in build-tagged files (`_unix.go` / `_windows.go`).
- [ ] `GOOS=windows GOARCH=amd64 go build ./...` and `GOOS=windows go vet ./...` in shatter-go pass in `task check` (or the existing Go lint gate), so the Windows release leg cannot be broken by this change.
- [ ] Expiry is reported for that target as outcome `timed_out` with a build-phase reason that names `--build-timeout`.
- [ ] Test in shatter-go, failing on current main and passing on the branch: with a fake `go` on `PATH` that spawns a child and both sleep past a 1-2 s `SHATTER_BUILD_TIMEOUT`, the build returns within the timeout plus a small margin, the outcome is `timed_out`, and the child process is gone afterwards. Runs on Linux in CI; the Windows variant is at least compiled.
- [ ] Session test, run with **default** `--request-timeout` (i.e. relying on `timeout-budget-invariant`): a Go frontend session whose first target's build exceeds a short `--build-timeout` reports that target `timed_out`, and the next target in the same session executes successfully (the session is not tainted). Record the output in the close note.
- [ ] `SHATTER_HARNESS_RELEASE` is either honoured by the Go frontend (e.g. build flags that matter for Go) or declared Rust-only: `shatter-go/CLAUDE.md`, the `--help` text for the flag that sets it, and `protocol/parity-matrix.yaml` say so.
- [ ] The per-frontend env contract for both variables is recorded in `protocol/parity-matrix.yaml` (or wherever str-qwua7.20.2 puts the env table), and `task parity` passes.
- [ ] `task e2e-go` passes (it runs `cargo test --test e2e_concolic_go -- --include-ignored`; paste the summary line showing `0 ignored`), and `task affected` passes with `Gates selected` recorded.

## Suggested approach

Add a `buildTimeout()` helper in a shared Go package (the launcher already owns the build), thread a `context.Context` into the launcher build and `setup/loader.go`, and set `cmd.Cancel` to the platform tree-kill function. For the env contract, a small table test that every `SHATTER_*` variable the CLI exports is either read by each frontend or listed as declared-ignored would catch the next drift.

## Out of scope

- The CLI-wide request/build deadline invariant itself (`timeout-budget-invariant`).
- Documenting every `SHATTER_*` variable (str-qwua7.20.2).

## Dependencies

- Blocked by: `timeout-budget-invariant` (audit bucket shatter-frontend-rust; must cover Go execute requests that build, not only Rust).
- Related: str-qwua7.20.2 (env-var table), str-9smo (closed; original Go build-timeout const), commit 9c39ad94 (removal).

## Size

M

## References

- Finding frontend-go-08 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-08). No earlier draft. Revised after cross-check: deadline ordering now depends on `timeout-budget-invariant`, Windows tree-kill required, nonexistent `str-kzxt` reference removed (`bd show str-kzxt`: no issue found; the id appears only in the commit message).

---

<!-- file: 10-go-connection-failures-divergence.md -->
---
slug: go-connection-failures-divergence
kind: new
title: "Record the Go execute-response gap in the parity matrix: Go never emits connection_failures or runtime_crypto_boundaries, so LiveFirst fallback never fires for Go"
priority: P2
type: task
labels: [go-frontend, parity, protocol, mocking, docs, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Record the Go execute-response gap in the parity matrix: Go never emits connection_failures or runtime_crypto_boundaries

## Problem

The core's LiveFirst mock policy (try the live dependency, fall back to a mock when it is unreachable) is driven solely by `connection_failures` in the execute result. The Go frontend's `Response` type has neither `connection_failures` nor `runtime_crypto_boundaries`, and no Go code produces them, so for Go targets LiveFirst can never fall back and runtime crypto boundary splitting never happens. The parity matrix records this gap only for Rust; its Rust entry says vaguely that "TypeScript and Go emit the subset each supports", which hides the Go gap.

This issue has one deliverable: make the gap visible and tracked, per the matrix rules. Implementing the signals is a separate issue, `go-connection-failures-impl`, because the Go frontend has no live outbound-call interception point today and building one is a much larger job.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `git grep -n -i "connection_failures\|ConnectionFailures\|runtime_crypto_boundaries\|RuntimeCryptoBoundaries" -- shatter-go ':!*_test.go'` returns nothing.
- `shatter-go/protocol/types.go:206-267` `type Response struct` has no such fields.
- `shatter-core/src/explorer.rs:891` `update_live_first_states` is driven only by `result.connection_failures`; called at `explorer.rs:1602` (random explorer) and `:2545`, and at `shatter-core/src/orchestrator.rs:3185` (concolic).
- `protocol/parity-matrix.yaml:1172-1190` `rust-execute-response-fields-partial` is the only record; its description ends "TypeScript and Go emit the subset each supports"; there is no Go entry.
- Tracker (checked with `bd show` on 2026-09-23): str-2fjn (open) notes the missing Go type fields only as registry-vs-types drift; str-924ca (open) is the Rust counterpart; str-3ky9.9.2 is the TS reference implementation (dial / connection-refused classification).

## Acceptance criteria

- [ ] A `go-execute-response-fields-partial` divergence entry in `protocol/parity-matrix.yaml` names both fields, states the LiveFirst consequence (Go targets never fall back to a mock), and cites the issue filed from `go-connection-failures-impl` (look up its id in the epic) as its tracking issue. It is mirrored in `shatter-go/CLAUDE.md` per the matrix rules.
- [ ] The Rust entry's "TypeScript and Go emit the subset each supports" sentence is replaced with an accurate statement of which of the five fields TS and Go each emit (checked against `shatter-ts` and `shatter-go` source; list the file:line for each field TS emits).
- [ ] `task parity` and `task conformance` pass; `task affected` passes with `Gates selected` recorded.

## Out of scope

- Implementing the fields in Go (`go-connection-failures-impl`).
- The Rust counterpart (str-924ca).
- The registry-vs-implementation cross-validation (str-2fjn).

## Dependencies

- Blocked by: none.
- Related: `go-connection-failures-impl` (the tracking issue the entry cites), str-2fjn, str-924ca, str-3ky9.9.2.

## Size

S

## References

- Finding protocol-parity-11 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/protocol-parity.md`). Old draft: `drafts/shatter-docs-ui/27-go-connection-failures-divergence.md`. Split after the Codex cross-check (finding 12: implement-or-document left the deliverable open).

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

str-tbk9e fixed a cold-cache thundering herd for **scan** by adding `BuildWarmupGate`: the first harness build of a run goes alone, then the other workers fan out once the build cache is warm. **Explore** never got the gate. A multi-target Go explore at default parallelism starts all workers on a cold build cache, each triggers a full build at once, and under load they all hit the 30 s request timeout.

This is a parity gap between the scan and explore paths (see the "parallel parity" rule in the root CLAUDE.md). The user-facing failure was seen only on a heavily loaded host, so it is P3 until it is reproduced on a quiet one. The fix is still worth making for parity, and its acceptance is a deterministic ordering test rather than a timing test on a quiet host, which could not distinguish fixed from unfixed behaviour.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `grep -rn "BuildWarmupGate\|warmup_gate" shatter-core/src shatter-cli/src` finds hits only in `shatter-core/src/scan_orchestrator.rs` (struct at `:2138`, `WarmupLeaderGuard` at `:2181`, created at `:3870`, entered at `:3300`); none on the explore path (`shatter-cli/src/commands/explore.rs`, `shatter-core/src/explorer.rs`, `orchestrator.rs`). Existing unit tests of the gate: `scan_orchestrator.rs:6634` `warmup_gate_serializes_first_task_then_fans_out`, `:6682`, `:6699`.
- str-tbk9e (closed, P1) close reason says it serialises the first build "of a scan".
- **Caches.** The Go frontend ignores a caller-supplied `GOCACHE`: `shatter-go/workspace/workspace.go:199-214` `GoEnv` replaces it with `<workspace>/cache/build`. The workspace root comes from `SHATTER_GO_WORKSPACE_ROOT` (`workspace.go:13`, `:86-116`), else `<repo>/.shatter/...`, else the user-data default. Compiled launchers are cached in the workspace's `BinariesDir` (`shatter-go/launcher/launcher.go:10-19`, `:201-203`). So "cold" means a fresh workspace root, not an empty `GOCACHE`.
- Audit run (finding goals-12, `audits/2026-09-22/goals-runs/go-all-default.err`), host load average 150-200: `Spawned 1 frontend session(s) for 18 target(s) (16 parallel worker(s))`, then `Error: explore: all 18 attempted target(s) failed (build_failed=0, runtime_failed=0, timed_out=43)`, each `request timed out after 30s`. The same explore with `-w 2 --request-timeout 180` completed. The verifier downgraded to P3: the load confounds the timeouts and it was not reproduced on a quiet host.

## Acceptance criteria

- [ ] `BuildWarmupGate` / `WarmupLeaderGuard` move to a shared module and are used by explore's multi-target worker pool on both paths (random `explorer.rs` and concolic `orchestrator.rs`), entered before each target's first execute. Scan keeps using the same type.
- [ ] **Deterministic ordering test** (shatter-core, no real frontend, no timing assumptions): drive the explore worker pool with several targets and a fake executor that records start/end events and blocks the first execute on a channel. Assert that no second target's first execute starts until the leader's first execute returns, and that after it returns at least two other targets' executes are in flight at once (fan-out). Also assert the gate opens when the leader fails. The test fails on current main (no gate) and passes on the branch; record the failing output. Run it once for each explorer path.
- [ ] **Real-frontend check** (Go E2E, `#[ignore]` like the rest of `e2e_concolic_go.rs`): explore several targets across multiple Go files with `SHATTER_GO_WORKSPACE_ROOT` set to a fresh empty temp dir (cold launcher and build caches) at default parallelism. Assert from the frontend log or a recorded event trace that exactly one harness build ran before any other started, then that builds overlapped. Record the host load average with the result in the close note.
- [ ] `task e2e-go` passes (it runs `cargo test --test e2e_concolic_go -- --include-ignored`; paste the summary line showing `0 ignored` and the new test name). `task affected` passes with `Gates selected` recorded.

## Suggested approach

Lift the gate out of `scan_orchestrator.rs` into a shared module; have the explore worker pool `enter()` it before a target's first execute. Model the new ordering test on the three existing gate tests at `scan_orchestrator.rs:6634-6699`. Emit a debug-level event when the leader starts and ends so the E2E can assert order without timing.

## Out of scope

- Request/build timeout budgeting (`timeout-budget-invariant`).

## Dependencies

- Blocked by: none.
- Related: str-tbk9e (closed; scan-only fix), `timeout-budget-invariant`.

## Size

S-M

## References

- Finding goals-12 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/goals.md`). Old draft: `drafts/shatter-code/81-go-explore-warmup-gate.md`. Revised after the Codex cross-check (finding 11): an empty external `GOCACHE` does not produce a cold build, and a quiet-host pass could not distinguish the fix.

---

<!-- file: 12-go-small-correctness-tidy.md -->
---
slug: go-small-correctness-tidy
kind: new
title: "Go frontend housekeeping: dead `_ = anchorImport` / `_ = fresh` assignments, and unchecked lock-file PID writes that silently weaken stale-lock detection"
priority: P3
type: chore
labels: [go-frontend, cleanup, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go frontend housekeeping: dead assignments and unchecked lock-file PID writes

## Problem

Two small Go-frontend housekeeping items from the 2026-09-04 audit (rec 13) that were never filed:

1. **Dead assignments** that hide intent: `_ = anchorImport`, `_ = fresh`.
2. **Unchecked lock-file PID writes**: `_, _ = fmt.Fprintf(lockFile, ...)`. `lockIsStale` reads the PID back and falls back to a ModTime timeout when it cannot, so a failed write silently degrades stale-lock detection.

The other two items originally grouped here have their own issues: phantom line-0 coverage records (`go-line-zero-records`) and generated mock code (`go-mock-codegen-json`).

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go/launcher/launcher.go:391` `_ = anchorImport`; `shatter-go/build/builder.go:277` `_ = fresh`.
- `shatter-go/build/builder.go:188` and `shatter-go/launcher/launcher.go:616`: `_, _ = fmt.Fprintf(lockFile, "%d\n", os.Getpid())`.
- Prior-audit rec 13 (2026-09-04) named both items. str-qwua7.32 (closed) covered errcheck on frontend code but left these.

## Acceptance criteria

- [ ] `_ = anchorImport` and `_ = fresh` are removed, or the variables are used for what they were meant for; the commit message says which, per site.
- [ ] Lock-file PID write errors are handled at both sites (returned, or logged at warn level), with the existing ModTime fallback kept. A unit test forces the write to fail (e.g. a read-only file handle) and asserts the error is surfaced and the lock is still usable via the fallback.
- [ ] `go test ./...` in shatter-go passes; `task affected` passes with `Gates selected` recorded.

## Out of scope

- Splitting the large `protocol` package (prior audit P2; not filed here).
- golangci-lint gating (the audit's Go-lint issue, `go-lint-and-gofmt-gated`).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.32, `go-line-zero-records`, `go-mock-codegen-json`.

## Size

XS

## References

- Finding frontend-go-15 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-14 and `areas/prior-audit-regress.md`). Old draft: `drafts/shatter-code/56-go-small-correctness-tidy.md`. Split after the Codex cross-check (finding 13): coverage and mock-generation fixes moved to their own issues.

---

<!-- file: 13-go-cgo-refusal-covers-bodies.md -->
---
slug: go-cgo-refusal-covers-bodies
kind: new
title: "Go cgo \"detect and refuse\" is signature-only and its only consumer (planner.Classify) is unreachable: refuse every function in a file that imports \"C\", on the live path"
priority: P3
type: bug
labels: [go-frontend, scope-limits, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go cgo "detect and refuse" is signature-only and its only consumer is unreachable: make the refusal real

## Problem

`docs/go-frontend-scope-limits.md` says cgo is a permanent non-goal and that "The Go frontend will detect and refuse cgo-bearing functions at analysis time rather than attempt a partial model", because Z3 cannot reason across the C ABI and analysing around cgo calls would produce unsound tests or misleading coverage.

The implementation does not do that:

- Detection only inspects parameter and result types for the `C` pseudo-package. A function with a Go-typed signature that calls `C.foo()` in its body is not flagged.
- The flag it sets (`HasCGoDep`) is read only by `planner.Classify`, which `deadcode` reports as unreachable, and the Go frontend never emits the `cgo_dependency` unsatisfied-requirement kind in production code. So it is likely that no cgo refusal happens at all, even for signature-level cgo (inferred from code reading, not yet executed).

**Deliverable (chosen):** make the documented behaviour true. The doc's rationale (unsound tests) applies to body calls as much as to signatures, and the doc is the product contract, so narrowing the claim is not the fix here. If the maintainer prefers to narrow the doc instead, that is a different issue and this one should be closed as won't-fix with that decision recorded.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `docs/go-frontend-scope-limits.md:36-40` (cgo section): the "detect and refuse" claim.
- `shatter-go/protocol/discovered_target.go:100-125` `fnHasCGoDep` checks only `fn.Type.Params` and `fn.Type.Results`; it sets `HasCGoDep` at `:201`.
- `HasCGoDep` has one non-test Go reader: `shatter-go/planner/classify.go:54` inside `Classify` (`:42`). `cd shatter-go && deadcode ./...` lists `planner/classify.go:42:6: unreachable func: Classify`.
- `HasCGoDep` is also a wire field: `shatter-go/protocol/types.go:59` `HasCGoDep bool json:"has_cgo_dep,omitempty"`. Nothing in `shatter-core` reads `has_cgo_dep` (`grep -rn has_cgo_dep shatter-core/src` is empty).
- `shatter-go/protocol/invocation_plan.go:198` declares `UnsatisfiedRequirementKindCGODependency = "cgo_dependency"`; no non-test code emits it. The core accepts it (`shatter-core/src/protocol.rs:313`) and `protocol/parity-matrix.yaml:756` lists it among `UnsatisfiedRequirementKind` variants the Go frontend emits.
- Audit finding frontend-go-13 (code reading; body-call case not reproduced). str-hy9b.H5 (closed) wrote the scope-limits doc.

## Acceptance criteria

- [ ] **Red state recorded first.** A probe fixture under `shatter-go` testdata with (i) a function whose parameter type is `C.int` and (ii) a Go-typed function whose body calls `C.puts`, run through `shatter analyze` and `shatter explore` on main; paste both outcomes into the issue.
- [ ] **Detection.** Every function in a file that imports `"C"` is flagged (the conservative file-level rule); the signature check stays as a subset. Method and closure bodies are covered by the file-level rule.
- [ ] **Refusal on the live path.** Flagged targets reach the output as skipped/unsupported with unsatisfied-requirement kind `cgo_dependency` and a reason naming cgo, from the production analyze/explore/scan paths (not from the dead `planner.Classify`). Neither target in the probe is executed.
- [ ] Tests for both probe shapes, in shatter-go and at the CLI level (explore and scan), each failing on main and passing on the branch.
- [ ] **`has_cgo_dep` wire field:** either given a consumer in shatter-core (e.g. the scan report's skip reason) or removed from `protocol/types.go` and the protocol schema/registry; the choice is recorded in `shatter-go/CLAUDE.md`. If removed, regenerate bindings from the schema rather than editing them.
- [ ] **`planner.Classify`:** if the refusal does not reuse it, comment on the issue filed from `go-dead-code-and-property-targets` that `Classify` can be deleted (it is on that issue's allowlist until then). If it is reused, it is wired into the production path and removed from that allowlist.
- [ ] `protocol/parity-matrix.yaml:756` remains true (Go now emits `cgo_dependency`); `task parity` and `task conformance` pass; `task affected` passes with `Gates selected` recorded.

## Suggested approach

Put the refusal where other unsupported-target reasons are produced on the live path. The file-level "imports C" rule needs only the parsed file's imports, so it can sit next to `fnHasCGoDep` in `discovered_target.go`.

## Out of scope

- Any cgo support or modelling.
- Narrowing the doc claim (a maintainer decision; see Problem).

## Dependencies

- Blocked by: none.
- Related: str-hy9b.H5 (closed; wrote the doc), `go-dead-code-and-property-targets` (holds `planner.Classify` on its allowlist until this decides).

## Size

S-M

## References

- Finding frontend-go-13 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-11). The unreachable-consumer observation was added while re-verifying for this draft. Revised after cross-check: deliverable fixed to "refuse" (Codex finding 12), wire field `has_cgo_dep` addressed (same-runtime review).

---

<!-- file: 14-go-release-relocation-smoke.md -->
---
slug: go-release-relocation-smoke
kind: new
title: "Release smoke must prove the shipped Go frontend works where its build checkout does not exist (fresh path, no shatter-go/ source, cold caches)"
priority: P1
type: task
labels: [go-frontend, release, ci, distribution, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [go-harness-runtime-embed, release-publish-and-install-smoke]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Release smoke must prove the shipped Go frontend works where its build checkout does not exist

## Problem

`go-harness-runtime-embed` fixes the Go frontend's dependence on its compile-time source path and proves it with a local test. The release pipeline needs its own proof, because the published binary is what users run.

`release-publish-and-install-smoke` adds a smoke job to `release.yml` that installs the published binary and runs `shatter explore` on `examples/go/05-conditional-merge.go:Categorize` from a checkout. As drafted, that smoke would **not** catch the relocation bug. A GitHub-hosted runner checks the repo out at the same path the build job used (`/home/runner/work/shatter/shatter`), so `shatter-go/harness/go.mod` exists at the compiled-in path, and the pre-fix binary would pass.

## Evidence

- `shatter-go/instrument/executor.go:112-133` resolves the harness runtime via `runtime.Caller(0)` (the compiled-in source path); see `go-harness-runtime-embed` for the full evidence and repro.
- `.github/workflows/release.yml:175` builds the Go frontend in the runner checkout (`go build -o "../staging/$GO_BINARY" .`).
- `release-publish-and-install-smoke` (audit bucket shatter-ci-workflows) runs its explore smoke "from a checkout with Go set up".
- `shatter-go/workspace/workspace.go:13` `SHATTER_GO_WORKSPACE_ROOT` selects the workspace root; `workspace.GoEnv` (`:194-214`) pins `GOCACHE` under it, so a fresh workspace root gives cold launcher and Go build caches.

## Acceptance criteria

- [ ] The Linux x86_64 smoke leg in `release.yml` runs the installed binary in a state where the build checkout's harness source is unavailable: the examples are checked out to a different path from the build job's (e.g. `actions/checkout` with `path: smoke-src`) **and** `shatter-go/` is deleted from that checkout before the explore runs. `SHATTER_GO_WORKSPACE_ROOT` points at a fresh empty directory. The step asserts that `shatter-go/harness/go.mod` does not exist at the path baked into the binary (`go version -m` or `strings` on the frontend binary gives the path; `test ! -e` on it).
- [ ] The explore in that state exits 0 and reports at least one explored branch.
- [ ] Negative control, recorded once: the same step run against a pre-fix build (a `workflow_dispatch` run on a branch that reverts `go-harness-runtime-embed`, or a build from the parent of that fix) fails with the harness-runtime error. Paste the run URL and the failing log line in the close note.
- [ ] Close-time proof (D1): the URL of a green push-to-main `release.yml` run in which this step executed, with every matrix leg (including Windows and aarch64) `success`. No matrix leg is dropped or made `continue-on-error` to get there.

## Suggested approach

Add the steps to the smoke job that `release-publish-and-install-smoke` creates, rather than a new job. If that job is not yet merged when this is picked up, wait: this issue is blocked by it.

## Out of scope

- The runtime fix itself (`go-harness-runtime-embed`).
- The publish, install.sh and action.yml smoke (`release-publish-and-install-smoke`), and the Windows/aarch64 build fixes.

## Dependencies

- Blocked by: `go-harness-runtime-embed`, `release-publish-and-install-smoke` (which is itself blocked by `release-windows-z3-build`, `release-aarch64-openssl-cross` and `release-publish-guard-and-target`).

## Size

S

## References

- Finding frontend-go-01 (audit 2026-09-22). Split from `go-harness-runtime-embed` after the Codex cross-check (findings 3 and 4 on 01: undeclared release dependency; same-path runner checkout does not prove relocation).

---

<!-- file: 15-go-concolic-escaped-string-miss.md -->
---
slug: go-concolic-escaped-string-miss
kind: new
title: "Concolic Go explore misses `s == \"a\\tb\"` although the runtime constraint carries the real tab: find where the escaped value is lost and fix it"
priority: P2
type: bug
labels: [go-frontend, solver, concolic, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [go-rune-and-escape-literals]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Concolic Go explore misses `s == "a\tb"` although the runtime constraint carries the real tab

## Problem

In the audit probe below, `shatter explore --concolic` never reached `return 1`. The runtime `branch_path` constraint for `s == "a\tb"` already carries the decoded tab (`symextract.go` uses `strconv.Unquote` for STRING literals), so the analyzer's `strings.Trim` bug (fixed by `go-rune-and-escape-literals`) may not explain it. Candidate causes: the orchestrator prefers the static (mangled) constant, the core's string encoding of control characters to/from Z3 loses the tab, or the solved value is re-encoded wrongly on the way back into the execute request. None has been checked.

## Evidence

- Audit probe (finding frontend-go-03), `lit.go`:
  ```go
  func Classify(s string, c rune) int {
      if s == "a\tb" { return 1 }
      if s == "'q'"  { return 2 }
      if c == 'x'    { return 3 }
      return 0
  }
  ```
  `--concolic`, 200 iterations (stopped at 24): 5/7 lines, `return 1` and `return 3` missed. The default explorer reached `return 1` in 60 iterations (probably through literal harvesting, whose `strconv.Unquote` path is correct).
- `shatter-go/instrument/symextract.go:139-150`: STRING literals decoded with `strconv.Unquote`.
- `shatter-go/protocol/analyzer.go:2351-2353`: the static builder's `strings.Trim` (fixed by the blocking issue).

## Acceptance criteria

- [ ] After `go-rune-and-escape-literals` lands, re-run the probe with `--concolic` and record the result in the issue. If `return 1` is now reached, add the E2E case below, record the run, and close with that evidence (no engine change needed).
- [ ] Otherwise, record the root cause in the issue with a trace showing where the tab is lost (solver model value, execute request JSON, or frontend input decoding), and fix it there.
- [ ] Either way, `shatter-core/tests/e2e_concolic_go.rs` gains a known-answer concolic case for string compares against literals containing `\t`, `\n`, `\\` and `\x00`, each arm reached. It fails on the commit before the fix (or, in the no-change case, on main before `go-rune-and-escape-literals`), and passes after; record the failing output.
- [ ] If the defect is in the core's string encoding, a proptest in shatter-core: for random strings including control characters, a string constant encoded to Z3 and back through the model is byte-identical.
- [ ] `task e2e-go` passes (it runs `cargo test --test e2e_concolic_go -- --include-ignored`); paste the cargo summary line showing `0 ignored` and the new test name. `task affected` passes with `Gates selected` recorded.

## Out of scope

- Literal decoding/typing in the Go builders (`go-rune-and-escape-literals`).
- The "3/3 branches" metric while arms are missed (engine-correctness bucket).

## Dependencies

- Blocked by: `go-rune-and-escape-literals`.

## Size

S-M

## References

- Finding frontend-go-03 (audit 2026-09-22; evidence `audits/2026-09-22/areas/frontend-go.md` go-03). Split out of `go-rune-and-escape-literals` after the Codex cross-check (finding 5: its E2E AC required reaching an arm whose cause was out of scope).

---

<!-- file: 16-go-connection-failures-impl.md -->
---
slug: go-connection-failures-impl
kind: new
title: "Go frontend: detect outbound connection failures during execute and emit connection_failures so LiveFirst can fall back to mocks"
priority: P3
type: feature
labels: [go-frontend, parity, protocol, mocking, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go frontend: detect outbound connection failures during execute and emit connection_failures

## Problem

The core's LiveFirst policy falls back to a mock only when the execute result reports `connection_failures`. The Go frontend never reports them (see `go-connection-failures-divergence`, which records the gap in the parity matrix and cites this issue as its tracker). So a Go target with an unreachable live dependency under LiveFirst keeps failing live instead of switching to a mock.

Unlike TS, the Go frontend has no live outbound-call interception point today. Its side-effect handling is a pre-execution classification (`shatter-go/protocol/policy.go:28-36`, `SideEffectClass` values such as `network` and `database`) plus mock substitution at call sites (`shatter-go/instrument/mocksubst.go`); nothing observes a real `net.Dial` failing at runtime. This issue therefore starts by choosing that interception point.

## Evidence

- `git grep -n -i "connection_failures\|ConnectionFailures" -- shatter-go ':!*_test.go'`: no hits (audit worktree, 56c86168).
- `shatter-go/protocol/types.go:206-267` `Response` has no `connection_failures` field.
- `shatter-core/src/explorer.rs:891` `update_live_first_states` reads only `result.connection_failures` (callers `explorer.rs:1602`, `:2545`; `orchestrator.rs:3185`).
- str-3ky9.9.2 (TS reference implementation: dial / connection-refused / DNS classification).

## Acceptance criteria

- [ ] Design note in the issue (before code): where the Go harness observes outbound connection attempts (candidates: wrapping `net/http.DefaultTransport` and `net.Dialer` in the generated harness, or instrumenting call sites that the policy already classifies as `network`/`database`), what it can and cannot see (e.g. a target that builds its own `net.Dialer`), and how failures map to the core's `connection_failures` shape.
- [ ] The Go `Response` carries `connection_failures` with the core's shape, populated for dial errors, connection refused and DNS failures on the intercepted paths, mirroring the TS classification.
- [ ] Known-answer test: a Go fixture under `examples/go/` whose target calls an unreachable live dependency under LiveFirst. The first execute reports a connection failure and a later iteration runs with the mock. Added to `shatter-core/tests/e2e_concolic_go.rs` (or a conformance case); fails on current main and passes on the branch.
- [ ] Both explorer paths are covered (random `explorer.rs` and concolic `orchestrator.rs`), per the parallel-parity rule.
- [ ] `go-connection-failures-divergence`'s matrix entry is updated: `connection_failures` removed from the Go gap (the entry stays for `runtime_crypto_boundaries` unless that is also implemented); `shatter-go/CLAUDE.md` matches.
- [ ] `task e2e-go` passes (it runs `cargo test --test e2e_concolic_go -- --include-ignored`; paste the summary line showing `0 ignored` and the new test name). `task parity`, `task conformance` and `task affected` pass (`Gates selected` recorded).

## Out of scope

- `runtime_crypto_boundaries` for Go (may stay a recorded divergence).
- The Rust counterpart (str-924ca).

## Dependencies

- Blocked by: none. (`go-connection-failures-divergence` should land first so the matrix entry exists to update, but the two can proceed in parallel.)
- Related: `go-connection-failures-divergence`, str-3ky9.9.2, str-924ca, str-2fjn.

## Size

L

## References

- Finding protocol-parity-11 (audit 2026-09-22; evidence `audits/2026-09-22/areas/protocol-parity.md`). Split out of `go-connection-failures-divergence` after the Codex cross-check (finding 12) and the same-runtime review (no live-call hook exists in Go).

---

<!-- file: 17-go-line-zero-records.md -->
---
slug: go-line-zero-records
kind: new
title: "Go instrumenter records line 0 on every execution (synthetic call_enter/call_exit statements), so lines_executed always contains phantom zeros"
priority: P3
type: bug
labels: [go-frontend, coverage, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go instrumenter records line 0 on every execution

## Problem

The Go instrumenter emits a line-record call for every statement, including the synthetic `call_enter`/`call_exit` statements it prepends, whose position resolves to line 0. Only the denominator (`instrumentableLines`) is guarded, so every execution reports `lines_executed: [0, 0, ...]` and every consumer must filter the zeros.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go/instrument/visitor.go:131-139`: `newList = append(newList, makeLineRecordCall(line))` at :132 is unconditional; the `if line > 0` guard at :137 covers only `instrumentableLines`, and the comment there says synthetic statements resolve to line 0.
- Audit explore artifact: `"lines_executed": [0, 0, 4, 7, 10, 13]`.
- str-qo1.12 (closed) fixed a different line-coverage gap.

## Acceptance criteria

- [ ] No line-record call is emitted for `line <= 0`.
- [ ] `instrument/visitor_test.go` asserts that instrumenting a function with a body produces no line-record call with line 0 (fails on main, passes on the branch).
- [ ] An execute-level check: the `lines_executed` of a Go execute response for a small fixture contains no `0` (Go test through the handler, or an assertion added to an existing case in `shatter-core/tests/e2e_concolic_go.rs`). Fails on main, passes on the branch.
- [ ] Any consumer that filters line 0 for Go specifically is left alone or simplified, with the change listed in the close note.
- [ ] `go test ./...` in shatter-go passes. If the E2E file is touched, `task e2e-go` passes (it runs `cargo test --test e2e_concolic_go -- --include-ignored`; paste the summary line showing `0 ignored`). `task affected` passes with `Gates selected` recorded.

## Out of scope

- Other coverage-metric issues (engine-correctness bucket).

## Dependencies

- Blocked by: none.
- Related: str-qo1.12, `go-small-correctness-tidy`.

## Size

XS

## References

- Finding frontend-go-14 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-12). Split out of `go-small-correctness-tidy` after the Codex cross-check (finding 13).

---

<!-- file: 18-go-mock-codegen-json.md -->
---
slug: go-mock-codegen-json
kind: new
title: "Generated Go mock harness embeds mock JSON in a backtick raw string (a backtick in a mock value breaks the build) and ignores json.Unmarshal errors"
priority: P3
type: bug
labels: [go-frontend, mocking, codegen, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Generated Go mock harness embeds mock JSON in a backtick raw string and ignores json.Unmarshal errors

## Problem

The mock harness generator embeds the mock return-value JSON inside a Go raw string literal (backticks). `json.Marshal` does not escape a backtick, so a mock value containing one ends the raw string early and the generated harness fails to compile. The generated code also ignores `json.Unmarshal` errors, so a malformed value silently becomes a zero value and the target runs with a mock return it was never given.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go/instrument/executor.go:225-231`: `retValsJSON, _ := json.Marshal(...)` (error discarded), then line 231 emits the Go source ``json.Unmarshal([]byte(`<json>`), &vals)`` via `fmt.Fprintf` (JSON inside a backtick raw string; the Unmarshal result is unchecked).
- `executor.go:299`: `b.WriteString("\t\tjson.Unmarshal(retvals[idx], &retVal)\n")` (unchecked).
- The backtick breakage is by reasoning, not executed. The first AC below executes it.
- str-qwua7.32 (closed) covered errcheck on frontend code, not on generated code.

## Acceptance criteria

- [ ] First, confirm the defect: a test that generates, compiles and runs a mock harness whose mock return value is a string containing a backtick. Record its failure on main in the issue.
- [ ] Generated mock code embeds the JSON via `strconv.Quote` (an interpreted string literal) and checks both `json.Unmarshal` results; a failure panics with a message naming the mocked function, or is reported through the harness error channel (pick one and say which in `shatter-go/CLAUDE.md`). The generator's own `json.Marshal` error is returned, not discarded.
- [ ] The backtick test passes on the branch. A second test feeds a malformed return-value payload at runtime and asserts the error is reported, not silently zeroed.
- [ ] rapid property: for random strings (including backticks, quotes, backslashes, newlines and non-UTF-8 bytes where JSON allows), the generated mock source parses with `go/parser` and the decoded value round-trips.
- [ ] `go test ./...` in shatter-go passes. `task e2e-go` passes (it runs `cargo test --test e2e_concolic_go -- --include-ignored`; paste the summary line showing `0 ignored`). `task affected` passes with `Gates selected` recorded.

## Out of scope

- Property tests for the other code generators (str-qwua7.48, re-scoped by `go-dead-code-and-property-targets`).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.32, str-qwua7.48, `go-small-correctness-tidy`.

## Size

S

## References

- Finding prior-22 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/prior-audit-regress.md`). Split out of `go-small-correctness-tidy` after the Codex cross-check (finding 13).
