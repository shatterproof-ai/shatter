---
repo: shatter
type: task
priority: 2
labels: agents, stories
existing: none
---
# Triage: adopt storystore for shatter (land str-u394l.3) or mark it not-adopted

Triage: maintainer decision required. This issue records the decision; the chosen branch becomes the work.

## Problem
storystore is installed and dormant: `docs/stories/` does not exist, `str-u394l.3` (stories coverage gate) has been open since 2026-06-17, `agent-env-doctor` prints "storystore is installed but dormant — docs/stories is missing" every session, and `scripts/drift-patrol.py` reports a `docs-stories` PENDING slot on every run. There is no intent/story documentation anywhere; the closest artefacts are `demo/walkthrough.yaml` and SPEC §2. Not deciding costs every session.

## Current code facts
- `docs/stories/` absent; `scripts/drift-patrol.py:452 check_docs_stories` → PENDING tied to `str-u394l.3`; `docs/DRIFT-PATROL.md:42` row "docs-stories … str-u394l.3 (not implemented)".
- `.agent-mode.local` contains only `dangerous`; the documented silence key is `agent_env_doctor_skip_plugin=storystore`.
- storystore skills available: `stories-init`, `stories-generate`, `stories-coverage`, `stories-audit`, `stories-impact-check`, `stories-update`.
- The audit's storystore recommendation (item 21) asks the plugin for an explicit "not adopted" marker that both the doctor and `check_docs_stories` recognise; today only the skip key exists.

## Options
1. **Adopt**: run `storystore:stories-init`; generate observed-mode stories for `explore`, `scan`, `spec-diff`, `init`; link `docs/stories/INDEX.md` from `docs/INDEX.md`; land `str-u394l.3` so `check_docs_stories` PASSes; `stories-impact-check` becomes part of the CLI-change checklist.
2. **Not adopted (proposed default)**: add `agent_env_doctor_skip_plugin=storystore` to `.agent-mode.local`; close `str-u394l.3` as won't-do with the reason; remove the `docs-stories` row from `drift-patrol.py` and `DRIFT-PATROL.md`; one line in CONTRIBUTING "Agent tooling" (new subsection) recording the decision. Rationale for the default: no story content exists after three months, SPEC §2 already serves as the behavioral intent record, and the gate would add a fourth docs surface to keep in sync.

## Acceptance checks
- Decision recorded in the close reason and in CONTRIBUTING; SessionStart shows no storystore nudge; `drift-patrol.py` has no PENDING `docs-stories` slot (PASS or removed).

## Scope
In: shatter-side config, tracker closure, drift-patrol row, optional stories scaffold. Out: storystore plugin changes.

## Size
small (not adopted) / medium (adopt).

## Provenance
Audit 2026-09-04, section 11, action item 40; evidence audits/2026-09-04/agent-system.md §3 (plugin dormancy), rec 8; docs-quality.md P3-4.
