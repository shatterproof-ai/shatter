---
slug: land-py-verifier-log-kept
kind: new
title: "land.py reports verifier.log as output_path after deleting it along with the preview worktree"
priority: P1
type: bug
labels: [audit, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land.py reports verifier.log as output_path after deleting it along with the preview worktree

## Problem

When the verify step fails under `land.py`, the final JSON's `output_path` points to `<preview>/.land-work/verifier.log`. By the time that JSON is printed, land.py has already removed the preview worktree, so the file does not exist. The agent cannot see why the verifier failed. In shatter the agent had to rebuild a preview by hand and rerun the gates, which took several more minutes each time.

bento-rdtn.4 (closed) was meant to persist raw verifier output. bento-rdtn.14 (closed) added land.py, whose failure path cleans up the preview. Each was tested on its own, and no test covers the two together. On the land.py path, rdtn.4's goal is not met.

## Evidence

Re-verified 2026-09-23 at bento origin/main b1bb787. The land-work scripts are unchanged since the audit's 1c0c1e6. Paths are relative to `catalog/skills/land-work/scripts/`.

- `land-work-run-verifier.py:351`: `log_path = Path(args.log).resolve() if args.log else candidate / ".land-work" / "verifier.log"`. The default path is inside the candidate, which is the preview.
- `land-work-run-verifier.py:138`: already computes `diagnostics["verifier_log_tail"]` (last 20 lines).
- `land.py:296-305`: builds `verifier_args` without `--log`.
- `land.py:126`: `raise StepFailure(step, message, output_path=payload.get("verifier_log"))`. `verifier_log_tail` is dropped.
- `land.py:365-375`: the `except StepFailure` handler calls `driver.cleanup_preview()` (`git worktree remove --force` on the preview) and then emits `"output_path": exc.output_path`.
- Shatter session 9f13ca23 had three verify failures: 2026-09-19 15:38, 2026-09-20 23:49 and 2026-09-20 23:55. Each final JSON reported `output_path=/tmp/land-work-preview-XXXX/.land-work/verifier.log` after `cleanup: passed`, and the file no longer existed. Previews involved include g663hhnu, jus6t3ug and mz7t9g27.

## Acceptance criteria

- [ ] After a failed verify step, `output_path` in land.py's final JSON names a file that exists after land.py exits. The file is outside any preview or scratch worktree.
- [ ] The failure JSON includes `verifier_log_tail` (last N lines, reusing run-verifier's existing value).
- [ ] Successful runs keep their log too, at the same location, and the path is included in the success JSON. Retention is bounded, for example the last 20 logs per repo, and the bound is documented.
- [ ] Regression test in `tests/land_work/test_land_driver.py`: a verifier stub that fails and prints a marker line. The test asserts `os.path.exists(result["output_path"])`, that the log contains the marker, and that `verifier_log_tail` contains it. The test is committed failing against the current code first, then passing.
- [ ] Proof at close: the test name and the failing-then-passing `python3 -m unittest tests.land_work.test_land_driver` output. "Merged" is not sufficient.

## Suggested approach

- land.py passes `--log <state_dir>/<repo-name>/<branch>-<UTC timestamp>.log` to run-verifier. `<state_dir>` is `$XDG_STATE_HOME/bento/land-work`, falling back to `~/.local/state/bento/land-work`. Use the same directory as the progress log from `land-py-invocation-progress-log`, so one landing leaves its files side by side.
- Add an optional `tail` argument to `StepFailure` and fill it from `payload.get("verifier_log_tail")`.
- Keep cleanup-before-emit unchanged. The log now lives outside the preview, so the order no longer matters.

## Out of scope

- Shatter's own verifier discarding gate output with `>/dev/null 2>&1`. That is shatter str-qwua7.55. The bento-side contract is `verifier-contract-migration`.
- Preview ownership and locking (bento-e583).

## Dependencies

- Blocked by: none.
- Related: bento-rdtn.4 and bento-rdtn.14 (closed; see `verifier-log-reopen-note`), `land-py-invocation-progress-log` (shares the state directory), `verifier-contract-migration`.

Priority: P1 · Type: bug · Labels: audit, land-work · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/02, bento-02
