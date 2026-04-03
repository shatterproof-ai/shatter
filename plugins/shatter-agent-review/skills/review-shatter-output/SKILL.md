---
name: review-shatter-output
description: Review Shatter output for a downstream user, explain the most important observed behaviors in human terms, and preserve precise evidence such as concrete samples, exact outputs, and spec fragments. Use after a Shatter run has been captured.
---

## Purpose

Turn a Shatter run into an analyst-style review.

This skill should not only spot possible tool problems. It should also explain what the target appears to do and which cases matter most to a human reader.

## Inputs

Prefer this evidence order:

1. spec JSON or other machine-readable artifacts
2. captured stdout and stderr from the run
3. generated reports or test exports

If exploration is partial, say so explicitly.

## What to produce

Write a review with these sections:

1. `Overall interpretation`
2. `Most important cases`
3. `Precise observed results`
4. `Possible issues or ambiguities`
5. `Recommended next step`

For the exact headings and per-section expectations, read `../../references/report-schema.md`.

## How to choose the most important cases

Prioritize 3-7 cases that best explain the target's behavior:

- thrown errors or failure paths
- broad input-domain splits
- boundary values
- surprising coercions, nullish handling, or edge cases
- cases that dominate the function's behavior
- signs that exploration is incomplete or unstable

## Case format

For each important case, include both:

- a human explanation of what the case means and why it matters
- precise evidence: representative inputs, exact outputs or errors, and any path condition or spec fragment available

Do not collapse the review into raw dumps. The human explanation is required.

## Distinguish behavior from tool issues

Separate:

- normal target-program behavior
- uncertainty caused by partial exploration
- likely Shatter bugs or UX problems

Program exceptions discovered by Shatter are often useful findings, not tool failures. Mark them as tool issues only when the evidence points to Shatter itself: crashes, malformed output, inconsistent samples, deserialization failures, impossible summaries, or missing artifacts.
