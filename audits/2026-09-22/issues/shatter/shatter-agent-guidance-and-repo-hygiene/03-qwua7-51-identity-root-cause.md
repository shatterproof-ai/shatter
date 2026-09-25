---
slug: qwua7-51-identity-root-cause
kind: note-to-existing
title: "Note on str-qwua7.51: 'Owner: Test' most likely came from the leaked repo-local fixture identity (removed 2026-09-23); bd's actor override is BEADS_ACTOR, not BD_ACTOR; close-reason SHAs must be labelled"
priority: P2
type: task
labels: [agents, beads, git]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.51
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.51: probable root cause of "Owner: Test", actor variable, close-reason SHAs

Target: **str-qwua7.51** (open, P2, "Configure bd identity in the SessionStart
hook and require a close reason at landing"). Action: `bd comments add str-qwua7.51`
with the text below. The owner/maintainer then re-scopes the issue as the
comment proposes. Do not close it: the close-reason half is still valid.

## Comment text

> Audit 2026-09-22 root-cause correction (maintainer decision D5, 2026-09-23;
> evidence `audits/2026-09-22/findings.json` agent-repo-01, prior-04,
> agent-repo-16).
>
> **1. The environment variable in this issue is wrong.** This issue's body
> plans to export `BD_ACTOR` from a SessionStart hook. Installed bd 1.1.0
> does not read `BD_ACTOR`: `bd --help` documents
> `--actor string  Actor name for audit trail (default: $BEADS_ACTOR, git user.name, $USER)`.
> A hook that exports `BD_ACTOR` would change nothing.
>
> **2. Probable root cause (to be confirmed by the probe below).** The
> primary checkout's repo-local `.git/config` carried a leaked test-fixture
> identity (`[user] name = Test, email = test@example.com`), written there by
> the str-jttrf/str-y0rcz GIT_DIR fixture leak. With no `--actor` and no
> `BEADS_ACTOR`, bd's documented actor fallback is git `user.name`, so the
> leak would explain "Test" as the actor for every bd write from the primary;
> all issues created since 2026-09-05 show `Owner: Test`. The "Test User"
> variant matches fixtures that set `user.name "Test User"`
> (`scripts/test_walkthrough_examples_checkout.py:63`,
> `shatter-cli/tests/implicit_init_gitignore_test.rs:52`). This is inferred
> from the documented fallback and the matching names; it was not traced
> through bd's source.
>
> The maintainer removed the leaked `[user]` section on 2026-09-23. Now
> `git -C /home/ketan/project/shatter config --show-origin user.name` resolves
> to `~/.gitconfig` (Ketan Gangatirkar). Follow-ups: `.mailmap` and a fixture
> config guard in <mailmap-and-fixture-config-snapshot>; the recurrence check
> on str-qwua7.1.
>
> **3. Proposed re-scope of the identity half.** bd records three distinct
> identities; check each separately before dropping anything:
> - **actor** (audit trail / event author): `--actor` > `$BEADS_ACTOR` > git
>   `user.name` > `$USER`, per `bd --help`;
> - **assignee**: set by `bd update <id> --claim` ("sets assignee to you");
> - **owner / created_by**: set at create time; its source is not
>   documented in `bd create --help`.
>
> Probe, in a fresh session from the primary checkout with `BEADS_ACTOR`
> unset: create a scratch issue, claim it, close it, then
> `bd show <scratch-id> --json` and record owner, created_by, assignee and
> the event actor, plus `echo "${BEADS_ACTOR-unset}"` and
> `git config --show-origin user.name`. Delete the scratch issue afterwards.
> If all four show the real name, record that and drop the SessionStart
> identity requirement. If any is wrong, the fix sets **`BEADS_ACTOR`** (not
> `BD_ACTOR`) or the documented config key, and the probe is re-run to show
> the corrected value.
>
> **4. Keep the close-reason half, with one precision.** Every landing close
> carries a SHA or a duplicate/won't-do reason, and drift-patrol warns on
> reasonless closes. Add: a close reason (or diagnosis) that cites a commit
> must say what that commit is. A claim that work **is landed** or that
> behaviour was checked **on main** must cite a SHA for which
> `git merge-base --is-ancestor <sha> origin/main` exits 0. Any other SHA
> (an unmerged reproduction, a feature-branch fix, a bisect point) is allowed
> but must be labelled as such, for example "tested on feature branch
> `<branch>` at `<sha>` (not on main)". Motivating case: str-qwua7.14 was
> closed "Not reproducible against current main (e50fc399)", but `e50fc399`
> is a stray fixture commit that is not an ancestor of origin/main (see
> <fixture-corruption-incident-reverify> and <qwua7-14-reverify-on-main>).
>
> **5. Housekeeping.** Update the body's bd facts: the installed bd is now
> **1.1.0**, not v0.63.3. Re-check the `bd close` reason flag against
> `bd close --help` on 1.1.0. Existing issues keep `created_by: Test`
> (historical). Do not bulk-edit them.
