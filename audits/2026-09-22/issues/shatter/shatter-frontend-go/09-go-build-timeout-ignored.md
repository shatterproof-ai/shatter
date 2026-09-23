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
