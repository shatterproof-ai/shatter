---
slug: release-aarch64-openssl-cross
kind: new
title: "Release: aarch64-unknown-linux-gnu build fails at openssl-sys under cross, and would embed an x86_64 Go frontend; fix both and prove the arm64 binary runs"
priority: P1
type: bug
labels: [release, ci, distribution, cross, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [release-publish-guard-and-target]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Release: aarch64-unknown-linux-gnu build fails at openssl-sys under cross, and would embed an x86_64 Go frontend; fix both and prove the arm64 binary runs

## Problem

The `aarch64-unknown-linux-gnu` leg of `.github/workflows/release.yml` builds `shatter-cli` with `cross`, and it fails on every run. `openssl-sys` cannot find an OpenSSL installation for the aarch64 target inside the cross container. Together with the Windows leg, this keeps the release job (`needs: [build-ts, build]`) skipped, so no GitHub release has ever been published.

More failures are queued behind openssl:

1. **arm64 Z3.** The cross image needs arm64 Z3 headers and libraries for `z3-sys`. The repo used to supply them through `Cross.toml` and `cross/Dockerfile.aarch64-unknown-linux-gnu`. str-qwua7.41 (commit 5abb7bd5, 2026-09-06) deleted both on the premise that "no workflow invokes cross". That premise was false, because `release.yml` uses cross. The build was already failing at openssl-sys before that commit, so the deletion did not cause the current failure. It did remove the Z3 setup the fix will need.
2. **Embedded Go frontend built for the wrong architecture.** `shatter-cli/build.rs:196-203` runs `go build` with no `GOOS`/`GOARCH`, so it builds for the machine running the build script. `release.yml` sets `GOOS`/`GOARCH` (`:168-175`) only for the separately staged `shatter-go` binary, not for the copy that build.rs embeds into `shatter`. Under cross, build.rs also needs `go`, `npm` (`:113,117`) and `sha256sum` (`:229-250`) inside the container. A cross build that succeeded would therefore ship an arm64 `shatter` that carries an x86_64 (or missing) Go frontend.

A green cross build plus the existing x86_64 smoke would not detect problem 2. This issue therefore requires an arm64 runtime smoke.

Maintainer decision D1 (2026-09-23): aarch64 Linux stays in the release matrix. This issue fixes the build and proves the artifact runs. Dropping or disabling the target is not an acceptable resolution.

## Evidence

Re-verified 2026-09-23 against the audit worktree (main 70465921 plus audit files):

- `gh run list --workflow release.yml -L 300` gives `{"cancelled":98,"failure":169}`, with 0 successes. In the latest run, 35773969737, `Build (aarch64-unknown-linux-gnu)` is `failure`.
- aarch64 job log (job 106902345926):
  - `warning: openssl-sys@0.9.116: Could not find directory of OpenSSL installation`
  - `error: failed to run custom build command for 'openssl-sys v0.9.116'`
  - `AARCH64_UNKNOWN_LINUX_GNU_OPENSSL_LIB_DIR unset`, `OPENSSL_LIB_DIR unset`
- `.github/workflows/release.yml:57-64`: the aarch64 row, with `cli-build-tool: cross`, `go-os: linux` and `go-arch: arm64`. `:121-123`: `cargo install cross --git https://github.com/cross-rs/cross`. `:142-144`: `cross build --release --target ${{ matrix.target }} -p shatter-cli`. `:157-160`: the cross build of shatter-rust. `:168-175`: the staged Go build with `GOOS`/`GOARCH`.
- Where openssl comes from: `cargo tree -p shatter-cli -i openssl-sys -e normal` shows `openssl-sys ← native-tls ← hyper-tls/reqwest 0.12.28 ← shatter-llm ← shatter-cli`. `shatter-llm/Cargo.toml:15` has `reqwest = { version = "0.12", features = ["json"] }`, which uses default features and so pulls in native-tls. `shatter-rust` has no openssl-sys dependency.
- `shatter-cli/build.rs:194-212`: `Command::new("go").args(["build", "-buildvcs=false", "-o"])...` with no target mapping. The resulting bytes are embedded by `shatter-cli/src/embedded_go_frontend.rs:7` (`include_bytes!(concat!(env!("OUT_DIR"), "/shatter-go"))`) and extracted and executed at runtime.
- The deleted cross config can be recovered with `git show 5abb7bd5^:Cross.toml` and `git show 5abb7bd5^:cross/Dockerfile.aarch64-unknown-linux-gnu`:
  - `Cross.toml`: `[target.aarch64-unknown-linux-gnu] dockerfile = "cross/Dockerfile.aarch64-unknown-linux-gnu"`
  - Dockerfile: `FROM ghcr.io/cross-rs/aarch64-unknown-linux-gnu:main`, `dpkg --add-architecture arm64`, `apt-get install -y libclang-dev libz3-dev:arm64`
- The 5abb7bd5 commit message says "the unused aarch64 cross-compile Dockerfile and Cross.toml (no workflow invokes cross)".
- GitHub provides native arm64 Linux runners (`ubuntu-24.04-arm`) for public repositories. With one, the leg can build natively and run a smoke on the target architecture.

## Acceptance criteria

- [ ] **openssl.** `openssl-sys` no longer blocks the aarch64 build. Fix it one of these ways:
  - (preferred) switch `shatter-llm`'s reqwest to `default-features = false, features = ["json", "rustls-tls"]`, so that `cargo tree -p shatter-cli -i openssl-sys` reports no match;
  - use vendored OpenSSL (`native-tls-vendored`);
  - install `libssl-dev:arm64` in a restored cross pre-build.
- [ ] If rustls is chosen, `cargo test -p shatter-llm` still passes, and one real HTTPS request through shatter-llm's client succeeds on x86_64 Linux. Record the command and its output in the close reason.
- [ ] **Embedded Go frontend matches the target.** `shatter-cli/build.rs` maps the cargo target (`CARGO_CFG_TARGET_OS` / `CARGO_CFG_TARGET_ARCH`) to `GOOS` / `GOARCH` and sets `CGO_ENABLED=0` for the embedded `go build`. A unit test covers the mapping for at least linux/x86_64, linux/aarch64, darwin/aarch64 and windows/x86_64.
- [ ] **Z3 for arm64.** Z3 is provided to the build in one of these ways:
  - build the leg natively on `ubuntu-24.04-arm` with `libz3-dev` installed;
  - restore `Cross.toml` with a `pre-build` or `dockerfile` that installs `libz3-dev:arm64`, `libclang-dev`, Go and Node. If you restore it, add a comment in `Cross.toml` naming `release.yml` as its consumer, so it is not deleted as dead again;
  - enable z3 `gh-release` or `bundled` (both link statically) for this target only.
- [ ] **arm64 runtime smoke.** A job on an arm64 runner (`ubuntu-24.04-arm`) downloads this leg's staged artifact, then:
  - runs `./shatter --version`;
  - runs `file` on the Go frontend that `shatter` extracts to its cache dir, and asserts the output contains `ARM aarch64`;
  - runs `./shatter explore --max-iterations 3 --timeout-explore 30 examples/go/05-conditional-merge.go:Categorize` with `SHATTER_ALLOW_HOST_WRITES=1`, which must exit 0.

  This job has to fail on the current design. Before the build.rs mapping lands, run it once with the fix reverted and show that it goes red, or explain in the close reason why that red run was impossible (for example, because the build itself had not yet gone green).
- [ ] No drop-from-matrix, `continue-on-error`, or skip guard for the aarch64 leg (D1).
- [ ] **Close-time proof.** Paste the URL of a `release.yml` run in which `Build (aarch64-unknown-linux-gnu)` and the arm64 smoke job both concluded `success`, in the close reason. A branch run is acceptable only once `release-publish-guard-and-target` has landed.

## Suggested approach

Switching to rustls removes openssl from every target and is a one-line Cargo change. After that, a native `ubuntu-24.04-arm` build is probably simpler than restoring cross: Go, Node, sha256sum and `libz3-dev` all install normally, and the smoke runs on the same runner. If you keep cross, restore the config from `5abb7bd5^` and add Go and Node to the image. Either way, the build.rs `GOOS`/`GOARCH` mapping is needed, because it also affects `x86_64-apple-darwin` builds on non-Intel hosts. Record which approach you chose in the close reason.

## Out of scope

- The Windows Z3 and portability failure (`release-windows-z3-build`).
- Publishing and the install smoke tests (`release-publish-and-install-smoke`).

## Dependencies

- Blocked by: `release-publish-guard-and-target`.
- Blocks: `release-publish-and-install-smoke`.
- Related: str-qwua7.41 (closed; deleted Cross.toml and cross/ on a false premise), str-lj7s (see `release-reopen-note`), str-74j.1.

Priority: P1 · Type: bug · Labels: release, ci, distribution, cross, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/70, tests-ci-02, prior-02 · Decision: D1
