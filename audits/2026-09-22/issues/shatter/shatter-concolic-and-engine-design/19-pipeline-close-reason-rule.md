---
slug: pipeline-close-reason-rule
kind: new
title: "CLAUDE.md completion checklist: pipeline fixes must name the production call site and the pipeline-level test; test workarounds in prose must be filed"
priority: P3
type: task
labels: [audit-2026-09-22, docs, agent-guidance, testing]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# CLAUDE.md completion checklist: pipeline fixes must name the production call site and the pipeline-level test; test workarounds in prose must be filed

## Problem

Two issues were closed on evidence from non-production paths: str-0s76.6, via a test that calls `orchestrator::explore` directly while production callers pass `None` for the setup context, and str-55ep, which fixed a dead shrinker copy. The CLAUDE.md Completion Checklist (`CLAUDE.md:43` onward) accepts unit and API tests as proof of pipeline behaviour for anything except the named E2E commands. It never asks which production caller was exercised.

Test workarounds are also recorded only as prose. For example `shatter-ts/CLAUDE.md:283-287` says the E2E reads `raw_results` because switch emits no `branch_path`. That is a product gap (ts-switch-ternary-instrumentation) described as a testing note.

This was part of the combined draft engine-parity-e2e and is split out as a separate deliverable.

## Acceptance criteria

- [ ] The CLAUDE.md Completion Checklist gains a rule: the close reason for a pipeline feature or fix names (a) the production call site exercised, as `file:line` of the caller, and (b) the pipeline-level test that proves it, which must go through `pipeline_orchestrator`/`run_pipeline` or the CLI, not `orchestrator::explore` or `explorer::explore_function`.
- [ ] The same checklist gains a rule: a test workaround that exists because of a product gap is filed as an issue, and the prose links that issue ID.
- [ ] A grep over every `CLAUDE.md` in the repo for workaround phrasing ("because", "workaround", "instead of", "reads raw_results") is reviewed. Each product-gap workaround found is linked to an existing issue or a new one. The list goes in the close note.
- [ ] The rule is cross-linked with bento's close-reason-evidence draft (bento bucket), so the generic bento rule and this repo rule agree. If that draft is not filed yet, the close note says so.
- [ ] Proof at close: the CLAUDE.md diff and the grep review list.

## Out of scope

- The engine_parity suite (engine-parity-e2e).
- Changing bento's skill text (close-reason-evidence, bento bucket).

## Metadata

- Priority: P3
- Type: task
- Labels: audit-2026-09-22, docs, agent-guidance, testing
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: engine-parity-e2e, ts-switch-ternary-instrumentation, close-reason-evidence (bento); bento-m4en, bento-a0nz; str-0s76.6, str-55ep
- Source findings: core-22 (split from draft shatter-agent/23)
