# Add a contract test between catalog skills and the pinned shatter CLI; add requires/status metadata

## Filing metadata

- tracker/repo: shatter-agents
- action: create new issue
- type: task
- priority: P2
- labels: audit-2026-09-22
- parent: repo epic (see INDEX)
- dedupe relation: partially-covered (shatter str-wurp, str-u394l.4 cover only shatter's own docs)
- source findings: plugins-03

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

Nothing checks that commands and flags cited in shatter-agents skills exist in the shatter CLI. `.github/workflows/ci.yml` runs only `scripts/check-plugins-clean` and `python -m pytest tests/`, and no test invokes a shatter binary. The shatter repo has no reference to shatter-agents. As a result, skills citing nonexistent commands (shatter-diff, recipe runs) shipped undetected.

## Acceptance criteria

- [ ] A pytest test extracts every `shatter <subcommand> [--flag...]` invocation from `catalog/**/*.md` and `catalog/**/scripts/*` and validates each against `shatter <sub> --help` of a pinned shatter build (downloaded in CI by BUILD tag). It fails on an unknown subcommand or flag.
- [ ] Skill `metadata.json` supports `requires_shatter` (min build) and `status: experimental`. `scripts/build-plugins` excludes experimental skills from the published payload.
- [ ] The test fails today on shatter-diff (or passes only after that skill is marked experimental).

## Dependencies

Related to the shatter-diff and recipe issues in this epic.

## Source

Shatter audit 2026-09-22 finding plugins-03.
