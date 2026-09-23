---
slug: verifier-log-reopen-note
kind: reopen-note
title: "Comment on closed bento-rdtn.4: on the land.py path the persisted verifier log is deleted before it is reported"
priority: P1
type: note
labels: [audit, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-rdtn.4
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Comment on closed bento-rdtn.4

Target: **bento-rdtn.4** (closed, "land-work-run-verifier: persist raw verifier output and report 'killed' distinctly from 'failed'"). Post as a comment only. Do not reopen it; the fix is tracked in the new issue.

Comment text:

> Audit 2026-09-22 (shatter) follow-up, finding bento-02. This issue's goal is not met when landing goes through `land.py` (bento-rdtn.14). run-verifier persists the raw log by default at `<candidate>/.land-work/verifier.log` (`land-work-run-verifier.py:351`), which is inside the preview worktree. land.py does not pass `--log` (`land.py:296-305`). On failure it raises `StepFailure(..., output_path=payload.get("verifier_log"))` (`land.py:126`), then calls `cleanup_preview()`, which runs `git worktree remove --force`, before emitting `output_path` (`land.py:365-375`). The reported log therefore never exists. `verifier_log_tail` is also dropped.
>
> Observed in shatter session 9f13ca23 on all three verify failures (2026-09-19 15:38, 2026-09-20 23:49, 2026-09-20 23:55): `output_path=/tmp/land-work-preview-XXXX/.land-work/verifier.log` was reported after `cleanup: passed`, and the file was gone. The agent rebuilt a preview by hand to find the failure.
>
> Tracked in `<id of land-py-verifier-log-kept>`, which moves the log out of the preview, propagates the tail, and requires a failing-then-passing test that checks the file exists. For future closes: the test for a persistence fix should run through the real driver (land.py), not only through the component script.

(Filer: replace `<id of land-py-verifier-log-kept>` with the id assigned to that slug.)
