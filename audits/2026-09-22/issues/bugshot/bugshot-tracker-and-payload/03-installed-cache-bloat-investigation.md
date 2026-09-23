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
