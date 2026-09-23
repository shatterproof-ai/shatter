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
