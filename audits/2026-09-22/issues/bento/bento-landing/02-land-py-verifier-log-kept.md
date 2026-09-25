---
slug: land-py-verifier-log-kept
kind: new
title: "land.py reports verifier.log as output_path after deleting it with the preview; ordinary verifier failures carry no log tail"
priority: P1
type: bug
labels: [audit, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land.py reports verifier.log as output_path after deleting it with the preview; ordinary verifier failures carry no log tail

## Problem

When the verify step fails under `land.py`, the final JSON's `output_path` points to `<preview>/.land-work/verifier.log`. By the time that JSON is printed, land.py has already removed the preview worktree, so the file does not exist. The agent cannot see why the verifier failed. In shatter the agent had to rebuild a preview by hand and rerun the gates, which took several more minutes each time.

A second gap makes this worse: run-verifier attaches `verifier_log_tail` only when the verifier was `killed` or timed out. An ordinary failure (the verifier ran and reported a failing check) carries no tail at all, so even the JSON has nothing to show.

bento-rdtn.4 (closed) was meant to persist raw verifier output. bento-rdtn.14 (closed) added land.py, whose failure path cleans up the preview. Each was tested on its own, and no test covers the two together. On the land.py path, rdtn.4's goal is not met.

**Relation to bento-x4bm (open P2).** bento-x4bm ("land.py: validate the closure note before merging, record the landing durably ...") also passes `--log <git-common-dir>/bento/landing/<issue-id>/verifier.log`, but only when land.py is run with `--issue`/`--closure-note`, and it is blocked by four other issues (bento-wzbt, bento-sy49, bento-79j2, bento-bo9c). Without those flags, behaviour is unchanged under x4bm. This issue makes the log survive on **every** land.py run, now, and uses the same `<git-common-dir>/bento/landing/` root so x4bm can adopt the location instead of introducing a second one. A companion comment on bento-x4bm (below) records the agreed location.

## Evidence

Re-verified 2026-09-23 at bento origin/main 0b8d488 (land-work scripts unchanged since 1c0c1e6). Paths are relative to `catalog/skills/land-work/scripts/`.

- `land-work-run-verifier.py:351`: `log_path = Path(args.log).resolve() if args.log else candidate / ".land-work" / "verifier.log"`. The default path is inside the candidate, which is the preview.
- `land-work-run-verifier.py:136-138` (`_fail()`): `verifier_log_tail` is set only when `status in ("killed", "timeout")`. A verifier that exits and reports `failed` gets no tail.
- `land.py:296-305`: builds `verifier_args` without `--log`.
- `land.py:126`: `raise StepFailure(step, message, output_path=payload.get("verifier_log"))`. Any `verifier_log_tail` in the payload is dropped.
- `land.py:366-375`: the `except StepFailure` handler calls `driver.cleanup_preview()` (which runs `--cleanup`, i.e. `git worktree remove --force`, on the preview) and then emits `"output_path": exc.output_path`.
- Shatter session 9f13ca23 had three verify failures: 2026-09-19 15:38, 2026-09-20 23:49 and 2026-09-20 23:55. Each final JSON reported `output_path=/tmp/land-work-preview-XXXX/.land-work/verifier.log` after `cleanup: passed`, and the file no longer existed. Previews involved include g663hhnu, jus6t3ug and mz7t9g27.

## Acceptance criteria

- [ ] land.py always passes `--log <landing-dir>/verifier.log` to run-verifier, where `<landing-dir>` is `<git rev-parse --git-common-dir>/bento/landing/<key>/` and `<key>` is the issue id when land.py knows it (x4bm's `--issue`), otherwise `<sanitized-branch>-<UTC timestamp>`. The directory survives preview removal and is never tracked.
- [ ] After a failed verify step, `output_path` in land.py's final JSON names that file, and the file exists after land.py exits.
- [ ] run-verifier's `_fail()` attaches `verifier_log_tail` (last 20 lines) for every failure in which the verifier command ran, including an ordinary `failed` result, not only `killed`/`timeout`. land.py copies it into the failure JSON as `verifier_log_tail`.
- [ ] Successful runs keep their log at the same location and the success JSON includes `verifier_log`. Retention is bounded (for example the newest 20 non-issue landing dirs per repo are kept) and the bound is documented in SKILL.md.
- [ ] Regression test in `tests/land_work/test_land_driver.py`: a verifier stub that prints a marker line to stderr and then emits **valid verifier JSON with a failing check** and exits normally (so the run is classified `failed`, not `killed` or `timeout`; the test asserts `verifier_status == "failed"` in the payload). The test asserts `os.path.exists(result["output_path"])`, that the file contains the marker, that `result["verifier_log_tail"]` contains the marker, and that no registered preview remains. It is committed failing against the current code first, then passing.
- [ ] Unit test in `tests/land_work/test_land_work_verifier.py`: an ordinary failing verifier produces `verifier_log_tail`.
- [ ] Proof at close: the test names and the failing-then-passing `python3 -m unittest tests.land_work.test_land_driver tests.land_work.test_land_work_verifier` output in the close reason. "Merged" is not sufficient.

## Suggested approach

- Compute `<landing-dir>` once in `Driver.run()` after `prepare` (the git common dir is stable across worktrees).
- Add an optional `tail` field to `StepFailure` and fill it from `payload.get("verifier_log_tail")`.
- In `_fail()`, change the tail condition to "the verifier command ran and `log_text` is not None".
- Keep cleanup-before-emit unchanged. The log now lives outside the preview, so the order no longer matters.

## Out of scope

- Shatter's own verifier discarding gate output with `>/dev/null 2>&1`. That is shatter str-qwua7.55. The bento-side contract is `verifier-contract-migration`.
- Preview ownership and locking (bento-e583).
- The closure-note record and tracker close (bento-x4bm).

## Dependencies

- Blocked by: none.
- Related: bento-x4bm (open; adopts this location for its durable record), bento-rdtn.4 and bento-rdtn.14 (closed; see `verifier-log-reopen-note`), `land-py-invocation-progress-log` (writes its progress log into the same landing dir), `verifier-contract-migration`.

Priority: P1 · Type: bug · Labels: audit, land-work · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/02, bento-02

## Comment for bento-x4bm

> Audit 2026-09-22 (shatter), cross-reference: `<id of land-py-verifier-log-kept>` makes land.py pass `--log <git-common-dir>/bento/landing/<key>/verifier.log` on every run (key = issue id when known, else branch plus timestamp), so the verifier log survives preview cleanup even without `--issue`. When this issue adds the durable landing record, please reuse that directory and file name rather than a second location; the "verifier.log still exists after the preview is removed" check here then holds for both paths.
