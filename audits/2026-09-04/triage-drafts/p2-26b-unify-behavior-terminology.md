---
repo: shatter
type: task
priority: 2
labels: cli, docs
existing: none
---
# Triage: unify "path / class / cluster / behavior" terminology across explore, scan, spec, JSON and help

Triage: maintainer decision required — pick the canonical term and the JSON-key policy.

## Problem
One concept — a group of executions sharing a branch path — has four user-visible names in one session: explore prints "path(s)", the spec prints "Class N" / "Behavioral classes", scan prints "Cluster N", JSON uses `classes`, `equivalence_classes`, and `behaviors`, and help text says "equivalence classes, behavior map". Users and agents cannot tell whether these are different things.

## Current code facts
- Explore markdown report (`shatter-core/src/explorer.rs format_exploration_report`, captured `ui/ts-explore.out`): "Unique paths", "New paths".
- Spec (`spec.rs:192 SpecClass`, label "Class N — …"; markdown heading "Behavioral classes").
- Scan report (`report.rs:298 BehaviorClusterSummary`; markdown "Cluster N").
- JSON keys: `FunctionSpec.classes`; scan JSON `equivalence_classes`; snapshot `behaviors`.
- `--help` (`args.rs`): "equivalence classes", "behavior map"; `--show-clusters` flag name.
- SPEC.md §3.1 defines "Equivalence Classes"; §3.2 "Behavior Maps"; docs/GLOSSARY.md defines neither.

## Options
1. **"behavior class" everywhere (proposed default)**: prose in explore/scan/spec markdown, `--help`, SPEC §3/§5 and GLOSSARY use "behavior class"; JSON keys keep `classes`/`equivalence_classes`/`behaviors` as **documented synonyms for one release** (SPEC §5 states the canonical key `classes` and marks the others deprecated), then rename with a `version` bump in `FileSpecBundle`/scan JSON.
2. "path" everywhere (matches the mechanism, but clashes with file paths in the same reports).
3. Leave prose, document the synonyms only.

## Acceptance checks
- Decision recorded. If option 1: one term in all markdown/help/SPEC; GLOSSARY gains behavior class, behavior map, spec vs snapshot vs report, harness, adapter, opaque type; synonyms table in SPEC §5; `demo/walkthrough.sh` expectations and golden tests updated; `task docs-smoke`, `task golden-test` green; follow-up issue for the key rename with a date.

## Scope
In: user-visible wording, glossary, synonym policy. Out: struct renames in core beyond serde keys.

## Size
small–medium.

## Provenance
Audit 2026-09-04, section 11, action item 26; evidence audits/2026-09-04/usability-ui.md §2, rec 10; docs-quality.md P3-1.
