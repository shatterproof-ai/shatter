---
slug: release-windows-z3-build
kind: new
title: "Release: x86_64-pc-windows-msvc build fails in z3-sys ('z3.h' file not found); provide Z3 on Windows and keep the target"
priority: P1
type: bug
labels: [release, ci, distribution, windows, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Release: x86_64-pc-windows-msvc build fails in z3-sys ('z3.h' file not found); provide Z3 on Windows and keep the target

## Problem

`.github/workflows/release.yml` ("Build and Release") has never succeeded. One of its two failing matrix legs is `x86_64-pc-windows-msvc`. The Windows runner has no Z3 installation, so the `z3-sys` build script cannot find `z3.h` when it runs bindgen, and `shatter-cli` never compiles. The release job has `needs: [build-ts, build]`, so this one leg is enough to stop any GitHub release from being published.

Maintainer decision D1 (2026-09-23): Windows stays in the release matrix. This issue fixes the build. Dropping or disabling the target is not an acceptable resolution.

This is one of three issues split from the audit's release finding. The others are `release-aarch64-openssl-cross` (the other failing leg) and `release-publish-and-install-smoke` (publishing and install smoke, blocked by both build fixes).

## Evidence

Re-verified 2026-09-23 against the worktree at `56c86168` and GitHub Actions:

- `gh run list --workflow release.yml -L 300` → `{"cancelled":98,"failure":169}`, 0 successes. The latest run 35773969737 (2026-09-22, push) has these job results: TS, both darwin targets and x86_64-linux `success`; `Build (x86_64-pc-windows-msvc)` `failure`; `Build (aarch64-unknown-linux-gnu)` `failure`; `Create continuous GitHub Release` `skipped`.
- Windows job log (job 106902345979, via `gh api repos/{owner}/{repo}/actions/jobs/106902345979/logs`):
  - `error: failed to run custom build command for 'z3-sys v0.10.7'`
  - `wrapper.h:1:10: fatal error: 'z3.h' file not found`
  - `panicked at ...z3-sys-0.10.7\build.rs:325:14: Unable to generate bindings`
- `.github/workflows/release.yml:87-93`: Windows matrix row (`os: windows-latest`, cargo build tool). No step installs Z3 on Windows. The only Z3 installs are `brew install z3` for macOS (`:124-127`). On Linux, `ci.yml:64` installs `z3`, but release.yml's Linux step (`:115-119`) installs only `libclang-dev`.
- `shatter-core/Cargo.toml:23-24`: `z3 = "0.19"`, `z3-sys = "0.10.7"`, both with default features, which means the system Z3 is linked dynamically.
- z3 0.19.10 / z3-sys 0.10.7 provide these Cargo features: `gh-release` (downloads a prebuilt Z3 from the Z3 GitHub releases at build time), `bundled` (builds Z3 from source with cmake), `static-link-z3` (bundled + static link) and `vcpkg`.
- Only `shatter-cli` (via `shatter-core`) depends on Z3. `shatter-rust` and the Go/TS frontends do not.

## Acceptance criteria

- [ ] The Windows leg of `release.yml` builds `shatter-cli` with Z3 available, using one of: z3 `static-link-z3`/`bundled`, `gh-release`, or `vcpkg`. The chosen mechanism is enabled only for the Windows release build (a target-specific dependency `[target.'cfg(windows)'.dependencies]` or a cargo feature passed from the Windows matrix row). Linux and macOS builds and `task check` must not change behaviour or build time.
- [ ] The staged Windows artifact runs. A release.yml step on the Windows runner executes `staging\shatter.exe --version` successfully. If Z3 is linked dynamically, the needed `libz3.dll` is staged in the artifact and included in `shatter-windows-x86_64.zip`.
- [ ] No "drop from the matrix", `continue-on-error`, or `if:` guard that skips the Windows leg is introduced (D1).
- [ ] Close only with the URL of a `release.yml` run in which `Build (x86_64-pc-windows-msvc)` concluded `success`, pasted in the close reason. "Merged" is not sufficient.

## Suggested approach

1. Try `static-link-z3` first. It gives a self-contained `shatter.exe`, but a cmake build of Z3 on the runner adds roughly 15-30 minutes. Cache `target/` (the workflow already caches it per target) so the cost is paid once per Cargo.lock change.
2. If the build time is unacceptable, use `gh-release` (prebuilt Z3 download) and stage `libz3.dll` next to `shatter.exe`.
3. `vcpkg` (`vcpkg install z3:x64-windows-static-md` with a cached `VCPKG_ROOT`) is the fallback.
4. Iterate with `gh workflow run release.yml --ref <branch>`. It has a `workflow_dispatch` trigger, so iterating does not require pushing to main.

## Out of scope

- The aarch64 openssl/cross failure (`release-aarch64-openssl-cross`).
- Publishing the release and install smoke tests (`release-publish-and-install-smoke`).
- Windows support in `install.sh`, which rejects non-Linux/macOS hosts (`install.sh:30-34`). Windows users install from the zip.

## Dependencies

- Blocks: `release-publish-and-install-smoke`.
- Related: str-lj7s (closed; created the five-target matrix; see `release-reopen-note`), str-74j.1 (closed; cross-platform builds), `workflow-health-patrol`.

Priority: P1 · Type: bug · Labels: release, ci, distribution, windows, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/70, tests-ci-02, prior-02 · Decision: D1
