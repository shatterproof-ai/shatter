# Revision log: bucket `shatter-frontend-ts` (2026-09-23)

Primary review: `issues/crosscheck/shatter-frontend-ts.codex.md` (Codex, 15 findings). The secondary same-runtime review `issues/crosscheck/shatter-frontend-ts.md` does not exist, so nothing came from it.

Live tracker checks (`bd show`, run from `/home/ketan/project/shatter`): str-rf2v (OPEN; description puts the refactor out of scope; 2026-09-04 notes already reject option (b)), str-0z1im (**CLOSED** 2026-09-19, landed 4f673612), str-mhinv.3 (OPEN), str-a4c (CLOSED), str-qwua7.37 (OPEN), str-qwua7.31 (OPEN).

Code evidence was re-checked against the audit worktree at 793f2b0b; no TS, triage, solver or coverage code changed since the 56c86168 baseline.

## Finding -> action

| # | Sev | Finding (one line) | Action | Files |
|---|---|---|---|---|
| 1 | BLOCKER | 06/10: "in ALL_COMMANDS but not SUPPORTED_CAPABILITIES -> not_supported" would reject handshake/shutdown; SUPPORTED_CAPABILITIES has `complex_type:*` entries | **applied**. Verified `handlers.ts:56-68`. 06 now requires handshake/shutdown as always-dispatched control commands and an explicit supported-**command** set separate from SUPPORTED_CAPABILITIES, with a test over every ALL_COMMANDS value. 10's option (a) text corrected the same way. | 06, 10 |
| 2 | MAJOR | 01's core guard trusts `triage::evaluate_constraint`, which uses f64 division and merges null/undefined | **applied**. Verified `triage.rs:369, 442-455, 473`. The guard was split out of 01 into new issue 16, which requires language-aware evaluation that returns indeterminate for unmodelled semantics, plus Go integer-division and JS null/undefined negative tests. | 01, 16 (new) |
| 3 | MAJOR | 01 havoc misses `for` incrementors and loop-condition mutations; param lookup precedes the flow map | **applied**. Verified `instrumentor.ts:346-351, 422-431, 1871-1873`. 01 now requires havoc over body, incrementor and condition, routes reassigned params through the program-point map, and adds `forIncr`, `condMut`, `paramReassign`, `paramOpaque` regression cases. | 01 |
| 4 | MAJOR | 02 allows a `(line, type)` join that still collides for same-line branches | **applied**. The join option is removed; one shared enumerator assigns ids; the alignment corpus adds two ternaries on one line and two `if`s on one line. | 02, 03 |
| 5 | MAJOR | 02 leaves switch fallthrough/default and `??` semantics undefined | **applied**. Verified the analyzer emits no default-case branch and there is no nullish BranchType. 02 now has a normative decision-semantics table (per evaluated case label; no decision for fallthrough or default; `&&`/`||` = leftmost operand, analyzer condition changed to match), fallthrough/default/effectful-label tests, and `??` moved out of scope. | 02, 03 |
| 6 | MAJOR | 09 assumes instrument responses carry branch metadata | **applied**. Verified `protocol.ts:211-216`, `instrumentor.ts:19-31`. 09 now extracts instrument-side `(id, line)` from the `__shatter_branch` probes in the instrumented `output_file`, which keeps it independent of 02. | 09 |
| 7 | MAJOR | 13's nullish `ite` is wrong under the core's null=0 model; solver rejects Shl/Shr | **applied**. Verified `solver.rs:522, 601-608`. `??` is now a documented collapse rule to `unknown` (with an optional sound non-nullable reduction); shifts are wire emission only with a test that the solver reports them unsupported; the E2E flip case must use `as`/`!`. | 13 |
| 8 | MAJOR | 07 turns the str-rf2v investigation into an implementation by comment | **applied (split)**. Confirmed with `bd show str-rf2v`: "Out of scope: the actual consolidation refactor". 07 is now evidence only and proposes closing rf2v via its own option (a); the implementation moved to new issue 15 (including the parity-matrix correction). | 07, 15 (new), 01, 04, 08, 13 (references) |
| 9 | MAJOR | 08 treats schema-example request fixtures as runnable | **applied**. Verified `requests/valid/*.json` use `src/example.ts`, a fabricated `prepare_id` and a non-existent setup. 08 now requires a materialized temp project, path/id substitution, protocol ordering, an expected-status table per fixture, and rejects `file_not_found`-style results as passing. Invalid fixtures moved to 06. | 08, 06 |
| 10 | MAJOR | 09 adds an unbounded cross-frontend workaround sweep | **applied**. The sweep is removed from 09 (scoped to the known `raw_results` workaround) and listed as out of scope, pointing at pipeline-close-reason-rule. | 09 |
| 11 | MAJOR | 14 bundles lifecycle, packaging and a dependency major upgrade | **applied (split)**. 14 keeps lifecycle (slug unchanged, retitled, type chore -> bug); packaging moved to new 17; js-yaml upgrade moved to new 18. | 14, 17 (new), 18 (new) |
| 12 | MAJOR | 14 accepts `unref()`; shutdown terminates the worker before draining | **applied**. Verified `executor.ts:1272-1282`, `handlers.ts:1058-1061`. 14 now requires `clearTimeout` on settle (explicitly not `unref`), a fake-timer test, drain-before-worker-terminate on `shutdown`, and an `instrument`-then-`shutdown` regression test that checks response ordering. | 14 |
| 13 | MAJOR | Bare `cargo test --test e2e_concolic` close-time commands bypass setup and the gate wrapper | **applied**. Verified `Taskfile.yml:591-609` and AGENTS.md "Shared-Machine Resource Etiquette". Every close-time E2E proof now uses `task --force e2e-ts` (or `e2e-go`/`e2e-rust`) and pastes the `test result:` line with a non-zero passed count; convention stated in the BUNDLE header. | 01, 02, 04, 09, 13, 15, 16 |
| 14 | MINOR | 06's example requests lack `id`/`protocol_version` | **applied**. Probes rewritten with full envelopes. | 06 |
| 15 | MINOR | 08 overstates the round-trip tests' uselessness; overlaps str-0z1im | **partially applied**. Wording softened and a required list of serialization guarantees (BigInt, `-0`, NaN/Infinity/undefined) added. **Disputed part:** the overlap claim. `bd show str-0z1im` shows it CLOSED on 2026-09-19 (landed 4f673612), not in progress, so there is no ownership to reconcile; it is cited as precedent only. | 08 |

## Splits and conversions

- **01 ts-flow-map-program-point** -> 01 (TS frontend fix) + **16 core-constraint-consistency-guard** (new, P2 feature, core-side backstop).
- **07 rf2v-fourth-walker-and-analyze-dataflow** stays a note on str-rf2v but no longer widens its scope; the implementation became **15 ts-flow-analysis-consolidation** (new, P2 task, L).
- **14 ts-lifecycle-and-packaging-hygiene** (slug kept) -> 14 (lifecycle only) + **17 ts-packaging-hygiene** (new, P3 chore) + **18 ts-js-yaml-v4** (new, P3 chore). 18 also records that js-yaml 3.x `load` accepts `!!js/function` on repo-supplied `.shatter/config.yaml` (found while re-verifying; `opaque-stub-registry.ts:28`).
- No slug was removed or converted. No `blocked_by` edges were added; couplings are listed in BUNDLE.md.

## Other edits

- 03 reopen-note: fix summary no longer mentions `??` or the line/type join.
- 04, 08, 13: references to "str-rf2v's shared builder" now point at ts-flow-analysis-consolidation.
- 02: analyzer line ranges corrected (`:1384-1398` ternary, `:1401-1424` logical).
- 14: `jest.config.js` `forceExit` is at `:7`, not `:6`.
- BUNDLE.md regenerated from the revised drafts (header, decisions, filing order, index, drafts).
- Decisions D1-D6: re-checked; nothing in this bucket contradicts them (no timeout env var or hook-bypass guidance; nothing filed).
