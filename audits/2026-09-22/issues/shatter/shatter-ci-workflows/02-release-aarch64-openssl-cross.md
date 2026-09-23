---
slug: release-aarch64-openssl-cross
kind: new
title: "Release: aarch64-unknown-linux-gnu cross build fails at openssl-sys (and needs arm64 Z3 next); fix it and keep the target"
priority: P1
type: bug
labels: [release, ci, distribution, cross, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Release: aarch64-unknown-linux-gnu cross build fails at openssl-sys (and needs arm64 Z3 next); fix it and keep the target

## Problem

The `aarch64-unknown-linux-gnu` leg of `.github/workflows/release.yml` builds `shatter-cli` with `cross`, and it fails every run. `openssl-sys` cannot find an OpenSSL installation for the aarch64 target inside the cross container. Along with the Windows leg, this keeps the release job (`needs: [build-ts, build]`) skipped, so no GitHub release has ever been published.

A second failure is waiting behind the first. The cross image also needs arm64 Z3 headers and libraries for `z3-sys`. The repo used to supply them through `Cross.toml` and `cross/Dockerfile.aarch64-unknown-linux-gnu`. str-qwua7.41 (commit 5abb7bd5, 2026-09-06) deleted both on the premise that "no workflow invokes cross". That premise was false: `release.yml` uses cross. The build was already failing at openssl-sys before that commit, so the deletion did not cause this failure. It did remove the Z3 setup the fix will need.

Maintainer decision D1 (2026-09-23): aarch64 Linux stays in the release matrix. This issue fixes the build. Dropping or disabling the target is not an acceptable resolution.

## Evidence

Re-verified 2026-09-23 against the worktree at `56c86168` and GitHub Actions:

- `gh run list --workflow release.yml -L 300` → `{"cancelled":98,"failure":169}`, 0 successes. In latest run 35773969737, `Build (aarch64-unknown-linux-gnu)` is `failure`.
- aarch64 job log (job 106902345926):
  - `warning: openssl-sys@0.9.116: Could not find directory of OpenSSL installation`
  - `error: failed to run custom build command for 'openssl-sys v0.9.116'`
  - `AARCH64_UNKNOWN_LINUX_GNU_OPENSSL_LIB_DIR unset`, `OPENSSL_LIB_DIR unset`
- `.github/workflows/release.yml:57-62`: aarch64 row with `cli-build-tool: cross`. `:121-123`: `cargo install cross --git https://github.com/cross-rs/cross`. `:142-144`: `cross build --release --target ${{ matrix.target }} -p shatter-cli`. `:157-160`: cross build of shatter-rust.
- Where openssl comes from: `cargo tree -p shatter-cli -i openssl-sys -e normal` → `openssl-sys ← native-tls ← hyper-tls/reqwest 0.12.28 ← shatter-llm ← shatter-cli`. `shatter-llm/Cargo.toml:15`: `reqwest = { version = "0.12", features = ["json"] }` (default features, which means native-tls). `shatter-rust` has no openssl-sys dependency.
- Deleted cross config, recoverable with `git show 5abb7bd5^:Cross.toml` and `git show 5abb7bd5^:cross/Dockerfile.aarch64-unknown-linux-gnu`:
  - `Cross.toml`: `[target.aarch64-unknown-linux-gnu] dockerfile = "cross/Dockerfile.aarch64-unknown-linux-gnu"`
  - Dockerfile: `FROM ghcr.io/cross-rs/aarch64-unknown-linux-gnu:main`, `dpkg --add-architecture arm64`, `apt-get install -y libclang-dev libz3-dev:arm64`
- The 5abb7bd5 commit message says: "the unused aarch64 cross-compile Dockerfile and Cross.toml (no workflow invokes cross)".

## Acceptance criteria

- [ ] `openssl-sys` no longer blocks the aarch64 build, fixed one of these ways:
  - (preferred) switch `shatter-llm`'s reqwest to `default-features = false, features = ["json", "rustls-tls"]`, so that `cargo tree -p shatter-cli -i openssl-sys` reports no match;
  - use vendored OpenSSL (`native-tls-vendored`);
  - install `libssl-dev:arm64` in a restored cross pre-build.
- [ ] Z3 for arm64 is provided to the cross build: restore `Cross.toml` with a `pre-build` or `dockerfile` that installs `libz3-dev:arm64` and `libclang-dev`, or enable a z3 `bundled`/`gh-release` feature for this target only. Add a comment in `Cross.toml` naming `release.yml` as its consumer so it is not deleted as dead again.
- [ ] If rustls is chosen, `cargo test -p shatter-llm` still passes, and an LLM HTTPS request path (the existing shatter-llm tests or a manual `shatter` LLM call) still works on x86_64 Linux.
- [ ] No drop-from-matrix, `continue-on-error`, or skip guard for the aarch64 leg (D1).
- [ ] Close only with the URL of a `release.yml` run in which `Build (aarch64-unknown-linux-gnu)` concluded `success`, pasted in the close reason.

## Suggested approach

Switching to rustls removes openssl from every target and is a one-line Cargo change. Then restore the Cross config from `5abb7bd5^` (git history) for Z3, and iterate with `gh workflow run release.yml --ref <branch>`. Expect the Z3 link step to be the next error once openssl is gone.

## Out of scope

- The Windows Z3 failure (`release-windows-z3-build`).
- Publishing and install smoke tests (`release-publish-and-install-smoke`).
- Switching the aarch64 leg to a native `ubuntu-24.04-arm` runner instead of cross. That is acceptable only if it is simpler, and the implementer must record the choice in the close reason.

## Dependencies

- Blocks: `release-publish-and-install-smoke`.
- Related: str-qwua7.41 (closed; deleted Cross.toml/cross/ on a false premise), str-lj7s (see `release-reopen-note`), str-74j.1.

Priority: P1 · Type: bug · Labels: release, ci, distribution, cross, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/70, tests-ci-02, prior-02 · Decision: D1
