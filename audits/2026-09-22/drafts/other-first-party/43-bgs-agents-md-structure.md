# AGENTS.md covers only the bugshot skill; add viz/wire skills, shared modules, sync rules and a README

## Filing metadata

- tracker/repo: bugshot
- action: create new issue
- type: chore
- priority: P3
- labels: audit-2026-09-22
- parent: repo epic (see INDEX)
- dedupe relation: partially-covered (bgs-3tq covers ANSI docs only)
- source findings: plugins-20

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

The `AGENTS.md` "Project Structure" section lists bugshot_cli, bugshot_workflow, gallery_server, ansi_render, static/templates and skills/bugshot. It omits the vizline/vizdiff/wire-bugshot skills and the shared modules `capture_runner.py`, `image_diff.py` and `baseline_manifest.py`. "Documentation Sync Rules" cover only `skills/bugshot/SKILL.md`. The repo has no README.md. ANSI (`.ansi`) capture support in `image_diff.py` is undocumented; bgs-3tq tracks that part.

## Acceptance criteria

- [ ] Project Structure lists all four skills and the shared modules.
- [ ] Sync Rules map each shared module or flag to the SKILL.md files it affects (for example, capture_runner/wire flags -> `wire-bugshot` SKILL.md).
- [ ] A short README.md exists (purpose, install, skills).

## Source

Shatter audit 2026-09-22 finding plugins-20.
