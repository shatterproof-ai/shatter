---
slug: demo-complete-with-errors-green
kind: new
title: "walkthrough.sh and gauntlet.sh print 'complete with errors' in green"
priority: P3
type: bug
labels: [demo, ux, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# walkthrough.sh and gauntlet.sh print 'complete with errors' in green

## Problem

When the walkthrough or gauntlet finishes with errors, the final line is printed in bold green, the same color as success, so a failing run looks like a pass at a glance.

## Evidence

- `demo/walkthrough.sh:388`: `echo "${BOLD}${GREEN}Walkthrough complete with errors.${RESET}"`.
- `demo/gauntlet.sh:920`: `echo "${BOLD}${GREEN}Gauntlet complete with errors.${RESET}"`.
- Finding artifacts-16 (verified by reading the scripts).

## Acceptance criteria

- [ ] Both lines use red (or yellow) instead of green; `grep -n 'GREEN}.*with errors' demo/*.sh` returns nothing.
- [ ] The scripts' exit status on errors is unchanged (the gauntlet still exits 1).
- [ ] `task walkthrough` and `task gauntlet` run; the close comment records their final lines.

## Dependencies

- Blocked by: none.

## Source

Audit 2026-09-22 finding artifacts-16. Split from cli-minor-output-and-help-polish item 9.
