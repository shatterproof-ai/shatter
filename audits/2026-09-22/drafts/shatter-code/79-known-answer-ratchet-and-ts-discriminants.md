# Turn EXPECTED BRANCHES comments into a known-answer ratchet gate; link gauntlet allowlist entries to issues; preserve TS discriminant literals

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | task |
| priority | P2 |
| labels | testing,gauntlet,typescript,examples,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qwua7.10, str-knf0v, str-v0yjq, str-jeen.57 |
| source findings | goals-07 |

<!-- body -->
## Problem

Canonical examples are weakly explored (24/39 expected outcomes in files 01-05; 55.7% lines over 52 TS functions) and the failures are suppressed by a 17-entry gauntlet allowlist unchanged since May with no issue links. computeArea stays at 0/6 branches because the TS analyzer types the discriminant `kind` as plain `str`.

## Current code facts / evidence

- 37 of 66 standalone examples carry EXPECTED BRANCHES comments.
- `demo/gauntlet-scan-allowlist.yaml` (113 lines, 1 str- reference, last touched 8734407f/398e4a7e 2026-05-08); entries like computeArea/routeRequest cite 'union-input synthesis'.
- computeArea analysis: `{"kind":"str"}` in all three variants; generated kind values '', '0', ' ', random strings, never circle/rectangle/triangle; generator emitted `{"kind":"true","radius":2.0}`.
- `benchmarks/sample-manifest.json:1-21`; walkthrough exercises only files 01, 02, 03, 04, 18.

## Acceptance criteria

- Machine-readable known-answer manifest generated from EXPECTED BRANCHES; gate fails when found-outcome counts regress (ratchet, not absolute).
- Each allowlist entry names a tracker issue; gauntlet fails if coverage for an allowlisted function drops below its recorded value.
- TS analyzer emits enum_values/const literal for object-field discriminants; computeArea reaches all 3 variants.

## Suggested approach

Split into 2-3 child tasks at pickup if preferred (manifest+gate, allowlist links, TS discriminants).

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: L

## References

- Audit findings: goals-07 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.10, str-knf0v, str-v0yjq, str-jeen.57
