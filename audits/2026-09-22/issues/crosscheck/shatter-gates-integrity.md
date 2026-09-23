# Cross-check review

- **Reviewer:** claude (DEGRADED same-runtime fallback)
- **Artifact type:** issue
- **Mode:** degraded

> **DEGRADED REVIEW.** The counterpart runtime was unavailable, so this review came from an independent agent of the *same* runtime. It shares the original author's model and blind spots; weight it accordingly.

## Findings

DEGRADED: same-runtime (Claude) fallback review. Codex review failed identity validation (exit 4). Read-only; claims checked against /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (56c86168), main (70465921), and bd in /home/ketan/project/shatter.

## Verified claims (spot checks)

- 01: `scripts/test_affected_gates.py:203-212` runs `task --list-all --json` with cwd=ROOT. `Taskfile.yml:459` runs it in `meta`, and `meta` is a dep of `check-static` (:535). I reproduced it with go-task 3.50.0 in a scratch Taskfile: `--list-all` wrote no checksum, `--list-all --json` wrote `.task/checksum/a`, and the next `task a` printed `is up to date`. Both `--dry` and `TASK_TEMP_DIR=$(mktemp -d)` prevented the write to the repo's `.task`. The root cause holds.
- 03: The `sources:` gaps are real: cli:test/test-fast, core:test-ignored, parity (:252-261), rust-fe:test (no `tests/`) and ts:test (no `jest.config.js`). build.rs embeds ts (:90) and go (:163). render.rs uses the templates at :17 and :40.
- 04: I called `select_gates` myself and got every row in the table. `docs-smoke` is not in `GATE_ORDER`. `.md` is classified before the crate rules (:126-127).
- 05: `precommit-rust.sh:33-37` runs `cargo test`/clippy. No script under `scripts/` references `run-heavy`.
- 06: My own sweep found the same 8 unwired modules with the same test counts (57/54/30/25/24/11/6/2). The note at `GOVERNANCE.md:120-121` is correct. `test_ci_workflow_structure.py` runs only at `ci.yml:110`.
- 07/08: The regexes at `gauntlet_check_output.py:30-35` are dead. `report.rs` emits PASS/WARN/LOW only, with a test at :4886. The repro exits 0 with no output on `scan-mix.stdout` (4 failed, 7 interrupted). The allowlist still names 11-/12- fixtures. CLAUDE.md:41 is stale.
- 09: `land_work_verifier.sh` matches the draft: `>/dev/null 2>&1`, the trio of checks, and `{name,status}` only. `verifier.json` has no timeout.
- 10: gate-times.csv failure counts match (44/156, 36/151, 26/143). `/usr/bin/sccache` is installed and `RUSTC_WRAPPER` is unset.

## Findings

1. **MAJOR — verifier-per-language-evidence (09) largely duplicates open issues.** str-qwua7.55's acceptance already requires that "`land_work_verifier.sh`'s claim matches what it runs; `verifier.json` has a timeout". str-qwua7.2's acceptance already requires the verifier to "print a summary `executed: [..] cached: [..]` and exit non-zero when the executed list is empty". str-35vtk.24 (open, P1) rewrites the verifier to run one exact `task check`. 09's option to "run `task affected` for the landing diff" conflicts with that. The unique parts are the tee/log output, `wall_seconds` and the three /pre-completion rows. Narrow 09 to those, and drop the timeout, header and executed-flag criteria or turn them into notes on .55/.2. Otherwise two issues will edit the same `run_check` in conflicting directions.

2. **MAJOR — ci-executed-leaf-guard (02) overlaps str-qwua7.2 without settling who owns it.** str-qwua7.2's acceptance already switches `ci.yml:89` to `task check-fresh` and requires "a live run asserts no `Task "<leaf>" is up to date` line for the selected stages". That is the same CI guarantee 02 adds with a grep guard. 02 says it covers "only the simple CI guard", but it gives no reason why qwua7.2's CI item should not own this. Either make 02 the triage issue plus an explicit amendment of qwua7.2 (CI item moves to 02, or 02 becomes a stepping stone qwua7.2 later replaces), or post the guard as a note on qwua7.2. Also note that qwua7.2's `check-fresh` design (delete checksums, then run) was itself defeated by this root cause, since meta re-poisons after deletion. 01 or 02 should state this so qwua7.2 is re-scoped rather than implemented as written.

3. **MINOR — the bundle status line is false: "Main differs from it only in `shatter-core/Taskfile.yml`".** `git diff $(merge-base) main` also shows `shatter-core/src/scan_orchestrator.rs` (+791/-83) and `shatter-core/src/cache.rs` (+422) from str-8q1b4 (52d9c965). As a result, the cite `scan_orchestrator.rs:6120` in 07 and 08 is line 6301 on main. The format string is unchanged, so the substance holds. Several drafts say "unchanged on main", and that is wrong for this file. Fix the header and the line cite, or cite by symbol or string.

4. **MINOR — the E2E claims in 03 and 05 contradict the code.** All tests in `e2e_concolic.rs` (26), `e2e_concolic_go.rs` (23) and `e2e_concolic_rust.rs` (13) are `#[ignore]`d. `workspace-test` runs plain `cargo test --workspace` (Taskfile.yml:134) and `precommit-rust.sh:33` runs plain `cargo test -p ...`, so neither runs the E2E suites. 05's title "(incl. E2E)" and body "integration and E2E suites" are wrong. 03's "workspace-test ... runs the Go and Rust E2E suites" repeats a stale Taskfile comment. The frontend-glob requirement still stands because build.rs embeds the frontends, but restate the reason.

5. **MINOR — affected-gates-routing (04) uses paths that do not exist as table rows.** `shatter-core/src/report/html.rs` and `shatter-ts/src/index.ts` do not exist. The classifier is prefix-based, so the outputs hold, but the acceptance criterion says to add table cases for every row. Use real files, such as `shatter-core/src/report.rs` and an existing `shatter-ts/src/*.ts`.

6. **MINOR — fast-hermetic-precommit (05) says to coordinate with str-jttrf, but str-jttrf is closed.** Reword it as a historical reference, or point to any open GIT_DIR follow-up.

7. **MINOR — gauntlet-scan-checker-consumes-json (07) misses a cheaper mechanism that already exists.** `shatter scan --fail-on-failures[=PERCENT]` (args.rs:1150-1162, from str-izhn) exits non-zero on failed attempts. The default is exit 0 for backwards compatibility. The gauntlet's process-level exit check would then catch failures. Mention it as an alternative or complement, or say why JSON diffing against a per-function allowlist is needed. It likely is needed for allowlisting.

8. **MINOR — task-list-json-poisons-checksums (01) replaces str-qwua7.3's acceptance but drops the "adjust check-fresh (p1-02)" branch without comment.** The root cause is Task-runner behaviour, which is exactly the branch qwua7.3 said should adjust qwua7.2's `check-fresh`. Add a line saying `check-fresh` no longer needs that adjustment once the meta side effect is gone, or add it to "Out of scope" with a pointer.

9. **MINOR — gate-telemetry-executed-vs-cached (10) and qwua7.2 both parse `is up to date` lines.** Each builds its own parser. Suggest one shared helper, for example `scripts/task_executed.py`, used by gate-wrapper, the verifier (09) and the CI guard (02). That avoids three divergent regexes, which is the same failure class 07 fixes.

## Verdict

The bundle is mostly ready. The evidence is unusually concrete, and the root-cause claim in 01 reproduces. Drafts 01, 03, 04, 05, 06, 07, 08 and 10 can be filed after the MINOR text fixes. Drafts 09 and 02 need rescoping against the open issues str-qwua7.55, str-qwua7.2 and str-35vtk.24 before filing, or they will duplicate or conflict with them.

Top fixes:
1. Narrow 09 to what the open issues do not cover (the output tee, `wall_seconds`, the /pre-completion rows). Remove its timeout, header and executed criteria, and its alternative of running `task affected`.
2. Settle who owns the CI "no up-to-date leaf" guarantee between 02 and qwua7.2. Record that qwua7.2's `check-fresh` premise changes under 01's root cause.
3. Correct the "main differs only in Taskfile" header and the `scan_orchestrator.rs` line cite. Also correct the E2E claims in 03 and 05.
