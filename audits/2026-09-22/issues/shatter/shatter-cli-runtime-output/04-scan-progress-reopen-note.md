---
slug: scan-progress-reopen-note
kind: reopen-note
title: "Comment on closed str-7pkp.5: scan progress now prints after the scan ends"
priority: P2
type: note
labels: [cli, progress, scan, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-7pkp.5
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-7pkp.5

Target: `str-7pkp.5` ("Scan shows live progress", closed). Action: add the comment below. Do not reopen. The fix is tracked in the new issue it names.

## Comment text

> Audit 2026-09-22: this behavior has regressed. Default `shatter scan` no longer shows live progress. `shatter-cli/src/commands/scan.rs:1412-1424` computes `scan_start.elapsed()` once after the scan returns, then loops over `result.function_results` and logs `[i/N] <fn> (<elapsed>s elapsed)`. All N lines appear together after the scan, with the same elapsed value (for example ten lines of `(26.0s elapsed)`). Nothing is printed while the scan runs. `scan --progress` is live but interleaves raw JSON objects with human `[info]` lines on stderr. `shatter run` prints no progress at all (0 bytes of stderr over 74 s). Evidence: `audits/2026-09-22/cli-ux-transcripts/ts-scan.err`, `scan-progress.err` and `run.err`, and finding cli-ux-06. Tracked in the new issue **scan-progress-post-hoc** (<id of scan-progress-post-hoc>), which also adds a test asserting that one function's completion line reaches stderr while another function is still running (a stderr-before-stdout ordering check would not catch this regression).

## Filing note

The filer script must replace each `<id of slug>` placeholder with the id assigned to scan-progress-post-hoc (this bucket, 03).
