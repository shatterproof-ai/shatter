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
