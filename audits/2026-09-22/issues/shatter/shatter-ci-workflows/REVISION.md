# Revision: shatter-ci-workflows (2026-09-23)

Inputs: the Codex cross-check (`../../crosscheck/shatter-ci-workflows.codex.md`, primary) and the earlier degraded same-runtime review (`../../crosscheck/shatter-ci-workflows.md`, secondary). Nothing is filed (D6).

I re-verified every finding against the audit worktree (main 70465921 plus audit files). I also checked the cargo registry source (z3-sys 0.10.7), live `gh` job logs, and `bd show` for str-2tyfk and str-qwua7.32.

## Codex findings

| # | Sev | Finding (one line) | Action | Files |
|---|---|---|---|---|
| 1 | BLOCKER | #07's check counts Drift Patrol's own red runs, so its failures perpetuate themselves | **applied**. A Drift Patrol run that failed at the report step (`id: patrol`) counts as a report, not a failure; only setup or build failures count. Added a recovery unit test (3 runs red at `Run drift patrol` → no FAIL) and a setup-failure test (→ FAIL). | 07 |
| 2 | MAJOR | #07 has no token wiring for the scheduled run | **applied**. Confirmed that the patrol job has only `contents: read` and no GH_TOKEN/GITHUB_TOKEN env. Now requires `actions: read` plus `GH_TOKEN: ${{ github.token }}`, and FAIL (not SKIP) on missing auth when `GITHUB_ACTIONS=true`, with a unit test. Close-time proof is a scheduled or dispatched run summary with non-SKIP workflow-health rows. | 07 |
| 3 | MAJOR | #01 ignores Unix-only code; Z3 alone cannot make Windows build | **applied**. Confirmed that `embedded_go_frontend.rs:2` has an unconditional `std::os::unix::fs::PermissionsExt` and that `main.rs:13` has an unconditional `mod`. Also found that build.rs shells out to `sha256sum` and names the Go binary without `.exe`. Added source-portability criteria, a `cargo check --target x86_64-pc-windows-msvc` step, and a Windows runtime smoke (a Go explore that exercises the embedded frontend). | 01 |
| 4 | MAJOR | #02/#03 could pass an arm64 release whose embedded Go frontend is x86_64 | **applied**. Confirmed that `build.rs:196-203` runs `go build` with no GOOS/GOARCH, and that release.yml sets them only for the staged binary (`:168-175`). #02 now requires a CARGO_CFG_TARGET_* → GOOS/GOARCH mapping with a unit test, plus an arm64 runtime smoke on `ubuntu-24.04-arm` (`file` shows aarch64 for the extracted frontend, and a Go explore). #03's smoke matrix includes arm64. | 02, 03 |
| 5 | MAJOR | Branch dispatch can publish; `gh release create` has no `--target` | **applied as a new issue**. Confirmed that the release job has no `if:` and that `:295` has no `--target`. New draft `release-publish-guard-and-target` (15): the publish job is guarded to push-on-main, `--target "$GITHUB_SHA"` is added, there is a structure test, and a branch-dispatch run with the release job `skipped` is the proof. It blocks 01, 02 and 03. Branch runs in 01 and 02 count as proof only after it lands. | 15 (new), 01, 02, 03, 04 |
| 6 | MAJOR | #03's `release: published` trigger won't fire under GITHUB_TOKEN; #07's discovery would miss other triggers | **applied**. Confirmed that the release is created with `GITHUB_TOKEN` (`:288-290`). #03 now requires the smoke to run inside release.yml with `needs: release` (the alternatives are removed), so #07 covers it through release.yml's push trigger. #07's discovery also includes `workflow_run`. | 03, 07 |
| 7 | MAJOR | #08/#09 misdiagnose Task preconditions: they fail loudly, they do not silently no-op | **applied**. #08's Problem now states that the precondition fails the task and that the real gap is that nothing invokes `go:lint` and CI does not install the tool. The `CI=1` criterion is removed and replaced by "no CI-conditional skip logic", a precondition message fix, and a pinned-install doc. #09's comment corrects the diagnosis on str-2tyfk. | 08, 09 |
| 8 | MAJOR | #14 misses `.config/nextest-standalone.toml`, and its CI-URL proof contradicts option B | **applied**. Both config files are now in scope (the standalone one is confirmed used by shatter-rust and shatter-rust-runtime `--config-file`, with the same fail-fast and unused `[profile.ci]`). The close-time proof is conditional on the option: (A) CI URL with nextest ci-profile summaries; (B) diff plus forced-gate output, no CI URL. | 14 |
| 9 | MAJOR | #07 and #13 ask implementers to file issues, which conflicts with D6 | **applied**. Interpretation: D6 governs this audit's filing, so the follow-ups are pre-drafted for the maintainer's filer. New drafts: `devcontainer-workflow-red` (16; root cause taken from the job log: post-create.sh `bd init --from-jsonl` refused because origin has Dolt history, aligned with D4), `docker-publish-workflow-red` (17; root cause taken from the job log: the Dockerfile never copies `shatter-llm`), and `ubuntu-26-runner-trial` (19; blocked by 13). #07 files nothing. #13 names 19. #20's "disable" path no longer requires filing (a temporary removal keeps 20 open). | 07, 13, 16 (new), 17 (new), 19 (new), 20 |
| 10 | MINOR | #03's `shatter analyze` smoke never touches a frontend or the solver | **applied**. Confirmed at `args.rs:1175-1180`. Replaced with `shatter explore ... examples/go/05-conditional-merge.go:Categorize`, which must exit 0 with at least one branch explored. | 03 (also used in 01, 02) |
| 11 | MINOR | #01 gets z3 feature linkage wrong | **applied**. Checked the z3-sys 0.10.7 source: `gh-release` links a downloaded libz3 statically, `bundled` builds it statically, and `static-link-z3` is a deprecated alias for `bundled`. Removed the `libz3.dll` staging fallback and the `static-link-z3` recommendation. `gh-release` is now tried first. | 01 |
| 12 | MINOR | #09 posts the same "false acceptance" comment on str-2tyfk, which was a scoped errcheck cleanup | **applied as a split**. `bd show str-2tyfk` confirms that its body allowed the unused/govet residuals to be "file[d] separately". 09 now targets only str-2tyfk (the residuals were never filed, and the diagnosis is corrected). New reopen-note `go-lint-qwua7-32-note` (18) targets str-qwua7.32, whose acceptance "task go:lint passes" was false. | 09, 18 (new), 08 |
| 13 | MINOR | #05's path test has no contract for globs, expressions or runtime paths | **applied**. Spelled out the contract: literal keys must exist; `hashFiles` args and glob values must match at least one file; `${{ }}` non-hashFiles values are skipped and listed; runtime-created paths go in an explicit allowlist with reasons. Unit cases for each. | 05 |
| 14 | MINOR | #12 and #14 each bundle independent deliverables | **applied**. Perf CI moved out of 12 into `perf-ci-stable-scenarios-red` (20; failure line re-verified first-hand from job 106403653337). The parity fallback moved out of 14 into `parity-governed-stale-fallback` (21). Slugs 12 and 14 are unchanged. | 12, 14, 20 (new), 21 (new) |

Disputed: none.

## Secondary (degraded same-runtime) review items

| Sev | Finding | Action | Files |
|---|---|---|---|
| MAJOR | Runtime libz3 dependency of the Linux and macOS artifacts is ignored | **applied**. #03 requires a libz3 decision (static or documented and checked) and asserts that libz3 is absent on the smoke runners before install. | 03 |
| MINOR | `static-link-z3` is deprecated | **applied** (same as Codex 11) | 01 |
| MINOR | The x86_64-linux green leg is a warm-cache result | **applied**. #03 requires one cold-cache Linux leg in the proving run. | 03 |
| MINOR | go:lint "silently no-ops" is false | **applied** (same as Codex 7) | 08, 09 |
| MINOR | CHECKS line reference drift | **applied**. Now `757-767`, with the registration line `:765`. | 05, 07 |
| MINOR | rustfmt hidden-context citations | **applied**. Marked as provenance only; nothing in the work depends on them. | 10 |
| MINOR | Perf-ci failure reason is second-hand | **applied**. Re-verified from the job 106403653337 log. | 20 |

## Splits, conversions and new drafts

- **New:** 15 `release-publish-guard-and-target` (Codex 5), 16 `devcontainer-workflow-red` and 17 `docker-publish-workflow-red` (Codex 9; were implementer-filed follow-ups of 07), 18 `go-lint-qwua7-32-note` (Codex 12; split from 09), 19 `ubuntu-26-runner-trial` (Codex 9; was an implementer-filed follow-up of 13), 20 `perf-ci-stable-scenarios-red` (Codex 14; split from 12), 21 `parity-governed-stale-fallback` (Codex 14; split from 14).
- **No slugs removed or converted.** Slug 14 is kept although its title no longer mentions the parity fallback.
- **blocked_by changes:** 01 and 02 are now blocked by 15. 03 is also blocked by 15. 19 is blocked by 13.
- **Cross-bucket notes:**
  - bento-landing `04-land-work-post-push-workflow-health.md` says automatic filing for red workflows "belongs to the shatter-side workflow-health-patrol". workflow-health-patrol is read-only and files nothing, so that sentence is inaccurate.
  - shatter-gates-integrity `02-ci-executed-leaf-guard` still correctly points at `nextest-ci-profile-and-stale-parity-fallback` for nextest.
  - `devcontainer-workflow-red` is one more JSONL consumer for the tracker bucket's `beads-jsonl-consumers-drop-bd-sync` / `beads-retire-jsonl-import-dolt-remote` (D4).
  - shatter-agents drafts citing `release-publish-and-install-smoke` are unaffected.
