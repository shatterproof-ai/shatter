# Enable plugin autoUpdate for first-party marketplaces and detect stale installed plugins

## Filing metadata

- tracker/repo: dotfiles
- action: create new issue
- type: bug
- priority: P2
- labels: bug
- parent: repo epic (see INDEX)
- dedupe relation: new
- source findings: plugins-04

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

The installed `shatter@shatterproof` and `refute@shatterproof` plugins are 0.1.1 (gitCommitSha `efa59682`, 2026-06-18). The source checkout `/home/ketan/project/shatter-agents` is at plugin.json 0.1.12, which is 27 commits ahead (`git rev-list --count efa59682..HEAD` = 27). Sessions therefore run old skills and do not have `compose-shatter-recipe`/`shatter-diff` at all. In `~/.claude/plugins/known_marketplaces.json`, the `shatterproof` entry has `lastUpdated` 2026-06-19 and no `autoUpdate`, while `bento` has `"autoUpdate": true`.

## Current code facts

- The `shatterproof` marketplace entry in `~/.claude/settings.json` comes from `/home/ketan/dotfiles/claude/hosts/pontoon/settings.json` (and the rendered `settings.rendered.json`), not from `dotfiles/claude/settings.json`.
- Caveat: `typesafe-ai` also lacks autoUpdate yet refreshed on 2026-09-21. A missing autoUpdate may therefore not be the whole cause, and a fetch failure for `shatterproof-ai/agents` has not been ruled out.

## Acceptance criteria

- [ ] Every first-party marketplace (shatterproof and any others in the host settings) has `"autoUpdate": true` in the dotfiles host settings template, and the rendered settings include it.
- [ ] After the change, a fresh session installs shatter/refute plugins at the current source version (verify with `installed_plugins.json`).
- [ ] If the refresh still fails, the fetch error is diagnosed and recorded in this issue.
- [ ] A staleness check exists. Either a dotfiles script, or a filed bento issue for an agent-env-doctor check that compares the installed version/commit with a `~/project/<repo>` checkout and warns when it is behind.

## Out of scope

Plugin content fixes.

## Source

Shatter audit 2026-09-22 finding plugins-04.
