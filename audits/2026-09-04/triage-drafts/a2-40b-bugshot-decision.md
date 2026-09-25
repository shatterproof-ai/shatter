---
repo: shatter
type: feature
priority: 2
labels: agents
existing: none
---
# Wire bugshot for walkthrough output once bugshot supports CLI capture (bgs-3tq)
## Decision (2026-09-06)
Wire bugshot once it ships a CLI/TUI capture template (bugshot bgs-3tq). Until then set `agent_env_doctor_skip_plugin=bugshot` in .agent-mode.local so the nudge stops; when bgs-3tq lands, run `wire-bugshot --kind cli` with a capture command that snapshots `demo/walkthrough.sh --auto --delay 0` output (text and rendered HTML) so vizline/vizdiff give /walkthrough-review a baseline.

## Problem
bugshot is installed and dormant because `.agent-plugins/bento/bugshot/viz/capture-command` is missing; `agent-env-doctor` prints a nudge every session. bugshot's `vizline`/`vizdiff` skills were designed for web screenshots and nobody has decided whether Shatter's rendered outputs (walkthrough transcripts, HTML/markdown reports) count. The audit filed a bugshot-side item (audit item 20 / draft 53) to add a documented CLI capture template; wiring today means hand-writing the capture-command against an undocumented contract.

## Current code facts
- SessionStart: "bugshot is installed but dormant — `.agent-plugins/bento/bugshot/viz/capture-command` is missing; run the bugshot wire-bugshot skill".
- `.agent-mode.local` contains only `dangerous`; silence key `agent_env_doctor_skip_plugin=bugshot`.
- Candidate captures: `demo/walkthrough.sh --auto --delay 0` (text/ANSI transcript), `shatter explore -o report.html` / `scan -o scan.html` (gauntlet already writes `explore.html`, `scan.html`), `demo/walkthrough.yaml` scenario list. `/walkthrough-review` reviews output by eye today.
- bugshot README/skills assume web screenshots; no `--kind cli` path exists yet (bugshot draft 53).

## Options
1. **Wire (proposed default, deferred until bugshot ships a CLI capture template)**: once bugshot draft 53 lands, run `bugshot:wire-bugshot --kind cli` with a capture-command that executes `demo/walkthrough.sh --auto --delay 0` into a snapshot dir (text + the HTML reports), seed the baseline, and reference `vizdiff` from `/walkthrough-review`. Until then set `agent_env_doctor_skip_plugin=bugshot` with a comment naming the bugshot issue to revisit.
2. **Skip permanently**: same skip key; record "CLI output is reviewed by the gauntlet/walkthrough checkers (p1-08), not visual diff" in CONTRIBUTING.

## Acceptance checks
- Decision recorded in the close reason and CONTRIBUTING "Agent tooling"; SessionStart shows no bugshot nudge.
- If wired: capture-command present, baseline seeded, `vizdiff` produces a gallery for a deliberate report change.

## Scope
In: shatter-side config/wiring. Out: bugshot plugin changes.

## Size
small.

## Provenance
Audit 2026-09-04, section 11, action item 40; evidence audits/2026-09-04/agent-system.md §3 (plugin dormancy), rec 8; usability-ui.md §5.
