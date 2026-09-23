# Bundle: bugshot-tracker-and-payload

- Audit: Shatter audit 2026-09-22. These are final issue drafts, and none have been filed (D6).
- Bucket: `bugshot-tracker-and-payload`. Theme: Bugshot tracker and payload hygiene. There are fewer than 8 issues because buckets are single-repo and bugshot has only these findings.
- Repo: bugshot. Tracker: bd in /home/ketan/project/bugshot (prefix bgs).
- Parent epic: "Epic: Audit 2026-09-22 findings (bugshot)"
- Entries: 3 new issues (01, 03, 05) and 2 notes on existing issues (02 on bgs-3tq, 04 on closed bgs-3cz). Filing order constraint: 03 must be filed before 04, so that 04 can substitute its id.

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
| 03 | installed-cache-bloat-investigation | new | new | P3 | Investigate why the installed bugshot plugin cache is 125 MB (node_modules, .beads, tests) after the bgs-3cz fix |
| 04 | bgs-3cz-pointer | note-to-existing | bgs-3cz | P3 | Note on closed bgs-3cz: installed cache still 125 MB; see installed-cache-bloat-investigation (do not reopen yet) |
| 05 | agents-md-structure-and-readme | new | new | P3 | AGENTS.md covers only the bugshot skill; add viz/wire skills, shared modules and their sync rules, and a README |

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
> P2 because of a cross-repo dependency. Bd cannot express a dependency across
> repos, so priority did not carry over automatically.
>
> - Dependent: shatter **str-qwua7.53** "Wire bugshot for walkthrough output
>   once bugshot supports CLI capture (bgs-3tq)". It is P2 and OPEN, created
>   2026-09-07, and implements the 2026-09-06 maintainer decision to adopt
>   bugshot in shatter for walkthrough/gauntlet ANSI transcripts.
> - Until this issue lands, bento's agent-env doctor keeps reporting bugshot as
>   "dormant: capture-command missing" in every shatter session, and
>   str-qwua7.53 cannot start.
> - Verified 2026-09-23: `bd show bgs-3tq` shows P3 OPEN;
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
title: "Investigate why the installed bugshot plugin cache is 125 MB (node_modules, .beads, tests) after the bgs-3cz fix"
priority: P3
type: task
labels: [audit-2026-09-22, packaging, investigation]
parent_epic: "Epic: Audit 2026-09-22 findings (bugshot)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bugshot (prefix bgs)"
---

# Investigate why the installed bugshot plugin cache is 125 MB after the bgs-3cz fix

## Problem

bgs-3cz ("Published plugin bundle is 124 MB; 122 MB is node_modules", P2) was
closed on 2026-06-16 with the reason "f8bf685 landed on main". f8bf685 added an
opt-in `scripts/build-plugin --bundle-dir dist/bugshot` staging mode, a test
for it, and INSTALL.md text. The installed Claude plugin cache is still
125 MB, mostly `node_modules`. It also includes `.beads/`, `tests/` and
`docs/`, which no consumer needs.

It is not yet known which of these explains it:

- (a) the cache is stale,
- (b) something inside the cache ran `npm install`,
- (c) the published source (repo root) is inherently bloated, because the fix
  is opt-in and the marketplace does not point at a staged bundle.

This issue is an investigation. It must establish the mechanism before
anyone concludes that bgs-3cz is unfixed.

## Evidence

Re-verified 2026-09-23:

- Marketplace entry (`~/.claude/plugins/marketplaces/bento/.claude-plugin/marketplace.json:47-53`):
  `"source": {"source": "github", "repo": "ketang/bugshot"}`, which is the
  repo root and not a staged bundle.
- `du -sh ~/.claude/plugins/cache/bento/bugshot/1.0.20` gives `125M`.
  `node_modules/` is about 122 MB. Top-level entries also include `.beads/`
  (with `issues.jsonl`), `tests/`, `docs/`, `.claude/`, `.codex-plugin/`,
  `__pycache__/` and `.gitignore`. There is no `.git`.
- `node_modules` is listed in bugshot's `.gitignore` and is not tracked. A
  git-sourced install therefore cannot carry it, so it was produced after
  checkout.
- The cache's `node_modules` mtime is `2026-06-17 12:11:26 -0500`. This is the
  same instant as `installed_plugins.json` `bugshot@bento.lastUpdated`
  `2026-06-17T17:11:26.473Z`. **Lead:** node_modules was created by, or
  during, the plugin update itself (for example an install-time npm step, or
  a copy from a local directory that had node_modules), not by a later
  manual action.
- `installed_plugins.json` records `gitCommitSha 4fb4d82` (2026-04-26) and
  version `1.0.20`. `git merge-base --is-ancestor 4fb4d82 f8bf685` succeeds,
  so the recorded SHA predates the fix. **However**, excluding
  `node_modules/.beads/__pycache__/.in_use`, the cache's files are identical
  to bugshot HEAD `e622d73` (2026-06-17 09:35). `diff -rq` reports only
  local-only dirs in the primary checkout. The cache's `scripts/build-plugin`
  is byte-identical to the post-f8bf685 version. So the cache **contains** the
  fix code, and the recorded SHA looks like stale metadata. The version
  number was never bumped from 1.0.20 across the fix
  (`plugin-version.json` and `.claude-plugin/plugin.json` both say `1.0.20`).
- Conclusion so far: the fix is present but opt-in, and the marketplace still
  installs the repo root. This explains `.beads/`, `tests/` and `docs/`. It
  does not explain `node_modules`, which still needs tracing.

Source: Shatter audit 2026-09-22 finding plugins-18. The verifier rated the
original claim "partially confirmed" and asked for a trace before filing.

## Acceptance criteria

- [ ] The issue records the traced mechanism that put `node_modules` into
      the cache, with evidence: which process ran npm, or which source
      directory was copied. Reproduce by removing the cache and reinstalling
      (`claude plugin uninstall bugshot@bento && claude plugin install bugshot@bento`,
      or the current equivalent), then `du -sh` the new cache and check
      whether `node_modules/` appears and when.
- [ ] The issue records whether a fresh install at current HEAD still ships
      `.beads/`, `tests/` and `docs/`, with a `ls -a` listing.
- [ ] The issue records why `gitCommitSha` stayed at `4fb4d82` while the
      content matches `e622d73`.
- [ ] Then one of these outcomes, stated explicitly:
  - **Fix path:** publish a staged bundle (a `dist` branch, a release
    directory or a subdirectory source), point the bento marketplace entry at
    it, bump the plugin version so consumers refresh, and add a CI or test
    assertion that fails if the published payload contains `node_modules/`,
    `.beads/` or `tests/`, or exceeds a size budget (for example 5 MB).
    Close proof: a fresh-install `du -sh` below budget, plus the
    assertion's failing-then-passing run.
  - **No-fix path:** if node_modules comes from a local or one-off action
    that consumers never see, close with that explanation plus a fresh
    install's `du -sh`/`ls -a`. Record separately whether `.beads/`/`tests/`
    shipping is acceptable.
- [ ] If the fix path is taken, comment on bgs-3cz with the result. Reopen it
      only if the investigation shows its fix was ineffective.

## Suggested approach

1. Check Claude Code's plugin install behaviour for a `package.json` at the
   plugin root. Bugshot has `package.json` and `package-lock.json` at the
   root, and the install may run `npm install`/`npm ci`. The mtime match
   points this way.
2. Reinstall into a clean `CLAUDE_CONFIG_DIR`, or a temp HOME, to see what
   a consumer really gets.
3. If root `package.json` triggers the install, the staged bundle from
   `scripts/build-plugin --bundle-dir` (which should omit dev tooling) is the
   natural source to publish. Verify that it contains no `package.json`, or
   only a runtime-only one.
4. Check storystore's cache for the same pattern (`.beads`, tests, plan docs),
   and add a one-line note to the storystore epic if it matches.

## Out of scope

- Reopening bgs-3cz before the trace is done. A pointer comment on bgs-3cz is
  filed separately.
- Changing the gallery's frontend build toolchain.
- Storystore's packaging (note only).

## Metadata

- Priority: P3
- Type: task (investigation)
- Labels: audit-2026-09-22, packaging, investigation
- Parent epic: Epic: Audit 2026-09-22 findings (bugshot)
- Dependencies: none

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
first, then replace `<NEW-ID>` below with its id. The `blocked_by` in the
front-matter is an ordering constraint for the filer, not a bd edge on this
closed issue.

## Comment text

> Audit 2026-09-22 (Shatter audit finding plugins-18): the installed plugin cache
> `~/.claude/plugins/cache/bento/bugshot/1.0.20` is still 125 MB, with
> `node_modules/` at about 122 MB, plus `.beads/`, `tests/` and `docs/`. The bento
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
> Reopen this issue only if that investigation concludes the f8bf685 fix does
> not address the shipped payload.

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
- Shared modules are copied into skill dirs: `skills/{vizline,vizdiff}/` hold
  `capture_runner.py`, `image_diff.py` and `baseline_manifest.py`, and
  `skills/wire-bugshot/` holds `image_diff.py` and `baseline_manifest.py`.
  The same copies appear under `.codex-plugin/skills/`. AGENTS.md does not say
  these copies are generated, or by what (presumably `scripts/build-plugin`).
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
      They also include a rule to rebuild the generated copies after editing a
      shared module.
- [ ] A short `README.md` exists at the repo root: purpose, install (link to
      INSTALL.md), the four skills with one line each, and where to find
      AGENTS.md.
- [ ] Proof at close: a mechanical check, run and pasted into the close
      reason, showing that every `skills/*/SKILL.md` and every top-level
      `*.py` module is named in AGENTS.md. For example
      `for f in skills/*/ *.py; do grep -q "$(basename $f)" AGENTS.md || echo MISSING $f; done`
      printing nothing. Optionally add it as a pytest so future skills cannot
      be omitted.
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
