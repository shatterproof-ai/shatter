---
slug: first-party-plugin-staleness-check
kind: new
title: "Warn at SessionStart when an installed first-party plugin is behind its local source checkout"
priority: P2
type: enhancement
labels: [enhancement]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Warn at SessionStart when an installed first-party plugin is behind its local source checkout

Part of #<epic>. Priority: P2. Type: enhancement. The missing `autoUpdate` setting itself was fixed directly on 2026-09-24 (dotfiles commit 8764d01e: `autoUpdate: true` on the `shatterproof` marketplace in `claude/hosts/pontoon/settings.json`). This issue adds the check that catches the next failure of any kind, including a refresh that silently fails. Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic.

## Problem

The shatter and refute plugins ran 27 commits and three months stale (0.1.1 installed; source declares shatter 0.1.12 and refute 0.1.2) and nothing warned. `autoUpdate` fixes one cause. It does not detect a refresh that fails, a marketplace that is removed from the settings, or a version that is never bumped.

## Evidence

Installed vs source versions (re-verified 2026-09-23): the `shatterproof` marketplace clone at `~/.claude/plugins/marketplaces/shatterproof` was at `efa5968` (2026-06-18), 27 commits behind shatter-agents HEAD; `known_marketplaces.json` showed its last refresh on 2026-06-19. No script in dotfiles @ `81f35e1` compares installed plugins with their sources. bento's agent-env doctor (bento-m4y5, `in_progress`, P1, checked with `bd show` on 2026-09-23) is about broken agent wiring, not plugin freshness.

## Acceptance criteria

- [ ] A dotfiles script, for example `claude/plugin_staleness.py`, is registered as a SessionStart hook with a `$HOME/dotfiles/...` path. For each enabled plugin in `~/.claude/plugins/installed_plugins.json` whose marketplace repo is also checked out under `~/project/<repo>`, it compares the installed `gitCommitSha` and `version` with the checkout's `HEAD` and that plugin's `plugin.json` version. It prints one warning line per plugin that is behind, naming the plugin, both versions and the commit count.
- [ ] It prints nothing and exits 0 when every plugin is current, when no local checkout exists, or when a file is missing. It must never block a session.
- [ ] The mapping from marketplace to local checkout is explicit (a small table in the script, or the marketplace's `source` URL matched against checkout remotes), not guessed from names.
- [ ] Test `claude/tests/test_plugin_staleness.py`, run with `python3 -m pytest claude/tests/test_plugin_staleness.py -q`, uses a temp `HOME` with a fixture `installed_plugins.json` and a temp git repo as the checkout:
  - an installed sha behind `HEAD` produces the warning;
  - an installed sha equal to `HEAD` produces no output;
  - a missing checkout produces no output and exit 0.

## Proof at close

The closing comment includes the pytest output, and the script's output against the real `~/.claude` before `first-party-plugin-autoupdate` lands (a warning) or, if it has already landed, after seeding a fixture (a warning) and against the real state (silent).

## Out of scope

- The `autoUpdate` setting (already enabled, dotfiles 8764d01e).
- Extending bento's agent-env doctor (bento-m4y5). It could adopt this check later.

## Maintainer decisions that apply

D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Dependencies

None blocking. Related: dotfiles 8764d01e (autoUpdate enabled), `hooks-dotfiles-env-unset` (use `$HOME/dotfiles` paths).

## Source

Shatter audit 2026-09-22 finding plugins-04. Split out during the Codex cross-check of 2026-09-23.
