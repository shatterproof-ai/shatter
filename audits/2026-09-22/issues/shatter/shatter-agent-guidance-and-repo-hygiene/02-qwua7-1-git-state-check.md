---
slug: qwua7-1-git-state-check
kind: new
title: "drift-patrol git-state check: also fail on a repo-local git identity override or an example.com author email (the leaked fixture identity went unnoticed for three months)"
priority: P2
type: chore
labels: [agents, git, tooling, drift-patrol, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# drift-patrol git-state check: also fail on a leaked repo-local git identity

## Problem

A test fixture's identity (`[user] name = Test`, `email = test@example.com`) leaked into the shared
repo-local `.git/config` of the primary checkout (the str-jttrf GIT_DIR leak). From mid-June 2026
until 2026-09-23, commits made there were authored "Test" / "Test User", and bd showed "Owner:
Test". Nothing flagged it. The maintainer removed the section on 2026-09-23 (audit decision D5).

str-qwua7.1 (closed 2026-09-24, landed at 0975daf9) added a detection-only git-state check to
`scripts/drift-patrol.py`. It covers `core.bare=true`, a `core.hooksPath` override, prunable
worktrees, dead worktree directories and stale preview directories. It has no identity check:
`git show origin/main:scripts/drift-patrol.py | grep -n 'user.email\|example.com'` finds nothing.

**Recurrence (2026-09-24).** A second fixture identity leaked the same way. `demo/walkthrough.sh`
sets `user.name "Shatter Demo"` / `user.email "demo@shatter"` on its throwaway TIA repo; run from a
hook with an inherited `GIT_DIR`, it wrote them into the real shared `.git/config`. The fix
(c6cb2927, 2026-09-24 10:20) is itself the first commit authored `Shatter Demo`, and 22 commits on
origin/main through ee5f0a28 carry that identity. The section was removed again on 2026-09-24
(backup kept by the maintainer's session). The mailmap must cover both identities.

## Acceptance criteria

- [ ] Extend the existing git-state check (do not add a second one) so it FAILs when either holds
  for the primary checkout it already inspects:
  1. a repo-local `user.name` or `user.email` exists (`git config --file <common-dir>/config
     --get user.email`, the same `git config --file` approach the check already uses);
  2. the effective `user.email` matches `*@example.com`, `*@example.org`, `*.invalid` or a
     domain with no dot (for example `demo@shatter`).
- [ ] The finding names the file and the values, and the repair text says to remove the section
  (`git config --file <path> --remove-section user`) and check the fixture that leaked it.
- [ ] Regression tests next to the existing git-state tests: a temp repo with a local
  `user.email=test@example.com` FAILs; a temp repo with only a global identity passes. Both tests
  isolate `HOME`/`GIT_CONFIG_GLOBAL` so the developer's real config cannot affect them. The
  failing case fails on current main (record it).
- [ ] Run drift-patrol live against the primary checkout after the fix and paste the git-state
  section (expected: clean, since the leak was removed on 2026-09-23).
- [ ] `task affected` passes, with `Gates selected` recorded.

## Out of scope

- The `.mailmap` for historical commits and the fixture-side `.git/config` snapshot guard
  (`mailmap-and-fixture-config-snapshot`).
- Anything str-qwua7.1 already covers.

## Related

str-qwua7.1 (closed; the check this extends), str-jttrf (closed; the leak mechanism),
`mailmap-and-fixture-config-snapshot`. Audit evidence: `audits/2026-09-22/findings.json`
agent-repo-01.
