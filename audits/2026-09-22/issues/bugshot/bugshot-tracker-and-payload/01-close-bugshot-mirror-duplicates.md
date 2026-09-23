---
slug: close-bugshot-mirror-duplicates
kind: new
title: "Close the 55 bugshot-* mirror duplicates of bgs-* issues and rewire their dependencies"
priority: P2
type: chore
labels: [audit-2026-09-22, tracker, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bugshot)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bugshot (prefix bgs)"
---

# Close the 55 bugshot-* mirror duplicates of bgs-* issues and rewire their dependencies

## Problem

The bugshot Beads database holds a large part of its history twice. There are
115 issues: 60 `bgs-*` and 55 `bugshot-*`. Every `bugshot-*` issue has a
`bgs-*` twin with the same title and the same suffix. This looks like the side
effect of a prefix rename or re-import. Because of the duplicates, `bd ready`,
`bd list` and search show doubled results, and an agent can claim or update
either twin, so the two copies drift apart.

Most mirrors are already closed. The live ones are the problem: 4 are open and
1 is deferred, so they show up in work queues. Some mirrors also have
dependency edges.

## Evidence

Re-verified 2026-09-23 in `/home/ketan/project/bugshot` (HEAD `e622d73`, live
`bd`, `issue_prefix` = `bgs`):

- `bd list --all --limit 0 --json` returns 115 issues. By prefix: `bgs` 60,
  `bugshot` 55. (`bd list` defaults to `--limit 50`; any inventory must pass
  `--limit 0`.)
- 54 distinct titles occur under both prefixes. All 55 `bugshot-*` issues have
  a same-suffix `bgs-*` twin with an identical title and identical status. The
  title "Bump plugin version" appears four times (`bgs-5q9`, `bugshot-5q9`,
  `bgs-06c`, `bugshot-06c`), which accounts for 55 mirrors over 54 titles.
- Status of the `bugshot-*` mirrors: 50 closed, 4 open, 1 deferred.
  - Open twin pairs: `bgs-47p`/`bugshot-47p`, `bgs-hx3`/`bugshot-hx3`,
    `bgs-7g0`/`bugshot-7g0`, `bgs-wyf`/`bugshot-wyf` (chat-agent increments
    5, 4, 3 and 2).
  - Deferred pair: `bgs-6zc`/`bugshot-6zc` (chat-agent increment 1).
- Mirrors with dependency edges (id, dependency_count, dependent_count):
  `bugshot-47p` (1,0), `bugshot-hx3` (2,0), `bugshot-7g0` (1,1),
  `bugshot-wyf` (1,0), `bugshot-6zc` (0,4), `bugshot-5wi` (1,0),
  `bugshot-qh9` (2,1), `bugshot-egh` (0,2), `bugshot-wpj` (1,0),
  `bugshot-7p0` (0,1).
- The edges sampled so far are mirror-to-mirror, and the `bgs-*` twins already
  carry the equivalent `bgs-*`-to-`bgs-*` edges. Example:
  `bd dep list bugshot-6zc --direction=up` lists `bugshot-wyf`, `-47p`, `-hx3`
  and `-7g0` via `blocks`, and `bd dep list bgs-6zc --direction=up` lists
  `bgs-wyf`, `-7g0`, `-47p` and `-hx3` via `blocks`. The same holds for
  `bugshot-hx3`/`bgs-hx3` and `bugshot-qh9`/`bgs-qh9`. Nobody has yet checked
  every edge in both directions. That check is step 1 below.
- No mirror has comments (`comment_count` is 0 for all 55).
- Mirror `created_at` dates run from 2026-04-27 to 2026-06-12. The mirrors are
  already present in the git-tracked `.beads/issues.jsonl` at `616affd`
  (2026-06-11). `.beads/config.yaml` has only its bd-init commit and records
  no prefix history.

Source: Shatter audit 2026-09-22, finding plugins-16. The shatter-agents half
of that finding is tracked in the shatter-agents epic, not here.

## Acceptance criteria

- [ ] **Inventory saved before any write.** Run
      `bd list --all --limit 0 --json > <scratch>/before.json`. For every
      `bugshot-*` issue, also save both edge directions:
      `bd dep list <id> --json` (down) and `bd dep list <id> --direction=up --json`.
      Keep these files with the work, because the verifier below reads them.
- [ ] **Edge mapping is complete and deduplicated.** For every edge that
      touches a mirror, in either direction, compute the mapped edge: replace
      *each* `bugshot-XXX` endpoint with `bgs-XXX` and keep the dependency type
      (`blocks`, `parent-child`, `related`, and so on). If the mapped edge
      already exists on the twin with the same type, add nothing. Otherwise add
      it with `bd dep add` and the same `--type`. Never add an edge with a
      `bgs-*` endpoint on one side and a `bugshot-*` endpoint on the other.
      Record the number of edges added. If every twin edge already exists, the
      expected count is 0.
- [ ] **Twin state check.** For each pair, diff description, priority,
      assignee, labels and status between mirror and twin. Titles and statuses
      already match. Copy any field that exists only on the mirror onto the
      twin, and list each copied field, or state "no mirror-only state".
- [ ] **Mirrors are removed from all queues.** The 5 live mirrors
      (`bugshot-47p`, `-hx3`, `-7g0`, `-wyf`, `-6zc`) are closed with the reason
      `duplicate of bgs-XXX (prefix-mirror cleanup, audit 2026-09-22)`.
      The 50 already-closed mirrors are all handled one way: either each gets
      a `duplicate of bgs-XXX` close reason, or all are deleted with
      `bd delete`. The close reason records which option was chosen and why.
      If `bd delete` refuses or cascades because a mirror has dependents,
      remove the mirror-side edges first. Do not force the delete.
- [ ] **Executable close proof.** Run a verifier script and paste its output
      into the close reason. It must exit non-zero on any violation. Against
      a fresh `bd list --all --limit 0 --json` plus both-direction
      `bd dep list` output for every remaining `bugshot-*` id and every `bgs-*`
      twin, it asserts all of the following:
      1. The total count is 115 if mirrors were annotated, or 60 if they were
         deleted. No `bgs-*` issue is missing compared with `before.json`.
      2. Every remaining `bugshot-*` issue has `status == "closed"` and a close
         reason that begins `duplicate of bgs-`.
      3. No non-closed issue has a dependency edge, in either direction, with
         a `bugshot-*` endpoint.
      4. Every mapped edge from the inventory exists on the `bgs-*` twin with
         its original type.
      5. `bd ready --json` contains no `bugshot-*` id.
      The script must fail when it is run against `before.json`, because 5
      mirrors are open there. Paste that failing output too. It shows the
      checks can fail.
- [ ] **Root cause** is recorded in one sentence in the close reason. If the
      cause cannot be found, record "unknown" together with what was checked:
      `git log -p -- .beads/issues.jsonl` around the first commit that
      contains `bugshot-` ids, and any embedded-Dolt or JSONL import that ran
      at that time.

## Suggested approach

1. Build the inventory (AC 1). Map the pairs by suffix, which matches one to
   one, and confirm that the titles are identical.
2. Compute the mapped edge set from the down and up listings of both twins,
   then diff it against the edges the twins already have. Given the samples
   above, the expected diff is empty.
3. Close the 5 live mirrors with `bd close <id> --reason "duplicate of bgs-XXX ..."`.
   Bd writes are slow, so run them in the background.
4. Decide between deleting and annotating the 50 closed mirrors. Deleting
   makes searches cleaner, and annotating keeps the history. Either is
   acceptable if it is applied consistently.
5. For the root cause, bisect `.beads/issues.jsonl` history for the first
   commit that contains `bugshot-` ids. Do not use `.beads/config.yaml` for
   this, because it has no prefix history.

## Out of scope

- The shatter-agents `agents-*`/`sa-*` mirrors and the stale
  `sa-d1b`/`bento-m4y5` issues. They come from the same finding and are
  tracked in other repos' epics.
- A generic cross-prefix duplicate detector in bento's beads-issue-flow. That
  is a bento-side concern.
- Any work on the chat-agent increments themselves.

## Metadata

- Priority: P2
- Type: chore
- Labels: audit-2026-09-22, tracker, hygiene
- Parent epic: Epic: Audit 2026-09-22 findings (bugshot)
- Dependencies: none
