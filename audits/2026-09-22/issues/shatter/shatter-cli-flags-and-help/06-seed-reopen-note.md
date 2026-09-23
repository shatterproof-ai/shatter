---
slug: seed-reopen-note
kind: reopen-note
title: "Comment on closed str-0m0vn: symptom named explore, but --seed exists only on scan"
priority: P2
type: comment
labels: [cli, seeds, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-0m0vn
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on str-0m0vn (closed; do not reopen)

Target: `str-0m0vn`. Action: add a comment only. Leave the issue closed.

## Comment text

> Audit 2026-09-22 follow-up (finding cli-ux-08): this issue's symptom was "There is no way to make `shatter scan` or `shatter explore` reproducible", but the fix (merge aafc8ba7) added `--seed` to `scan` only (`shatter-cli/src/args.rs:977-979`). As of HEAD `56c86168` (verified 2026-09-23), `shatter explore --help` and `shatter run --help` have no `--seed`. The follow-ups filed at close (str-pbqyr, str-9m9o3) are scan-only.
>
> explore and run are now tracked in **seed-for-explore-and-run** (<new id>): a shared `--seed` on scan/explore/run reaching both engine paths, an explore reproducibility test, and the seed included in the resume key.

The filer substitutes `<new id>` with the id assigned to seed-for-explore-and-run.
