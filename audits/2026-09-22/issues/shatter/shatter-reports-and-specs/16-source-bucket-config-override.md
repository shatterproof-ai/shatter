---
slug: source-bucket-config-override
kind: new
title: "Let projects override the source bucket per path glob in shatter.config.json"
priority: P3
type: feature
labels: [scan, report, classification, config, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [source-bucket-fixture-dir]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Let projects override the source bucket per path glob in `shatter.config.json`

(Split from source-bucket-fixture-dir during the cross-check revision, so the heuristic fix is not held up by a new configuration feature.)

## Problem

The source-bucket classifier (`shatter-core/src/source_bucket.rs`) is a path heuristic. source-bucket-fixture-dir makes it follow conventions, but any heuristic will misclassify some layouts (the zolem `internal/fixture` production package was one). Today a project has no way to correct a wrong bucket, so its production denominator and coverage summaries stay wrong until Shatter's heuristic changes.

## Evidence

- `shatter-core/src/source_bucket.rs:145-170`: `classify_path` is path-only with a fixed precedence (policy_excluded > generated > unsupported > declaration_only > fixture_sample > test_spec > production_ish) and no configuration input.
- `shatter-core/src/config.rs:461-475`: `shatter.config.json` holds **scan-global** settings (file discovery, output, caching, limits, parallelism); `.shatter/config.yaml` holds per-function settings. Source bucketing is a file-discovery/report property, so it belongs in `shatter.config.json`.
- Audit evidence: `audits/2026-09-22/goals-runs/zolem-fixture-default.json` (goals-16), on branch `audit-2026-09-22` until the audit reports land.

## Contract

- **Location.** A new optional `source_buckets` array in `shatter.config.json`: `[{ "glob": "internal/fixture/**", "bucket": "production_ish" }, ...]`. It is not read from `.shatter/config.yaml`, and there is no CLI flag.
- **Glob base.** Globs match the file path relative to the directory that contains `shatter.config.json` (the project root), using `/` separators on every OS. A scan started in a subdirectory uses the same config and the same base, so the result does not depend on where the scan started. Files outside the project root are never matched.
- **Precedence.** Entries are tried in array order; the first match wins. A matching override replaces the heuristic buckets `generated`, `declaration_only`, `fixture_sample`, `test_spec` and `production_ish`. It does not override `policy_excluded` or `unsupported`: a file that policy excludes or that no frontend can read keeps that bucket, and `shatter doctor` warns about an override that targets such a file.
- **Values.** `bucket` must be one of the overridable bucket names; an unknown name or an invalid glob is a config error at load time, naming the entry index.
- **Visibility.** Scan JSON records, per file, whether its bucket came from an override (for example `source_bucket_origin: "override" | "heuristic"`), and the scan report schema version is bumped. `shatter doctor` lists active overrides.

## Acceptance criteria

- [ ] The contract above is implemented. The README "Project Configuration" section and the SPEC config section document `source_buckets` with one example.
- [ ] Tests: an override reclassifies `internal/fixture/loader.go` to production_ish; first-match-wins order; an override on a policy-excluded path is ignored with a doctor warning; the same result for a scan started at the root and in a subdirectory; config load errors for an unknown bucket and an invalid glob. Each new test fails on the code before this change.
- [ ] A proptest checks that with an empty `source_buckets` list the classification of any generated path equals the heuristic's.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Out of scope

- Changing the heuristic itself (source-bucket-fixture-dir).
- Per-function bucket overrides.

## Related

source-bucket-fixture-dir, str-9awj, str-jeen.37, str-jeen.38. Source finding: goals-16 (confirmed).
