# wire-shatter-ci generates a workflow that needs plugin-internal run_targets.py and fetches install.sh from main

## Filing metadata

- tracker/repo: shatter-agents
- action: create new issue
- type: bug
- priority: P2
- labels: audit-2026-09-22
- parent: repo epic (see INDEX)
- dedupe relation: new
- source findings: plugins-11

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

`catalog/skills/wire-shatter-ci/SKILL.md` generates `.github/workflows/shatter.yml` with two defects:

1. The run step is `python3 scripts/run_targets.py --root . --json` (around line 140). "Required companion" (around lines 196-200) says the repo "must have access to it (either vendored in scripts/ or invoked through the installed Shatter plugin cache)". A GitHub runner has no plugin cache, no step vendors the file, and the skill's Verify step checks only BUILD and upload-artifact. The generated workflow therefore fails on first run.
2. The install step pins `BUILD=continuous-...` (around lines 65-72) but runs `curl .../shatterproof-ai/shatter/main/install.sh | bash` (around line 136), which does not use the pinned installer.

## Acceptance criteria

- [ ] The generated workflow either runs project wrappers or native `shatter scan` directly (preferred), or the skill vendors `run_targets.py` with a version header and update path as an explicit verified step.
- [ ] `install.sh` is fetched from the pinned tag.
- [ ] A test renders the workflow for a fixture repo and asserts every referenced script path exists in that repo after the skill runs. Optionally, run the workflow with `act`.

## Source

Shatter audit 2026-09-22 finding plugins-11.
