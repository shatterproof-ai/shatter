---
slug: qwua7-1-git-state-check
kind: note-to-existing
title: "Note on str-qwua7.1: core.bare is repaired; re-scope to the git-state check (local identity override, *@example.com, core.bare, local hooksPath)"
priority: P1
type: chore
labels: [agents, git, tooling, drift]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.1
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.1: re-scope to the git-state check

Target: **str-qwua7.1** (open, P1, "Repair primary checkout (core.bare=true)
and add a git-state hygiene check"). Action: `bd comments add str-qwua7.1`
with the text below. Do not close it and do not change its priority.

## Comment text

> Audit 2026-09-22 update (maintainer decision D5, 2026-09-23). Evidence:
> `audits/2026-09-22/findings.json` agent-repo-01, prior-04, prior-09.
>
> **Repair half is done or moving elsewhere:**
> - `core.bare` is already `false` in the primary checkout
>   (`git -C /home/ketan/project/shatter config --get core.bare` -> `false`).
> - Unregistered /tmp land-work previews: none registered at the time of the
>   audit (`git worktree list` shows none).
> - The five dead dirs under `~/.local/share/worktrees/shatter/` and the
>   `.claude/worktrees/str-umw3/` orphan still exist. Their operator-confirmed
>   removal (or documented retention) is now tracked by
>   <orphan-worktree-dirs-cleanup>. Drop them from this issue's acceptance.
> - A second instance of the same damage class was found: the fixture identity
>   `[user] name = Test, email = test@example.com` had leaked into the primary's
>   repo-local `.git/config` (str-jttrf leak). The maintainer removed it
>   2026-09-23. The `.mailmap` and fixture-side config guard are tracked in
>   <mailmap-and-fixture-config-snapshot>.
>
> **Re-scoped acceptance for this issue (the check only):**
> - A repo-state check (a new entry in `scripts/drift-patrol.py` `CHECKS`,
>   `:757`, as this issue already chose; optionally surfaced by
>   `scripts/setup-hooks.sh --check`) inspects **the checkout drift-patrol is
>   invoked from** (its repo root, not an arbitrary cwd such as a fixture
>   repo) and FAILs when any of these holds:
>   1. a repo-local `user.name` or `user.email` override exists
>      (`git config --local --get user.email` / `user.name` non-empty);
>   2. the effective `user.email` (any scope) matches `*@example.com` (also
>      `*.invalid` / `example.org`, if cheap);
>   3. `core.bare=true`;
>   4. a repo-local `core.hooksPath` override exists.
> - **Discovery precedence (this order, tested):** (a) locate the repository
>   with `git rev-parse --git-dir` / `--git-common-dir`, which succeed even
>   when `core.bare=true` makes `--is-inside-work-tree` return false; (b) read
>   the common dir's `config` directly (`git config --file <common>/config`)
>   and evaluate conditions 1-4; (c) only if no git directory can be
>   discovered at all, or the run is in CI, report SKIP. A checkout whose
>   config says `core.bare=true` must never be reported as "not a work tree ->
>   SKIP".
> - Unit tests in `scripts/test_drift_patrol.py` build a temporary repo per
>   condition and assert FAIL, plus one clean repo asserting PASS, plus one
>   directory with no repository asserting SKIP. One test sets
>   `core.bare=true` on a non-bare checkout and asserts **FAIL, not SKIP**.
>   Include a failing-then-passing run in the close reason.
> - `python3 scripts/drift-patrol.py` shows the check PASS on the primary
>   checkout. Record the output in the close reason.
> - The prunable-worktree / stale-preview / non-repo-dir detections from the
>   original body may stay as extra conditions. Keep them only if they are
>   unit-tested the same way.
>
> This unblocks str-qwua7.18 and str-qwua7.19, which are `blocked_by` .1 only
> because of the repair premise. Consider removing those edges once this
> re-scope is accepted.
