# Revision: shatter-cli-flags-and-help (after Codex cross-check, 2026-09-23)

Primary review: `issues/crosscheck/shatter-cli-flags-and-help.codex.md`. Secondary (same-runtime, degraded): `issues/crosscheck/shatter-cli-flags-and-help.md`. Evidence re-checked against the `audit-2026-09-22` worktree (source at `56c86168`). Tracker state checked with `bd show` for str-qwua7.12, str-qwua7.33, str-v1tzz, str-9ee5 and str-qwua7 (epic description).

## Codex findings

| # | Sev | Finding (one line) | Action | Files |
|---|---|---|---|---|
| C1 | MAJOR | #02's "`**Summary:**` appears once" assertion already passes on broken output | Applied. The exactly-once predicate is now per function heading and per table row, with no replay blocks. The AC requires showing it fails on HEAD (`## \`classifyNumber\`` twice). Evidence notes that Summary appears once in `explore-o2.out`. | 02 |
| C2 | MAJOR | #02 misidentifies the two replay paths (`:4028` is `finalize_explore` via `--from-artifacts`, `:6687` is the live path) | Applied. The Problem section names both paths correctly. Replaced the sequential/parallel AC with an explicit `--from-artifacts` test AC. | 02 |
| C3 | MAJOR | #03 drops `--timing*` from non-executing commands although timing is persisted for every command | Applied. `--timing*` stays global. The AC adds a test that `list-targets --timing ... --timing-output` still writes the file, and fixes the wrong `args.rs:285` comment and hide list. `--set` is scoped to its only consumer (explore, `main.rs:437`). The behavior change (rejecting now-ignored flags) must be recorded in SPEC §8 and scripts swept. | 03, 04 |
| C4 | MAJOR | #03's `mut_subcommand` fallback cannot meet its own ACs | Applied. The fallback is removed, and the AC states that hide-only is not an acceptable resolution. | 03 |
| C5 | MAJOR | #05 can close with seed-sensitive resume still broken | Applied. The draft now has `blocked_by: [explore-resume-options-key]` (cross-bucket). The AC requires a `--seed 1` then `--seed 2` resume test against the same artifact dir. | 05 |
| C6 | MAJOR | #08 bundles code cleanup with an unbounded tracker reconciliation; item 20 undescribed; D6 conflict | Applied (split). #08 is code/docs lint only. New draft 19 `unfiled-0904-ui-items-reconcile` is bounded to items 12, 16, 17 and 20, with descriptions from cli-ux.md F17. It records that the 09-04 report is in no ref (branch `audit-2026-09-04` exists neither locally nor on origin), prescribes a source search, and allows "unrecoverable" for item 20. Its output is drafts handed to the maintainer, not filed issues (D6). | 08, 19 |
| C7 | MAJOR | #09 misstates the telemetry loss; `command_run.subcommand` is populated independently | Applied. The Problem now limits the defect to `sanitized_args` (in `command_run` and `bad_cli_args`) and cites `main.rs:1543-1555` and `:96-110`. The AC tests `sanitize_args` directly. The title is changed; the slug is unchanged. | 09 |
| C8 | MAJOR | #10 is several independent issues disguised as polish | Applied (split). #10 keeps the slug and now covers help-string fixes only (items 4-5). New drafts: 11 analyze-only-sandbox-refusal (item 1), 12 analyze-only-output-detail (2), 13 explore-function-not-found-diagnostics (3), 14 spec-flag-dropped-with-spec-out (6), 15 html-source-non-executable-lines (7), 16 failure-table-language-any (8), 17 demo-complete-with-errors-green (9), and 18 exit-codes-qwua7-12-note (10, 11). | 10-18 |
| C9 | MAJOR | #10's str-qwua7.12 closure is unsupported: the live AC wants partial failure = exit 1, but code pins exit 0 | Applied. Verified with `bd show str-qwua7.12` (open, P1; the AC says exit 1 for mixed success/failure). `decide_exit_status_ok_partial_success_with_some_failed_targets` (`explore.rs:7075`) pins 0. The closure requirement is removed and replaced by note-to-existing 18, which records what holds, what does not (partial failure, `print_stdout` exit 1) and the required close evidence. | 18 (10 no longer mentions closure) |
| C10 | MAJOR | #10's HTML item hides a coverage-data dependency (only a span and a covered set reach the renderer) | Applied. Draft 15 states that `render_source_block` gets only a span and a covered set, and that `total_lines` is a count, not a set. Diagnosing where the executable-line set lives comes first, with a GOVERNANCE path if the protocol changes. Text heuristics are forbidden. The test fails on HEAD. | 15 |
| C11 | MAJOR | #01's `--render` retirement reaches beyond explore (global flag, also passed to scan) | Applied. #01 no longer retires `--render`. It sets explore precedence (`--format` wins, deprecation warning on explicit `--render`), and an AC requires scan output to stay unchanged. Global retirement and scan migration go to str-9ee5 (verified open). | 01 |
| C12 | MINOR | #05's reproducibility test leaves caches, seed pool, scheduling and time budget uncontrolled | Applied. The AC mirrors `scan_seed_reproducibility.rs` controls (`--no-cache`, `--no-seeds`, fixed parallelism, bounded budget; lines cited). A **run** reproducibility test is added too (secondary review MINOR). | 05 |

## Secondary (same-runtime) review findings also applied

| Sev | Finding | Action | Files |
|---|---|---|---|
| MAJOR | #07 miscites the wiring (helpers.rs:1574 is the LLM resolver) and misses that `--set` is ignored by scan/run | Applied. Both load sites are cited (`config.rs:1597` via `explore.rs:4832-4838`, and `helpers.rs:1571` via `explore.rs:4314`). The AC requires a warning on every load site and makes `scan --set` non-silent. | 07 |
| MINOR | #01 evidence: `format-text.out` shows `**0 path(s)**`, and text `-o` files are also corrupted | Applied | 01 |

## Other corrections made during re-verification

- Item 8 (now 16): the `any` language row is a deliberate cross-language rollup (`explore.rs:3244-3277`), not a mislabel. The draft now asks for a real-language row and an unambiguous rollup label.
- #02: `examples/ts/01-arithmetic.ts` is not in the tree, and `demo/fixtures/arithmetic-v1.ts` has one function. The AC asks for a two-function fixture.
- #03: `persist_timing_run` is defined at `main.rs:1483` and called at `:1456`. The repo has no CHANGELOG, so the SPEC §8 changelog (`SPEC.md:1173`) is used.

## Disputed

None.

## Splits, conversions, slugs

- Slugs kept: all of 01-10. Titles changed for 03, 08, 09 and 10.
- New slugs: analyze-only-sandbox-refusal (11), analyze-only-output-detail (12), explore-function-not-found-diagnostics (13), spec-flag-dropped-with-spec-out (14), html-source-non-executable-lines (15), failure-table-language-any (16), demo-complete-with-errors-green (17), exit-codes-qwua7-12-note (18, note-to-existing on str-qwua7.12), unfiled-0904-ui-items-reconcile (19).
- Removed or converted slugs: none.
- New cross-bucket edge: seed-for-explore-and-run is blocked by explore-resume-options-key (shatter-artifacts-correctness/01).
- BUNDLE.md was regenerated from the revised drafts.
