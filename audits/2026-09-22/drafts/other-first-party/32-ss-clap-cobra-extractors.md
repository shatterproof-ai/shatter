# inventory: add Rust clap and Go cobra CLI extractors; warn when a detected language has no extractor

## Filing metadata

- tracker/repo: storystore
- action: create new issue
- type: feature
- priority: P2
- labels: audit-2026-09-22
- parent: repo epic (see INDEX)
- dedupe relation: partially-covered (ss-yoa closed added only skill surfaces) -> new issue
- source findings: plugins-05

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

`python3 shared/inventory.py --repo-root <shatter checkout>` returns 2753 surfaces: `{test: 2664, heading: 73, skill: 15, bin: 1}`, with **no `cli-command`**, and `languages: {detected: [go, javascript, rust, typescript], extracted: [javascript, typescript]}`. The only CLI extractor is `_CLI_COMMAND_RE` (around `shared/inventory.py:139`), which matches commander.js `.command('name')`. Shatter's roughly 30 clap subcommands are invisible, so `stories-coverage`, whose headline surface kind is cli-command, silently reports nothing uncovered. Closed ss-yoa noted the extractors were TS/JS-only but added only skill surfaces.

## Current code facts

- Shatter defines subcommands in `shatter-cli/src/args.rs` as `#[derive(Subcommand)]` enum variants, with kebab-cased names and `#[command(name = "...")]` overrides.

## Acceptance criteria

- [ ] Rust clap extraction covers derive `Subcommand` variants (kebab-cased), `#[command(name=...)]` overrides and builder `Command::new("...")`, with fixtures.
- [ ] Go cobra (`Use: "name"`) extraction, with fixtures. stdlib `flag` is optional.
- [ ] `stories-coverage` output shows detected vs extracted languages and warns when a detected language has no extractor.
- [ ] On the shatter repo, the inventory lists its CLI subcommands (e.g. `explore`, `scan`, `spec-diff`, `list-targets`).

## Source

Shatter audit 2026-09-22 finding plugins-05.
