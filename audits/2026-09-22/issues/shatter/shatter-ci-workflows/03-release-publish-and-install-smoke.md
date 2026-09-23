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
