---
slug: known-answer-ratchet-and-ts-discriminants
kind: new
title: "Turn EXPECTED BRANCHES comments into a known-answer ratchet gate; link gauntlet allowlist entries to issues; preserve TS discriminant literals"
priority: P2
type: task
labels: [testing, gauntlet, typescript, examples, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Turn EXPECTED BRANCHES comments into a known-answer ratchet gate; link gauntlet allowlist entries to issues; preserve TS discriminant literals

## Problem

The canonical examples are weakly explored, and the failures are hidden instead of tracked:

- In examples 01-05, 24 of 39 expected outcomes are found. Across 52 TS functions, line coverage is 55.7%.
- The gauntlet suppresses the resulting FAIL rows with a 17-entry allowlist that has not changed since 2026-05-08 and links no issue for most entries, so there is no ratchet: coverage can drop further without any gate noticing.
- `computeArea` (TS `05-unions.ts`) stays at 0/6 branches because the TS analyzer types the discriminant field `kind` as plain `str`. The generator produces `kind` values such as `""`, `"0"`, `" "` and random strings, never `circle`, `rectangle` or `triangle`, and emitted `{"kind":"true","radius":2.0}`.

## Evidence

Re-checked on 2026-09-23:

- `demo/gauntlet-scan-allowlist.yaml` (audit worktree HEAD 56c86168): 113 lines, one `str-` reference, last changed in 8734407f and 398e4a7e (2026-05-08). Entries for `computeArea` (:33) and `routeRequest` (:38) cite "union-input synthesis" with no issue. Its header says the gauntlet scans `examples/standalone/ts/`.
- The examples corpus is the sibling repo `/home/ketan/project/examples` (HEAD 9f653d0, 2026-04-01), wired through `SHATTER_EXAMPLES_DIR` in `Taskfile.yml` (:132, :160, :607). Today 36 files under `standalone/` contain `EXPECTED BRANCHES` comments (the audit counted 37 of 66 standalone examples; recount at pickup).
- `computeArea` is in `/home/ketan/project/examples/standalone/ts/05-unions.ts`. Its analysis shows `{"kind":"str"}` in all three union variants; the audit's concolic run reached 21 iterations, 1 path, 0/6 branches.
- `benchmarks/sample-manifest.json:1-21`; the walkthrough exercises only examples 01, 02, 03, 04 and 18.
- Audit write-up: `audits/2026-09-22/areas/goals.md` item 7 (goals-07), on branch `audit-2026-09-22` until the audit reports land. The verifier did not re-verify the 24/39 tally or the walkthrough file list.

## Acceptance criteria

- [ ] A machine-readable known-answer manifest is generated from the `EXPECTED BRANCHES` comments in the examples repo (function -> expected outcomes). A gate fails when the count of found outcomes for any function drops below its recorded value (a ratchet, not an absolute target). Proof at close: a forced gate run that passes, and a demonstration that lowering one recorded value's corresponding result makes it fail.
- [ ] Each allowlist entry in `demo/gauntlet-scan-allowlist.yaml` names a tracker issue. The gauntlet fails if coverage for an allowlisted function drops below its recorded value, and fails on an allowlist entry with no issue id.
- [ ] The TS analyzer emits `enum_values` (or a const literal type) for object-field discriminants of union types. A known-answer test shows `computeArea` reaching all 3 variants; it fails on current code. Parity: record in `protocol/parity-matrix.yaml` / the TS frontend `CLAUDE.md` if the analysis JSON changes shape, and run `task parity` + `task conformance`.
- [ ] `task gauntlet` passes after the change (record output).

## Suggested approach

Split into 2-3 child tasks at pickup if preferred: (1) manifest + ratchet gate, (2) allowlist issue links and ratchet, (3) TS discriminant literals. The allowlist links can reuse the issues filed by this audit where they apply.

## Out of scope

- Raising the coverage of the examples beyond what the TS discriminant fix gives.
- Pinning the examples repo to a revision (pin-examples-repo).

## Related

str-qwua7.10, str-knf0v, str-v0yjq, str-jeen.57, gauntlet-scan-checker-consumes-json, pin-examples-repo. Source finding: goals-07 (confirmed).
