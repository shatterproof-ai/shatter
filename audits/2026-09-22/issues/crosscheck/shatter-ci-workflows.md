# Cross-check review

- **Reviewer:** claude (DEGRADED same-runtime fallback)
- **Artifact type:** issue
- **Mode:** degraded

> **DEGRADED REVIEW.** The counterpart runtime was unavailable, so this review came from an independent agent of the *same* runtime. It shares the original author's model and blind spots; weight it accordingly.

## Findings

# Cross-check review (DEGRADED: same-runtime fallback): shatter-ci-workflows bundle

DEGRADED. The Codex counterpart review failed identity validation (exit 4), so a Claude reviewer did this review read-only. It is not a cross-runtime review.

Scope: 14 drafts in audits/2026-09-22/issues/shatter/shatter-ci-workflows/BUNDLE.md. Claims were checked against worktree 56c86168, live `gh`, the local cargo registry, and `bd` in /home/ketan/project/shatter.

## Verified claims (spot checks)

- The release.yml run history is failure 169, cancelled 98, success 0, and `gh release list` is empty. In run 35773969737, windows and aarch64 fail and the release job is skipped. drift-patrol shows failure 7 and success 2; perf-ci failure 13; devcontainer failure 19 and success 2. All match.
- There is no root go.mod. `drift-patrol.yml` sets `go-version-file: go.mod`, and its patrol job carries `if: github.event_name != 'pull_request'`. `scripts/test_ci_workflow_structure.py` never mentions drift-patrol.
- DRIFT-PATROL.md lists 7 checks, while `CHECKS` registers 8 (tracker-server). The "cannot rot in place" text and the rotation owner line are both present.
- Cross.toml and cross/ are absent. The 5abb7bd5 message contains "no workflow invokes cross", but release.yml uses `cross build`. shatter-llm uses reqwest with default features, so it pulls in native-tls.
- The cross and cargo release steps are at the cited lines. The Linux release step installs only `libclang-dev`, and macOS installs `brew install z3`.
- rustfmt reports 97 workspace files, 9 in shatter-rust and 1 in shatter-rust-runtime. No `fmt --check` exists in any Taskfile or workflow, and there is no rust-toolchain.toml.
- gofmt lists 10 non-testdata files. `handler.go:1325` is `if preparedExec == nil && err == nil`. `lint` deps omit go:lint, and the `.golangci.yml` header says "Runs via: task go:test".
- `.config/nextest.toml` defines `[profile.ci]`, and nothing uses it. The `parity-governed` fallback is present, and str-7jgm.2 is CLOSED.
- The action-major counts are all exact: checkout x11, cache x4, setup-go x4, and the rest. ubuntu-latest appears 10 times. setup-go has no cache-dependency-path in ci.yml or perf-ci.yml.
- Duplication: `bd search` (open) found nothing for windows, aarch64, golangci, rustfmt, nextest, drift patrol or devcontainer. For perf-ci it found only str-qwua7.42, which moves paths, as the drafts note. str-35vtk.35 and str-qwua7.10 are open, as cited.

## Findings

### MAJOR: release-publish-and-install-smoke (with release-windows-z3-build and release-aarch64-openssl-cross): the runtime libz3 dependency of the Linux and macOS artifacts is ignored
README.md:63-71 says "`shatter-core` links the system Z3 library". The Linux and macOS release builds link libz3 dynamically: pkg-config and system headers on Linux, `brew install z3` on macOS. Neither install.sh nor action.yml installs or checks for libz3, and neither mentions Z3.

The drafts deal with Z3 only as a Windows build-time problem (01, where only a dynamic `libz3.dll` gets staged) and as an arm64 cross build-time problem (02). Draft 03 then smoke-tests `shatter --version` on ubuntu-latest and macOS runners. That test can pass or fail for the wrong reason:
- A GitHub runner image may already have libz3, which would mask the missing dependency.
- A clean macOS runner has no brew z3, so the smoke fails with a loader error that no draft owns.

Fix: in 03, add an acceptance criterion that either the artifacts are self-contained (z3 `bundled` for every target) or install.sh/action.yml document and check the libz3 runtime prerequisite. Also run the smoke on a runner where libz3 is confirmed absent, for example via `ldd`/`otool -L` plus a missing-lib assertion. Draft 01's "Linux and macOS builds must not change" constraint may then need relaxing.

### MINOR: release-windows-z3-build: the suggested `static-link-z3` feature is deprecated
In z3-sys 0.10.7, `static-link-z3 = ["bundled", "deprecated-static-link-z3"]`, and build.rs prints "The 'static-link-z3' feature is deprecated. Please use the 'bundled' feature." The suggested approach should lead with `bundled`. It should also say the feature has to be enabled consistently on `z3`/`z3-sys`, because shatter-core depends on both directly (Cargo.toml:23-24).

### MINOR: release-windows-z3-build / release-publish-and-install-smoke: the green x86_64-linux leg is a warm-cache result
The x86_64-linux job in run 35773969737 got a cache hit on `x86_64-unknown-linux-gnu-cargo-056bf6...`, compiled only 4 crates, and never rebuilt z3-sys. The release Linux step installs no libz3-dev, so whether a cold build still succeeds is unverified. Drafts 01 and 03 treat this leg as known-good. An acceptance criterion in 03 should require one cold-cache run, or install `libz3-dev` explicitly as ci.yml does (ci.yml installs `z3`, not `libz3-dev`, which is itself worth a look).

### MINOR: go-lint-and-gofmt-gated: "silently no-ops when the tool is absent" is false
Task preconditions fail loudly. A test Taskfile with the same `preconditions: - sh: command -v <missing>` shape exits 201 with "precondition not met". The real defect is that nothing depends on `go:lint`. The criterion "Under CI=1 a missing golangci-lint fails the task and does not skip it" is already current behaviour, so it can mislead an implementer into adding CI-conditional logic. Reword the Problem and drop or reframe that criterion.

### MINOR: drift-patrol-workflow-go-mod / workflow-health-patrol: small line-reference drift
`CHECKS` is at scripts/drift-patrol.py:758-767, not 757-766. This is harmless, but "re-verified" citations should be exact.

### MINOR: rustfmt-gate: hidden-context citations
The draft cites session c1689435, `scratchpad/apply_child_a.py` and the private memory file `project_shatter_tree_not_rustfmt_clean.md`. A fresh agent cannot see any of them. They are fine as provenance, but the body should make clear that nothing in the work depends on reading them. It mostly does already.

### MINOR: ci-runs-user-paths: the perf-ci failure reason is second-hand
"gauntlet-auto-warm failed on run 1 with exit code 1" comes from the audit's log read and was not re-verified in this pass. The acceptance criterion correctly requires surfacing stderr first, so this is low risk.

## Scope and structure

- The release split (01/02 build fixes feeding 03) and the drift-patrol dependency (05 blocking 07) are coherent. The reopen notes are properly comment-only, with placeholders.
- 07 packs a check, an AGENTS.md step and two follow-up filings into one issue. That is acceptable because the filings are small and explicit, and the perf-ci de-duplication against 12 is handled.
- 14 bundles two unrelated chores: the nextest CI profile and the parity-governed fallback. Both are tiny, so this is acceptable, but they could be split.

## Verdict

Ready to file after one substantive fix. Every factual claim checked holds, except the go:lint "silent no-op" wording. Top fixes, in order:
1. Add the libz3 runtime dependency of the Linux and macOS artifacts to 03, with a clean-runner smoke, and reconcile it with 01's "Linux/macOS unchanged" constraint.
2. Prefer z3 `bundled` over the deprecated `static-link-z3` in 01.
3. Correct the "silently no-ops" claim and the redundant CI=1 criterion in 08.
