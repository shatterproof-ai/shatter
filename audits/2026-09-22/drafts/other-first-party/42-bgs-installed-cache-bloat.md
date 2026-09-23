# Investigate why the installed bugshot cache is 125 MB with node_modules despite the bgs-3cz fix

## Filing metadata

- tracker/repo: bugshot
- action: create new issue
- type: task
- priority: P3
- labels: audit-2026-09-22
- parent: repo epic (see INDEX)
- dedupe relation: duplicate-closed-but-unfixed (bgs-3cz) -> new investigation issue
- source findings: plugins-18

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

bgs-3cz ("Published plugin bundle is 124 MB; 122 MB is node_modules") was closed after f8bf685 added an opt-in `scripts/build-plugin --bundle-dir dist/bugshot`. The installed cache `~/.claude/plugins/cache/bento/bugshot/1.0.20` is still 125 MB (node_modules 122 MB, dir mtime 2026-06-17), plus `.beads/`, `tests/` and `docs/`. The bento marketplace entry is `{"source":"github","repo":"ketang/bugshot"}`, which is the repo root.

## Known facts that complicate the diagnosis

- `node_modules` is in bugshot's `.gitignore` and not tracked, so a git-sourced install cannot carry it. It was probably produced inside the cache by a local npm install or build.
- `installed_plugins.json` records gitCommitSha 4fb4d82 (2026-04-26), which predates f8bf685 (not an ancestor), while the version stays 1.0.20. The cache may simply be stale.

## Acceptance criteria

- [ ] Determine and record in this issue: how node_modules got into the cache, and whether a fresh install at HEAD still contains `.beads/`, `tests/`, `docs/`.
- [ ] Based on the result, either publish a staged bundle (a dist branch or release directory) and point the marketplace at it with a CI size/contents assertion, or close with the explanation and a version bump that refreshes consumers.

## Source

Shatter audit 2026-09-22 finding plugins-18.
