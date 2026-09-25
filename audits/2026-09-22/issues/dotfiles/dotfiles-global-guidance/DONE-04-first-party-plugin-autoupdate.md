<!-- DONE 2026-09-24: landed directly in dotfiles as 8764d01e (+ 89468c43 overlay adopt) and rendered live; not filed. -->
---
slug: first-party-plugin-autoupdate
kind: new
title: "Installed shatter/refute plugins are 27 commits stale: enable autoUpdate for the shatterproof marketplace"
priority: P2
type: bug
labels: [bug]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Installed shatter/refute plugins are 27 commits stale: enable autoUpdate for the shatterproof marketplace

Part of #<epic>. Priority: P2. Type: bug. The staleness checker that was part of this draft is split out as `first-party-plugin-staleness-check`. Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic.

## Problem

Every session runs the `shatter@shatterproof` and `refute@shatterproof` plugins at version 0.1.1 from June. The source repo is 27 commits ahead; it declares shatter `0.1.12` and refute `0.1.2`. The `shatterproof` marketplace has not refreshed since 2026-06-19. It is the only first-party marketplace without `"autoUpdate": true`.

## Evidence

Re-verified on 2026-09-23:

- `~/.claude/plugins/installed_plugins.json` lists both `shatter@shatterproof` and `refute@shatterproof` at version `0.1.1`, gitCommitSha `efa59682…`, lastUpdated `2026-06-19T16:19Z`.
- `/home/ketan/project/shatter-agents` @ `119b807`: `.claude-plugin/marketplace.json` and the plugins' `plugin.json` files declare shatter `0.1.12` and refute `0.1.2`.
- In `~/.claude/plugins/known_marketplaces.json`, `shatterproof` has lastUpdated `2026-06-19T16:19:59Z` and no autoUpdate. By contrast:
  - `bento` has autoUpdate `true` and lastUpdated 2026-09-23.
  - `claude-plugins-official` and `claude-code-plugins` have no autoUpdate but refreshed on 2026-09-23.
  - `typesafe-ai` has no autoUpdate and refreshed on 2026-09-21.
- The marketplace clone `~/.claude/plugins/marketplaces/shatterproof` is at `efa5968` (2026-06-18).
- The remote is reachable: `git -C ~/.claude/plugins/marketplaces/shatterproof ls-remote origin HEAD` returns `119b8074…`, matching shatter-agents HEAD. `git rev-list --count efa59682..HEAD` returns 27. A fetch failure is therefore unlikely, and the missing refresh is the probable cause.
- The setting's source is `~/dotfiles/claude/hosts/pontoon/settings.json:36-41`, where the `shatterproof` entry under `extraKnownMarketplaces` has no `autoUpdate`. The `bento` entry at lines 29-35 has it. `claude/settings-sync.sh` renders the host overlay into `~/.claude/settings.json`, with the snapshot in `claude/settings.rendered.json`.

## Acceptance criteria

- [ ] The `shatterproof` entry in `claude/hosts/pontoon/settings.json` has `"autoUpdate": true` (and in any other host overlay that declares it). Other marketplaces are unchanged; whether `typesafe-ai` should auto-update is left to the maintainer.
- [ ] `claude/settings-sync.sh status` reports the render as current, and `~/.claude/settings.json` contains `autoUpdate: true` for `shatterproof`.
- [ ] In a fresh session after the change, `installed_plugins.json` shows `shatter@shatterproof` at the version in shatter-agents' `plugins/claude/shatter/.claude-plugin/plugin.json` and `refute@shatterproof` at the version in `plugins/claude/refute/.claude-plugin/plugin.json`, both with the shatter-agents HEAD sha of that day. If they do not refresh, the closing comment records the diagnosis (for example, the marketplace fetch output) and the issue stays open.

## Proof at close

Paste both `installed_plugins.json` entries and the two `plugin.json` versions into the closing comment.

## Out of scope

- The staleness warning (`first-party-plugin-staleness-check`).
- Plugin content fixes. Current shatter-agents source still ships a `shatter-diff` skill for a command that does not exist. Under maintainer decision D2, the shatter-agents bucket withdraws that skill (`withdraw-shatter-diff-skill`). Refreshing before that lands exposes the skill for a while. The refresh should not wait for it.

## Maintainer decisions that apply

- D2: `shatter diff` is retired; the shatter-diff skill is withdrawn in shatter-agents.
- D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Dependencies

None blocking. Related: `first-party-plugin-staleness-check`, shatter-agents `withdraw-shatter-diff-skill`.

## Source

Shatter audit 2026-09-22 finding plugins-04.
