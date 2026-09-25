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
