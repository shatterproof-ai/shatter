# Bundle: shatter-ci-workflows (audit 2026-09-22)

- **Bucket:** shatter-ci-workflows. GitHub workflows that are permanently red or ungated: release matrix, drift patrol, workflow health, linters/formatters, CI user paths.
- **Repo / tracker:** shatter. bd in /home/ketan/project/shatter (prefix str).
- **Parent epic:** Epic: Audit 2026-09-22 findings.
- **Status:** final drafts. Nothing is filed (D6). Evidence was re-verified 2026-09-23 against the worktree at 56c86168, main at 70465921, and live `gh run list` data.

## Maintainer decisions (2026-09-23); these override the report and the old drafts

- **D1 Releases.** Keep x86_64-pc-windows-msvc and aarch64-unknown-linux-gnu in the release matrix. The drafts fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64) and do not drop them. Release work closes only with a green release-run URL.
- **D2 shatter diff.** Retire the snapshot `shatter diff` command and the unused Snapshot writer path; spec-diff is the regression tool. Update SPEC/README/QUICKSTART. str-81xiw decides whether diff-scoped exploration later takes the freed `diff` name. The shatter-agents plugin's `shatter diff --staged` docs get corrected. (Not used in this bucket.)
- **D3 Concolic positioning.** Measure first: P1 benchmark comparing default and concolic, P1 fix for concolic early termination, then a follow-up decision issue. No doc softening now. (Not used in this bucket.)
- **D4 Beads hook stall.** Retire the JSONL import in shatter and move tracker sync to a Dolt remote. The first step checks for clobbered DB state. AGENTS.md drops `bd sync`, and str-qwua7.28 is superseded. No BEADS_HOOK_TIMEOUT or hook-bypass guidance. (Touches this bucket only in drift-patrol-workflow-go-mod: the CI patrol reads .beads/issues.jsonl, and beads-jsonl-consumers-drop-bd-sync owns changing that.)
- **D5 Git identity.** The leaked [user] section is already removed. Add .mailmap, a git-state check and a fixture .git/config snapshot. (Not used in this bucket.)
- **D6 Filing.** After reconciliation and the Codex cross-check, the maintainer runs one filer script. No agent files anything.

## Entries

| # | Slug | Kind | P | Existing | Blocked by | Title |
|---|---|---|---|---|---|---|
| 01 | release-windows-z3-build | new | P1 | - | - | Release: x86_64-pc-windows-msvc build fails in z3-sys ('z3.h' file not found); provide Z3 on Windows and keep the target |
| 02 | release-aarch64-openssl-cross | new | P1 | - | - | Release: aarch64-unknown-linux-gnu cross build fails at openssl-sys (and needs arm64 Z3 next); fix it and keep the target |
| 03 | release-publish-and-install-smoke | new | P1 | - | [release-windows-z3-build, release-aarch64-openssl-cross] | Release: publish the first continuous-* prerelease for all five targets and smoke-test install.sh and action.yml against it in CI |
| 04 | release-reopen-note | reopen-note | P1 | str-lj7s | - | Comment on closed str-lj7s: closed on 'landed' with no green run; release.yml has 0 successes in 267 runs |
| 05 | drift-patrol-workflow-go-mod | new | P1 | - | - | Fix the scheduled Drift Patrol workflow (setup-go points at a nonexistent root go.mod; 7/7 scheduled runs red) and test every workflow path |
| 06 | drift-patrol-reopen-note | reopen-note | P1 | str-u394l.1 | - | Comment on closed str-u394l.1: the scheduled Drift Patrol has failed 7/7 runs (setup-go points at a nonexistent root go.mod) |
| 07 | workflow-health-patrol | new | P1 | - | [drift-patrol-workflow-go-mod] | Surface persistently red GitHub workflows to agents: drift-patrol workflow-health check, landing step, and follow-up issues for perf-ci, devcontainer and docker-publish |
| 08 | go-lint-and-gofmt-gated | new | P2 | - | - | Go lint is red on main (10 golangci-lint issues, 10 non-gofmt files) and ungated: fix the findings and gate golangci-lint and gofmt in check-static |
| 09 | go-lint-reopen-note | reopen-note | P2 | str-2tyfk | - | Comment on closed str-2tyfk and str-qwua7.32: closed with residuals and a false 'lint passes'; golangci-lint still reports 10 issues |
| 10 | rustfmt-gate | new | P2 | - | - | Restore rustfmt cleanliness in one dedicated commit (107 files drifted after str-fr1v) and gate `cargo fmt --check` in check-static |
| 11 | rustfmt-reopen-note | reopen-note | P2 | str-fr1v | - | Comment on closed str-fr1v: the tree drifted again (58-file churn on 2026-09-21; 107 files unformatted on 09-23); no fmt gate exists |
| 12 | ci-runs-user-paths | new | P2 | - | - | Smoke, walkthrough, gauntlet and E2E user paths never run in CI; the only CI gauntlet path (Perf CI) has 0/13 successes |
| 13 | workflow-action-versions | new | P3 | - | - | Workflows use deprecated Node-20 action majors, unpinned ubuntu-latest (Ubuntu 26 from 2026-10-19), and setup-go cache that cannot find go.sum |
| 14 | nextest-ci-profile-and-stale-parity-fallback | new | P3 | - | - | CI runs plain `cargo test` (nextest `[profile.ci]` is dead config, local and CI runners diverge) and parity-governed keeps a stale 'pending str-7jgm.2' fallback |

Dependency edges: release-windows-z3-build and release-aarch64-openssl-cross block release-publish-and-install-smoke; drift-patrol-workflow-go-mod blocks workflow-health-patrol. Soft relations are given in each body, for example the str-qwua7.3 fix for ci-runs-user-paths and nextest.

Reconciliation notes for the reviewer:
- The perf-ci follow-up that workflow-health-patrol calls for is ci-runs-user-paths, which already requires surfacing perf-ci's stderr and fixing or disabling the job. workflow-health-patrol links it and files new follow-ups only for devcontainer and docker-publish, to avoid a duplicate issue.
- code/73 and code/07 said "blocked by draft 01" (the Task checksum fix, now a note on str-qwua7.3). The manifest's dependency edges do not include that, so it is recorded as a relation, not a blocker.
- The go-lint reopen-note is posted on both str-2tyfk and str-qwua7.32 (front-matter existing_id is str-2tyfk).


---

<!-- file: 01-release-windows-z3-build.md -->

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


---

<!-- file: 02-release-aarch64-openssl-cross.md -->

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


---

<!-- file: 03-release-publish-and-install-smoke.md -->

---
slug: release-publish-and-install-smoke
kind: new
title: "Release: publish the first continuous-* prerelease for all five targets and smoke-test install.sh and action.yml against it in CI"
priority: P1
type: bug
labels: [release, ci, distribution, install, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [release-windows-z3-build, release-aarch64-openssl-cross]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Release: publish the first continuous-* prerelease for all five targets and smoke-test install.sh and action.yml against it in CI

## Problem

No GitHub release of Shatter exists. `install.sh` resolves the latest `continuous-*` prerelease, and `action.yml` calls `install.sh`, so both documented install paths fail for every user today. Nothing in CI exercises either path, which is why this went unnoticed through 267 red release runs.

Under D1 the full five-target matrix ships, so this issue waits for both build fixes. It does not add a partial-matrix publish. Once the builds are green, this issue proves that the release job publishes and that the install paths work end to end, and it keeps them tested from then on.

## Evidence

Re-verified 2026-09-23:

- `gh release list` → empty. `gh run list --workflow release.yml -L 300` → `{"cancelled":98,"failure":169}`, 0 successes. In run 35773969737, `Create continuous GitHub Release` is `skipped`.
- `.github/workflows/release.yml:183-186`: the release job is `needs: [build-ts, build]`, so any failed matrix leg skips it. `:190-196` builds the tag `continuous-${stamp}-${short_sha}`. `:211-236` package the five archives and `SHA256SUMS`. `:252-255` write the platform manifest. `:297` passes `--prerelease`.
- `install.sh:64-81`: calls `https://api.github.com/repos/${REPO}/releases?per_page=100` and selects the first `prerelease` whose tag starts with `continuous-`. With no release, it errors `Could not determine latest build`. `install.sh:30-40` supports only Linux/macOS × x86_64/aarch64.
- `action.yml` is a composite action. `action.yml:47` runs `bash "${{ github.action_path }}/install.sh"` and exposes `version` from `shatter --version`.
- `.github/workflows/cleanup-continuous-releases.yml` (weekly) already applies retention to continuous prereleases (str-fl9g.6), but there has never been anything to retain.
- No workflow invokes `install.sh` or `uses: ./` (grep of `.github/workflows/`).

## Acceptance criteria

- [ ] A `release.yml` run on main completes with every job `success`, including `Create continuous GitHub Release`, and `gh release list` shows a `continuous-*` prerelease with all five archives (`shatter-linux-x86_64.tar.gz`, `shatter-linux-aarch64.tar.gz`, `shatter-macos-x86_64.tar.gz`, `shatter-macos-aarch64.tar.gz`, `shatter-windows-x86_64.zip`), `SHA256SUMS` and the manifest.
- [ ] A CI job (a `release.yml` job with `needs:` on the release job, or a separate workflow triggered by `release: published` / `workflow_run`) smoke-tests the published release:
  - on `ubuntu-latest` (x86_64) and on a macOS runner: `curl -fsSL .../install.sh | bash` with `BUILD` unset (resolves latest), then `shatter --version` and a trivial `shatter analyze` on a bundled example;
  - a job that uses the composite action (`uses: ./`) and asserts its `version` output is non-empty;
  - each smoke job fails if the downloaded archive's checksum does not match `SHA256SUMS`.
- [ ] The smoke test is covered by `workflow-health-patrol` (it runs on push/schedule to main), so a future regression turns red visibly.
- [ ] Close only with (a) the URL of the green `release.yml` run and (b) the URL of the green install-smoke run against that release, both in the close reason.

## Suggested approach

Put the smoke job in `release.yml` after the release job so it tests exactly the tag just published: pass the tag through a job output and also run once with `BUILD` unset to exercise resolution. If Windows is to be covered by the action later, that requires Windows support in install.sh, which is out of scope here.

## Out of scope

- Fixing the Windows or aarch64 builds (the two blocking issues).
- Adding Windows support to `install.sh`.
- Changing retention policy (`cleanup-continuous-releases.yml`).

## Dependencies

- Blocked by: `release-windows-z3-build`, `release-aarch64-openssl-cross`.
- Related: str-lj7s (closed; see `release-reopen-note`), str-fl9g.6 (retention), `workflow-health-patrol`.

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
> Maintainer decision D1 (2026-09-23): both targets stay in the matrix and get fixed. The work is tracked in three new issues:
> - `<id of release-windows-z3-build>`: Windows Z3 build
> - `<id of release-aarch64-openssl-cross>`: aarch64 openssl/cross build (note that str-qwua7.41 deleted Cross.toml/cross/ on the false premise that no workflow uses cross)
> - `<id of release-publish-and-install-smoke>`: first published continuous-* prerelease, plus install.sh/action.yml smoke tests in CI
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

Re-verified 2026-09-23 against the worktree at `56c86168`:

- `.github/workflows/drift-patrol.yml:82-85`: `uses: actions/setup-go@v5` / `go-version-file: go.mod` (wrong, and no `cache-dependency-path`). The other workflows use `shatter-go/go.mod`: `ci.yml:53`, `perf-ci.yml:36`, `release.yml:131-132` (only release also sets `cache-dependency-path: shatter-go/go.sum`).
- `gh run list --workflow drift-patrol.yml` → `{"failure":7,"success":2}`. The failures are the schedule runs of 08-10, 08-17, 08-24, 08-31, 09-07, 09-14 and 09-21, and the two successes are the 08-07 PR runs. `gh run view 35620815498` (09-21 schedule) shows: `The specified go version file at: go.mod does not exist`.
- The patrol job installs no system packages. `ci.yml:61-64` installs `libclang-dev z3`. Once setup-go is fixed, the `task ts:build go:build rust-fe:build` step (`:115-116`) or the `--require-conformance` patrol (`:118-126`) may surface the next missing dependency.
- `scripts/test_ci_workflow_structure.py` never mentions `drift-patrol.yml`, and it checks only `ci.yml`'s shape. str-35vtk.35 (open) wires that script into a local Taskfile task.
- `docs/DRIFT-PATROL.md:27-28`: "...so the patrol cannot rot in place." `:24` Owner: "The maintainer on the weekly triage rotation; if there is no rotation, whoever is landing work that week". No rotation exists.
- `docs/DRIFT-PATROL.md:33-43` "What it checks" lists 7 checks. `scripts/drift-patrol.py:757-766` `CHECKS` registers 8 (the extra one is `("tracker-server", check_tracker_server)`, defined at `:686`, added by str-qwua7.16). `AGENTS.md:22` references `--only tracker-server`.
- Tracker data in CI: `scripts/drift-patrol.py:183-221` `load_tracker()` falls back to the committed `.beads/issues.jsonl` when `bd` is absent, as it is in CI. D4 (2026-09-23) retires the JSONL import. `beads-jsonl-consumers-drop-bd-sync` (shatter-tracker-and-beads bucket) owns changing that data source.
- Run locally, `python3 scripts/drift-patrol.py` does run and reports a tracker-hygiene FAIL. The script works; only the workflow is broken.

## Acceptance criteria

- [ ] `drift-patrol.yml` uses `go-version-file: shatter-go/go.mod` and `cache-dependency-path: shatter-go/go.sum`.
- [ ] A test fails on the current tree and passes after the fix. It lives in `scripts/test_ci_workflow_structure.py` or a new module that `task meta` runs; coordinate with str-35vtk.35. For every `.github/workflows/*.yml` it asserts that each `go-version-file`, `cache-dependency-path`, `working-directory` and `hashFiles(...)` literal path exists in the repo. Paste the failing and then passing output in the close reason.
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
title: "Surface persistently red GitHub workflows to agents: drift-patrol workflow-health check, landing step, and follow-up issues for perf-ci, devcontainer and docker-publish"
priority: P1
type: task
labels: [agents, ci, github-actions, drift, landing, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [drift-patrol-workflow-go-mod]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Surface persistently red GitHub workflows to agents: drift-patrol workflow-health check, landing step, and follow-up issues for perf-ci, devcontainer and docker-publish

## Problem

Several main-branch and scheduled workflows fail on every run, and until this audit none of them had a tracker issue. Landing and all agent checks look only at the local verifier and, at most, `ci.yml`, so a workflow that stays red is invisible to agents. The failures include Build and Release (0 successes in 267 runs), Drift Patrol (7/7 scheduled runs), Perf CI (13/13) and Devcontainer (19 failures, last success February). The patrol that should report drift is itself one of the broken workflows (`drift-patrol-workflow-go-mod`).

This issue adds the missing feedback loop on the shatter side: a read-only `workflow-health` drift-patrol check, plus a landing instruction to run it. It also files the per-workflow fix issues that do not exist yet. The bento-side counterpart, polling CI for the landed main SHA inside `land-work`, is `land-work-post-push-workflow-health` in the bento tracker. That is an intentional cross-repo split.

## Evidence

Re-verified 2026-09-23 with `gh run list --workflow <w> -L 300 --json conclusion`:

| Workflow | Trigger | Conclusions | Failing step (latest run) |
|---|---|---|---|
| `release.yml` | push main, dispatch | failure 169, cancelled 98, success 0 | Windows `z3.h` not found; aarch64 `openssl-sys` (run 35773969737). Now filed: `release-windows-z3-build`, `release-aarch64-openssl-cross`, `release-publish-and-install-smoke` |
| `drift-patrol.yml` | schedule Mon 09:00, dispatch, PR | failure 7, success 2 (PR only) | setup-go `go.mod does not exist`. Now filed: `drift-patrol-workflow-go-mod` |
| `perf-ci.yml` | schedule Mon 09:00, dispatch | failure 13 | `perf` / `Run stable perf scenarios` (run 35620979400); the audit saw "gauntlet-auto-warm failed on run 1 with exit code 1" with the reason not surfaced |
| `devcontainer.yml` | push main (`.devcontainer/**`), schedule, dispatch | failure 19, success 2 (2026-02) | `build-and-test` / `Build devcontainer and run tests` (run 35619881868) |
| `docker-publish.yml` | push tags `v*`, PR to main | failure 7 | `build-and-push` / `Build and push` (run 31210298465, cargo build exit 101). It has no main or schedule trigger, so the check below does not cover it; its last run was a 2026-08-07 PR |
| `parity-expiry.yml` | schedule | success 9, failure 3 (08-31, 09-07, 09-14; green 09-21) | not persistently red; the check must not flag it |
| `ci.yml` | push/PR main | success 98, failure 72, cancelled 4 (latest green) | n/a |

- `scripts/drift-patrol.py:757-766`: the `CHECKS` registry has no workflow-health check.
- AGENTS.md mentions `gh run` only in an rtk example (`AGENTS.md:581`). No AGENTS.md, CLAUDE.md or skill instruction tells agents to check workflow conclusions after landing.
- `bd search` for windows / aarch64 / "release workflow" / perf-ci / devcontainer / docker found no open issue at audit time (agent-repo-03, prior-02).
- str-qwua7.42 (open) only moves `perf-ci.yml` paths during the benchmarks merge. It does not address the red runs.

## Acceptance criteria

- [ ] New drift-patrol check `workflow-health`, registered in `CHECKS` and documented in `docs/DRIFT-PATROL.md` (the table test added by `drift-patrol-workflow-go-mod` enforces this):
  - It covers each workflow in `.github/workflows/` that triggers on `push` to main or on `schedule`.
  - It reads the last N (default 3, flag `--workflow-health-runs`) completed runs on `main` via `gh run list --workflow <file> --branch main --json conclusion,databaseId,url,createdAt`.
  - It reports FAIL when all N failed, printing the workflow name, the last run URL and the first failing job and step (`gh run view <id> --json jobs`).
  - It reports SKIP (not PASS) when `gh` is missing or unauthenticated, or when a workflow has fewer than N completed runs.
  - With the current data it FAILs on release, perf-ci and devcontainer (and on drift-patrol until that fix lands) and PASSes on ci and parity-expiry.
- [ ] Unit tests for the check in `scripts/test_drift_patrol.py` use canned `gh` JSON and cover: all-red → FAIL, mixed → PASS, `gh` unavailable → SKIP, fewer than N runs → SKIP.
- [ ] The AGENTS.md landing section says: after pushing main, run `python3 scripts/drift-patrol.py --only workflow-health`, and make sure a tracker issue exists for every FAIL before ending the session.
- [ ] Every persistently red workflow has a linked fix issue, each with the failing run URL and first error:
  - (a) perf-ci `Run stable perf scenarios` / gauntlet-auto-warm: already covered by `ci-runs-user-paths` in this bucket, which requires surfacing the suppressed stderr and fixing or disabling the job. Link it. Do not file a duplicate.
  - (b) devcontainer `Build devcontainer and run tests`: file a new child of the audit epic.
  - (c) docker-publish `Build and push` (cargo exit 101): file a new child of the audit epic.
  For perf-ci, devcontainer and docker-publish, the fix issue may choose to fix the workflow, or to disable it with the reason documented in the workflow file and the issue.
- [ ] Release targets are not disabled. Under D1 they are fixed in `release-windows-z3-build` / `release-aarch64-openssl-cross` / `release-publish-and-install-smoke`. This issue links those and files nothing more for release.
- [ ] Proof at close: paste the output of `python3 scripts/drift-patrol.py --only workflow-health` from a real, authenticated run, and include the ids of the devcontainer and docker-publish follow-up issues.

## Suggested approach

Reuse the existing `Result` / PASS-FAIL-SKIP-PENDING structure and `run_command` helper in `scripts/drift-patrol.py`. Keep the check read-only: it reports, it never files. Filing the devcontainer and docker-publish follow-ups is a manual step of this issue. In CI the scheduled patrol has `GITHUB_TOKEN`. Give the patrol job `actions: read` permission so `gh run list` works there too.

## Out of scope

- Fixing the perf-ci (`ci-runs-user-paths`), devcontainer or docker-publish builds (the follow-ups do that).
- Fixing the release builds (already filed in this bucket).
- Bento `land-work` CI polling (bento tracker: `land-work-post-push-workflow-health`).

## Dependencies

- Blocked by: `drift-patrol-workflow-go-mod`. The check is only useful once the patrol itself runs on schedule.
- Related: `release-windows-z3-build`, `release-aarch64-openssl-cross`, `release-publish-and-install-smoke`, `ci-runs-user-paths` (perf-ci gauntlet overlap), str-qwua7.42, bento `land-work-post-push-workflow-health` (cross-repo; link in body text only).

Priority: P1 · Type: task · Labels: agents, ci, github-actions, drift, landing, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-agent/03, agent-repo-03, tests-ci-03 · Decision: D1


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

`shatter-go` has a golangci-lint configuration, and the go-conventions skill describes lint as enforced. In practice no gate and no CI job runs it. The `go:lint` task has an "optional" precondition and silently no-ops when the tool is absent, and nothing depends on it. The tree now has 10 lint findings, including a tautological nil check in the execute handler and 7 dead functions, and 10 files that are not gofmt-clean. Two issues were closed on the claim that lint passed: str-2tyfk (empty close reason, residuals listed in its own body) and str-qwua7.32 (its acceptance "task go:lint passes" was false at close).

This merges audit drafts shatter-code/57 and shatter-agent/25 (report §15.1).

## Evidence

Re-verified 2026-09-23 in the worktree at `56c86168` with golangci-lint 2.12.2:

- `cd shatter-go && golangci-lint run --timeout 9m ./...` → exit 1, `10 issues:`
  - `protocol/handler.go:1325:32: nilness: tautological condition: nil == nil (govet)`, on the line `if preparedExec == nil && err == nil {`
  - `instrument/property_test.go:544:18: SA5011: possible nil pointer dereference (staticcheck)` (related `:541:7`)
  - unused: `protocol/analyzer.go:592` analyzeFunc, `:799` extractParams, `:1518` mapTypeInfo, `:1534` structTypeInfo; `protocol/handler.go:1964` (*Handler).lookupAnalyzedByTargetID; `protocol/prepared_launcher.go:474` toWrapperConstructors, `:506` toWrapperConstructorParams
- `cd shatter-go && gofmt -l . | grep -v testdata` → 10 files:
  - non-test: `instrument/symextract.go`, `protocol/analysis_cache.go`, `workspace/run.go`
  - test: `instrument/mockfingerprint_test.go`, `instrument/overlay_test.go`, `launcher/launcher_buildvcs_test.go`, `protocol/analysis_cache_handler_test.go`, `protocol/generated_enums_test.go`, `protocol/invocation_plan_test.go`, `protocol/property_test.go`
- `shatter-go/Taskfile.yml:61-71`: `lint` precondition `command -v golangci-lint` with msg `golangci-lint not installed (optional)`, then `golangci-lint run ./...`.
- Root `Taskfile.yml:184-186`: `lint` deps `[workspace-clippy, rust-fe:clippy, rust-rt:clippy, go:vet]` (no go:lint). `:535-546` `check-static` has no Go lint or format step. `:548-554` `check-unit` runs `go:test` and `go:vet` only. `.github/workflows/ci.yml` installs no golangci-lint.
- `shatter-go/.golangci.yml:3-4`: "Runs via: task go:test" (false). `:7` `timeout: 3m` (too short under load; the audit needed 8 minutes).
- `shatter-go/Taskfile.yml:13-16`: comment says CI "runs build via parity/conformance deps but never go:vet/go:test". This is stale, since `check-unit` runs both.
- str-2tyfk closed 2026-09-08 with an empty close reason. str-qwua7.32 closed with reason "Closed".

## Acceptance criteria

- [ ] All 10 golangci-lint findings are fixed: delete the dead functions, fix the tautology at `handler.go:1325` (decide what the second condition was meant to test), and fix the test nil dereference. `golangci-lint run ./...` in `shatter-go` exits 0.
- [ ] `gofmt -l` prints nothing for non-testdata files.
- [ ] `check-static` runs `go:lint` and a gofmt check (`test -z "$(gofmt -l $(git ls-files '*.go' | grep -v /testdata/))"` or equivalent). Under `CI=1` a missing golangci-lint fails the task and does not skip it. `ci.yml` installs a pinned golangci-lint version (for example `golangci/golangci-lint-action` with `install-only`, or `go install ...@v2.x.y`).
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
- Related: str-2tyfk, str-qwua7.32 (closed; see `go-lint-reopen-note`), `rustfmt-gate`, `task-sources-cover-real-inputs`.

Priority: P2 · Type: bug · Labels: go, shatter-go, quality-gates, agents, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/57, shatter-agent/25, prior-08, frontend-go-05


---

<!-- file: 09-go-lint-reopen-note.md -->

---
slug: go-lint-reopen-note
kind: reopen-note
title: "Comment on closed str-2tyfk and str-qwua7.32: closed with residuals and a false 'lint passes'; golangci-lint still reports 10 issues"
priority: P2
type: note
labels: [go, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-2tyfk
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-2tyfk (and str-qwua7.32)

Targets: **str-2tyfk** (closed) and **str-qwua7.32** (closed). Post the same comment on both. Do not reopen either.

Comment text:

> Audit 2026-09-22 follow-up. This issue was closed without its lint acceptance being true. str-2tyfk has an empty close reason and was closed on a narrowed "changed files only" check. str-qwua7.32 was closed as "Closed" with the acceptance "task go:lint passes". On main as of 2026-09-23, `golangci-lint run ./...` in shatter-go still exits 1 with 10 issues:
> - govet nilness at `protocol/handler.go:1325` (`nil == nil`)
> - staticcheck SA5011 at `instrument/property_test.go:544`
> - 7 unused functions: analyzeFunc, extractParams, mapTypeInfo, structTypeInfo, lookupAnalyzedByTargetID, toWrapperConstructors, toWrapperConstructorParams
>
> str-2tyfk's own body listed several of these residuals. `gofmt -l` also lists 10 files.
>
> The root cause is that `go:lint` is optional ("golangci-lint not installed (optional)") and no gate or CI job runs it, so "lint passes" was never checked by anything. The fix, gating golangci-lint and gofmt in check-static, is tracked in `<id of go-lint-and-gofmt-gated>`. That issue requires forced-gate output at close.

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

Re-verified 2026-09-23 in the worktree at `56c86168` with local `rustfmt 1.9.0-stable (ac68faa20c 2026-05-25)`:

- `cargo fmt --all -- --check` (workspace) lists 97 files: shatter-core 61, shatter-cli 25, shatter-llm 11 (for example `shatter-cli/build.rs:37`, `shatter-cli/src/args.rs:361`).
- `cd shatter-rust && cargo fmt --all -- --check` lists 9 files. `cd shatter-rust-runtime && cargo fmt --all -- --check` lists 1 file. These crates are excluded from the workspace (`Cargo.toml:3`), so the workspace command does not cover them.
- A grep of `Taskfile.yml`, `taskfiles/`, `shatter-*/Taskfile.yml` and `.github/` finds no `fmt --check` / `fmt -- --check`.
- There is no `rust-toolchain.toml`. CI uses `dtolnay/rust-toolchain@stable` (`ci.yml:38-41`), so the rustfmt version floats. That is how str-fr1v's "1.93 drift" happened.
- Session c1689435 (2026-09-21T22:53): `cargo fmt -p shatter-core` → "58 files changed, 3068 insertions(+), 789 deletions(-)", then `... | xargs git checkout --` and `scratchpad/apply_child_a.py` (sessions-11).
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
title: "Smoke, walkthrough, gauntlet and E2E user paths never run in CI; the only CI gauntlet path (Perf CI) has 0/13 successes"
priority: P2
type: task
labels: [ci, smoke, walkthrough, gauntlet, e2e, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Smoke, walkthrough, gauntlet and E2E user paths never run in CI; the only CI gauntlet path (Perf CI) has 0/13 successes

## Problem

`ci.yml` runs `task check` plus the shatter-llm steps, and nothing else. `task check` does not include `smoke`, `walkthrough`, `gauntlet` or `e2e`. Those gates, which exercise what a user actually runs, run only when an agent chooses to run them. The one workflow that touches the gauntlet, the weekly Perf CI (`gauntlet-auto-warm` is in its stable scenario list), has failed every run with its reason suppressed. A regression in the demo or user path can therefore land on main and stay there until someone happens to run the gate locally.

## Evidence

Re-verified 2026-09-23 in the worktree at `56c86168`:

- `.github/workflows/ci.yml:88-110`: the steps are `task check`, `cargo clippy -p shatter-llm`, `cargo test -p shatter-llm`, and `python3 scripts/test_ci_workflow_structure.py`. No workflow mentions `smoke`, `walkthrough` or `gauntlet` (`grep -rn` over `.github/workflows/`).
- `Taskfile.yml:648-660` `smoke` (about 15 s; TS + two Go explores + `scripts/test_empty_report_regression.sh`). `:680-689` `walkthrough` / `walkthrough-governed` (`demo/walkthrough.sh --auto --delay 0`). `:701-710` `gauntlet`. `:577-589` `e2e` / `e2e-governed` (TS, Go, Rust). `:535-575` `check-static`/`check-unit`/`check-integration` include none of them.
- `perf-ci.yml:58` runs `perf_runner.py run --scenario-file perf/stable-scenarios.txt`. `perf/stable-scenarios.txt` lists `gauntlet-auto-warm`, `explore-ts-arithmetic-warm`, `scan-standalone-ts-warm` and `go-frontend-instrument-tests`. `gh run list --workflow perf-ci.yml` → `{"failure":13}`. The latest run, 35620979400, fails in `Run stable perf scenarios`. The audit's log read showed "gauntlet-auto-warm failed on run 1 with exit code 1", with the underlying stderr not printed.
- Separately, CI's `task check` test leaves have reported "up to date" since about 2026-08-29 (see the str-qwua7.3 note `task-list-json-poisons-checksums` and `ci-executed-leaf-guard` in the shatter-gates-integrity bucket). The E2E coverage that `check-integration`'s `core:test-ignored` provides in CI is therefore hollow as well.

## Acceptance criteria

- [ ] A CI job on push to main (or nightly on schedule) runs `task smoke` and a bounded walkthrough (`task walkthrough`, or a documented subset if it exceeds the job budget) and uploads their output as workflow artifacts.
- [ ] The same job or a sibling runs `task e2e`, or the issue records why `check-integration`'s E2E coverage is sufficient once `ci-executed-leaf-guard` makes it real.
- [ ] Each new job fails on a real regression. On a branch, show a deliberate break (for example making `demo/walkthrough.sh` exit 1, or breaking a smoke target) producing a red job, then revert, and cite both run URLs in the close reason. If str-qwua7.10's content assertions have landed, the walkthrough job enforces them.
- [ ] perf-ci's `Run stable perf scenarios` step prints the failing scenario's stderr. `perf_runner.py` surfaces the child output on failure. Then either the gauntlet-auto-warm failure is fixed, with a green `perf-ci.yml` run URL cited, or the scenario/job is disabled with the reason and a tracking issue id written in the workflow file.
- [ ] `scripts/test_ci_workflow_structure.py` asserts the new job and steps exist, so they cannot be dropped silently.

## Suggested approach

Add a separate `user-paths` job in `ci.yml` (push to main only, not PRs, to keep PR latency down), or a new nightly workflow. Reuse the apt/toolchain setup from the `test` job. `workflow-health-patrol` will then cover the nightly run. Combine with str-qwua7.10 so the walkthrough job checks content, not just the exit code.

## Out of scope

- The Task checksum problem itself (str-qwua7.3 / `ci-executed-leaf-guard`).
- Gauntlet allowlist content.
- Moving perf-ci paths (str-qwua7.42).

## Dependencies

- None hard-blocking. The smoke/walkthrough job does not depend on the checksum fix.
- Related: str-qwua7.10 (open; demo gates fail on bad content), str-qwua7.42 (perf-ci paths), `ci-executed-leaf-guard`, `workflow-health-patrol` (links this issue as the perf-ci follow-up).

Priority: P2 · Type: task · Labels: ci, smoke, walkthrough, gauntlet, e2e, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/73, tests-ci-11


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

Re-verified 2026-09-23 in the worktree at `56c86168`:

- Run 35756993223 (CI) annotations:
  - "Node.js 20 is deprecated ... actions/cache@v4, actions/checkout@v4, actions/setup-go@v5, actions/setup-node@v4, arduino/setup-task@v2"
  - "The ubuntu-latest label will migrate to Ubuntu 26 beginning October 19, 2026"
  - "Restore cache failed: Dependencies file is not found ... Supported file pattern: go.sum"
- Action usage across `.github/workflows/*.yml`: `actions/checkout@v4` ×11, `actions/cache@v4` ×4, `actions/setup-go@v5` ×4, `actions/setup-node@v4` ×4, `actions/setup-python@v5` ×3, `actions/upload-artifact@v4` ×3, `actions/download-artifact@v4` ×1, `arduino/setup-task@v2` ×2, plus docker/*, `devcontainers/ci@v0.3`, `dtolnay/rust-toolchain@stable`.
- `runs-on: ubuntu-latest` appears 10 times, in `ci.yml`, `drift-patrol.yml`, `perf-ci.yml`, `release.yml`, `devcontainer.yml`, `docker-publish.yml`, `parity-expiry.yml` and `cleanup-continuous-releases.yml`. `release.yml` also uses `windows-latest` and macOS labels through `matrix.os`.
- setup-go without `cache-dependency-path`: `ci.yml:51-53`, `drift-patrol.yml:83-85` (fixed separately by `drift-patrol-workflow-go-mod`) and `perf-ci.yml:34-36`. Only `release.yml:131-132` sets `cache-dependency-path: shatter-go/go.sum`.

## Acceptance criteria

- [ ] Every action whose major runs on Node 20 is bumped to a Node-24-based major. Check each action's releases for the current major; do not guess version numbers.
- [ ] Linux runners are pinned to `ubuntu-24.04` until a trial run on Ubuntu 26 (`ubuntu-26.04` label, or `ubuntu-latest` after the migration) proves that `apt-get install libclang-dev z3` and `task check` work. Record that follow-up as an issue.
- [ ] Every `actions/setup-go` step sets `cache-dependency-path: shatter-go/go.sum`.
- [ ] Proof at close: link one post-change run each of `ci.yml` and `release.yml` whose annotations show no Node-20 deprecation warning and no "Restore cache failed ... go.sum" warning.

## Suggested approach

Make one mechanical PR across all workflow files. If `drift-patrol-workflow-go-mod` has landed first, `drift-patrol.yml`'s setup-go is already fixed. Dependabot for `github-actions` (`.github/dependabot.yml`) is an optional way to keep the majors current, and is the implementer's call.

## Out of scope

- Fixing workflows that are red for other reasons (the release, drift-patrol, perf-ci and devcontainer issues in this bucket).
- `dtolnay/rust-toolchain@stable` pinning (see `rustfmt-gate` for the fmt toolchain).

## Dependencies

- None blocking.
- Related: `drift-patrol-workflow-go-mod`, `workflow-health-patrol`.

Priority: P3 · Type: chore · Labels: ci, github-actions, maintenance, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/77, tests-ci-18


---

<!-- file: 14-nextest-ci-profile-and-stale-parity-fallback.md -->

---
slug: nextest-ci-profile-and-stale-parity-fallback
kind: new
title: "CI runs plain `cargo test` (nextest `[profile.ci]` is dead config, local and CI runners diverge) and parity-governed keeps a stale 'pending str-7jgm.2' fallback"
priority: P3
type: chore
labels: [ci, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# CI runs plain `cargo test` (nextest `[profile.ci]` is dead config, local and CI runners diverge) and parity-governed keeps a stale 'pending str-7jgm.2' fallback

## Problem

The Rust test tasks use cargo-nextest when it is on PATH and fall back to plain `cargo test` otherwise. CI never installs nextest, so CI and local gates run different test runners, and `.config/nextest.toml`'s `[profile.ci]` (`retries = 1`, `fail-fast = false`, `final-status-level = "flaky"`) is never applied anywhere. Locally, the default profile's `fail-fast = true` means one failing or timed-out test hides the rest of the suite. The audit saw 87 of 3531 tests unrun when `bench_frontier_ranking` timed out, before str-6nul9 excluded that benchmark. In CI, the E2E suites have no per-test timeout at all.

Separately, the `parity-governed` task still carries an `if [ -f scripts/validate-parity.py ] ... else echo "[skip] ... pending str-7jgm.2"` fallback. The script exists and str-7jgm.2 is closed, so the fallback is dead code that would silently turn a deleted script into a skip.

Verifier correction (tests-ci-07): the `rust-frontend-harness` test group (`max-threads = 1`) exists because nextest's process-per-test model defeats the tests' in-process mutexes. Under plain `cargo test` in CI those mutexes do serialize the fixture builds. So "flake serialization only applies locally" is not a problem, and this issue does not claim it. The remaining points are the dead config and the runner divergence.

## Evidence

Re-verified 2026-09-23 against `main` (70465921):

- `.github/workflows/ci.yml`: no cargo-nextest install step and no `NEXTEST_PROFILE` or `--profile ci`. A repo-wide grep finds no reference to the ci profile outside audit drafts.
- `.config/nextest.toml`:
  - `[profile.default]`: `test-threads = 4`, `slow-timeout = { period = "60s", terminate-after = 2 }`, `fail-fast = true`.
  - `[profile.ci]`: `retries = 1`, `fail-fast = false`, `final-status-level = "flaky"`, same `rust-frontend-harness` override. Nothing uses it.
- nextest-or-fallback branches: `shatter-core/Taskfile.yml:28-32` (test), `:61-65` (test-ignored), `:88-92` (test-ignored-fast), `shatter-cli/Taskfile.yml:27`, `:46`, `shatter-rust/Taskfile.yml:22`, `shatter-rust-runtime/Taskfile.yml:22`. For example, `:65` falls back to `cargo test -p shatter-core -- --include-ignored --skip bench_frontier_ranking`.
- `Taskfile.yml:265-275` `parity-governed`: `if [ -f scripts/validate-parity.py ]; then python3 scripts/validate-parity.py; else echo "[skip] validate-parity.py not present (pending str-7jgm.2)"; fi`. `scripts/validate-parity.py` exists, and `bd show str-7jgm.2` shows it CLOSED.
- The CI test leaves are currently not executing at all (Task checksum poisoning; see the str-qwua7.3 note `task-list-json-poisons-checksums` and `ci-executed-leaf-guard`). This issue matters once those land, because CI will then really run the fallback path.

## Acceptance criteria

- [ ] CI installs cargo-nextest (for example `taiki-e/install-action@<pinned>` with `tool: cargo-nextest`) and sets `NEXTEST_PROFILE=ci`, or the Taskfiles pass `--profile ci` when `CI` is set. Alternatively, delete `[profile.ci]` with a comment in `nextest.toml` explaining why CI stays on `cargo test`. Either way, no unused profile remains.
- [ ] The landing gate does not stop at the first failure. Either the profile the gate uses has `fail-fast = false`, or the gate passes `--no-fail-fast`. A failing test's summary lists all failures.
- [ ] If nextest is adopted in CI, the flaky-retry report (`final-status-level = "flaky"`) appears in the CI log. The close reason cites a CI run URL showing the nextest summary line from `core:test-ignored`.
- [ ] The `else` fallback in `parity-governed` is removed, so `python3 scripts/validate-parity.py` runs unconditionally.
- [ ] Proof at close: forced-gate output (`task parity --force`, or with the checksum cleared) showing validate-parity running, plus the CI run URL above.

## Suggested approach

Prefer installing nextest in CI. It removes the local/CI divergence and gives the E2E suites the 120 s per-test terminate-after in CI too. Keep the `cargo test` fallback in the Taskfiles for developer machines without nextest, but make the CI path explicit.

## Out of scope

- Excluding `bench_frontier_ranking` from the gate (done in str-6nul9).
- The Task checksum problem (str-qwua7.3).
- NEXTEST thread budget (str-35vtk.14).

## Dependencies

- None hard-blocking. The work gains most value after the str-qwua7.3 fix (`task-list-json-poisons-checksums`) makes CI test leaves execute.
- Related: str-6nul9 (closed), str-35vtk.7 (closed; wired nextest locally), str-35vtk.14, `ci-executed-leaf-guard`.

Priority: P3 · Type: chore · Labels: ci, quality-gates, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/07, gates-08, tests-ci-07
