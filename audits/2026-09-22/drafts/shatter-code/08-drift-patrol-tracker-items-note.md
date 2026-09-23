# NOTE on str-qwua7.17: drift-patrol tracker-hygiene is red again (2 stale claims, 4 orphans)

| field | value |
|---|---|
| action | **append comment to existing issue `str-qwua7.17`** (no new issue) |
| suggested priority for target | P3 |
| source findings | gates-09 |

<!-- body -->
**Audit 2026-09-22 note** (findings: gates-09)

Drift-patrol tracker-hygiene fails on main as of 2026-09-22.

### Current code facts
- str-8q1b4 in_progress 22 days (branch is 105 commits behind main); str-mpgg1 in_progress since 2026-09-01 although 84941b37 is merged into main.
- Orphaned open children of closed parents: str-qwua7.56.1, str-qwua7.9.1, str-hy9b.J3, str-hy9b.1.
- docs-stories PENDING (str-u394l.3); cli-surface-drift PENDING (str-wurp).
- Evidence: `audits/2026-09-22/gates/drift-patrol.log:17-39`.

### Proposed changes / acceptance additions
- Close str-mpgg1 (merged); decide str-8q1b4 (rebase+land or release); re-parent or close the 4 orphans.
- Consider `--strict-pending` once str-wurp and str-u394l.3 land.
