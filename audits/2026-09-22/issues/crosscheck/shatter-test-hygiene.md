# Cross-check review

- **Reviewer:** claude (DEGRADED same-runtime fallback)
- **Artifact type:** issue
- **Mode:** degraded

> **DEGRADED REVIEW.** The counterpart runtime was unavailable, so this review came from an independent agent of the *same* runtime. It shares the original author's model and blind spots; weight it accordingly.

## Findings

# Cross-check review (DEGRADED: same-runtime fallback): shatter-test-hygiene bundle

Codex run failed identity validation (exit 4). This review was done by an independent Claude reviewer, read-only, against origin/main 70465921 and the audit worktree (HEAD 56c86168).

## Claims verified

- 01: four `fn assert_snapshot` helpers with a self-create-on-missing branch (outcome_md:45-46, run_markdown_ordering:38-39, source_set_summary:32-33, html:63-64); `normalize_ws` used at html:72-73 and outcome_md:54-55; 8 snapshot files as listed; no `insta` in any Cargo.toml; shatter-cli/tests has no snapshots. `standalone/{go,ts}/01-arithmetic.*` exist in the examples repo.
- 02: `examples_checkout.py` constants (:19-21, :48) and the fetch/checkout/reset/clean sequence (`_refresh_checkout_locked`, around :131-138) match. There is no lock file.
- 03: executor.rs fallbacks at :856 (bin-only), :1101, :1119, :3247 (crate-bridge), and `harness_cache_root` at :1067. `harness_scratch_root`/`SHATTER_HARNESS_SCRATCH` exist. scan_orchestrator.rs flag files are at :9670/:9803/:9915 on main.
- 04: `testTimeout: 30000` at jest.config.js:5; check-unit.log:82 `FAIL src/handlers.test.ts (679.399 s)`; 18 `Exceeded timeout` lines.
- 05: all listed entry points and line numbers match the origin/main Taskfile. `pre-completion-e2e` = `check` + `e2e`. `core:test-ignored` runs `--run-ignored all -E 'not binary(bench_frontier_ranking)'`, so it still includes `e2e_concolic*`. The keep-in-sync comments are present.
- 06: the only tracked `.fail` is the instrument/ March file; shatter-go/.gitignore has only the planner rule; the test is at property_test.go:86.
- 08: both Task entries, their sources/cmds and last-touch commits (630e8ccb 2026-05-13, a14370ef 2026-05-02) match. No workflow invokes either gate (the drift-patrol.yml grep hit is only the English word "broad").
- 09: SKILL.md lines 14/34-38/63 are quoted correctly; the fuzz_deserialization.rs header matches; there are 8 + 14 Go Fuzz targets, and no `-fuzz=`/cargo-fuzz invocation exists anywhere.

## Findings

- **MAJOR** [tests-leak-tmp-dirs] **Scope is undercounted: executor.rs has ~30 fixed `temp_dir()` test paths, not 4.** On main, `shatter-test-*` fixed dirs also appear at :10809, :11171, :11248, :11288, :11306, :11337, :11580, :11702, :11772, :11811 and :12140-12486, and the generated harness writes `__capture_dir = std::env::temp_dir()` (:2525, :2704). The AC "No test writes to a shared temp_dir path" therefore covers far more than the evidence suggests. Size M is likely L, and a fresh agent working from the listed lines would under-deliver. List them all, or state "all `temp_dir()` uses in `#[cfg(test)]` modules (~30)" and address the generated-harness capture dir explicitly.
- **MAJOR** [collapse-test-tiers] **The cheap, unblocked E2E-dedupe fix is held behind two unrelated blockers and a maintainer tier decision.** The draft calls the E2E duplication "a one-line change [that] does not need the tier decision", but the issue as a whole is blocked by task-list-json-poisons-checksums (str-qwua7.3) and task-sources-cover-real-inputs. Split the E2E-once item into its own unblocked issue, or drop `blocked_by` for it. As written the issue also bundles three tasks (tier redesign, cache identity, E2E dedupe).
- MINOR [bundle header] **The claim that audit HEAD differs from main only in two files is inaccurate.** `git diff --stat origin/main HEAD` also shows `shatter-core/src/cache.rs` (422 lines, str-8q1b4). It does not affect these drafts but should be corrected.
- MINOR [snapshot-test-helpers] **Bundles three separable changes.** It combines never-self-create/shared helper, byte-exact markdown, and brand-new CLI output snapshots. The CLI snapshot part is new coverage built on the unpinned examples repo, so it risks flakiness until pin-examples-repo lands. Consider splitting it or making it depend on pin-examples-repo.
- MINOR [pin-examples-repo] **The consumer list misses a direct fallback, and the proof command is imprecise.** `shatter-rust/src/executor.rs:11444` falls back to `temp_dir().join("shatter-examples-main")` without going through the script, and should be covered by the pin. `test-standard` has no `sources:` of its own (the cached unit is internal `workspace-test`), so `task --status workspace-test` (or `--dry` on the concrete task) is the more reliable proof.
- MINOR [collapse-test-tiers] **The double-run only happens when caches are cold.** The e2e-* subtasks are checksum-cached (`sources:`), so E2E runs twice only when sources changed since the last run. That is the common case after edits, but the problem statement should say so.
- MINOR [ts-handlers-test-timeouts] **The load ~100 acceptance target is hard to reproduce on demand.** Specify the induced-load recipe (e.g. `stress-ng --cpu 96`), so that "no timeouts at load ~100" is checkable.
- MINOR [fuzz-policy-vs-reality] **The "Either option" drift check adds a third deliverable.** A new drift-patrol check is scope beyond the policy/reality fix. Acceptable, but consider making it optional.
- MINOR [rapid-failfile-purge / reopen-note] No issues found. The `<rapid-failfile-purge id>` placeholder must be substituted by the filer, as the note says.

## Verdict

No BLOCKERs. Drafts 02, 04, 06, 07, 08 and 09 are ready to file as-is. Top fixes:
1. tests-leak-tmp-dirs: enumerate all ~30 fixed test temp paths plus the generated-harness capture dir, and resize.
2. collapse-test-tiers: split the E2E-once fix into an unblocked issue.
3. snapshot-test-helpers: split out the CLI-snapshot addition, or link it to pin-examples-repo.
