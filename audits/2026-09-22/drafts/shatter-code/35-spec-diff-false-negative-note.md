# NOTE on str-qwua7.38: branch-id pairing causes false negatives, not only noise — raise to P1

| field | value |
|---|---|
| action | **append comment to existing issue `str-qwua7.38`** (no new issue) |
| suggested priority for target | P2 |
| source findings | artifacts-06 |

<!-- body -->
**Audit 2026-09-22 note** (findings: artifacts-06)



### Current code facts
- v1 grade: <0 invalid, <50 fail, else pass. v2 inserts n>100 → 'overflow'. v1 'fail' and v2 'overflow' share BranchPath [(0,F),(1,T)] and are paired.
- `spec-diff` output (`audits/2026-09-22/artifact-samples/sd-diff.txt`): '2 added, 1 removed, 1 precondition(s) changed, 1 class(es) with insufficient comparison evidence'; 'overflow' never appears, so the real regression (inputs >100 now return overflow instead of pass) is missing.
- Pairing code: `shatter-core/src/spec_diff.rs:93-114`.

### Proposed changes / acceptance additions
- Add this v1/v2 pair as a known-answer spec-diff test that must report a CHANGED postcondition for inputs >100.
- Consider raising str-qwua7.38 to P1.
