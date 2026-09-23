---
slug: first-party-plugin-autoupdate
kind: new
title: "Installed shatter/refute plugins are 27 commits stale: enable autoUpdate for first-party marketplaces and add a staleness check"
priority: P2
type: bug
labels: [bug]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Installed shatter/refute plugins are 27 commits stale: enable autoUpdate for first-party marketplaces and add a staleness check

Part of #<epic>. Priority: P2. Type: bug.

## Problem

Every session runs the `shatter@shatterproof` and `refute@shatterproof` plugins at 0.1.1 from June. The source repo is at 0.1.12, 27 commits ahead. The `shatterproof` marketplace has not refreshed since 2026-06-19. It is the only first-party marketplace without `"autoUpdate": true`, and nothing warns when an installed first-party plugin falls behind its source.

## Evidence

Re-verified on 2026-09-23:

- `~/.claude/plugins/installed_plugins.json` lists `shatter@shatterproof` and `refute@shatterproof` at version `0.1.1`, sha `efa59682`, lastUpdated `2026-06-19T16:19Z`.
- In `~/.claude/plugins/known_marketplaces.json`, `shatterproof` has lastUpdated `2026-06-19T16:19:59Z` and no autoUpdate. By contrast:
  - `bento` has autoUpdate `true` and lastUpdated 2026-09-23.
  - `claude-plugins-official` and `claude-code-plugins` have no autoUpdate but refreshed on 2026-09-23.
  - `typesafe-ai` has no autoUpdate and refreshed on 2026-09-21.
- The marketplace clone `~/.claude/plugins/marketplaces/shatterproof` is at `efa5968` (2026-06-18).
- The remote is reachable: `git -C ~/.claude/plugins/marketplaces/shatterproof ls-remote origin HEAD` returns `119b8074…`, which matches `/home/ketan/project/shatter-agents` HEAD `119b807`. `git rev-list --count efa59682..HEAD` returns 27. A fetch failure is therefore unlikely, and the missing refresh is the probable cause.
- The source of the setting is `~/dotfiles/claude/hosts/pontoon/settings.json:36-41`, where the `shatterproof` entry under `extraKnownMarketplaces` has no `autoUpdate`. The `bento` entry at lines 29-35 has it. `claude/settings-sync.sh` renders the host overlay into `~/.claude/settings.json`, with the snapshot in `claude/settings.rendered.json`. The base `claude/settings.json` does not contain the entry.

## Acceptance criteria

- [ ] Every first-party marketplace in the host settings (`shatterproof` and any other non-Anthropic first-party entry) has `"autoUpdate": true`, and `claude/settings-sync.sh status` reports the render as current.
- [ ] Proof at close: in a fresh session after the change, `installed_plugins.json` shows the shatter and refute plugins at the `shatter-agents` HEAD sha and version current at that time. Paste the entry into the closing comment. If they do not refresh, diagnose the refresh failure and record it here before closing.
- [ ] A staleness check exists. For each enabled plugin whose marketplace repo is also checked out under `~/project/<repo>`, it compares the installed sha or version with the checkout's HEAD and `plugin.json`, and warns when the plugin is behind. Put it in either:
  - a dotfiles script run at SessionStart, with a test that seeds an older sha and asserts the warning; or
  - an issue filed against bento's agent-env doctor (bento-m4y5), linked from here.

## Suggested approach

Add `"autoUpdate": true` to the `shatterproof` block in `claude/hosts/pontoon/settings.json`, re-render with `claude/settings-sync.sh render`, then start a new session. The staleness check is the more robust half, because it also catches refresh failures.

## Out of scope

- Plugin content fixes. Current shatter-agents source still ships a `shatter-diff` skill for a command that does not exist. Under maintainer decision D2, the shatter-agents bucket withdraws that skill (`withdraw-shatter-diff-skill`). Refreshing the install before that lands exposes the skill for a while. The refresh should not wait for it.

## Dependencies

None blocking. Related: shatter-agents `withdraw-shatter-diff-skill`.

## Source

Shatter audit 2026-09-22 finding plugins-04.
