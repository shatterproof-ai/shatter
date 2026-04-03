---
name: report-shatter-issues
description: Write a markdown file that enumerates issues found during a Shatter review and includes relevant system and project context. Use when a downstream user wants a durable issue report instead of tracker-specific automation.
---

## Purpose

Create a markdown report file, not a tracker ticket.

The report should be durable, portable, and detailed enough that a user can keep it locally or paste it into GitHub later.

## Required inputs

- the review output from `review-shatter-output`
- the run directory and saved artifacts
- the relevant targets or files that were explored

Before writing the report, collect environment and project context with:

```bash
../../scripts/collect-context.sh --run-dir <run-dir> --target <path> --artifact <path> ...
```

Save that output alongside the report and include it in the final markdown file.

## Report requirements

The report must be a markdown file with:

1. `Run summary`
2. `Environment and project context`
3. `Enumerated issues`
4. `Evidence and artifact references`

For the exact issue schema, read `../../references/report-schema.md`.

## Issue selection

Include only actual issues or clearly labeled uncertainties. Do not restate every observed behavior.

Good issue categories:

- probable Shatter bug
- report quality or usability problem
- incomplete exploration that blocks trust
- ambiguous result that needs confirmation

If the review found no issues, still write the markdown file and state that no actionable issues were found.

## Per-issue content

For each issue, include:

- title
- severity
- category
- human description
- why it matters
- repro command
- expected versus actual behavior
- precise evidence
- related targets and artifact paths

Prefer one numbered issue section per finding.
