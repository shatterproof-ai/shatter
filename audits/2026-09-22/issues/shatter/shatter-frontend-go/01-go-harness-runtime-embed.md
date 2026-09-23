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
