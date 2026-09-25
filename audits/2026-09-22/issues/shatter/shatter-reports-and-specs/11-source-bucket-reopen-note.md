---
slug: source-bucket-reopen-note
kind: reopen-note
title: "Note on closed str-9awj: the segment-name misclassification recurs for production packages named 'fixture'"
priority: P3
type: bug
labels: [scan, classification, audit-2026-09-22]
parent_epic: ""
blocked_by: [source-bucket-fixture-dir, source-bucket-config-override]
existing_id: str-9awj
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on closed str-9awj: the segment-name misclassification recurs for production packages named `fixture`

Target: **str-9awj** (closed, P2, "Specs bucket mislabels prod"). Action: add the comment below. Do not reopen; the follow-up work is the new issues source-bucket-fixture-dir and source-bucket-config-override (the filer substitutes their ids). `blocked_by` above only means the new issues must be filed first so the comment can cite their ids. str-9awj status (closed) verified with `bd show` on 2026-09-23.

## Comment text

**Audit 2026-09-22 note (finding goals-16): the same class of bug recurs for `fixture`.**

This issue fixed production files under `internal/specs` being bucketed as `test_spec`. The fixture rule still uses the same any-segment heuristic: `shatter-core/src/source_bucket.rs:253-267` treats a path as `fixture_sample` if any segment is `testdata`, `test-data`, `fixtures`, `fixture`, `examples`, `example`, `samples`, `sample` or `__fixtures__`.

A zolem scan of its production package `internal/fixture` (a fixture loader and selector) reported `fixture_sample {10 files, 1279 lines}`, `production_ish {0, 0}` and `productionish_source_lines: 0`, so the production denominator for that package was zero.

Follow-up: <source-bucket-fixture-dir id> (convention-based classification anchored to the project root, and a regression test for `internal/fixture/loader.go`). A per-glob config override is a separate follow-up: <source-bucket-config-override id>. Please keep the `internal/specs` regression test from this issue when that change lands.
