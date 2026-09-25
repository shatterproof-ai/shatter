# Revision: shatter-gates-integrity (Codex cross-check, 2026-09-23)

Inputs: `issues/crosscheck/shatter-gates-integrity.codex.md` (primary) and `issues/crosscheck/shatter-gates-integrity.md` (degraded same-runtime review, secondary). Tracker state was checked with `bd show` for str-qwua7.2, str-qwua7.3, str-qwua7.55, str-35vtk.19, str-35vtk.24, str-35vtk.25, str-35vtk.26, str-35vtk.35 and str-jttrf. Code claims were re-checked against main `70465921` and the audit snapshot `56c86168`. The installed bento land-work scripts (2.3.88) were checked for the timeout claim.

## Codex findings

| # | Sev | Finding (one line) | Action | Files |
|---|---|---|---|---|
| 1 | MAJOR | #02 guard rejects `is up to date` only; passes when a leaf disappears; adds a second `task check` | applied: per-leaf positive evidence (`executed`/`cached`/`missing`) against an explicit expected-leaf file, a fixture matrix (cached, absent, mixed, failed), a meta test that the expected leaves exist and are reachable from `check`, and replacement of the existing CI step with one teed run | 02 |
| 2 | MAJOR | #03 `touch` proof cannot show checksum invalidation | applied: prime the cache, change file **contents**, re-run without `--force`, once per new glob class | 03 |
| 3 | MAJOR | #06 guard in `meta` can be cached away (sources enumerate files) | applied: glob `sources:` required and asserted; regression that adds an unwired module after priming the cache | 06 |
| 4 | MAJOR | #06 treats staged gate-pressure/event-log infra as disposable | applied: tests wired only; the scripts are not invoked, integrated or deleted (str-35vtk.19/.25 own them) | 06 |
| 5 | MAJOR | #06 and #07 allow incompatible destinations for the gauntlet checker test | applied: both now say `meta` (reachable from `check`); #06 allows a temporary allowlist entry that #07 must remove | 06, 07 |
| 6 | MAJOR | #07 misses interruptions (`skipped_functions[]`, `category == "interrupted"`) | applied: checker flags failed and interrupted entries; interruption-only CLI fixture; explicit handling of the `--timeout-total` and `--dry-run` steps. Verified in the scan-mix sample: 4 `failed[]` plus 7 interrupted `skipped_functions[]` | 07, 08 |
| 7 | MAJOR | #09 verifier.json timeout is not read by bento's runner | applied: confirmed that `land-work-run-verifier.py` (`:208`, `:344-383`) and `land.py` (`:77`, `:303-304`) use only `--timeout`. The criterion moved to a note on str-qwua7.55 with supported wiring (the verifier bounds its own run via a command argument, or the landing instructions pass `--timeout`) and a runtime timeout test | 11 (new), 09 |
| 8 | MAJOR | #09 duplicates str-qwua7.55 / qwua7.2 / str-35vtk.24 and its `task affected` option conflicts with .24 | applied: #09 is narrowed to the `/pre-completion` rows only. The verifier items are now notes on str-qwua7.55 (#11) and str-qwua7.2 (#12). The `task affected` option is removed and #11 states it conflicts with .24 | 09, 11, 12 |
| 9 | MAJOR | #09 execution flag undefined for mixed results | applied: per-required-leaf definition plus a test matrix (all executed, mixed, one cached, one absent, all cached, failed), in the str-qwua7.2 note and the shared parser in #02 | 12, 02, 09 |
| 10 | MAJOR | #10 assumes per-leaf telemetry granularity that does not exist | applied: new event schema (`invocation_id`, per-leaf CSV), a mechanism for nested leaves under `SHATTER_GATE_LOCK_HELD`, an aggregation script, and budgets only from new post-fix measurements (≥10 executed runs) with historical rows excluded | 10 |
| 11 | MAJOR | #05 `cargo check`/clippy still triggers build.rs frontend builds | applied: defined "warm" and "cold". Cold may run build.rs but must not depend on prebuilt artifacts, ambient config or an examples checkout (proved with empty HOME/TMPDIR). The ≤30 s budget is warm-only, with reproducible prime/measure commands | 05 |
| 12 | MAJOR | #05 misstates governance (pre-push is already wrapped) and receipt reuse (.25 is shadow only; .26 threshold) | applied: governance is scoped to pre-commit only. Receipt reuse is moved out of scope to str-35vtk.25/.26/.9. Added an assertion that pre-push still covers the tests removed from pre-commit | 05 |
| 13 | MAJOR | #01 closure proof depends on the blocked follow-up (staged `check` stops at first failure) | applied: per-leaf execution proof (`task meta` then each leaf individually; failures still count as executed). Triage split into the new #13 | 01, 13 (new) |
| 14 | MAJOR | "Main differs only in Taskfile" is false; evidence needs retrieval instructions | applied: the header and each draft say which files were re-verified where. Scan-orchestrator is cited by format string, with lines for both commits. Retrieval via `git show 56c86168:<path>`; `check.log` is marked gitignored and local-only | BUNDLE, 01, 02, 03, 04, 05, 06, 07, 08 |

## Degraded same-runtime review (secondary)

| # | Sev | Finding | Action | Files |
|---|---|---|---|---|
| 1 | MAJOR | #09 duplicates qwua7.55/.2/35vtk.24 | applied (same as Codex 8) | 09, 11, 12 |
| 2 | MAJOR | #02 vs str-qwua7.2 CI ownership unsettled; check-fresh defeated by the root cause | applied: #02 takes over the CI item; #12 records the transfer and the check-fresh note; #01 mentions it | 02, 12, 01 |
| 3 | MINOR | Bundle header / `scan_orchestrator.rs:6120` cite wrong on main | applied (Codex 14) | BUNDLE, 07, 08 |
| 4 | MINOR | E2E claims in #03/#05 wrong (all E2E tests `#[ignore]`d: 26/23/13) | applied: re-verified the counts and corrected the claims; the stale Taskfile comment at `Taskfile.yml:105-108` is named | 03, 05 |
| 5 | MINOR | #04 table uses nonexistent paths | applied: `report.rs` and `analyzer.ts` (outputs re-run); plus a test that case paths exist | 04 |
| 6 | MINOR | #05 coordinates with closed str-jttrf | applied: now a historical reference | 05 |
| 7 | MINOR | #07 ignores `--fail-on-failures` | applied: mentioned as a complement; it cannot allowlist and misses interruptions | 07 |
| 8 | MINOR | #01 drops qwua7.3's "adjust check-fresh" branch | applied: an "Effect on str-qwua7.2" paragraph | 01 |
| 9 | MINOR | three divergent `is up to date` parsers | applied: one shared parser defined in #02 and reused by #09, #10 and #12 | 02, 09, 10, 12 |

No findings are disputed.

## Splits and conversions

- **02 `ci-executed-leaf-guard`** was split. The guard stays in 02. Triage of the failures that surface on the first real CI run moves to the new **13 `ci-first-real-run-triage`** (blocked by 01 and 02). The CLAUDE.md CI-claim correction is dropped from this bucket, because it is owned by `test-tier-docs-overstate-coverage` (shatter-docs).
- **09 `verifier-per-language-evidence`** keeps its slug but is rescoped to `/pre-completion` evidence rows only (blocked by 02 for the shared parser). The verifier parts become two note-to-existing drafts: **11 `qwua7-55-verifier-timeout-note`** (timeout wiring, one `task check` per .24, output sentinels) and **12 `qwua7-2-scope-note`** (the check-fresh premise, the CI item moving to 02, and the per-leaf evidence definition with its test matrix).
- **10 `gate-telemetry-executed-vs-cached`** was split. Telemetry stays in 10 (now also blocked by 02). The sccache measurement moves to the new **14 `sccache-for-gate-runs`** (P3). The cross-project slot criterion is dropped to out of scope (bento-dyp7).
- No slugs were removed.
