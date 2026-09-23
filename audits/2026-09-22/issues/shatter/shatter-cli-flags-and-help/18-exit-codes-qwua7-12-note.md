---
slug: exit-codes-qwua7-12-note
kind: note-to-existing
title: "Note on str-qwua7.12: most single-target exit-2 classes now hold, but multi-target partial failure exits 0 (contrary to its AC) and print_stdout exits 1 on I/O errors"
priority: P1
type: note
labels: [cli, exit-codes, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.12
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.12 (open, P1; do not close)

Target: `str-qwua7.12` ("Implement SPEC §2.11 exit codes: 2 for tool/usage errors, 1 only for fired gates"), verified open at P1 with `bd show` on 2026-09-23. Action: add a comment only. Do not close it: its acceptance checks are not met.

Earlier drafts proposed closing str-qwua7.12 because the single-target cases now exit 2. The live issue also requires "exit 1 if at least one target succeeded and at least one failed (partial failure is a fired-gate class)", and current code explicitly enforces exit 0 for that case, so closure is not supported.

## Comment text

> Audit 2026-09-22 status (findings cli-ux-19, areas/cli-ux.md section 0, prior-audit-regress.md), checked against source at `56c86168`:
>
> - **Now exit 2 (single target):** missing file, `.py` target, unknown function, bad `--set` value, function glob, spec-diff bad JSON, missing spec, host-write refusal. `shatter-cli/src/main.rs` `error_exit_code` (`:1475`) maps any non-`GateFailure` error to 2. These came from audit transcripts, not a fresh run; re-run them before relying on them.
> - **Not met — multi-target partial failure:** this issue's acceptance checks require exit 1 when at least one target succeeded and at least one failed. `shatter-cli/src/commands/explore.rs` `decide_explore_exit_status` (`explore.rs:744`) returns `Ok` for that case, and the test `decide_exit_status_ok_partial_success_with_some_failed_targets` (`explore.rs:7075`) pins exit 0 ("Partial-success policy"). Either the acceptance check or that policy must change; the two currently contradict. scan and run need the same check. The maintainer must choose one policy: either change the code and that test (with a red-then-green CLI test for `explore`, `scan` and `run`), or amend this acceptance check and SPEC §2.11 to say partial failure exits 0.
> - **Not met — stdout I/O error:** `shatter-cli/src/helpers.rs:337-347` `print_stdout` calls `std::process::exit(1)` on a non-EPIPE write error. Under SPEC §2.11 that is a tool error (2). Add a test (for example writing to a closed or full fd, `/dev/full` on Linux) asserting exit 2.
> - **Not met — integration tests:** one CLI integration test per error class plus the two multi-target cases in `shatter-cli/tests/`, each asserting the exit code. Check which already exist before writing new ones and list them in the close reason. (`categorize_error` at `main.rs:1521` still matches substrings, but it only labels telemetry; the exit class itself is typed.)
> - **Not met — SPEC wording:** `SPEC.md:634` still cites the nonexistent `--failure-threshold` (the flag is `--fail-on-failures=PERCENT`). Fix it together with the multi-target wording.
> - The SPEC §2.11 paragraph that cites this issue ("Do not wait for that issue to land ...", `SPEC.md:645-647`) is being removed by **help-tracker-ids-lint** (<id of help-tracker-ids-lint>).
> - Closing evidence should be a table of every class in the acceptance checks with the command run on a fresh build, its observed exit code, and the test that pins it, including both multi-target cases.

The filer substitutes each `<id of slug>` placeholder with the id assigned to help-tracker-ids-lint.

## Source

Audit 2026-09-22 findings cli-ux-19 and the str-qwua7.12 status in areas/cli-ux.md section 0. Split from cli-minor-output-and-help-polish items 10 and 11. During cross-bucket assembly (2026-09-23) this note absorbed the duplicate `qwua7-12-rescope-note` (bucket shatter-tracker-and-beads, finding prior-09): its integration-test, SPEC.md:634 and policy-choice items are merged above, and that draft was removed so only one comment is posted on str-qwua7.12.
