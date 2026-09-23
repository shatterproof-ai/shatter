---
slug: qwua7-51-identity-root-cause
kind: note-to-existing
title: "Note on str-qwua7.51: 'Owner: Test' came from the leaked repo-local fixture identity (removed 2026-09-23), not a missing SessionStart identity"
priority: P2
type: task
labels: [agents, beads, git]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.51
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.51: real root cause of "Owner: Test"

Target: **str-qwua7.51** (open, P2, "Configure bd identity in the SessionStart
hook and require a close reason at landing"). Action: `bd comments add str-qwua7.51`
with the text below. The owner/maintainer then re-scopes the issue as the
comment proposes. Do not close it: the close-reason half is still valid.

## Comment text

> Audit 2026-09-22 root-cause correction (maintainer decision D5, 2026-09-23;
> evidence `audits/2026-09-22/findings.json` agent-repo-01, prior-04).
>
> The "Test" / "Test User" owners and assignees are **not** caused by a
> missing SessionStart bd identity. With no `BD_ACTOR` and no configured
> actor, bd falls back to git `user.name`; the match between the bd owners
> and the git authors below is consistent with that. The primary checkout's repo-local `.git/config` carried a leaked
> test-fixture identity (`[user] name = Test, email = test@example.com`),
> which the str-jttrf/str-y0rcz GIT_DIR fixture leak wrote there. It overrode
> the global identity for every git commit and every bd write made from the
> primary: all issues created since 2026-09-05 have `created_by: Test`. The
> "Test User" variant comes from fixtures that set `user.name "Test User"`
> (`scripts/test_walkthrough_examples_checkout.py:63`,
> `shatter-cli/tests/implicit_init_gitignore_test.rs:52`).
>
> The maintainer removed the leaked `[user]` section on 2026-09-23. Now
> `git -C /home/ketan/project/shatter config --show-origin user.name` resolves
> to `~/.gitconfig` (Ketan Gangatirkar). Follow-ups: `.mailmap` and a fixture
> config snapshot in the new audit issue `mailmap-and-fixture-config-snapshot`;
> the recurrence check on str-qwua7.1.
>
> **Proposed re-scope of this issue:**
> - Drop the `BD_ACTOR`-from-SessionStart requirement unless a fresh claim
>   still shows a wrong owner. First step: in a fresh session, run
>   `bd update <scratch-id> --claim` (or create and delete a scratch issue),
>   then `bd show` it. If the owner is the real name, record that and drop the
>   identity half.
> - Keep the close-reason half (every landing close carries a SHA or a
>   duplicate/won't-do reason; drift-patrol warns on reasonless closes).
> - Update the body's bd facts: the installed bd is now **1.1.0**, not
>   v0.63.3. Re-check the `bd close` reason flag against `bd close --help` on 1.1.0.
> - Existing issues keep `created_by: Test` (historical). Do not bulk-edit them.
