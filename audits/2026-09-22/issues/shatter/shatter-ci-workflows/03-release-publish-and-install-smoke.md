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
