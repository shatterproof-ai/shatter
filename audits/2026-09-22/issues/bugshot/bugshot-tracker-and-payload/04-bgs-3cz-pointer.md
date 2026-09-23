---
slug: bgs-3cz-pointer
kind: note-to-existing
title: "Note on closed bgs-3cz: installed cache still 125 MB; see installed-cache-bloat-investigation (do not reopen yet)"
priority: P3
type: task
labels: [audit-2026-09-22, packaging]
parent_epic: "(existing closed issue; not reparented)"
blocked_by: [installed-cache-bloat-investigation]
existing_id: bgs-3cz
tracker: "bd in /home/ketan/project/bugshot (prefix bgs)"
---

# Note on closed bgs-3cz: pointer to the cache-bloat investigation

Target: **bgs-3cz** (CLOSED, P2, "Published plugin bundle is 124 MB; 122 MB is
node_modules", close reason "f8bf685e45993c7f793b747aeacd1e5bfaea8287 landed
on main"; it has no comments yet).

Action: `bd comments add bgs-3cz` with the text below. **Do not reopen and do
not change status or priority.** Filer: file `installed-cache-bloat-investigation`
first, then replace `<NEW-ID>` below with its id. Filing
`publish-staged-plugin-bundle` first is not required. The `blocked_by` in the
front-matter is an ordering constraint for the filer, not a bd edge on this
closed issue.

## Comment text

> Audit 2026-09-22 (Shatter audit finding plugins-18): the installed plugin cache
> `~/.claude/plugins/cache/bento/bugshot/1.0.20` is still 125 MB, with
> `node_modules/` at about 122 MB, plus development-only `.beads/`, `tests/` and
> `docs/plans/`. The bento
> marketplace entry still points at the repo root
> (`{"source":"github","repo":"ketang/bugshot"}`), and the f8bf685 staging
> mode (`scripts/build-plugin --bundle-dir`) is opt-in.
>
> This does **not** yet show that this fix was ineffective. `node_modules` is
> gitignored, so it cannot come from the git source. Its mtime equals the
> plugin's `lastUpdated` (2026-06-17 17:11:26Z). The recorded install
> `gitCommitSha` 4fb4d82 predates f8bf685, even though the cached files match
> HEAD e622d73. The mechanism is being traced in **<NEW-ID>**
> ("Investigate why the installed bugshot plugin cache is 125 MB ...").
> Any publication fix is owned by the follow-up issue that the investigation
> blocks ("Publish a slim bugshot plugin payload ..."). This issue stays
> closed, so the fix has a single open owner.

## Close proof

This is a note, and nothing closes it. It is complete when the comment is
present on bgs-3cz with the real new-issue id substituted.
