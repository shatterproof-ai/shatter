# Bundle: bugshot-tracker-and-payload

- Audit: Shatter audit 2026-09-22. These are final issue drafts, and none have been filed (D6).
- Bucket: `bugshot-tracker-and-payload`. Theme: Bugshot tracker and payload hygiene. There are fewer than 8 issues because buckets are single-repo and bugshot has only these findings.
- Repo: bugshot. Tracker: bd in /home/ketan/project/bugshot (prefix bgs).
- Parent epic: "Epic: Audit 2026-09-22 findings (bugshot)"
- Entries: 4 new issues (01, 03, 05, 06) and 2 notes on existing issues (02 on bgs-3tq, 04 on closed bgs-3cz). Filing order constraints: 03 must be filed before 04, so that 04 can substitute its id. 03 must also be filed before 06, and a bd edge must record that 06 is blocked by 03. Revised 2026-09-23 after the Codex cross-check (see REVISION.md).

## Maintainer decisions (2026-09-23)

These override the report and the old drafts. None changes this bucket directly; they are listed for the cross-runtime reviewer.

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix. Fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64); do not drop them. Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire snapshot `shatter diff` and the unused Snapshot writer; spec-diff is the regression tool. Update SPEC/README/QUICKSTART. The `diff` name becomes free, and str-81xiw decides what uses it. Correct the shatter-agents `shatter diff --staged` docs.
- **D3 Concolic positioning:** measure first. P1 controlled default-vs-concolic benchmark; P1 fix for concolic early termination; a follow-up decision issue, blocked by both, re-decides the "concolic-first" positioning. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import in shatter and move tracker sync to a Dolt remote. The first step checks whether the stale JSONL import has been clobbering newer DB state. AGENTS.md drops `bd sync`, str-qwua7.28 is superseded, and bento beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT env var or hook-bypass guidance.
- **D5 Git identity:** the leaked `[user]` section was already removed. Add `.mailmap` (test@example.com "Test"/"Test User" -> Ketan Gangatirkar <33678+ketang@users.noreply.github.com>, no history rewrite), a git-state check (local identity override, example.com email, core.bare=true, hooksPath override; folded into str-qwua7.1), and a fixture `.git/config` snapshot in test_git_fixture_isolation.py.
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. Agents file nothing.

## Contents

| # | Slug | Kind | Target | Priority | Title |
|---|---|---|---|---|---|
| 01 | close-bugshot-mirror-duplicates | new | new | P2 | Close the 55 bugshot-* mirror duplicates of bgs-* issues and rewire their dependencies |
| 02 | bgs-3tq-raise-to-p2 | note-to-existing | bgs-3tq | P2 | Note on bgs-3tq: raise to P2 because shatter str-qwua7.53 (P2) depends on it |
| 03 | installed-cache-bloat-investigation | new | new | P3 | Investigate why the installed bugshot plugin cache is 125 MB (node_modules, .beads, tests) after the bgs-3cz fix, and decide the publication route |
| 04 | bgs-3cz-pointer | note-to-existing | bgs-3cz | P3 | Note on closed bgs-3cz: installed cache still 125 MB; see installed-cache-bloat-investigation (do not reopen yet) |
| 05 | agents-md-structure-and-readme | new | new | P3 | AGENTS.md covers only the bugshot skill; add viz/wire skills, shared modules and their sync rules, and a README |
| 06 | publish-staged-plugin-bundle | new | new | P3 | Publish a slim bugshot plugin payload and prove it with a fresh-install smoke test |

<!-- file: 01-close-bugshot-mirror-duplicates.md -->
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

---

<!-- file: 02-bgs-3tq-raise-to-p2.md -->
---
slug: bgs-3tq-raise-to-p2
kind: note-to-existing
title: "Note on bgs-3tq: raise to P2 because shatter str-qwua7.53 (P2) depends on it"
priority: P2
type: feature
labels: [audit-2026-09-22, cross-repo]
parent_epic: "(existing issue; not reparented)"
blocked_by: []
existing_id: bgs-3tq
tracker: "bd in /home/ketan/project/bugshot (prefix bgs)"
---

# Note on bgs-3tq: raise to P2

Target: **bgs-3tq** (open, P3, feature, "Document and template a CLI/TUI
capture-command (`wire-bugshot --kind cli`)", created and last updated
2026-09-05).

Actions:

1. `bd update bgs-3tq --priority 2`
2. `bd comments add bgs-3tq` with the text below.

Do not change its scope or reparent it under the audit epic.

## Comment text

> Audit 2026-09-22 (Shatter audit finding plugins-08): raising this from P3 to
> P2 because of a cross-repo dependency. The two issues have no dependency
> edge between them. Direct issue-ID dependencies do not work across separate
> bd databases. `bd dep add` does support `external:<project>:<capability>`
> references, but `external_projects` is not configured here
> (`bd config get external_projects` returns "not set"). Even if such an edge
> existed, it would not change this issue's priority. So the priority is
> raised by hand.
>
> - Dependent: shatter **str-qwua7.53** "Wire bugshot for walkthrough output
>   once bugshot supports CLI capture (bgs-3tq)". It is P2 and OPEN, created
>   2026-09-07, and implements the 2026-09-06 maintainer decision to adopt
>   bugshot in shatter for walkthrough/gauntlet ANSI transcripts.
> - Until this issue lands, bento's agent-env doctor keeps reporting bugshot as
>   "dormant: capture-command missing" in every shatter session, and
>   str-qwua7.53 cannot start.
> - Verified 2026-09-23 (live bd): `bd show bgs-3tq` shows P3 OPEN;
>   `bd show str-qwua7.53` (in /home/ketan/project/shatter) shows P2 OPEN.
>
> When this closes, comment on str-qwua7.53 in the shatter tracker so the
> dependent can be unblocked. The related AGENTS.md/README gap for the viz
> skills is tracked separately in the audit issue
> "AGENTS.md covers only the bugshot skill ..." and leaves ANSI capture
> documentation here.

## Close proof

This is a note, and nothing closes it. It is complete when `bd show bgs-3tq`
reports P2 and the comment above is present.

---

<!-- file: 03-installed-cache-bloat-investigation.md -->
---
slug: installed-cache-bloat-investigation
kind: new
title: "Investigate why the installed bugshot plugin cache is 125 MB (node_modules, .beads, tests) after the bgs-3cz fix, and decide the publication route"
priority: P3
type: task
labels: [audit-2026-09-22, packaging, investigation]
parent_epic: "Epic: Audit 2026-09-22 findings (bugshot)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bugshot (prefix bgs)"
---

# Investigate why the installed bugshot plugin cache is 125 MB after the bgs-3cz fix, and decide the publication route

## Problem

bgs-3cz ("Published plugin bundle is 124 MB; 122 MB is node_modules", P2) was
closed on 2026-06-16 with the reason "f8bf685 landed on main". f8bf685 added
three things: an opt-in `scripts/build-plugin --bundle-dir dist/bugshot`
staging mode, a staging test, and INSTALL.md text. The installed Claude plugin
cache is still 125 MB, and most of that is `node_modules`. The cache also
contains development-only content that the staged bundle deliberately leaves
out: `.beads/`, `tests/`, `docs/plans/`, `__pycache__/` and `package.json`.
(`docs/specs/` is *meant* to ship. `BUNDLE_PATHS` in `scripts/build-plugin`
includes it, and INSTALL.md describes public specs as shipped content.)

It is not yet known which of these explains the size:

- (a) the cache is stale,
- (b) something at install time ran `npm install`/`npm ci` because
  `package.json` sits at the plugin root,
- (c) the installer copied a local working tree that already had
  `node_modules/`. The primary checkout `/home/ketan/project/bugshot` has one.
- (d) the published source (the repo root) is bloated by design, because the
  fix is opt-in and the marketplace does not point at a staged bundle.

**This issue covers investigation and a decision only.** It traces the
mechanism, establishes what a *fresh consumer install* gets today, and records
which publication route to use. The implementation is tracked by the
follow-up issue `publish-staged-plugin-bundle`, which this issue blocks.

## Evidence

Re-verified 2026-09-23:

- The marketplace entry
  (`~/.claude/plugins/marketplaces/bento/.claude-plugin/marketplace.json:47-53`)
  is `"source": {"source": "github", "repo": "ketang/bugshot"}`. That points at
  the repo root, not at a staged bundle.
- `du -sh ~/.claude/plugins/cache/bento/bugshot/1.0.20` reports `125M`.
  `node_modules/` accounts for about 122 MB. Other top-level entries are
  `.beads/` (with `issues.jsonl`), `tests/`, `docs/`, `.claude/`,
  `.codex-plugin/`, `__pycache__/` and `.gitignore`. There is no `.git`.
- `node_modules` is listed in bugshot's `.gitignore` (line 7) and is not
  tracked. A clean git checkout cannot contain it, so it was produced or
  copied some other way.
- The cache's `node_modules` mtime is `2026-06-17 12:11:26 -0500`. That is the
  same instant as `bugshot@bento.lastUpdated` `2026-06-17T17:11:26.473Z` in
  `installed_plugins.json`. **Lead:** node_modules was created during the
  plugin update, either by an install-time npm step or by a copy from a
  directory that had node_modules. It was not added by a later manual action.
- `installed_plugins.json` records `gitCommitSha 4fb4d82` (2026-04-26) and
  version `1.0.20`. That SHA predates the fix:
  `git merge-base --is-ancestor 4fb4d82 f8bf685` succeeds. However, with
  `node_modules/.beads/__pycache__/.in_use` excluded, the cache's files match
  bugshot HEAD `e622d73` (2026-06-17 09:35), and the cache's
  `scripts/build-plugin` is byte-identical to the post-f8bf685 version. So the
  cache contains the fix code, and the recorded SHA looks like stale metadata.
  The version was never bumped across the fix: `plugin-version.json` and
  `.claude-plugin/plugin.json` both still say `1.0.20`.
- An existing test,
  `tests/test_build_plugin.py::test_bundle_dir_stages_slim_plugin_payload`
  (line 294), already asserts that the staged bundle leaves out
  `node_modules`, `.beads`, `tests` and `package.json`. Staging is therefore
  covered. The publication and install route is not.

Source: Shatter audit 2026-09-22 finding plugins-18. The verifier rated the
original claim "partially confirmed" and asked for a trace before filing.

## Acceptance criteria

- [ ] **Evidence preserved first.** Before running any reinstall, copy
      `~/.claude/plugins/installed_plugins.json` and a metadata snapshot of the
      current cache into the issue's scratch area. The snapshot is
      `find ~/.claude/plugins/cache/bento/bugshot -maxdepth 2 -printf '%TY-%Tm-%Td %TT %s %p\n'`
      plus `du -sh`. Do **not** uninstall, reinstall or modify the operator's
      live plugin install or cache.
- [ ] **Isolated fresh install.** Install `bugshot@bento` into a throwaway
      config/home (a temp `HOME` or `CLAUDE_CONFIG_DIR`) at current bugshot
      HEAD. Record the exact commands. Record `du -sh`, `ls -a` of the new
      cache root, whether `node_modules/` appears, and its mtime compared with
      the install time. Also record the recorded `gitCommitSha` and whether it
      matches the fetched content.
- [ ] **Mechanism verdict for today's route.** State which of (a) to (d)
      explains a fresh install's contents, and cite the output that shows it:
      the npm invocation seen in the install logs or strace, the copy source,
      or the file list. If node_modules does not appear in a fresh install,
      say so explicitly.
- [ ] **Historical verdict for the June install.** Explain why the live cache
      got `node_modules` and a stale `gitCommitSha`. If the cause cannot be
      reconstructed, the outcome "historical cause undetermined" is
      acceptable, but list what was checked (install logs, Claude Code version
      history, the marketplace clone state, and the local-checkout-copy
      hypothesis).
- [ ] **Decision recorded.** Pick one publication route and give the reason:
      - a staged-bundle source, such as a `dist` branch, a release directory,
        a subdirectory `source`, or a separate repo;
      - removing or moving the root `package.json` so installs do not trigger
        npm;
      - no change, because consumers do not get the bloat.

      If a change is needed, name each follow-up deliverable and its owning
      repo: bugshot for the publication artifact and the install smoke test;
      bento for the marketplace `source` entry. Add the decision and the
      evidence as a comment on the follow-up `publish-staged-plugin-bundle`.
      If the decision is "no change", close that follow-up as not needed and
      link this issue.
- [ ] **bgs-3cz is reconciled.** Comment on bgs-3cz with the verdict and a
      link to the follow-up. Do not reopen bgs-3cz. The implementation is owned
      by the follow-up issue, so there is exactly one open owner.

## Suggested approach

1. Snapshot the evidence (AC 1).
2. Check what Claude Code's plugin installer does when `package.json` and
   `package-lock.json` sit at the plugin root. The mtime match points toward
   an npm step at install time.
3. Run the isolated install (AC 2), and watch for npm processes, for example
   with `strace -f -e execve`, or with a `PATH` shim that logs `npm`
   invocations.
4. Diff the isolated cache against `scripts/build-plugin --bundle-dir` output
   to see exactly what a staged source would remove.
5. Optionally, check storystore's cache for the same pattern (`.beads`,
   tests, plan docs). If it matches, add a one-line note to the storystore
   epic.

## Out of scope

- Implementing the publication change, bumping the version, or editing the
  bento marketplace. Those belong to `publish-staged-plugin-bundle` and the
  bento follow-up it files.
- Changing the gallery's frontend build toolchain.
- Storystore's packaging (a note only).

## Metadata

- Priority: P3
- Type: task (investigation + decision)
- Labels: audit-2026-09-22, packaging, investigation
- Parent epic: Epic: Audit 2026-09-22 findings (bugshot)
- Dependencies: none. Blocks `publish-staged-plugin-bundle`.

---

<!-- file: 04-bgs-3cz-pointer.md -->
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

---

<!-- file: 05-agents-md-structure-and-readme.md -->
---
slug: agents-md-structure-and-readme
kind: new
title: "AGENTS.md covers only the bugshot skill; add viz/wire skills, shared modules and their sync rules, and a README"
priority: P3
type: chore
labels: [audit-2026-09-22, docs]
parent_epic: "Epic: Audit 2026-09-22 findings (bugshot)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bugshot (prefix bgs)"
---

# AGENTS.md covers only the bugshot skill; add viz/wire skills, shared modules and their sync rules, and a README

## Problem

Bugshot ships four skills: `bugshot`, `vizline`, `vizdiff` and `wire-bugshot`.
The repo's `AGENTS.md` was written when only `bugshot` existed, and the
additions since then were never documented:

- **Project Structure** omits the viz/wire entry points and workflows, and the
  shared modules they use.
- **Documentation Sync Rules** cover only `skills/bugshot/SKILL.md`. Nothing
  tells an agent that changing `capture_runner.py`, `image_diff.py`,
  `baseline_manifest.py` or the `wire_bugshot_*` flags requires updating the
  viz/wire `SKILL.md` files, or rebuilding the per-skill copies of those modules.
- The repo has no `README.md`, so a human visiting the GitHub repo sees no
  description, install steps or skill list. INSTALL.md exists.

An agent editing viz code gets no signal that docs must move with it. Doc
drift of exactly this kind (ANSI support that exists but is not documented)
is what bgs-3tq is fixing on the CLI capture side.

## Evidence

Re-verified 2026-09-23 against `/home/ketan/project/bugshot` HEAD `e622d73`:

- `AGENTS.md:37-50` "Project Structure" lists `bugshot_cli.py`,
  `bugshot_workflow.py`, `gallery_server.py`, `ansi_render.py`,
  `static/`/`templates/`, `skills/bugshot/SKILL.md`, `skills/bugshot/overlays/`,
  generated `.claude/skills/`/`.codex-plugin/skills/`, one spec doc and `tests/`.
- It does **not** list any of these, which exist at the repo root:
  `vizline_cli.py`, `vizline_workflow.py`, `vizdiff_cli.py`,
  `vizdiff_workflow.py`, `vizdiff_review_root.py`, `wire_bugshot_cli.py`,
  `wire_bugshot_workflow.py`, `capture_runner.py`, `image_diff.py`,
  `baseline_manifest.py`, or the skill dirs `skills/vizline/`,
  `skills/vizdiff/` and `skills/wire-bugshot/`.
- Shared modules are copied into skill dirs, and the copies are git-tracked:
  - `skills/vizline/` holds `capture_runner.py`, `image_diff.py` and
    `baseline_manifest.py`.
  - `skills/wire-bugshot/` holds `image_diff.py` and `baseline_manifest.py`.
  - `skills/vizdiff/` holds those three modules. It also holds copies of
    bugshot core modules: `ansi_render.py`, `bugshot_workflow.py`,
    `gallery_server.py`, `vizline_workflow.py`, `select-bind-address`,
    `static/` and `templates/`.
  - The same copies appear under `.codex-plugin/skills/`.

  AGENTS.md does not say that these copies are generated, or by what
  (presumably `scripts/build-plugin`). It also does not say that they must be
  committed after a rebuild.
- `AGENTS.md:72-83` "Documentation Sync Rules" has five bullets, all
  targeting `skills/bugshot/SKILL.md` or the bugshot overlays.
- `ls /home/ketan/project/bugshot/README*` returns "No such file or directory".

Source: Shatter audit 2026-09-22 finding plugins-20.

## Acceptance criteria

- [ ] Project Structure lists all four skills (`skills/<name>/SKILL.md`),
      the viz/wire CLI and workflow modules, and the shared modules
      `capture_runner.py`, `image_diff.py` and `baseline_manifest.py`, with
      one line each. It states which copies under `skills/*/` and
      `.codex-plugin/skills/` are generated, and by which command.
- [ ] Documentation Sync Rules map each shared module or flag surface to the
      SKILL.md files it affects. At minimum:
      `capture_runner.py`/`wire_bugshot_*` flags -> `skills/wire-bugshot/SKILL.md`
      (and vizline);
      `image_diff.py` recognized extensions/thresholds -> `vizdiff` and
      `vizline` SKILL.md;
      `baseline_manifest.py` schema -> `vizline`/`vizdiff` SKILL.md.
      `ansi_render.py`, `bugshot_workflow.py`, `gallery_server.py`,
      `static/` and `templates/` -> the bugshot SKILL.md plus the vizdiff
      copies. They also include a rule to rebuild **and commit** the
      generated copies after editing a shared module.
- [ ] A short `README.md` exists at the repo root: purpose, install (link to
      INSTALL.md), the four skills with one line each, and where to find
      AGENTS.md.
- [ ] Proof at close: add a pytest, for example
      `tests/test_agents_md_coverage.py`. It fails when AGENTS.md is missing
      the exact relative path `skills/<name>/SKILL.md` for any directory under
      `skills/`, or the exact filename of any top-level `*.py` module. Match
      whole literal strings, not basenames: `vizdiff` already appears in
      AGENTS.md outside a skill entry, so a basename match would pass without
      the fix. In shell terms, the check is equivalent to
      `for d in skills/*/; do grep -qF "${d}SKILL.md" AGENTS.md || { echo MISSING ${d}SKILL.md; fail=1; }; done; exit ${fail:-0}`.
      Paste the test failing on the pre-change AGENTS.md (it should list at
      least `skills/vizline/SKILL.md`, `skills/vizdiff/SKILL.md`,
      `skills/wire-bugshot/SKILL.md` and `vizline_cli.py`) and passing after
      the change.
- [ ] If the published plugin payload (see the cache-bloat investigation)
      should not ship README/AGENTS.md, confirm that `scripts/build-plugin`
      excludes them. Otherwise nothing is needed.

## Suggested approach

Read `scripts/build-plugin` to confirm which files it copies into each skill
dir. Then extend the two AGENTS.md sections and write a README of about 30
lines. Keep the RTK block (`<!-- headroom:rtk-instructions -->`) untouched,
because it is tool-managed.

## Out of scope

- Documenting ANSI (`.ansi`) capture support and the `wire-bugshot --kind cli`
  template. That belongs to **bgs-3tq**; this issue only makes sure
  AGENTS.md's structure and sync rules cover the modules involved.
- Any behavioural change to the skills or modules.

## Metadata

- Priority: P3
- Type: chore
- Labels: audit-2026-09-22, docs
- Parent epic: Epic: Audit 2026-09-22 findings (bugshot)
- Dependencies: none (related: bgs-3tq)

---

<!-- file: 06-publish-staged-plugin-bundle.md -->
---
slug: publish-staged-plugin-bundle
kind: new
title: "Publish a slim bugshot plugin payload and prove it with a fresh-install smoke test"
priority: P3
type: task
labels: [audit-2026-09-22, packaging]
parent_epic: "Epic: Audit 2026-09-22 findings (bugshot)"
blocked_by: [installed-cache-bloat-investigation]
existing_id: ""
tracker: "bd in /home/ketan/project/bugshot (prefix bgs)"
---

# Publish a slim bugshot plugin payload and prove it with a fresh-install smoke test

## Problem

The bento marketplace installs bugshot from the repo root
(`{"source":"github","repo":"ketang/bugshot"}`). The installed cache is 125 MB,
mostly `node_modules/`, and it also carries `.beads/`, `tests/`,
`docs/plans/` and `__pycache__/`. bgs-3cz added a slim staging mode
(`scripts/build-plugin --bundle-dir`), but nothing publishes that bundle.
Consumers still get the repo root.

This issue implements the publication route chosen by
`installed-cache-bloat-investigation`, which blocks it. **If that
investigation decides "no change", close this issue as not needed and link
the investigation's verdict.**

## Evidence

- See `installed-cache-bloat-investigation` for the trace and the decision.
- `tests/test_build_plugin.py::test_bundle_dir_stages_slim_plugin_payload`
  (line 294) already covers the *staging* step. A test for the payload must not
  duplicate it. It must exercise the publish and install route.
- `plugin-version.json` and `.claude-plugin/plugin.json` are both still
  `1.0.20`. The fix never bumped them, so existing consumers were never
  prompted to refresh.

Source: Shatter audit 2026-09-22 finding plugins-18 (the implementation half).

## Acceptance criteria

- [ ] The route chosen in the investigation is implemented in bugshot: the
      publication artifact (branch, directory or subdirectory source) is
      produced from `scripts/build-plugin --bundle-dir` or its successor. It
      contains the runtime files and `docs/specs/`, and it does not contain
      `node_modules/`, `.beads/`, `tests/`, `docs/plans/`, `__pycache__/` or a
      root `package.json` that triggers npm at install time.
- [ ] The plugin version is bumped in `plugin-version.json` and
      `.claude-plugin/plugin.json`, so existing installs refresh.
- [ ] **Install-route smoke test.** Add a scripted check to bugshot's test or
      CI surface. It installs the plugin *through the published route* into a
      throwaway home or config dir (not the staging step alone), then asserts:
      1. the cache holds none of the excluded paths;
      2. the cache is under a stated size budget (proposed: 5 MB; the
         investigation may revise it);
      3. the installed plugin works: each of the four skills' `SKILL.md` is
         present, and at least one installed entry point (for example
         `bugshot_cli.py --help` from the cache) exits 0.
- [ ] **Red to green proof.** Run the smoke test against the current
      repo-root route and paste the failing output. Then run it against the
      new route and paste the passing output. Include the fresh-install
      `du -sh` and `ls -a` in the close reason.
- [ ] **The bento marketplace change has an owner.** If the route requires
      changing the bento marketplace `source` entry, file a linked bento issue
      and cite its id in the close reason. The bento change itself is not
      implemented here. Close this issue only after that bento issue has
      landed, and after a fresh `bugshot@bento` install from the marketplace
      passes the smoke test.
- [ ] Comment on bgs-3cz with the result. It stays closed; this issue is the
      single owner of the fix.

## Out of scope

- Re-diagnosing the mechanism (that belongs to the investigation).
- Changing the gallery's frontend build toolchain.
- Storystore packaging.

## Metadata

- Priority: P3
- Type: task
- Labels: audit-2026-09-22, packaging
- Parent epic: Epic: Audit 2026-09-22 findings (bugshot)
- Dependencies: blocked by `installed-cache-bloat-investigation`
