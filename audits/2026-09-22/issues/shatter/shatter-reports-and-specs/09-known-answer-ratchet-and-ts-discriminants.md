---
slug: known-answer-ratchet-and-ts-discriminants
kind: new
title: "Known-answer ratchet gate: turn the examples' EXPECTED BRANCHES comments into an executable outcome manifest and fail when found outcomes drop"
priority: P2
type: task
labels: [testing, gauntlet, examples, quality-gates, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [pin-examples-repo]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Known-answer ratchet gate: turn the examples' EXPECTED BRANCHES comments into an executable outcome manifest and fail when found outcomes drop

(The slug keeps its original name for tracker cross-references. The TS discriminant-literal part of the original draft is now ts-union-discriminant-literals, and the allowlist issue-link part is a note on str-qwua7.10: qwua7-10-allowlist-issue-links-note.)

## Problem

The canonical examples are weakly explored, and nothing notices when exploration gets worse:

- In examples 01-05, the audit counted 24 of 39 expected outcomes found. Across 52 TS functions, line coverage is 55.7%.
- The only statement of what each example should reach is prose: `EXPECTED BRANCHES` comments in the examples repo. No gate reads them. The gauntlet compares coverage against a 100% threshold and suppresses the resulting FAIL rows with an allowlist, so coverage can drop further without any gate noticing.

The comments cannot be parsed into a check directly. They describe behavior with predicates and outcomes, for example `kind === "circle" AND radius > 0 → returns π * r²` (a continuous numeric output) or two different variants that throw the same `Error("non-positive dimension")`. Counting unique outputs, paths or branches does not measure them: two expected branches can share an output, and one output can be a continuum.

## Evidence

Re-checked on 2026-09-23:

- The examples corpus is the sibling repo `/home/ketan/project/examples` (HEAD 9f653d0, 2026-04-01), wired through `SHATTER_EXAMPLES_DIR` in `Taskfile.yml` (:132, :160, :607). 36 files under `standalone/` contain `EXPECTED BRANCHES` comments (the audit counted 37 of 66 standalone examples; recount at pickup).
- Comment examples: `standalone/ts/01-arithmetic.ts:4-8` (`n < 0 → returns "negative"` and three more); `standalone/ts/05-unions.ts:9-15` (`computeArea`, six entries with continuous returns and repeated error messages); `05-unions.ts:48-56` (`routeRequest`, eight entries, two of them `throws Error("body required")`).
- `demo/gauntlet-scan-allowlist.yaml` (audit worktree HEAD 793f2b0b): 113 lines, one `str-` reference, last changed in 8734407f and 398e4a7e (2026-05-08). Its header says the gauntlet scans `examples/standalone/ts/` against a 100% coverage threshold.
- `benchmarks/sample-manifest.json:1-21`; the walkthrough exercises only examples 01, 02, 03, 04 and 18.
- Audit write-up: `audits/2026-09-22/areas/goals.md` item 7 (goals-07), on branch `audit-2026-09-22` until the audit reports land. The verifier did not re-verify the 24/39 tally or the walkthrough file list.

## Oracle definition

A checked-in manifest (for example `tests/known-answers/manifest.yaml` in the shatter repo, keyed by examples-repo path and the SHA pinned by pin-examples-repo) lists, for each covered function, its expected outcomes. Each entry has:

- `id` (the number in the `EXPECTED BRANCHES` comment);
- `when`: an input predicate over the JSON-encoded arguments, written as a Python expression over `args` (for example `args[0]["kind"] == "circle" and args[0]["radius"] > 0`);
- `outcome`: `returns` with either an exact JSON value or `any` (for continuous outputs), or `throws` with a message substring;
- `witness`: one concrete input that satisfies `when`.

An expected outcome counts as **found** when at least one recorded execution in the explore output satisfies `when` and has a matching outcome. The manifest is authored by hand from the comments (not parsed from them); a validator checks that every witness satisfies exactly one entry's `when` for its function, and that running the witness gives the stated outcome.

The **expected coverage** is the manifest's entry count per function. The **ratchet baseline** is a separate checked-in file with the found count per function as of the last update; it never exceeds the expected count.

## Acceptance criteria

- [ ] The manifest covers every function in `standalone/ts/01-05` and their Go and Rust counterparts where they exist, and at least the functions named in `demo/gauntlet-scan-allowlist.yaml`. Its validator runs in the gate and fails on an entry whose witness matches zero or several entries or produces a different outcome.
- [ ] A gate (for example `task known-answers`) explores each manifest function under a fixed budget (`--max-iterations`, `--parallelism 1`, `--no-seeds`, a fresh cache directory; `--seed` where the command supports it), computes found counts with the oracle above, and fails when any function's found count is below its baseline. It prints found/expected/baseline per function.
- [ ] Baseline policy: raising a baseline is a normal commit via an explicit update command; lowering one requires a line in the baseline file naming a tracker issue. The gate fails on a lowered value without an issue id.
- [ ] Stability: the gate is run 10 times in a row on one machine at the recorded budget with no found-count differences; budgets are raised (or a function excluded with an issue id) until that holds. The close comment records the 10-run result.
- [ ] The gate is part of `task check` (or `task gauntlet`, if the maintainer prefers; the close comment says which) and is selected by `scripts/affected-gates.py` for changes to explorer, orchestrator, generator and frontend analyzer code, with a selector test.
- [ ] Close-time proof: a forced gate run (not a cached "up to date") that passes, and a run where one baseline value is raised above what exploration finds, showing the gate fail with that function named.

## Out of scope

- Raising coverage of the examples (separate issues; ts-union-discriminant-literals raises `computeArea`).
- Linking allowlist entries to issues (note on str-qwua7.10: qwua7-10-allowlist-issue-links-note).
- Pinning the examples repo (pin-examples-repo, which blocks this).

## Related

str-qwua7.10, str-knf0v, str-v0yjq, str-jeen.57, gauntlet-scan-checker-consumes-json, pin-examples-repo, ts-union-discriminant-literals. Source finding: goals-07 (confirmed).
