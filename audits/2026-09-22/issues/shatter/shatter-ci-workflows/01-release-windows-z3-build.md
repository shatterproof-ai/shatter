---
slug: release-windows-z3-build
kind: new
title: "Release: x86_64-pc-windows-msvc build fails (z3-sys 'z3.h' not found, then Unix-only code in shatter-cli); make shatter.exe build and run, keep the target"
priority: P1
type: bug
labels: [release, ci, distribution, windows, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [release-publish-guard-and-target]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Release: x86_64-pc-windows-msvc build fails (z3-sys 'z3.h' not found, then Unix-only code in shatter-cli); make shatter.exe build and run, keep the target

## Problem

`.github/workflows/release.yml` ("Build and Release") has never succeeded. One of its two failing matrix legs is `x86_64-pc-windows-msvc`. There are two layers of failure:

1. **Z3 (the error in the log today).** The Windows runner has no Z3, so the `z3-sys` build script cannot find `z3.h` when it runs bindgen.
2. **Unix-only source code (the next error once Z3 is fixed).** `shatter-cli/src/embedded_go_frontend.rs:2` imports `std::os::unix::fs::PermissionsExt` with no `cfg`, and `shatter-cli/src/main.rs:13` includes the module with no `cfg`. That module will not compile for an MSVC target. `shatter-cli/build.rs` also runs an external `sha256sum` process (`:229-250`) and writes the embedded Go binary as `shatter-go` with no `.exe` extension (`:196`). Installing Z3 alone therefore cannot make this leg pass.

The release job has `needs: [build-ts, build]`, so this one leg is enough to stop any GitHub release from being published.

Maintainer decision D1 (2026-09-23): Windows stays in the release matrix. This issue fixes the build and the runtime of the Windows artifact. Dropping or disabling the target is not an acceptable resolution.

This is one of the issues split from the audit's release finding. The others are `release-publish-guard-and-target` (makes branch iteration safe; blocks this issue), `release-aarch64-openssl-cross` (the other failing leg) and `release-publish-and-install-smoke` (publishing and install smoke, blocked by both build fixes).

## Evidence

Re-verified 2026-09-23 against the audit worktree (main 70465921 plus audit files):

- `gh run list --workflow release.yml -L 300` gives `{"cancelled":98,"failure":169}`, with 0 successes. The latest run, 35773969737 (2026-09-22, push), has these job results: TS, both darwin targets and x86_64-linux `success`; `Build (x86_64-pc-windows-msvc)` `failure`; `Build (aarch64-unknown-linux-gnu)` `failure`; `Create continuous GitHub Release` `skipped`.
- Windows job log (job 106902345979, via `gh api repos/{owner}/{repo}/actions/jobs/106902345979/logs`):
  - `error: failed to run custom build command for 'z3-sys v0.10.7'`
  - `wrapper.h:1:10: fatal error: 'z3.h' file not found`
  - `panicked at ...z3-sys-0.10.7\build.rs:325:14: Unable to generate bindings`
- `.github/workflows/release.yml:87-95`: the Windows matrix row (`os: windows-latest`, cargo build tool, `go-binary: shatter-go.exe`). No step installs Z3 on Windows. The only Z3 install is `brew install z3` for macOS (`:125-127`). The Linux step (`:115-119`) installs only `libclang-dev`.
- `shatter-core/Cargo.toml:23-24`: `z3 = "0.19"` and `z3-sys = "0.10.7"`, both with default features. With no linkage feature, z3-sys links a system Z3.
- z3-sys 0.10.7 features, read from the cargo registry source (`build.rs:25`, `:50-52`, `:100-111`, `:467-469`). The build script requires at most one of `bundled`, `vcpkg`, `gh-release`:
  - `bundled` builds Z3 from source with cmake and links it **statically**.
  - `gh-release` downloads a prebuilt Z3 from the Z3 GitHub releases at build time and links the downloaded `libz3` **statically** (`rustc-link-lib=static=...`). No `libz3.dll` has to be shipped.
  - `vcpkg` links whatever the vcpkg triplet provides.
  - `static-link-z3` is a **deprecated alias** for `bundled`. Its build script prints "The 'static-link-z3' feature is deprecated. Please use the 'bundled' feature."
  - The `z3` crate re-exports these features (`bundled`, `gh-release`, `vcpkg`). Because shatter-core depends on both `z3` and `z3-sys` directly, cargo unifies the features onto the single z3-sys build.
- `shatter-cli/src/embedded_go_frontend.rs:2`: `use std::os::unix::fs::PermissionsExt;`, with no `#[cfg(unix)]`. `EXECUTABLE_PERMISSIONS` (`:14`) is applied through that trait.
- `shatter-cli/build.rs:196-203` runs `go build -o $OUT_DIR/shatter-go` with no Windows extension. `:229-250` shells out to `sha256sum`. `:113,117` shell out to `npm`.
- Only `shatter-cli` (via `shatter-core`) depends on Z3. `shatter-rust` and the Go and TS frontends do not.

## Acceptance criteria

- [ ] **Z3.** The Windows leg of `release.yml` builds `shatter-cli` with Z3 through `bundled`, `gh-release` or `vcpkg`. The feature is enabled only for Windows, through a target-specific dependency (`[target.'cfg(windows)'.dependencies] z3 = { version = "0.19", features = ["gh-release"] }` or similar) or a feature passed from the Windows matrix row. Do not use the deprecated `static-link-z3`. `cargo tree -e features -p shatter-cli --target x86_64-unknown-linux-gnu` shows no z3 linkage feature enabled (Linux and macOS are unchanged).
- [ ] **Source portability.** `shatter-cli` compiles for `x86_64-pc-windows-msvc`:
  - `embedded_go_frontend.rs` gates the permission code behind `#[cfg(unix)]`;
  - `build.rs` names the embedded Go binary with the target's executable suffix (read `CARGO_CFG_TARGET_OS`; do not use `cfg!` in build.rs, which reflects the host);
  - `build.rs` no longer depends on an external `sha256sum`, or proves that the Windows runner provides one. Prefer hashing in-process with a small build-dependency.
- [ ] **Local compile check.** `cargo check -p shatter-cli --target x86_64-pc-windows-msvc` (with the Windows Z3 feature) either passes on a Linux dev box, or the close reason says why a cross `check` is not possible and cites the Windows CI job instead. A unit test covers the executable-name helper, including the `windows` case.
- [ ] **The artifact runs.** In the Windows leg of `release.yml`, after staging, these steps run and pass:
  - `staging\shatter.exe --version`;
  - `staging\shatter.exe explore --max-iterations 3 --timeout-explore 30 examples/go/05-conditional-merge.go:Categorize`, with `SHATTER_ALLOW_HOST_WRITES=1` as `task smoke` sets it. This proves the embedded Go frontend extracts and runs on Windows.

  If the explore fails for a Windows runtime reason outside this issue's scope, the issue stays open. The close reason must not paper over it with a narrower smoke.
- [ ] No "drop from the matrix", `continue-on-error`, or `if:` guard that skips the Windows leg is introduced (D1).
- [ ] **Close-time proof.** Paste the URL of a `release.yml` run in which `Build (x86_64-pc-windows-msvc)` concluded `success` and both smoke steps above ran, in the close reason. "Merged" is not sufficient. A branch run is acceptable only once `release-publish-guard-and-target` has landed, so that the branch run cannot publish.

## Suggested approach

1. Land `release-publish-guard-and-target` first, so that `gh workflow run release.yml --ref <branch>` builds without publishing.
2. Try `gh-release` first. It is a prebuilt static link with no cmake build. Fall back to `bundled` (static, but a cmake build of Z3 adds roughly 15-30 minutes per cold cache) and then to `vcpkg` (`z3:x64-windows-static-md` with a cached `VCPKG_ROOT`).
3. Fix the `cfg(unix)` gating and the build.rs portability together, since the next error after Z3 will be the `PermissionsExt` import.

## Out of scope

- The aarch64 openssl and cross failure (`release-aarch64-openssl-cross`).
- Publishing the release and the install smoke tests (`release-publish-and-install-smoke`).
- Windows support in `install.sh`, which rejects non-Linux and non-macOS hosts (`install.sh:30-34`). Windows users install from the zip.
- Changing Linux or macOS Z3 linkage. `release-publish-and-install-smoke` owns the runtime libz3 question for those targets.

## Dependencies

- Blocked by: `release-publish-guard-and-target`.
- Blocks: `release-publish-and-install-smoke`.
- Related: str-lj7s (closed; created the five-target matrix; see `release-reopen-note`), str-74j.1 (closed; cross-platform builds), `workflow-health-patrol`.

Priority: P1 · Type: bug · Labels: release, ci, distribution, windows, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/70, tests-ci-02, prior-02 · Decision: D1
