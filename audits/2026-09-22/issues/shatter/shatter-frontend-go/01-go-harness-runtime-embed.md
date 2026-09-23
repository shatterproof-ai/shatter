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
