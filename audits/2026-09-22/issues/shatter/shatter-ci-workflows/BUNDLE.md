# Bundle: shatter-ci-workflows (audit 2026-09-22)

- **Bucket:** shatter-ci-workflows. GitHub workflows that are permanently red or ungated: release matrix, drift patrol, workflow health, linters/formatters, CI user paths.
- **Repo / tracker:** shatter. bd in /home/ketan/project/shatter (prefix str).
- **Parent epic:** Epic: Audit 2026-09-22 findings.
- **Status:** final drafts, revised 2026-09-23 after the Codex cross-check (see REVISION.md). Nothing is filed (D6). Evidence was re-verified 2026-09-23 against the audit worktree (main 70465921 plus audit files), live `gh run list` / job logs, the cargo registry source, and `bd show`.

## Maintainer decisions (2026-09-23); these override the report and the old drafts

- **D1 Releases.** Keep x86_64-pc-windows-msvc and aarch64-unknown-linux-gnu in the release matrix. The drafts fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64) and do not drop them. Release work closes only with a green release-run URL.
- **D2 shatter diff.** Retire the snapshot `shatter diff` command and the unused Snapshot writer path; spec-diff is the regression tool. Update SPEC/README/QUICKSTART. str-81xiw decides whether diff-scoped exploration later takes the freed `diff` name. The shatter-agents plugin's `shatter diff --staged` docs get corrected. (Not used in this bucket.)
- **D3 Concolic positioning.** Measure first: P1 benchmark comparing default and concolic, P1 fix for concolic early termination, then a follow-up decision issue. No doc softening now. (Not used in this bucket.)
- **D4 Beads hook stall.** Retire the JSONL import in shatter and move tracker sync to a Dolt remote. The first step checks for clobbered DB state. AGENTS.md drops `bd sync`, and str-qwua7.28 is superseded. No BEADS_HOOK_TIMEOUT or hook-bypass guidance. (Touches this bucket only in drift-patrol-workflow-go-mod: the CI patrol reads .beads/issues.jsonl, and beads-jsonl-consumers-drop-bd-sync owns changing that.)
- **D5 Git identity.** The leaked [user] section is already removed. Add .mailmap, a git-state check and a fixture .git/config snapshot. (Not used in this bucket.)
- **D6 Filing.** After reconciliation and the Codex cross-check, the maintainer runs one filer script. No agent files anything. Follow-ups that drafts used to ask implementers to file (devcontainer, docker-publish, Ubuntu 26 trial) are now pre-drafted in this bundle. Follow-ups that drafts used to ask implementers to file (devcontainer, docker-publish, Ubuntu 26 trial) are now pre-drafted in this bundle.

## Entries

| # | Slug | Kind | P | Existing | Blocked by | Title |
|---|---|---|---|---|---|---|
| 01 | release-windows-z3-build | new | P1 | - | [release-publish-guard-and-target] | Release: x86_64-pc-windows-msvc build fails (z3-sys 'z3.h' not found, then Unix-only code in shatter-cli); make shatter.exe build and run, keep the target |
| 02 | release-aarch64-openssl-cross | new | P1 | - | [release-publish-guard-and-target] | Release: aarch64-unknown-linux-gnu build fails at openssl-sys under cross, and would embed an x86_64 Go frontend; fix both and prove the arm64 binary runs |
| 03 | release-publish-and-install-smoke | new | P1 | - | [release-windows-z3-build, release-aarch64-openssl-cross, release-publish-guard-and-target] | Release: publish the first continuous-* prerelease for all five targets and smoke-test install.sh and action.yml against it, inside release.yml, on clean runners |
| 04 | release-reopen-note | reopen-note | P1 | str-lj7s | - | Comment on closed str-lj7s: closed on 'landed' with no green run; release.yml has 0 successes in 267 runs |
| 05 | drift-patrol-workflow-go-mod | new | P1 | - | - | Fix the scheduled Drift Patrol workflow (setup-go points at a nonexistent root go.mod; 7/7 scheduled runs red) and test every workflow path |
| 06 | drift-patrol-reopen-note | reopen-note | P1 | str-u394l.1 | - | Comment on closed str-u394l.1: the scheduled Drift Patrol has failed 7/7 runs (setup-go points at a nonexistent root go.mod) |
| 07 | workflow-health-patrol | new | P1 | - | [drift-patrol-workflow-go-mod] | Surface persistently red GitHub workflows to agents: an authenticated, self-safe drift-patrol workflow-health check plus a landing step |
| 08 | go-lint-and-gofmt-gated | new | P2 | - | - | Go lint is red on main (10 golangci-lint issues, 10 non-gofmt files) and ungated: fix the findings and gate golangci-lint and gofmt in check-static |
| 09 | go-lint-reopen-note | reopen-note | P2 | str-2tyfk | - | Comment on closed str-2tyfk: the unused/govet residuals it deferred were never filed, and its 'silently no-ops' diagnosis was wrong (nothing invokes go:lint) |
| 10 | rustfmt-gate | new | P2 | - | - | Restore rustfmt cleanliness in one dedicated commit (107 files drifted after str-fr1v) and gate `cargo fmt --check` in check-static |
| 11 | rustfmt-reopen-note | reopen-note | P2 | str-fr1v | - | Comment on closed str-fr1v: the tree drifted again (58-file churn on 2026-09-21; 107 files unformatted on 09-23); no fmt gate exists |
| 12 | ci-runs-user-paths | new | P2 | - | - | Smoke, walkthrough and E2E user paths never run in CI: add a push-to-main user-paths job that fails on a real regression |
| 13 | workflow-action-versions | new | P3 | - | - | Workflows use deprecated Node-20 action majors, unpinned ubuntu-latest (Ubuntu 26 from 2026-10-19), and setup-go cache that cannot find go.sum |
| 14 | nextest-ci-profile-and-stale-parity-fallback | new | P3 | - | - | CI runs plain `cargo test` while local gates use nextest; both nextest configs carry an unused `[profile.ci]` and `fail-fast = true`: pick one runner policy and make CI apply it |
| 15 | release-publish-guard-and-target | new | P1 | - | - | Release: guard the publish job to push-on-main and pass --target $GITHUB_SHA, so branch runs of release.yml build without publishing |
| 16 | devcontainer-workflow-red | new | P2 | - | - | Devcontainer CI red since February: post-create.sh runs `bd init --from-jsonl`, which bd now refuses because origin has Dolt history |
| 17 | docker-publish-workflow-red | new | P2 | - | - | Docker image build fails (7/7): Dockerfile never copies shatter-llm, so `cargo build -p shatter-cli` cannot load the workspace |
| 18 | go-lint-qwua7-32-note | reopen-note | P2 | str-qwua7.32 | - | Comment on closed str-qwua7.32: acceptance 'task go:lint passes' was false at close; golangci-lint still reports 10 issues |
| 19 | ubuntu-26-runner-trial | new | P3 | - | [workflow-action-versions] | Trial the CI and release workflows on Ubuntu 26 runners and lift the ubuntu-24.04 pin once they pass |
| 20 | perf-ci-stable-scenarios-red | new | P2 | - | - | Perf CI red 13/13: gauntlet-auto-warm exits 1 with its output suppressed; surface child output in perf_runner.py, then fix the scenario |
| 21 | parity-governed-stale-fallback | new | P3 | - | - | parity-governed keeps a dead 'pending str-7jgm.2' fallback that would turn a deleted validate-parity.py into a silent skip |

Dependency edges: release-publish-guard-and-target blocks release-windows-z3-build; release-publish-guard-and-target blocks release-aarch64-openssl-cross; release-windows-z3-build blocks release-publish-and-install-smoke; release-aarch64-openssl-cross blocks release-publish-and-install-smoke; release-publish-guard-and-target blocks release-publish-and-install-smoke; drift-patrol-workflow-go-mod blocks workflow-health-patrol; workflow-action-versions blocks ubuntu-26-runner-trial. Soft relations are given in each body, for example the str-qwua7.3 fix / `ci-executed-leaf-guard` for ci-runs-user-paths and nextest.

Reconciliation notes for the reviewer:
- Revision 2026-09-23 (REVISION.md): four splits and one new guard issue. release-publish-guard-and-target (15) is new and blocks both release build fixes. workflow-health-patrol no longer files anything; its devcontainer and docker-publish follow-ups are drafts 16 and 17. The go-lint note is split per ticket (09 for str-2tyfk, 18 for str-qwua7.32). workflow-action-versions' Ubuntu 26 follow-up is draft 19. ci-runs-user-paths lost the perf-ci work to draft 20. nextest-ci-profile-and-stale-parity-fallback lost the parity fallback to draft 21; its slug is kept for stability.
- code/73 and code/07 said "blocked by draft 01" (the Task checksum fix, now a note on str-qwua7.3). The manifest's dependency edges do not include that, so it is recorded as a relation, not a blocker.
- devcontainer-workflow-red coordinates with the tracker bucket's D4 drafts (beads-retire-jsonl-import-dolt-remote, beads-jsonl-consumers-drop-bd-sync); that is a relation, not a blocked_by edge, because blocked_by stays within the bucket.


---

<!-- file: 01-release-windows-z3-build.md -->

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


---

<!-- file: 02-release-aarch64-openssl-cross.md -->

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


---

<!-- file: 03-release-publish-and-install-smoke.md -->

---
slug: release-publish-and-install-smoke
kind: new
title: "Release: publish the first continuous-* prerelease for all five targets and smoke-test install.sh and action.yml against it, inside release.yml, on clean runners"
priority: P1
type: bug
labels: [release, ci, distribution, install, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [release-windows-z3-build, release-aarch64-openssl-cross, release-publish-guard-and-target]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Release: publish the first continuous-* prerelease for all five targets and smoke-test install.sh and action.yml against it, inside release.yml, on clean runners

## Problem

No GitHub release of Shatter exists. `install.sh` resolves the latest `continuous-*` prerelease, and `action.yml` calls `install.sh`, so both documented install paths fail for every user today. Nothing in CI exercises either path, which is why this went unnoticed through 267 red release runs.

Under D1 the full five-target matrix ships, so this issue waits for both build fixes and the publish guard. It does not add a partial-matrix publish. Once the builds are green, this issue proves that the release job publishes, that the install paths work end to end, and that the published Linux and macOS binaries do not depend on a libz3 the user may not have. It then keeps all of this tested.

Two constraints decide where the smoke lives:

- The release is created with `GITHUB_TOKEN` (`release.yml:288-290`). Events created with `GITHUB_TOKEN` do not start new workflow runs, so a separate workflow on `release: published` would never fire.
- `workflow-health-patrol` watches workflows triggered by push to main or by schedule.

The smoke job therefore goes **in release.yml**, with `needs: release`.

## Evidence

Re-verified 2026-09-23:

- `gh release list` is empty. `gh run list --workflow release.yml -L 300` gives `{"cancelled":98,"failure":169}`, with 0 successes. In run 35773969737, `Create continuous GitHub Release` is `skipped`.
- `.github/workflows/release.yml:183-186`: the release job has `needs: [build-ts, build]`, so any failed matrix leg skips it. `:190-201` builds the tag `continuous-${stamp}-${short_sha}`. `:207-236` packages the five archives and `SHA256SUMS`. `:238-280` writes the platform manifest. `:295-297` runs `gh release create ... --prerelease`.
- `install.sh:64-81` calls `https://api.github.com/repos/${REPO}/releases?per_page=100` and picks the first `prerelease` whose tag starts with `continuous-`. With no release, it errors `Could not determine latest build`. `install.sh:30-40` supports only Linux and macOS, each on x86_64 or aarch64.
- `action.yml` is a composite action. `action.yml:47` runs `bash "${{ github.action_path }}/install.sh"` and exposes `version` from `shatter --version`.
- **Runtime libz3.** README.md:63-71 says "`shatter-core` links the system Z3 library". The Linux and macOS legs link a system libz3 dynamically (default z3-sys features; `brew install z3` on macOS at `release.yml:125-127`). Neither `install.sh` nor `action.yml` installs or checks for libz3. A runner image that happens to ship libz3 would hide the missing dependency. A clean macOS machine without brew z3 would fail with a loader error.
- **Warm cache.** In run 35773969737 the x86_64-linux leg restored a cargo cache (`x86_64-unknown-linux-gnu-cargo-056bf6...`) and did not rebuild z3-sys. That leg installs only `libclang-dev`, not `libz3-dev`, so it is unverified whether a cold build still succeeds.
- `analyze` (`shatter-cli/src/args.rs:1175-1180`) reads saved observation JSON and "requires no frontend or solver". It cannot serve as an install smoke.
- `.github/workflows/cleanup-continuous-releases.yml` (weekly) already applies retention to continuous prereleases (str-fl9g.6), but there has never been a release to retain.
- No workflow invokes `install.sh` or `uses: ./` (checked by grep of `.github/workflows/`).

## Acceptance criteria

- [ ] **Publish.** A push-to-main `release.yml` run completes with every job `success`, including `Create continuous GitHub Release`. `gh release view <tag>` then shows all five archives (`shatter-linux-x86_64.tar.gz`, `shatter-linux-aarch64.tar.gz`, `shatter-macos-x86_64.tar.gz`, `shatter-macos-aarch64.tar.gz`, `shatter-windows-x86_64.zip`), `SHA256SUMS` and the manifest, and the tag points at the run's `GITHUB_SHA`.
- [ ] **libz3 decision.** Each Linux and macOS artifact is one of:
  - self-contained: z3 `gh-release` or `bundled`, both static; `ldd shatter` or `otool -L shatter` shows no `libz3`;
  - documented and checked: `install.sh` and `action.yml` detect a missing libz3, print the install command, and exit non-zero; README states the prerequisite.

  Record the choice in the close reason. If the choice changes Linux or macOS linkage, `release-windows-z3-build`'s "Linux/macOS unchanged" constraint does not apply to this issue.
- [ ] **Smoke job in release.yml.** A job with `needs: release` receives the new tag through a job output. It runs on `ubuntu-latest` (x86_64), `ubuntu-24.04-arm` (aarch64) and one macOS runner. On each runner it:
  - asserts before installing that `libz3` is absent from the runner (`ldconfig -p | grep libz3` or `brew list z3` must find nothing; remove it first if the image ships it). This makes the libz3 decision above observable;
  - runs `curl -fsSL .../install.sh | bash` twice: once with `BUILD=<new tag>`, and once with `BUILD` unset, which exercises "latest" resolution;
  - verifies the downloaded archive against `SHA256SUMS`, and fails on a mismatch;
  - runs `shatter --version`, then `shatter explore --max-iterations 3 --timeout-explore 30 examples/go/05-conditional-merge.go:Categorize` from a checkout with Go set up and `SHATTER_ALLOW_HOST_WRITES=1`. The explore must exit 0 and report at least one explored branch. This exercises the embedded Go frontend and the solver, which `analyze` would not.
- [ ] A second job uses the composite action (`uses: ./` with `build: <new tag>`) and asserts that its `version` output is non-empty and equals the `shatter --version` output of the binary it installed.
- [ ] **Cold build.** At least one Linux x86_64 leg in the proving run ran without a cargo cache hit (delete the cache or bump the key), so z3-sys was really rebuilt. Cite that job's log line in the close reason.
- [ ] **Monitoring.** Because the smoke is part of `release.yml` (push to main), `workflow-health-patrol` covers it with no extra wiring. Confirm this in the close reason by citing that check's output listing `release.yml`.
- [ ] **Close-time proof.** The close reason contains the URL of the green push-to-main `release.yml` run, with the release, both smoke jobs and the cold-cache leg all `success`.

## Suggested approach

Pass the tag from the release job as an output. Run the three OS smokes as a matrix. Do the libz3 decision first, because it decides what the absent-libz3 assertion expects. If Windows is to be covered by the action later, install.sh first needs Windows support, which is out of scope here.

## Out of scope

- Fixing the Windows or aarch64 builds (`release-windows-z3-build`, `release-aarch64-openssl-cross`).
- The publish guard and `--target` (`release-publish-guard-and-target`).
- Adding Windows support to `install.sh`.
- Changing retention policy (`cleanup-continuous-releases.yml`).

## Dependencies

- Blocked by: `release-windows-z3-build`, `release-aarch64-openssl-cross`, `release-publish-guard-and-target`.
- Related: str-lj7s (closed; see `release-reopen-note`), str-fl9g.6 (retention), `workflow-health-patrol`. Cross-repo: shatter-agents `cli-contract-test` and `wire-shatter-ci-standalone` cite this issue as the source of a downloadable release.

Priority: P1 · Type: bug · Labels: release, ci, distribution, install, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/70, tests-ci-02, prior-02 · Decision: D1


---

<!-- file: 04-release-reopen-note.md -->

---
slug: release-reopen-note
kind: reopen-note
title: "Comment on closed str-lj7s: closed on 'landed' with no green run; release.yml has 0 successes in 267 runs"
priority: P1
type: note
labels: [release, ci, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-lj7s
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-lj7s

Target: **str-lj7s** (closed). Post as a comment only. Do not reopen.

Comment text:

> Audit 2026-09-22 follow-up. This issue was closed on "landed on main" (ab2a8515) with no green `release.yml` run cited. The workflow has never succeeded since: as of 2026-09-23, `gh run list --workflow release.yml -L 300` shows 169 failures, 98 cancellations and 0 successes, and `gh release list` is empty. As a result, `install.sh` and the `action.yml` GitHub Action cannot install Shatter for anyone.
>
> Two matrix legs fail on every run (latest: run 35773969737):
> - `x86_64-pc-windows-msvc`: `z3-sys v0.10.7 ... wrapper.h:1:10: fatal error: 'z3.h' file not found`
> - `aarch64-unknown-linux-gnu` (cross): `failed to run custom build command for openssl-sys v0.9.116`
>
> The release job `needs` every leg, so it is skipped every time.
>
> Maintainer decision D1 (2026-09-23): both targets stay in the matrix and get fixed. The work is tracked in four new issues:
> - `<id of release-publish-guard-and-target>`: restrict publishing to push-on-main and pass `--target $GITHUB_SHA`, so branch runs can iterate on the fixes without publishing
> - `<id of release-windows-z3-build>`: the Windows Z3 build, plus the Unix-only code in shatter-cli that fails next
> - `<id of release-aarch64-openssl-cross>`: the aarch64 openssl/cross build, plus build.rs embedding a host-arch Go frontend (str-qwua7.41 deleted Cross.toml and cross/ on the false premise that no workflow uses cross)
> - `<id of release-publish-and-install-smoke>`: the first published continuous-* prerelease, and install.sh/action.yml smoke tests on clean runners inside release.yml
>
> Each closes only with a green release-run URL. Release work should not be closed on "landed" again.

(Filer: replace the `<id of ...>` placeholders with the ids assigned to those slugs.)


---

<!-- file: 05-drift-patrol-workflow-go-mod.md -->

---
slug: drift-patrol-workflow-go-mod
kind: new
title: "Fix the scheduled Drift Patrol workflow (setup-go points at a nonexistent root go.mod; 7/7 scheduled runs red) and test every workflow path"
priority: P1
type: bug
labels: [ci, github-actions, drift, agents, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Fix the scheduled Drift Patrol workflow (setup-go points at a nonexistent root go.mod; 7/7 scheduled runs red) and test every workflow path

## Problem

The weekly Drift Patrol workflow has never run its patrol step on a schedule. `.github/workflows/drift-patrol.yml:85` sets `go-version-file: go.mod`, but the repo has no `go.mod` at its root, so the `patrol` job fails in `actions/setup-go` within about 20 seconds. The workflow's only green runs are the 2026-08-07 pull_request runs, and on pull requests the `patrol` job is skipped (`if: github.event_name != 'pull_request'`, `:60`). str-u394l.1 ("Scheduled drift patrol") was closed on that PR evidence. The patrol meant to catch drift has been red for seven weeks and nobody was alerted. This is a same-class recurrence of str-wnyzy, which fixed the identical bug in `ci.yml` only.

`docs/DRIFT-PATROL.md` also overstates the protection. It says the PR self-test means "the patrol cannot rot in place", but the self-test only unit-tests the Python. Its "What it checks" table also omits the `tracker-server` check that `AGENTS.md:22` tells agents to run (docs-23).

## Evidence

Re-verified 2026-09-23 against the audit worktree (main 70465921 plus audit files):

- `.github/workflows/drift-patrol.yml:82-85`: `uses: actions/setup-go@v5` / `go-version-file: go.mod` (wrong, and no `cache-dependency-path`). The other workflows use `shatter-go/go.mod`: `ci.yml:53`, `perf-ci.yml:36`, `release.yml:131-132` (only release also sets `cache-dependency-path: shatter-go/go.sum`).
- `gh run list --workflow drift-patrol.yml` → `{"failure":7,"success":2}`. The failures are the schedule runs of 08-10, 08-17, 08-24, 08-31, 09-07, 09-14 and 09-21, and the two successes are the 08-07 PR runs. `gh run view 35620815498` (09-21 schedule) shows: `The specified go version file at: go.mod does not exist`.
- The patrol job installs no system packages. `ci.yml:61-64` installs `libclang-dev z3`. Once setup-go is fixed, the `task ts:build go:build rust-fe:build` step (`:115-116`) or the `--require-conformance` patrol (`:118-126`) may surface the next missing dependency.
- `scripts/test_ci_workflow_structure.py` never mentions `drift-patrol.yml`, and it checks only `ci.yml`'s shape. str-35vtk.35 (open) wires that script into a local Taskfile task.
- `docs/DRIFT-PATROL.md:27-28`: "...so the patrol cannot rot in place." `:24` Owner: "The maintainer on the weekly triage rotation; if there is no rotation, whoever is landing work that week". No rotation exists.
- `docs/DRIFT-PATROL.md:33-43` "What it checks" lists 7 checks. `scripts/drift-patrol.py:757-767` `CHECKS` registers 8 (the extra one is `("tracker-server", check_tracker_server)`, defined at `:686`, registered at `:765`, added by str-qwua7.16). `AGENTS.md:22` references `--only tracker-server`.
- Tracker data in CI: `scripts/drift-patrol.py:183-221` `load_tracker()` falls back to the committed `.beads/issues.jsonl` when `bd` is absent, as it is in CI. D4 (2026-09-23) retires the JSONL import. `beads-jsonl-consumers-drop-bd-sync` (shatter-tracker-and-beads bucket) owns changing that data source.
- Run locally, `python3 scripts/drift-patrol.py` does run and reports a tracker-hygiene FAIL. The script works; only the workflow is broken.

## Acceptance criteria

- [ ] `drift-patrol.yml` uses `go-version-file: shatter-go/go.mod` and `cache-dependency-path: shatter-go/go.sum`.
- [ ] A test fails on the current tree and passes after the fix. It lives in `scripts/test_ci_workflow_structure.py` or a new module that `task meta` runs; coordinate with str-35vtk.35. It parses every `.github/workflows/*.yml` with a YAML parser and applies this path contract:
  - **Literal paths** (`with.go-version-file`, `with.cache-dependency-path`, `with.node-version-file`, and step or job `working-directory`) must exist in the checkout, relative to the repo root. `cache-dependency-path` values may be multi-line; check each line.
  - **Globs** (each argument of `hashFiles(...)`, and any path value containing `*`, `?` or `[`) are expanded with `glob.glob(..., recursive=True)` from the repo root and must match at least one file. For example, `hashFiles('**/Cargo.lock')` passes because `Cargo.lock` exists.
  - **Expressions.** A value containing `${{` that is not a `hashFiles(...)` call (for example `working-directory: ${{ matrix.dir }}`) is skipped and listed in the test output. It is not treated as a failure.
  - **Runtime-created paths.** A path that a workflow creates at run time (for example `staging/`) is exempt only if it appears in an explicit allowlist in the test file, each entry with a one-line reason. Today the allowlist is empty, since every `working-directory` in the workflows is a checked-in directory (`shatter-go`, `shatter-rust`, `shatter-ts`).
  - Unit cases in the test cover: a missing literal path fails; a glob with zero matches fails; a glob with matches passes; an expression is skipped; an allowlisted path passes.

  Paste the failing output (which names `drift-patrol.yml` `go-version-file: go.mod`) and then the passing output in the close reason.
- [ ] One `workflow_dispatch` or scheduled run of Drift Patrol reaches and executes the `Run drift patrol` step, with its report in the run summary. Cite the run URL in the close reason. A PR run is not sufficient. A patrol FAIL on real drift (for example tracker-hygiene) is acceptable here. A setup or build failure before the patrol step is not.
- [ ] `docs/DRIFT-PATROL.md` changes:
  - adds a `tracker-server` row to the "What it checks" table;
  - adds a unit test in `scripts/test_drift_patrol.py` asserting that every id in `CHECK_IDS` appears in that table;
  - drops the "cannot rot in place" claim, or makes it true by pointing to the path test and to `workflow-health-patrol`;
  - replaces the owner line with a concrete action, for example "the lead of the next landing session reads the last patrol summary and files a `drift` issue for each FAIL".
- [ ] No change to the tracker data source in this issue. If the workflow still reads `.beads/issues.jsonl`, leave a pointer comment to `beads-jsonl-consumers-drop-bd-sync` (D4).

## Suggested approach

Make the one-line workflow fix, add the generic path-existence test, and add the doc row and its test. After merge, run `gh workflow run drift-patrol.yml`, then `gh run watch`. Fix any further setup failure the first real run shows, for example by adding the `ci.yml` apt step.

## Out of scope

- Fixing what the patrol reports (tracker hygiene and so on). Those are separate issues.
- Workflow-health checks for other workflows (`workflow-health-patrol`, which this issue blocks).
- Moving tracker-hygiene off the JSONL (`beads-jsonl-consumers-drop-bd-sync`, D4).

## Dependencies

- Blocks: `workflow-health-patrol`.
- Coordinate with: `beads-jsonl-consumers-drop-bd-sync` (D4), str-35vtk.35.
- Related: str-u394l.1 (closed; see `drift-patrol-reopen-note`), str-wnyzy (closed; same bug in ci.yml), str-qwua7.16 (added tracker-server).

Priority: P1 · Type: bug · Labels: ci, github-actions, drift, agents, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-agent/02, prior-01, agent-repo-02, docs-23


---

<!-- file: 06-drift-patrol-reopen-note.md -->

---
slug: drift-patrol-reopen-note
kind: reopen-note
title: "Comment on closed str-u394l.1: the scheduled Drift Patrol has failed 7/7 runs (setup-go points at a nonexistent root go.mod)"
priority: P1
type: note
labels: [ci, drift, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-u394l.1
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-u394l.1

Target: **str-u394l.1** (closed). Post as a comment only. Do not reopen.

Comment text:

> Audit 2026-09-22 follow-up. This issue was closed (bddf2481) on the evidence of "CI Patrol self-test job -> pass" and the PR's ci.yml run. On pull requests, `drift-patrol.yml` skips the `patrol` job (`if: github.event_name != 'pull_request'`), so that evidence could not show a scheduled patrol running.
>
> The scheduled workflow has never executed the patrol. All 7 scheduled runs, from 2026-08-10 to 2026-09-21, failed in `actions/setup-go` with `The specified go version file at: go.mod does not exist` (for example run 35620815498). The cause is that `drift-patrol.yml:85` sets `go-version-file: go.mod`, and the repo has no root go.mod. It should be `shatter-go/go.mod`, the same bug str-wnyzy fixed in ci.yml only.
>
> The fix, a test that every workflow's file paths exist, the missing tracker-server row in DRIFT-PATROL.md, and the requirement that the close reason cite a real scheduled or dispatched patrol run are tracked in `<id of drift-patrol-workflow-go-mod>`.

(Filer: replace the `<id of ...>` placeholder.)


---

<!-- file: 07-workflow-health-patrol.md -->

---
slug: workflow-health-patrol
kind: new
title: "Surface persistently red GitHub workflows to agents: an authenticated, self-safe drift-patrol workflow-health check plus a landing step"
priority: P1
type: task
labels: [agents, ci, github-actions, drift, landing, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [drift-patrol-workflow-go-mod]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Surface persistently red GitHub workflows to agents: an authenticated, self-safe drift-patrol workflow-health check plus a landing step

## Problem

Several main-branch and scheduled workflows fail on every run. Until this audit, none of them had a tracker issue. Landing and all agent checks look only at the local verifier and, at most, `ci.yml`, so a workflow that stays red is invisible to agents. The persistently red workflows are Build and Release (0 successes in 267 runs), Drift Patrol (7/7 scheduled runs), Perf CI (13/13) and Devcontainer (19 failures; the last success was in February). The patrol that should report drift is itself one of the broken workflows (`drift-patrol-workflow-go-mod`).

This issue adds the missing feedback loop on the shatter side:

- a read-only `workflow-health` drift-patrol check;
- a landing instruction to run it.

The per-workflow fix issues already exist as drafts in this audit bundle and are filed by the maintainer's filer (D6). This issue files nothing.

The bento-side counterpart, which polls CI for the landed main SHA inside `land-work`, is `land-work-post-push-workflow-health` in the bento tracker. The cross-repo split is intentional.

Two design constraints come from how the patrol itself runs:

1. **The check must not be self-perpetuating.** The Drift Patrol workflow goes red whenever any patrol check FAILs; that is its reporting mechanism (`drift-patrol.yml:118-126`, "Explain a red run"). If `workflow-health` counted Drift Patrol's own red runs as "workflow broken", then after any three weeks of real drift findings, including `workflow-health` flagging release.yml, the check would FAIL on Drift Patrol too. That FAIL would keep the patrol red, the next run would count it again, and fixing every underlying defect would not break the loop until three clean weeks passed. A red run whose failing step is the patrol-report step is a report, not a broken workflow.
2. **The check needs explicit authentication in CI.** `drift-patrol.yml`'s patrol job exports neither `GH_TOKEN` nor `GITHUB_TOKEN` to its shell steps (`:58-126`). The `repo-token` passed to `arduino/setup-task` (`:87-91`) is consumed by that action only. The job's `permissions:` block grants only `contents: read`. As written, `gh run list` inside the patrol would be unauthenticated, and a check that SKIPs on missing auth would pass silently forever.

## Evidence

Re-verified 2026-09-23 with `gh run list --workflow <w> -L 300 --json conclusion` and `gh run view <id> --json jobs`:

| Workflow | Trigger | Conclusions | Failing step (latest run) |
|---|---|---|---|
| `release.yml` | push main, dispatch | failure 169, cancelled 98, success 0 | Windows `z3.h` not found; aarch64 `openssl-sys` (run 35773969737). Drafted: `release-publish-guard-and-target`, `release-windows-z3-build`, `release-aarch64-openssl-cross`, `release-publish-and-install-smoke` |
| `drift-patrol.yml` | schedule Mon 09:00, dispatch, PR | failure 7, success 2 (PR only) | `Setup Go`: `go.mod does not exist`. Drafted: `drift-patrol-workflow-go-mod` |
| `perf-ci.yml` | schedule Mon 09:00, dispatch | failure 13 | `perf` / `Run stable perf scenarios` (run 35620979400). Drafted: `perf-ci-stable-scenarios-red` |
| `devcontainer.yml` | push main (`.devcontainer/**`), schedule, dispatch | failure 19, success 2 (2026-02) | `build-and-test` / `Build devcontainer and run tests` (run 35619881868, schedule, 2026-09-21). Drafted: `devcontainer-workflow-red` |
| `docker-publish.yml` | push tags `v*`, PR to main | failure 7 | `build-and-push` / `Build and push` (run 31210298465, pull_request, 2026-08-07). It has no main or schedule trigger, so this check does not cover it. Drafted: `docker-publish-workflow-red` |
| `parity-expiry.yml` | schedule | success 9, failure 3 (08-31, 09-07, 09-14; green 09-21) | Not persistently red; the check must not flag it |
| `ci.yml` | push/PR main | success 98, failure 72, cancelled 4 (latest green) | n/a |

- `scripts/drift-patrol.py:757-767`: the `CHECKS` registry has no workflow-health check. `run_command` is at `:110`.
- `.github/workflows/drift-patrol.yml:58-68`: the `patrol` job has `permissions: contents: read` and an `env:` block with only `STALE_DAYS` and `STRICT_PENDING`.
- AGENTS.md mentions `gh run` only in an rtk example (`AGENTS.md:581`). No AGENTS.md, CLAUDE.md or skill instruction tells agents to check workflow conclusions after landing.
- `bd search` for windows, aarch64, "release workflow", perf-ci, devcontainer and docker found no open issue at audit time (agent-repo-03, prior-02). str-qwua7.42 (open) only moves `perf-ci.yml` paths during the benchmarks merge and does not address the red runs.

## Acceptance criteria

- [ ] **The check.** A new drift-patrol check, `workflow-health`, is registered in `CHECKS` and documented in `docs/DRIFT-PATROL.md` (the table test added by `drift-patrol-workflow-go-mod` enforces the documentation):
  - It covers each workflow in `.github/workflows/` whose `on:` includes `push` to main, `schedule` or `workflow_run`. Any smoke job inside `release.yml` is covered through `release.yml`.
  - It reads the last N completed runs on `main` (default 3, flag `--workflow-health-runs`) with `gh run list --workflow <file> --branch main --json conclusion,databaseId,url,createdAt`.
  - It classifies each failed run by its first failing job and step (`gh run view <id> --json jobs`).
  - It reports FAIL when all N runs failed, printing the workflow name, the last run URL, and the first failing job and step.
- [ ] **Self-safety.** A failed run of `drift-patrol.yml` whose first failing step is the patrol-report step (`Run drift patrol`) counts as a report, not a failure. Only a Drift Patrol failure in setup or build steps (for example `Setup Go`) counts toward FAIL. Identify the report step by its step `id: patrol`, or by a name constant shared with the workflow file, not by a hard-coded string that can drift silently. The structure test from `drift-patrol-workflow-go-mod` asserts that the step still exists.
- [ ] **Authentication.**
  - `drift-patrol.yml`'s patrol job grants `permissions: { contents: read, actions: read }` and exports `GH_TOKEN: ${{ github.token }}` to the patrol step.
  - Locally, the check reports SKIP when `gh` is missing or unauthenticated.
  - When `GITHUB_ACTIONS=true`, missing or unauthenticated `gh` is a FAIL, not a SKIP, so the scheduled patrol cannot pass silently.
  - A workflow with fewer than N completed runs on main reports SKIP with a reason.
- [ ] **Unit tests** in `scripts/test_drift_patrol.py`, using canned `gh` JSON:
  - all-red → FAIL;
  - mixed → PASS;
  - `gh` unavailable locally → SKIP;
  - `gh` unavailable with `GITHUB_ACTIONS=true` → FAIL;
  - fewer than N runs → SKIP;
  - **recovery:** drift-patrol.yml's last 3 runs all failed at `Run drift patrol` → no FAIL for drift-patrol.yml;
  - drift-patrol.yml's last 3 runs all failed at `Setup Go` → FAIL.
- [ ] **Current data.** Run locally, the check FAILs on release, perf-ci and devcontainer (and on drift-patrol while the `Setup Go` failure persists), and PASSes on ci and parity-expiry. Paste the output.
- [ ] **Landing step.** The AGENTS.md landing section says: after pushing main, run `python3 scripts/drift-patrol.py --only workflow-health`. For each FAIL, confirm that a tracker issue exists, and name it in the landing notes. If none exists, report it to the maintainer.
- [ ] Release targets are not disabled. Under D1 they are fixed by the release drafts in this bundle, and this issue only links them.
- [ ] **Close-time proof:**
  - (a) the local authenticated output of `python3 scripts/drift-patrol.py --only workflow-health`;
  - (b) the URL of a scheduled or `workflow_dispatch` Drift Patrol run on main whose step summary shows `workflow-health` rows with PASS or FAIL results (not SKIP), proving that the CI token wiring works.

## Suggested approach

Reuse the existing `Result` / PASS-FAIL-SKIP-PENDING structure and the `run_command` helper in `scripts/drift-patrol.py`. Keep the check read-only: it reports and never files. Put the report-step exemption in a small table keyed by workflow file, so a future reporter workflow can opt in the same way.

## Out of scope

- Fixing perf-ci (`perf-ci-stable-scenarios-red`), devcontainer (`devcontainer-workflow-red`), docker-publish (`docker-publish-workflow-red`) or the release builds (the release drafts).
- Bento `land-work` CI polling (bento tracker: `land-work-post-push-workflow-health`).

## Dependencies

- Blocked by: `drift-patrol-workflow-go-mod`. The check is only useful once the patrol itself runs on schedule.
- Related: `release-publish-guard-and-target`, `release-windows-z3-build`, `release-aarch64-openssl-cross`, `release-publish-and-install-smoke`, `perf-ci-stable-scenarios-red`, `devcontainer-workflow-red`, `docker-publish-workflow-red`, `ci-runs-user-paths`, str-qwua7.42, bento `land-work-post-push-workflow-health` (cross-repo; link in body text only).

Priority: P1 · Type: task · Labels: agents, ci, github-actions, drift, landing, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-agent/03, agent-repo-03, tests-ci-03 · Decision: D1, D6


---

<!-- file: 08-go-lint-and-gofmt-gated.md -->

---
slug: go-lint-and-gofmt-gated
kind: new
title: "Go lint is red on main (10 golangci-lint issues, 10 non-gofmt files) and ungated: fix the findings and gate golangci-lint and gofmt in check-static"
priority: P2
type: bug
labels: [go, shatter-go, quality-gates, agents, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go lint is red on main (10 golangci-lint issues, 10 non-gofmt files) and ungated: fix the findings and gate golangci-lint and gofmt in check-static

## Problem

`shatter-go` has a golangci-lint configuration, and the go-conventions skill describes lint as enforced. In practice no gate and no CI job runs it. **Nothing invokes `go:lint`**: it is not a dependency of `lint`, `check-static`, `check-unit` or any CI step, and `ci.yml` does not install golangci-lint.

The `go:lint` precondition is not the cause. Its message says "golangci-lint not installed (optional)", but a Task precondition that fails still fails the task, loudly (exit 201, "precondition not met"). "Optional" is only message text. The earlier diagnosis in str-2tyfk's body ("silently no-ops") was wrong, and this issue must not copy it into CI-conditional logic. The tree now has 10 lint findings, including a tautological nil check in the execute handler and 7 dead functions, and 10 files that are not gofmt-clean. Two closed issues touch this. str-qwua7.32's acceptance "task go:lint passes" was false when it closed. str-2tyfk was a scoped errcheck cleanup that explicitly allowed the unused and govet residuals to be "filed separately", but they never were.

This merges audit drafts shatter-code/57 and shatter-agent/25 (report §15.1).

## Evidence

Re-verified 2026-09-23 in the audit worktree (main 70465921 plus audit files) with golangci-lint 2.12.2:

- `cd shatter-go && golangci-lint run --timeout 9m ./...` → exit 1, `10 issues:`
  - `protocol/handler.go:1325:32: nilness: tautological condition: nil == nil (govet)`, on the line `if preparedExec == nil && err == nil {`
  - `instrument/property_test.go:544:18: SA5011: possible nil pointer dereference (staticcheck)` (related `:541:7`)
  - unused: `protocol/analyzer.go:592` analyzeFunc, `:799` extractParams, `:1518` mapTypeInfo, `:1534` structTypeInfo; `protocol/handler.go:1964` (*Handler).lookupAnalyzedByTargetID; `protocol/prepared_launcher.go:474` toWrapperConstructors, `:506` toWrapperConstructorParams
- `cd shatter-go && gofmt -l . | grep -v testdata` → 10 files:
  - non-test: `instrument/symextract.go`, `protocol/analysis_cache.go`, `workspace/run.go`
  - test: `instrument/mockfingerprint_test.go`, `instrument/overlay_test.go`, `launcher/launcher_buildvcs_test.go`, `protocol/analysis_cache_handler_test.go`, `protocol/generated_enums_test.go`, `protocol/invocation_plan_test.go`, `protocol/property_test.go`
- `shatter-go/Taskfile.yml:61-71`: the `lint` task has the precondition `command -v golangci-lint` with msg `golangci-lint not installed (optional)`, then runs `golangci-lint run ./...`. Task preconditions abort the task when they fail (Task docs, "Preconditions"). A same-shaped test Taskfile with a missing command exits 201 with "precondition not met" (checked by the same-runtime review).
- Root `Taskfile.yml:184-186`: `lint` deps `[workspace-clippy, rust-fe:clippy, rust-rt:clippy, go:vet]` (no go:lint). `:535-546` `check-static` has no Go lint or format step. `:548-554` `check-unit` runs `go:test` and `go:vet` only. `.github/workflows/ci.yml` installs no golangci-lint.
- `shatter-go/.golangci.yml:3-4`: "Runs via: task go:test" (false). `:7` `timeout: 3m` (too short under load; the audit needed 8 minutes).
- `shatter-go/Taskfile.yml:13-16`: comment says CI "runs build via parity/conformance deps but never go:vet/go:test". This is stale, since `check-unit` runs both.
- str-2tyfk closed 2026-09-08 with an empty close reason. str-qwua7.32 closed with reason "Closed".

## Acceptance criteria

- [ ] All 10 golangci-lint findings are fixed: delete the dead functions, fix the tautology at `handler.go:1325` (decide what the second condition was meant to test), and fix the test nil dereference. `golangci-lint run ./...` in `shatter-go` exits 0.
- [ ] `gofmt -l` prints nothing for non-testdata files.
- [ ] `check-static` depends on `go:lint` and on a gofmt check (`test -z "$(gofmt -l $(git ls-files '*.go' | grep -v /testdata/))"` or equivalent). No CI-conditional skip logic is added. A missing golangci-lint must keep failing the task, as the precondition already does.
- [ ] The precondition message drops "(optional)" and names the pinned install command. README or the setup docs list golangci-lint (pinned version) as a required dev tool, because `check-static` now needs it locally.
- [ ] `ci.yml` installs the same pinned golangci-lint version (for example `golangci/golangci-lint-action` with `install-only`, or `go install ...@v2.x.y`).
- [ ] Proof the gate executes: introduce an unused function and an unformatted file on a scratch branch, force the gate (`task check-static --force` or delete the checksum), show it failing, then revert. Paste both outputs in the close reason. Also cite a CI run URL in which the lint step executed.
- [ ] The `.golangci.yml` header (and timeout: 10m) and the stale comment at `shatter-go/Taskfile.yml:13-16` are corrected.
- [ ] The repo `check-go` skill runs `task go:lint`, and go-conventions no longer describes an unenforced rule.

## Suggested approach

Fix the findings first in one commit, and run the gofmt-only reformat as its own commit. Wire the gate in a third commit. Check whether the `task-sources-cover-real-inputs` issue (shatter-gates-integrity bucket) needs `.golangci.yml` added to the task's `sources:`.

## Out of scope

- Dead-code deletion beyond what the linter reports.
- The equivalent optional-linter problems for ESLint, clippy lint and markdownlint (str-qwua7.30/.31/.46).
- Rust formatting (`rustfmt-gate`).

## Dependencies

- None blocking.
- Related: str-2tyfk (closed; see `go-lint-reopen-note`), str-qwua7.32 (closed; see `go-lint-qwua7-32-note`), `rustfmt-gate`, `task-sources-cover-real-inputs`.

Priority: P2 · Type: bug · Labels: go, shatter-go, quality-gates, agents, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/57, shatter-agent/25, prior-08, frontend-go-05


---

<!-- file: 09-go-lint-reopen-note.md -->

---
slug: go-lint-reopen-note
kind: reopen-note
title: "Comment on closed str-2tyfk: the unused/govet residuals it deferred were never filed, and its 'silently no-ops' diagnosis was wrong (nothing invokes go:lint)"
priority: P2
type: note
labels: [go, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-2tyfk
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-2tyfk

Target: **str-2tyfk** (closed). Post as a comment only. Do not reopen. str-qwua7.32 gets a different comment (`go-lint-qwua7-32-note`), because its acceptance criteria differ.

Comment text:

> Audit 2026-09-22 follow-up. This issue was scoped to the 22 `fmt.Fprint*` errcheck findings, and its body allowed the "unrelated unused/govet findings" to be cleaned up "or file[d] separately". The errcheck cleanup landed (c1364378). The deferred residuals were never filed, and they are still on main as of 2026-09-23. `golangci-lint run ./...` in shatter-go exits 1 with 10 issues:
> - govet nilness at `protocol/handler.go:1325` (`nil == nil`)
> - staticcheck SA5011 at `instrument/property_test.go:544`
> - 7 unused functions: analyzeFunc, extractParams, mapTypeInfo, structTypeInfo, lookupAnalyzedByTargetID, toWrapperConstructors, toWrapperConstructorParams
>
> `gofmt -l` also lists 10 files.
>
> One correction to this issue's diagnosis. The `go:lint` precondition ("golangci-lint not installed (optional)") does not silently no-op: a failed Task precondition fails the task. The real gap is that no gate or CI job invokes `go:lint` at all, and CI does not install golangci-lint. The residual findings, and gating golangci-lint and gofmt in check-static, are tracked in `<id of go-lint-and-gofmt-gated>`. That issue requires forced-gate output at close.

(Filer: replace the `<id of ...>` placeholder.)


---

<!-- file: 10-rustfmt-gate.md -->

---
slug: rustfmt-gate
kind: new
title: "Restore rustfmt cleanliness in one dedicated commit (107 files drifted after str-fr1v) and gate `cargo fmt --check` in check-static"
priority: P2
type: task
labels: [rust, quality-gates, agents, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Restore rustfmt cleanliness in one dedicated commit (107 files drifted after str-fr1v) and gate `cargo fmt --check` in check-static

## Problem

No gate runs `cargo fmt --check`, so the Rust tree drifted again after str-fr1v ("Fix rustfmt 1.93 drift") closed. Agents cannot safely run `cargo fmt` now. On 2026-09-21, one crate-wide `cargo fmt -p shatter-core` churned 58 files (+3068/-789). The agent then ran a mass `git checkout --` revert and wrote a script to re-apply its own edits. The lesson was recorded only in a private agent memory ("never run crate-wide cargo fmt"), which works around the drift instead of removing it.

The fix has two separate parts, and they must not be mixed with any other change:

1. one commit that only formats the workspace and the standalone crates;
2. a `cargo fmt --check` gate so the tree stays clean.

## Evidence

Re-verified 2026-09-23 in the audit worktree (main 70465921 plus audit files) with local `rustfmt 1.9.0-stable (ac68faa20c 2026-05-25)`:

- `cargo fmt --all -- --check` (workspace) lists 97 files: shatter-core 61, shatter-cli 25, shatter-llm 11 (for example `shatter-cli/build.rs:37`, `shatter-cli/src/args.rs:361`).
- `cd shatter-rust && cargo fmt --all -- --check` lists 9 files. `cd shatter-rust-runtime && cargo fmt --all -- --check` lists 1 file. These crates are excluded from the workspace (`Cargo.toml:3`), so the workspace command does not cover them.
- A grep of `Taskfile.yml`, `taskfiles/`, `shatter-*/Taskfile.yml` and `.github/` finds no `fmt --check` / `fmt -- --check`.
- There is no `rust-toolchain.toml`. CI uses `dtolnay/rust-toolchain@stable` (`ci.yml:38-41`), so the rustfmt version floats. That is how str-fr1v's "1.93 drift" happened.
- Provenance only (not needed to do this work): session c1689435 (2026-09-21T22:53) ran `cargo fmt -p shatter-core`, which reported "58 files changed, 3068 insertions(+), 789 deletions(-)". The session then ran `... | xargs git checkout --` and a scratchpad re-apply script (sessions-11). The session transcript, the scratchpad script and the private memory file are not visible to an implementer. Everything this issue needs is reproducible with the `cargo fmt --check` commands above.
- str-fr1v is closed with reason "Closed". Its acceptance was `cargo fmt --all -- --check` must pass.
- `.claude/skills/rust-conventions/SKILL.md` has no formatting guidance.

## Acceptance criteria

- [ ] One dedicated commit, containing nothing but `cargo fmt` output, formats the workspace (`cargo fmt --all`), `shatter-rust` and `shatter-rust-runtime` with the same pinned rustfmt version the gate uses. The commit message states the rustfmt version. Land it quickly, since it touches many files and will conflict with in-flight branches. Announce it in the landing notes.
- [ ] Add that commit's SHA to a `.git-blame-ignore-revs` file.
- [ ] `check-static` runs `cargo fmt --all -- --check` plus the per-standalone-crate checks. CI runs them through `task check`.
- [ ] The rustfmt version is pinned in one place, used by both CI and the local gate. Options: a `rust-toolchain.toml` with `components = ["rustfmt", "clippy"]`, or a pinned toolchain for the fmt step only, with the local task checking `rustfmt --version`. This keeps a future stable release from turning the gate red on its own.
- [ ] Proof the gate executes: on a scratch branch, mis-indent one line, force the gate (`task check-static --force` or clear its checksum), show it failing, then revert. Paste both outputs in the close reason, along with a CI run URL in which the fmt step executed.
- [ ] The rust-conventions skill says to run `cargo fmt` normally. The maintainer is told the memory `project_shatter_tree_not_rustfmt_clean.md` can be deleted. Agents do not edit memory as part of this issue.

## Suggested approach

Pick and pin the toolchain first, then run the format commit from a clean `main` with no other agents mid-landing. Add the gate in the next commit, then land both together. Do not combine this with the go-lint work or any refactor.

## Out of scope

- Clippy lint changes.
- Go formatting (`go-lint-and-gofmt-gated`).
- Pinning the whole Rust toolchain for other reasons. Pin only what stable fmt needs, unless a rust-toolchain.toml is simply the easiest way to do that.

## Dependencies

- None blocking.
- Related: str-fr1v (closed; see `rustfmt-reopen-note`), `go-lint-and-gofmt-gated`.

Priority: P2 · Type: task · Labels: rust, quality-gates, agents, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-agent/26, sessions-11


---

<!-- file: 11-rustfmt-reopen-note.md -->

---
slug: rustfmt-reopen-note
kind: reopen-note
title: "Comment on closed str-fr1v: the tree drifted again (58-file churn on 2026-09-21; 107 files unformatted on 09-23); no fmt gate exists"
priority: P2
type: note
labels: [rust, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-fr1v
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-fr1v

Target: **str-fr1v** (closed). Post as a comment only. Do not reopen.

Comment text:

> Audit 2026-09-22 follow-up. This issue's acceptance was that `cargo fmt --all -- --check` must pass, and it was closed as "Closed". Nothing kept the tree clean afterwards: no Taskfile or CI step runs `cargo fmt --check`, and the rustfmt version floats with `dtolnay/rust-toolchain@stable`.
>
> On 2026-09-21 a crate-wide `cargo fmt -p shatter-core` churned 58 files (+3068/-789), and the agent had to revert it by hand. As of 2026-09-23 (rustfmt 1.9.0-stable), `cargo fmt --all -- --check` lists 97 workspace files, plus 9 in shatter-rust and 1 in shatter-rust-runtime.
>
> A one-time format commit, a pinned rustfmt version, and a `cargo fmt --check` gate in check-static (with forced-gate proof at close) are tracked in `<id of rustfmt-gate>`.

(Filer: replace the `<id of ...>` placeholder.)


---

<!-- file: 12-ci-runs-user-paths.md -->

---
slug: ci-runs-user-paths
kind: new
title: "Smoke, walkthrough and E2E user paths never run in CI: add a push-to-main user-paths job that fails on a real regression"
priority: P2
type: task
labels: [ci, smoke, walkthrough, e2e, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Smoke, walkthrough and E2E user paths never run in CI: add a push-to-main user-paths job that fails on a real regression

## Problem

`ci.yml` runs `task check` plus the shatter-llm steps, and nothing else. `task check` does not include `smoke`, `walkthrough`, `gauntlet` or `e2e`. Those gates, which exercise what a user actually runs, run only when an agent chooses to run them. The only workflow that touches the gauntlet is the weekly Perf CI, which has failed every run. Its fix is a separate issue, `perf-ci-stable-scenarios-red`, so that a perf-runner failure does not hold this one open. A regression in the demo or user path can therefore land on main and stay there until someone happens to run the gate locally.

## Evidence

Re-verified 2026-09-23 in the audit worktree (main 70465921 plus audit files):

- `.github/workflows/ci.yml:88-110`: the steps are `task check`, `cargo clippy -p shatter-llm`, `cargo test -p shatter-llm`, and `python3 scripts/test_ci_workflow_structure.py`. No workflow mentions `smoke`, `walkthrough` or `gauntlet` (`grep -rn` over `.github/workflows/`).
- `Taskfile.yml:648-660` `smoke` (about 15 s; TS + two Go explores + `scripts/test_empty_report_regression.sh`). `:680-689` `walkthrough` / `walkthrough-governed` (`demo/walkthrough.sh --auto --delay 0`). `:701-710` `gauntlet`. `:577-589` `e2e` / `e2e-governed` (TS, Go, Rust). `:535-575` `check-static`/`check-unit`/`check-integration` include none of them.
- Perf CI (`perf-ci.yml`, 13/13 failures) is covered by `perf-ci-stable-scenarios-red`.
- Separately, CI's `task check` test leaves have reported "up to date" since about 2026-08-29 (see the str-qwua7.3 note `task-list-json-poisons-checksums` and `ci-executed-leaf-guard` in the shatter-gates-integrity bucket). The E2E coverage that `check-integration`'s `core:test-ignored` provides in CI is therefore hollow as well.

## Acceptance criteria

- [ ] A CI job on push to main (or nightly on schedule), which also has `workflow_dispatch` so it can be run from a branch, runs `task smoke` and a bounded walkthrough (`task walkthrough`, or a documented subset if it exceeds the job budget) and uploads their output as workflow artifacts.
- [ ] The same job or a sibling runs `task e2e`, or `ci-executed-leaf-guard` has landed and the close reason cites a `ci.yml` run log in which `core:test-ignored` actually executed the three `e2e_concolic*` suites (test names visible in the log). A claim without that log does not satisfy this item.
- [ ] Each new job fails on a real regression. On a branch, show a deliberate break (for example making `demo/walkthrough.sh` exit 1, or breaking a smoke target) producing a red job, then revert, and cite both run URLs in the close reason. If str-qwua7.10's content assertions have landed, the walkthrough job enforces them.
- [ ] `scripts/test_ci_workflow_structure.py` asserts the new job and steps exist, so they cannot be dropped silently.

## Suggested approach

Add a separate `user-paths` job in `ci.yml` (push to main only, not PRs, to keep PR latency down), or a new nightly workflow. Reuse the apt/toolchain setup from the `test` job. `workflow-health-patrol` will then cover the nightly run. Combine with str-qwua7.10 so the walkthrough job checks content, not just the exit code.

## Out of scope

- The Task checksum problem itself (str-qwua7.3 / `ci-executed-leaf-guard`).
- Gauntlet allowlist content, and the gauntlet in CI. The gauntlet is broad and slow; add it here only if it fits the job budget.
- Perf CI (`perf-ci-stable-scenarios-red`).
- Moving perf-ci paths (str-qwua7.42).

## Dependencies

- None hard-blocking. The smoke/walkthrough job does not depend on the checksum fix.
- Related: str-qwua7.10 (open; demo gates fail on bad content), `perf-ci-stable-scenarios-red` (split from this issue), `ci-executed-leaf-guard`, `workflow-health-patrol` (watches the new job once it runs on push to main or on a schedule).

Priority: P2 · Type: task · Labels: ci, smoke, walkthrough, e2e, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/73, tests-ci-11


---

<!-- file: 13-workflow-action-versions.md -->

---
slug: workflow-action-versions
kind: new
title: "Workflows use deprecated Node-20 action majors, unpinned ubuntu-latest (Ubuntu 26 from 2026-10-19), and setup-go cache that cannot find go.sum"
priority: P3
type: chore
labels: [ci, github-actions, maintenance, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Workflows use deprecated Node-20 action majors, unpinned ubuntu-latest (Ubuntu 26 from 2026-10-19), and setup-go cache that cannot find go.sum

## Problem

GitHub Actions annotations warn about three things that will degrade or break CI without any repo change:

1. The Node-20 runtime is deprecated for several action majors the workflows use.
2. `ubuntu-latest` starts migrating to Ubuntu 26 on 2026-10-19. The workflows depend on distro packages (`libclang-dev`, `z3`) that have not been verified there.
3. `actions/setup-go` caching silently fails because it looks for `go.sum` at the repo root.

These annotations appear only in the web UI, so no agent has seen them.

## Evidence

Re-verified 2026-09-23 in the audit worktree (main 70465921 plus audit files):

- Run 35756993223 (CI) annotations:
  - "Node.js 20 is deprecated ... actions/cache@v4, actions/checkout@v4, actions/setup-go@v5, actions/setup-node@v4, arduino/setup-task@v2"
  - "The ubuntu-latest label will migrate to Ubuntu 26 beginning October 19, 2026"
  - "Restore cache failed: Dependencies file is not found ... Supported file pattern: go.sum"
- Action usage across `.github/workflows/*.yml`: `actions/checkout@v4` ×11, `actions/cache@v4` ×4, `actions/setup-go@v5` ×4, `actions/setup-node@v4` ×4, `actions/setup-python@v5` ×3, `actions/upload-artifact@v4` ×3, `actions/download-artifact@v4` ×1, `arduino/setup-task@v2` ×2, plus docker/*, `devcontainers/ci@v0.3`, `dtolnay/rust-toolchain@stable`.
- `runs-on: ubuntu-latest` appears 10 times, in `ci.yml`, `drift-patrol.yml`, `perf-ci.yml`, `release.yml`, `devcontainer.yml`, `docker-publish.yml`, `parity-expiry.yml` and `cleanup-continuous-releases.yml`. `release.yml` also uses `windows-latest` and macOS labels through `matrix.os`.
- setup-go without `cache-dependency-path`: `ci.yml:51-53`, `drift-patrol.yml:83-85` (fixed separately by `drift-patrol-workflow-go-mod`) and `perf-ci.yml:34-36`. Only `release.yml:131-132` sets `cache-dependency-path: shatter-go/go.sum`.

## Acceptance criteria

- [ ] Every action whose major runs on Node 20 is bumped to a Node-24-based major. Check each action's releases for the current major; do not guess version numbers.
- [ ] Linux runners are pinned to `ubuntu-24.04` until a trial run on Ubuntu 26 (`ubuntu-26.04` label, or `ubuntu-latest` after the migration) proves that `apt-get install libclang-dev z3` and `task check` work. That trial is tracked by `ubuntu-26-runner-trial`, drafted in this bundle and filed by the maintainer (D6). Add a comment next to each pin naming that issue.
- [ ] Every `actions/setup-go` step sets `cache-dependency-path: shatter-go/go.sum`.
- [ ] Proof at close: link one post-change run each of `ci.yml` and `release.yml` whose annotations show no Node-20 deprecation warning and no "Restore cache failed ... go.sum" warning. `release.yml` may still fail on its Windows or aarch64 legs; only the annotations matter here. A structure test asserts that every `setup-go` step has `cache-dependency-path` and that no `runs-on` uses `ubuntu-latest`.

## Suggested approach

Make one mechanical PR across all workflow files. If `drift-patrol-workflow-go-mod` has landed first, `drift-patrol.yml`'s setup-go is already fixed. Dependabot for `github-actions` (`.github/dependabot.yml`) is an optional way to keep the majors current, and is the implementer's call.

## Out of scope

- Fixing workflows that are red for other reasons (the release, drift-patrol, perf-ci and devcontainer issues in this bucket).
- `dtolnay/rust-toolchain@stable` pinning (see `rustfmt-gate` for the fmt toolchain).

## Dependencies

- None blocking.
- Blocks: `ubuntu-26-runner-trial`.
- Related: `drift-patrol-workflow-go-mod`, `workflow-health-patrol`, `perf-ci-stable-scenarios-red` (its log shows the go.sum cache warning).

Priority: P3 · Type: chore · Labels: ci, github-actions, maintenance, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/77, tests-ci-18


---

<!-- file: 14-nextest-ci-profile-and-stale-parity-fallback.md -->

---
slug: nextest-ci-profile-and-stale-parity-fallback
kind: new
title: "CI runs plain `cargo test` while local gates use nextest; both nextest configs carry an unused `[profile.ci]` and `fail-fast = true`: pick one runner policy and make CI apply it"
priority: P3
type: chore
labels: [ci, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# CI runs plain `cargo test` while local gates use nextest; both nextest configs carry an unused `[profile.ci]` and `fail-fast = true`: pick one runner policy and make CI apply it

(The slug is kept for filer stability. The stale `parity-governed` fallback that this draft also used to cover was split into `parity-governed-stale-fallback`.)

## Problem

The Rust test tasks use cargo-nextest when it is on PATH and fall back to plain `cargo test` otherwise. CI never installs nextest, so CI and local gates run different test runners.

Two nextest config files each define a `[profile.ci]` that nothing selects:

- `.config/nextest.toml`, used by the workspace crates;
- `.config/nextest-standalone.toml`, used explicitly by `shatter-rust` and `shatter-rust-runtime` through `--config-file`.

In both files, `[profile.default]` has `fail-fast = true`, so locally one failing or timed-out test hides the rest of the suite. The audit saw 87 of 3531 tests unrun when `bench_frontier_ranking` timed out, before str-6nul9 excluded that benchmark. In CI, the E2E suites have no per-test timeout at all.

Verifier correction (tests-ci-07): the `rust-frontend-harness` test group (`max-threads = 1`) exists because nextest's process-per-test model defeats the tests' in-process mutexes. Under plain `cargo test` in CI those mutexes do serialize the fixture builds. So "flake serialization only applies locally" is not a problem, and this issue does not claim it. What remains is the dead config, fail-fast, and the runner divergence.

## Evidence

Re-verified 2026-09-23 against the audit worktree (main 70465921 plus audit files):

- `.github/workflows/ci.yml` has no cargo-nextest install step and no `NEXTEST_PROFILE` or `--profile ci`. Its only explicit cargo test is `cargo test -p shatter-llm` (`:103`). A repo-wide grep finds no reference to the ci profile outside audit drafts.
- `.config/nextest.toml`:
  - `[profile.default]`: `test-threads = 4`, `slow-timeout = { period = "60s", terminate-after = 2 }`, `fail-fast = true`.
  - `[profile.ci]`: `retries = 1`, `fail-fast = false`, `final-status-level = "flaky"`, and the same `rust-frontend-harness` override. Nothing selects it.
- `.config/nextest-standalone.toml`:
  - `[profile.default]`: `fail-fast = true`, `final-status-level = "slow"`.
  - `[profile.ci]`: `retries = 1`, `fail-fast = false`, `final-status-level = "flaky"`. Nothing selects it.
  - It is consumed by `shatter-rust/Taskfile.yml:24` and `shatter-rust-runtime/Taskfile.yml:24` (`cargo nextest run --config-file ../.config/nextest-standalone.toml`), and listed as a source at root `Taskfile.yml:401`.
- The nextest-or-fallback branches are at `shatter-core/Taskfile.yml:28-32` (test), `:61-65` (test-ignored), `:88-92` (test-ignored-fast), `shatter-cli/Taskfile.yml:27` and `:46`, `shatter-rust/Taskfile.yml:22`, and `shatter-rust-runtime/Taskfile.yml:22`. For example, `:65` falls back to `cargo test -p shatter-core -- --include-ignored --skip bench_frontier_ranking`.
- The CI test leaves are currently not executing at all, because of Task checksum poisoning (see the str-qwua7.3 note `task-list-json-poisons-checksums` and `ci-executed-leaf-guard`). This issue matters once those land, because CI will then really run the fallback path.

## Acceptance criteria

- [ ] **Decide the policy** and write it in a comment at the top of both nextest config files. The options are:
  - **(A) adopt nextest in CI.** CI installs cargo-nextest (for example `taiki-e/install-action@<pinned sha>` with `tool: cargo-nextest`) and selects the `ci` profile for every nextest invocation, workspace and standalone, through `NEXTEST_PROFILE=ci` or by passing `--profile ci` when `CI` is set.
  - **(B) keep `cargo test` in CI.** Delete `[profile.ci]` from **both** files, with a comment explaining why CI stays on `cargo test`.

  Either way, no unused profile remains in either file.
- [ ] **No fail-fast in the gate.** Both default profiles have `fail-fast = false`, or every gate invocation passes `--no-fail-fast` (both nextest and cargo test accept it). Show a forced gate run (`task core:test --force`, or with its checksum cleared) on a scratch branch with two deliberately failing tests, in which the summary lists both failures. Paste the output in the close reason, then revert.
- [ ] **Close-time proof, by option:**
  - (A): cite a `ci.yml` run URL whose log shows nextest's summary line from `core:test-ignored` and from one standalone crate, run under the `ci` profile (the profile name is visible in the nextest header). This requires `ci-executed-leaf-guard` to have landed, so that the leaves execute.
  - (B): cite the diff removing both `[profile.ci]` sections, plus the forced-gate output above. No CI URL is required.

## Suggested approach

(A) is preferred. It removes the local/CI divergence and gives the E2E suites the 120 s per-test terminate-after in CI too. Keep the `cargo test` fallback in the Taskfiles for developer machines without nextest, but make the CI path explicit.

## Out of scope

- The `parity-governed` stale fallback (`parity-governed-stale-fallback`).
- Excluding `bench_frontier_ranking` from the gate (done in str-6nul9).
- The Task checksum problem (str-qwua7.3).
- The NEXTEST thread budget (str-35vtk.14).

## Dependencies

- None hard-blocking. Option (A)'s CI proof needs `ci-executed-leaf-guard` (shatter-gates-integrity bucket) to have landed.
- Related: str-6nul9 (closed), str-35vtk.7 (closed; wired nextest locally), str-35vtk.14, `ci-executed-leaf-guard`, `parity-governed-stale-fallback`.

Priority: P3 · Type: chore · Labels: ci, quality-gates, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/07, gates-08, tests-ci-07


---

<!-- file: 15-release-publish-guard-and-target.md -->

---
slug: release-publish-guard-and-target
kind: new
title: "Release: guard the publish job to push-on-main and pass --target $GITHUB_SHA, so branch runs of release.yml build without publishing"
priority: P1
type: bug
labels: [release, ci, distribution, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Release: guard the publish job to push-on-main and pass --target $GITHUB_SHA, so branch runs of release.yml build without publishing

## Problem

The Windows and aarch64 fixes (`release-windows-z3-build`, `release-aarch64-openssl-cross`) have to be iterated with `gh workflow run release.yml --ref <branch>`, because release.yml has no pull_request trigger. Once the matrix goes green, a dispatched branch run can publish a `continuous-*` prerelease from unreviewed code. Nothing restricts the `release` job to main:

- `release.yml:3-6` triggers on `push: branches: [main]` and on `workflow_dispatch`, and dispatch accepts any ref.
- The `release` job (`:183-186`) has only `needs: [build-ts, build]`. It has no `if:` on event or ref.
- `gh release create "$RELEASE_TAG"` (`:295-310`) passes no `--target`. When the tag does not exist yet, `gh release create` makes it on the repository's default branch, not on the commit that was built (see the `--target` flag of `gh release create`). A branch run would therefore publish branch binaries under a tag that points at main's HEAD. A push run could also tag a later main commit if main moved during the build.

`install.sh` picks the newest `continuous-*` prerelease, so a published branch build would be what users install.

## Evidence

Re-verified 2026-09-23 against the audit worktree (main 70465921 plus audit files). The line numbers above were read from `.github/workflows/release.yml`. `permissions: contents: write` is set for the whole workflow (`:8-9`).

## Acceptance criteria

- [ ] The `release` job runs only when `github.event_name == 'push' && github.ref == 'refs/heads/main'`. As an option, dispatch from main may also publish when an explicit `publish: true` input is set. A dispatch from any other ref runs the full build matrix, and the release job shows `skipped`.
- [ ] `gh release create` passes `--target "$GITHUB_SHA"`, so the tag points at the built commit.
- [ ] `scripts/test_ci_workflow_structure.py` (or a sibling test that `task meta` runs) parses `release.yml` and asserts both properties: the `if:` guard on the release job, and `--target` in the create step. Show the test failing on the current tree and passing after the fix, and paste both outputs in the close reason.
- [ ] **Close-time proof.** Cite a `gh workflow run release.yml --ref <non-main branch>` run URL in which the build jobs ran and `Create continuous GitHub Release` concluded `skipped`. The build jobs may still fail, since the Windows and aarch64 legs are fixed elsewhere.

## Out of scope

- Fixing any build leg.
- The install smoke (`release-publish-and-install-smoke`).
- Release retention (`cleanup-continuous-releases.yml`).

## Dependencies

- Blocks: `release-windows-z3-build`, `release-aarch64-openssl-cross`. Both iterate through branch dispatch.
- Related: `release-publish-and-install-smoke`, str-lj7s (see `release-reopen-note`).

Priority: P1 · Type: bug · Labels: release, ci, distribution, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: crosscheck codex #5 (new in revision) · Decision: D1


---

<!-- file: 16-devcontainer-workflow-red.md -->

---
slug: devcontainer-workflow-red
kind: new
title: "Devcontainer CI red since February: post-create.sh runs `bd init --from-jsonl`, which bd now refuses because origin has Dolt history"
priority: P2
type: bug
labels: [ci, github-actions, devcontainer, beads, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Devcontainer CI red since February: post-create.sh runs `bd init --from-jsonl`, which bd now refuses because origin has Dolt history

## Problem

`.github/workflows/devcontainer.yml` runs weekly and on pushes to `.devcontainer/**`. It has failed 19 times, and its last success was in 2026-02. The current failure happens before any test runs: `.devcontainer/post-create.sh` initializes beads from the committed JSONL, and bd refuses because the `origin` remote already carries Dolt history. The container never finishes `postCreateCommand`, so the `cargo test` / `clippy` / `npm test` / `go test` `runCmd` never executes. The devcontainer is therefore an untested onboarding path.

Maintainer decision D4 (2026-09-23) retires the JSONL import and moves tracker sync to a Dolt remote. The devcontainer's tracker bootstrap has to follow D4: adopt the remote rather than import JSONL. Do not add a timeout env var or any hook-bypass guidance.

This draft replaces the "file a devcontainer follow-up" step that `workflow-health-patrol` used to carry. Under D6, the maintainer's filer creates it.

## Evidence

Re-verified 2026-09-23:

- `gh run list --workflow devcontainer.yml -L 300` gives failure 19 and success 2 (2026-02). The last three runs are all `schedule` failures: 35619881868 (09-21), 34863054869 (09-14) and 34134350747 (09-07).
- Run 35619881868, job 106399983455, step `Build devcontainer and run tests`. The log, fetched with `gh api repos/shatterproof-ai/shatter/actions/jobs/106399983455/logs`, contains:
  - `==> Initializing beads issue tracker...`
  - `bd init refuses: remote 'origin' already has Dolt history (refs/dolt/data).` bd suggests `bd bootstrap` ("Adopt the remote (recommended)").
  - `postCreateCommand from devcontainer.json failed with exit code 10.`
- `.devcontainer/post-create.sh:43-51`:

  ```sh
  if [ -f .beads/issues.jsonl ] && [ ! -d .beads/dolt/beads_str ]; then
    bd init --prefix str --from-jsonl --quiet
    bd import -i .beads/issues.jsonl
  ```

  This is followed by `bd config set beads.role maintainer`.
- `.github/workflows/devcontainer.yml:13-24`: `devcontainers/ci@v0.3` with `runCmd` `cargo test`, `cargo clippy -- -D warnings`, `cd shatter-ts && npm test`, `cd ../shatter-go && go test ./...`. It runs bare `cargo test`, not `task` gates, and does not cover the standalone crates.
- Earlier failures (before about 2026-09) may have had other causes. Only the current first error is verified here.

## Acceptance criteria

- [ ] `post-create.sh` no longer imports `.beads/issues.jsonl`. It adopts the tracker the way D4's Dolt-remote design specifies (for example `bd bootstrap`). If CI has no credentials for the Dolt remote, the script skips tracker setup with an explicit log line; tracker setup is not needed to run tests. The script fails only on real errors. Do not add `|| true` around the whole block.
- [ ] Once post-create completes, the `runCmd` runs. Any test failures it then exposes are fixed, or each is recorded with the failing command and first error in the close reason, and this issue stays open until the workflow is green.
- [ ] The `runCmd` uses `task` targets consistent with the local gates (for example `task test-quick` or `task check-unit`), or a comment in the workflow explains why it runs bare commands.
- [ ] **Close-time proof.** The close reason contains the URL of a green `devcontainer.yml` run, `workflow_dispatch` or schedule, on main.

## Suggested approach

Coordinate with the tracker bucket's `beads-retire-jsonl-import-dolt-remote` and `beads-jsonl-consumers-drop-bd-sync`: this script is one more JSONL consumer. Iterate with `gh workflow run devcontainer.yml --ref <branch>`. The workflow publishes nothing, so branch runs are safe.

## Out of scope

- Designing the Dolt remote sync itself (tracker bucket, D4).
- Devcontainer feature changes that are not needed to go green.

## Dependencies

- None blocking in this bucket. Coordinate with `beads-retire-jsonl-import-dolt-remote` and `beads-jsonl-consumers-drop-bd-sync` (shatter-tracker-and-beads bucket).
- Related: `workflow-health-patrol` (flags this workflow), `workflow-action-versions`.

Priority: P2 · Type: bug · Labels: ci, github-actions, devcontainer, beads, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: tests-ci-03, agent-repo-03 (split out of workflow-health-patrol in revision) · Decision: D4, D6


---

<!-- file: 17-docker-publish-workflow-red.md -->

---
slug: docker-publish-workflow-red
kind: new
title: "Docker image build fails (7/7): Dockerfile never copies shatter-llm, so `cargo build -p shatter-cli` cannot load the workspace"
priority: P2
type: bug
labels: [ci, github-actions, docker, distribution, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Docker image build fails (7/7): Dockerfile never copies shatter-llm, so `cargo build -p shatter-cli` cannot load the workspace

## Problem

`.github/workflows/docker-publish.yml` builds the root `Dockerfile` on pull requests to main and pushes the image on `v*` tags. It has failed on all 7 recorded runs. The builder stage copies only some workspace members. `shatter-llm` (a workspace member, and a dependency of shatter-core and shatter-cli) is never copied, so `cargo build --release -p shatter-cli` fails while loading the manifest.

The workflow has no push-to-main or schedule trigger. The last run was a 2026-08-07 PR, and no `v*` tag has ever been built successfully, so nobody notices this failure. `workflow-health-patrol` does not cover it either, because it watches only main and scheduled workflows.

This draft replaces the "file a docker-publish follow-up" step that `workflow-health-patrol` used to carry. Under D6, the maintainer's filer creates it.

## Evidence

Re-verified 2026-09-23:

- `gh run list --workflow docker-publish.yml` gives failure 7. The latest is run 31210298465 (pull_request, 2026-08-07): job `build-and-push`, step `Build and push`.
- Log of that job, via `gh api repos/shatterproof-ai/shatter/actions/jobs/<id>/logs`:
  - `[linux/amd64 builder 16/19] RUN sed -i 's/features = \["gh-release"\]/features = []/' shatter-core/Cargo.toml`
  - `[builder 17/19] RUN cargo build --release -p shatter-cli`, which fails with `error: failed to load manifest for workspace member /build/shatter-core ... failed to load manifest for dependency shatter-llm ... failed to read /build/shatter-llm/Cargo.toml ... No such file or directory`
  - `buildx failed with: ... exit code: 101`
- `Dockerfile:32-43`: the builder copies `Cargo.toml`, `Cargo.lock`, `shatter-core/`, `shatter-cli/`, `shatter-ts/`, `shatter-go/` and `shatter-rust/`. There is no `shatter-llm/`. The Dockerfile's `sed` also rewrites a `features = ["gh-release"]` line in `shatter-core/Cargo.toml` that no longer exists (shatter-core now depends on `z3 = "0.19"` with default features), so that step is a silent no-op.
- `docker-publish.yml:3-7`: the triggers are `push: tags: ['v*']` and `pull_request: branches: [main]`, with `platforms: linux/amd64,linux/arm64` (`:48`).

## Acceptance criteria

- [ ] The Dockerfile copies every workspace member that the build needs. Prefer a single `COPY . .` with a `.dockerignore` over a hand-maintained per-crate list, because the per-crate list is how this broke. Remove the stale `sed` step, or replace it with an explicit, working Z3 strategy for the image.
- [ ] A regression check fails if a workspace member listed in root `Cargo.toml` `[workspace] members` is not copied by the Dockerfile. This can be a test in `scripts/test_ci_workflow_structure.py`, or it is unnecessary if `COPY . .` is used. Show that it fails before the fix.
- [ ] The built image runs `shatter --version` in a workflow step, for both `linux/amd64` and `linux/arm64`, or the arm64 platform is removed with a documented reason. If Windows and aarch64 release issues change the Z3 strategy, keep the image consistent with them.
- [ ] Either the workflow gains a schedule or `workflow_dispatch` trigger so `workflow-health-patrol` can watch it, or the close reason explains why PR-only coverage is enough.
- [ ] **Close-time proof.** The close reason contains the URL of a green `docker-publish.yml` run (a PR run is fine, since it builds without pushing).

## Out of scope

- Publishing a `v*` tag or changing image tagging policy.
- The GitHub release binaries (the release drafts in this bundle).

## Dependencies

- None blocking.
- Related: `workflow-health-patrol`, `release-windows-z3-build` (Z3 linkage options), `workflow-action-versions`.

Priority: P2 · Type: bug · Labels: ci, github-actions, docker, distribution, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: tests-ci-03, agent-repo-03 (split out of workflow-health-patrol in revision) · Decision: D6


---

<!-- file: 18-go-lint-qwua7-32-note.md -->

---
slug: go-lint-qwua7-32-note
kind: reopen-note
title: "Comment on closed str-qwua7.32: acceptance 'task go:lint passes' was false at close; golangci-lint still reports 10 issues"
priority: P2
type: note
labels: [go, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.32
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-qwua7.32

Target: **str-qwua7.32** (closed, close reason "Closed"). Post as a comment only. Do not reopen.

Comment text:

> Audit 2026-09-22 follow-up. One of this issue's acceptance checks was "The encoding/json.Unmarshal line is removed; task go:lint passes." It was closed with the reason "Closed" and no lint output cited. At that point `task go:lint` could not pass: str-2tyfk, filed from this issue's own work, recorded pre-existing unused and govet findings that were deferred and never fixed. On main as of 2026-09-23, `golangci-lint run ./...` in shatter-go still exits 1 with 10 issues:
> - govet nilness at `protocol/handler.go:1325` (`nil == nil`)
> - staticcheck SA5011 at `instrument/property_test.go:544`
> - 7 unused functions
>
> Whether the json.Unmarshal exclusion removal and its call-site fixes are complete was not re-audited. This note concerns only the lint acceptance.
>
> The gap that let this through is that no gate or CI job invokes `go:lint`, so "lint passes" was never checked mechanically. The fix, gating golangci-lint and gofmt in check-static with forced-gate proof at close, is tracked in `<id of go-lint-and-gofmt-gated>`.

(Filer: replace the `<id of ...>` placeholder.)


---

<!-- file: 19-ubuntu-26-runner-trial.md -->

---
slug: ubuntu-26-runner-trial
kind: new
title: "Trial the CI and release workflows on Ubuntu 26 runners and lift the ubuntu-24.04 pin once they pass"
priority: P3
type: chore
labels: [ci, github-actions, maintenance, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [workflow-action-versions]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Trial the CI and release workflows on Ubuntu 26 runners and lift the ubuntu-24.04 pin once they pass

## Problem

`workflow-action-versions` pins every Linux runner to `ubuntu-24.04`, because `ubuntu-latest` starts migrating to Ubuntu 26 on 2026-10-19 and the workflows depend on distro packages (`libclang-dev`, `z3`) that have not been verified there. A pin with no follow-up would last forever, and the runner image would eventually be retired from under it. This issue does the trial and removes the pin.

It was pre-drafted so that the maintainer's filer creates it (D6), instead of `workflow-action-versions` asking its implementer to file it.

## Evidence

- CI run 35756993223 has the annotation "The ubuntu-latest label will migrate to Ubuntu 26 beginning October 19, 2026".
- `ci.yml:61-64` runs `apt-get install libclang-dev z3`. `release.yml:115-119` installs `libclang-dev`. `devcontainer.yml` builds its own image. `perf-ci.yml` and `drift-patrol.yml` set up toolchains through actions.

## Acceptance criteria

- [ ] Each workflow that `workflow-action-versions` pinned runs once on an Ubuntu 26 label (`ubuntu-26.04`, once GitHub offers it), via `workflow_dispatch` from a branch or a temporary matrix entry. Record the run URLs.
- [ ] Any package or toolchain breakage is fixed, for example renamed `libclang` or `z3` packages, or a different default Python.
- [ ] The pins are changed to `ubuntu-26.04`, or back to `ubuntu-latest`, and the pin comments that name this issue are removed.
- [ ] **Close-time proof.** The close reason contains green run URLs on Ubuntu 26 for `ci.yml` and for the Linux legs of `release.yml`.

## Out of scope

- Action major-version bumps (`workflow-action-versions`).
- macOS and Windows runner labels.

## Dependencies

- Blocked by: `workflow-action-versions`.

Priority: P3 · Type: chore · Labels: ci, github-actions, maintenance, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/77, tests-ci-18 (split from workflow-action-versions in revision) · Decision: D6


---

<!-- file: 20-perf-ci-stable-scenarios-red.md -->

---
slug: perf-ci-stable-scenarios-red
kind: new
title: "Perf CI red 13/13: gauntlet-auto-warm exits 1 with its output suppressed; surface child output in perf_runner.py, then fix the scenario"
priority: P2
type: bug
labels: [ci, github-actions, perf, gauntlet, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Perf CI red 13/13: gauntlet-auto-warm exits 1 with its output suppressed; surface child output in perf_runner.py, then fix the scenario

## Problem

The weekly Perf CI workflow has failed on every run. It fails in `Run stable perf scenarios`, where `scripts/perf_runner.py` reports only that `gauntlet-auto-warm failed on run 1 with exit code 1` and prints none of the scenario's own output. Nobody can diagnose the failure from the log. The workflow is also the only place the gauntlet runs in CI.

This was split out of `ci-runs-user-paths` so that fixing the perf runner does not hold up adding the user-path CI job, and the other way round. It is the perf-ci follow-up that `workflow-health-patrol` links.

## Evidence

Re-verified 2026-09-23:

- `gh run list --workflow perf-ci.yml -L 300` gives `{"failure":13}`.
- Latest run 35620979400 (schedule, 2026-09-21), job 106403653337. The log, fetched with `gh api repos/shatterproof-ai/shatter/actions/jobs/106403653337/logs`, contains:
  - `gauntlet-auto-warm failed on run 1 with exit code 1`
  - `##[error]Process completed with exit code 1.`

  No scenario stderr precedes these lines. The same log also shows `Restore cache failed: Dependencies file is not found ... go.sum`; `workflow-action-versions` owns that.
- `.github/workflows/perf-ci.yml:57-58`: `python3 scripts/perf_runner.py run --scenario-file perf/stable-scenarios.txt --results-dir "$RESULTS_DIR"`. `perf/stable-scenarios.txt` lists `gauntlet-auto-warm`, `explore-ts-arithmetic-warm`, `scan-standalone-ts-warm` and `go-frontend-instrument-tests`. The scenario is defined in `perf/scenarios.json:28`.
- str-qwua7.42 (open) only moves perf-ci paths during the benchmarks merge. It does not address the failure.

## Acceptance criteria

- [ ] **Diagnosis first.** `scripts/perf_runner.py` prints the failing scenario's command, exit code, and the last 200 lines of its stdout and stderr when a scenario fails. A unit test (`scripts/test_perf_runner.py` or similar, run by `task meta`) uses a stub scenario that exits 1 with known stderr, and asserts that the stderr appears in the runner's output. Show the test failing before the change.
- [ ] The real cause of the `gauntlet-auto-warm` failure is recorded in the issue, quoted from a CI run that includes the new output (cite the run URL).
- [ ] **Resolution.** The cause is fixed, and a `perf-ci.yml` run on main (dispatch or schedule) is green with `gauntlet-auto-warm` still in `perf/stable-scenarios.txt`.
  - Interim option: if the fix is blocked on another issue, temporarily remove `gauntlet-auto-warm` from the list, with a comment naming this issue's id and the blocking issue. This issue then stays open. A temporary removal never satisfies this item.
  - Disabling the whole workflow is not an acceptable resolution.
- [ ] **Close-time proof.** The close reason contains the URL of a green `perf-ci.yml` run whose log shows `gauntlet-auto-warm` executing.

## Out of scope

- Adding smoke, walkthrough or E2E jobs to CI (`ci-runs-user-paths`).
- Perf baseline policy and thresholds.
- Moving perf-ci paths (str-qwua7.42).

## Dependencies

- None blocking.
- Related: `ci-runs-user-paths` (split from it), `workflow-health-patrol`, `workflow-action-versions` (go.sum cache warning), str-qwua7.42.

Priority: P2 · Type: bug · Labels: ci, github-actions, perf, gauntlet, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/73, tests-ci-11, tests-ci-03 (split from ci-runs-user-paths in revision)


---

<!-- file: 21-parity-governed-stale-fallback.md -->

---
slug: parity-governed-stale-fallback
kind: new
title: "parity-governed keeps a dead 'pending str-7jgm.2' fallback that would turn a deleted validate-parity.py into a silent skip"
priority: P3
type: chore
labels: [quality-gates, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# parity-governed keeps a dead 'pending str-7jgm.2' fallback that would turn a deleted validate-parity.py into a silent skip

## Problem

The `parity-governed` task runs `scripts/validate-parity.py` only if the file exists. Otherwise it prints `[skip] validate-parity.py not present (pending str-7jgm.2)` and carries on. The script exists and str-7jgm.2 is closed, so the fallback is dead code. Its only remaining effect is that deleting or renaming the script would turn the parity check into a silent skip instead of a gate failure.

This was split out of `nextest-ci-profile-and-stale-parity-fallback`, because it is unrelated to the test-runner work.

## Evidence

Re-verified 2026-09-23 against the audit worktree (main 70465921 plus audit files):

- `Taskfile.yml:265-276` (`parity-governed`):

  ```sh
  if [ -f scripts/validate-parity.py ]; then python3 scripts/validate-parity.py;
  else echo "[skip] validate-parity.py not present (pending str-7jgm.2)"; fi
  ```

- `scripts/validate-parity.py` exists. `bd show str-7jgm.2` shows it CLOSED.

## Acceptance criteria

- [ ] The `if`/`else` is replaced by an unconditional `python3 scripts/validate-parity.py`.
- [ ] Proof that a missing script now fails the gate: on a scratch branch, rename the script and run `task parity --force` (or clear its checksum). The run exits non-zero. Revert, rerun, and it passes with validate-parity's output visible. Paste both outputs in the close reason.
- [ ] A grep for `pending str-` in `Taskfile.yml` and `taskfiles/` finds no other fallback that references a closed issue. List any that are found; they are in scope here if trivial.

## Out of scope

- The nextest and CI runner work (`nextest-ci-profile-and-stale-parity-fallback`).
- The content of the parity checks.

## Dependencies

- None.
- Related: str-7jgm.2 (closed).

Priority: P3 · Type: chore · Labels: quality-gates, parity, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: gates-08 (split from nextest-ci-profile-and-stale-parity-fallback in revision)
