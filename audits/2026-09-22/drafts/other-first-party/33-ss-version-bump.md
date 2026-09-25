# Adopt automatic plugin version bumps; the installed cache is 128 commits behind

## Filing metadata

- tracker/repo: storystore
- action: create new issue
- type: chore
- priority: P2
- labels: audit-2026-09-22
- parent: repo epic (see INDEX)
- dedupe relation: new
- source findings: plugins-07

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

`plugin-version.json` has been 0.1.1 since 147af27 (2026-05-25), with 65 commits since. The bento marketplace lists storystore with version None and a github source, so an unchanged version keeps consumers on the old cache. Installed `storystore@bento` is 0.1.1 at sha ca16aef4 (2026-05-10); `git rev-list --count ca16aef4..HEAD` = 128. `diff -rq` against the cache shows audit.py, coverage.py, drift_todo.py, impact_check.py, inventory.py and list_candidates.py differ, and `shared/impact_trigger.py` is missing from the cache.

## Acceptance criteria

- [ ] `scripts/build-plugin` auto-bumps the patch version from a content hash of `shared/` and `skills/`, following the shatter-agents approach (sa-8xg).
- [ ] CI fails when `shared/` or `skills/` changed without a version change.
- [ ] The version is bumped now, and a fresh install picks up `impact_trigger.py`.

## Source

Shatter audit 2026-09-22 finding plugins-07.
