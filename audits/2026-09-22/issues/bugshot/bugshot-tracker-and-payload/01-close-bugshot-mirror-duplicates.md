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

The bugshot Beads database holds every issue twice for a period of its
history. There are 115 issues: 60 `bgs-*` and 55 `bugshot-*`. Every
`bugshot-*` issue has a `bgs-*` twin with the same title and the same suffix.
This looks like the side effect of a prefix rename or re-import. Because of the
duplicates, `bd ready`, `bd list` and search show doubled results, and an agent
can claim or update either twin, so the two copies drift apart.

Most mirrors are already closed. The ones still live are the problem: 4 are
open and 1 is deferred, so they show up in work queues. Some mirrors also have
dependency edges.

## Evidence

Re-verified 2026-09-23 in `/home/ketan/project/bugshot` (HEAD `e622d73`):

- `bd list --all --json` returns 115 issues. By prefix: `bgs` 60, `bugshot` 55.
- 54 distinct titles occur under both prefixes. All 55 `bugshot-*` issues have
  a `bgs-*` title twin, so there are no unmatched mirrors. The title
  "Bump plugin version" appears four times (`bgs-5q9`, `bugshot-5q9`,
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
- No mirror has comments (`comment_count` is 0 for all 55).
- Mirror `created_at` dates run from 2026-04-27 to 2026-06-12.

Command used:

```bash
cd /home/ketan/project/bugshot && bd list --all --json > /tmp/bgs.json
python3 -c 'import json,collections; L=json.load(open("/tmp/bgs.json")); print(collections.Counter(i["id"].split("-")[0] for i in L))'
# Counter({'bgs': 60, 'bugshot': 55})
```

Source: Shatter audit 2026-09-22, finding plugins-16 (the shatter-agents half
of that finding is tracked in the shatter-agents epic, not here).

## Acceptance criteria

- [ ] Before closing anything, the `bgs-*` twin of each mirror carries any
      state that only the mirror has: description edits, status, priority,
      assignee, and every dependency edge. After re-pointing,
      `bd dep list` on each twin shows only `bgs-*` endpoints.
- [ ] The 5 open or deferred mirrors (`bugshot-47p`, `-hx3`, `-7g0`, `-wyf`,
      `-6zc`) are closed with a reason of the form
      `duplicate of bgs-XXX (prefix-mirror cleanup, audit 2026-09-22)`.
- [ ] Every one of the 50 already-closed mirrors either has a
      `duplicate of bgs-XXX` close reason or is deleted with `bd delete`.
      Record which option was chosen and why in the close reason.
- [ ] Proof at close: rerunning the prefix/title script above shows zero
      non-closed `bugshot-*` issues and zero dependency edges that touch a
      `bugshot-*` id. `bd ready` lists no `bugshot-*` ids. Paste the output
      into the close reason.
- [ ] The root cause (which rename or import created the mirrors) is
      recorded in one sentence in the close reason, or recorded as
      "unknown" with what was checked (for example
      `.beads/config.yaml` issue-prefix history and git log of
      `.beads/issues.jsonl`).

## Suggested approach

1. Export the mirror/twin mapping from the JSON above (key on title plus
   suffix; the suffixes match one to one).
2. For each mirror that has dependencies, run `bd dep list bugshot-XXX`,
   add the same edge on the `bgs-*` twin (`bd dep add`), then remove the
   mirror edge.
3. Close the 5 live mirrors with `bd close <id> --reason "duplicate of bgs-XXX ..."`.
   Background the bd writes if they are slow.
4. Decide delete versus annotate for the 50 closed mirrors. Deleting gives
   cleaner searches, and annotating keeps history. Either is acceptable if
   it is applied consistently.
5. Check `.beads/config.yaml` and `git log -p -- .beads/` to find when the
   `bugshot-` prefix entered, so the same import does not recur.

## Out of scope

- The shatter-agents `agents-*`/`sa-*` mirrors and stale `sa-d1b`/`bento-m4y5`
  issues (same finding, tracked in other repos' epics).
- A generic cross-prefix duplicate detector in bento's beads-issue-flow
  (a bento-side concern).
- Any work on the chat-agent increments themselves.

## Metadata

- Priority: P2
- Type: chore
- Labels: audit-2026-09-22, tracker, hygiene
- Parent epic: Epic: Audit 2026-09-22 findings (bugshot)
- Dependencies: none
