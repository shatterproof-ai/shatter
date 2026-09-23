# Revision: shatter-reports-and-specs (Codex cross-check, 2026-09-23)

Source review: `audits/2026-09-22/issues/crosscheck/shatter-reports-and-specs.codex.md` (15 findings: 14 MAJOR, 1 MINOR). No same-runtime secondary review exists for this bucket (`crosscheck/shatter-reports-and-specs.md` is absent).

Verification done for this revision: code re-read at audit-worktree HEAD 793f2b0b (`git diff 56c86168 793f2b0b` outside `audits/` is empty); `target/release/shatter` run on `classifyNumber` to get matching markdown and JSON invariant output; `bd show` on str-qwua7.61, str-qwua7.38, str-qwua7.10 (all open) and str-9awj (closed); `sd-v1.json`/`sd-v2.json` and `regress/base.json`/`new.json` class examples inspected; examples-repo EXPECTED BRANCHES comments read; `Cargo.toml` files checked for `insta` (none).

| # | Sev | Codex finding (one line) | Action | Files changed |
|---|---|---|---|---|
| 1 | MAJOR | 06/07 require a regression verdict ("input 101: pass -> overflow") that file-only spec-diff cannot establish from fixtures with no shared input | applied | 07 (proposed AC now asks only for what files prove: `overflow` reported ADDED, no fail/overflow pairing, non-zero exit; explains why the concrete verdict is not provable), 06 (spec-diff verdict ACs removed), new 13 (verdict decided from symbolic class regions + Z3 witness, INCONCLUSIVE when regions are unknown; no replay) |
| 2 | MAJOR | 06 assumes every class has usable symbolic constraints; `SymConstraint::Unknown` exists and `BranchPath::from_decisions` discards constraints | applied | 06 (Contract section: retention, negation of not-taken steps, explicit `unknown` conjuncts, `complete`/`partial`/`none` status, per-language/per-engine pinned status table, disagreement test, proptest) |
| 3 | MAJOR | 06 duplicates comparison work it assigns to str-qwua7.38 | applied | 06 (now export-only), new 13 (owns region verdicts; explicit division of work with str-qwua7.38: pairing vs verdicts), 07 (points at 13) |
| 4 | MAJOR | 02/06 contradict D2 by allowing backward reading to be dropped; no legacy fixtures, mixed-version behavior or migration owner | applied | 05 (owns the versioned shared reader: version dispatch, upgrade-step list, legacy fixture dir, newer-version error), 02 and 06 (escape clause removed; each adds one upgrade step and a legacy fixture; mixed-version spec-diff behavior defined; both blocked by 05) |
| 5 | MAJOR | 01's "same run" JSON evidence (`ts-spec-invariants.json`) is for `categorizeUser`, not `classifyNumber` | applied | 01 (evidence replaced with a reproduced matching markdown + JSON pair; also corrected the "duplicate lines" claim: the two `!= null` lines are input and output invariants, so the fix is label rendering, not de-duplication; restatement suppression moved out of scope) |
| 6 | MAJOR | 04 allows a "rename to branch points reached" option that still shows 2/2 and fails its own test | applied | 04 (decision fixed to branch-side coverage; exact expected values `3/4 branch sides` and `6/6 branch sides`; JSON field and schema-version requirements) |
| 7 | MAJOR | 03 makes `qualified_id` relative although it is a stable identity key (`report.rs:448-460`) | applied | 03 (display paths only; `file_path`/`qualified_id` must stay unchanged, with a test; identity relativization moved to out of scope as needing its own migration issue) |
| 8 | MAJOR | 05's reader flattens to `Vec<FunctionSpec>`, losing file identity and schema version; YAML support unspecified | applied | 05 (Data contract: `SpecDocument` keeps shape, version, file; `(file, name)` addressing with ambiguity errors; JSON only, YAML rejected with a named error; reader unit tests and proptest) |
| 9 | MAJOR | 08 cannot pass without 05 and 03 but only declares the gauntlet-checker dependency | applied | 08 (`blocked_by` adds spec-json-shapes-compare, scan-report-headline-and-paths, pin-examples-repo; known-bug goldens may not include these blockers) |
| 10 | MAJOR | 08/09 lack a reproducibility contract (seeds do not remove scheduling/timeout nondeterminism) | applied | 08 (Reproducibility contract: pinned fixtures, iteration budgets, `--parallelism 1`, `--no-seeds`, fresh cache, byte-compare only search-independent structure, counts as facts, 10-run stability proof, baseline policy), 09 (same budgets, 10-run stability, baseline policy with issue ids for lowering; blocked by pin-examples-repo) |
| 11 | MAJOR | 09 never defines an executable "outcome"; comments are prose predicates with continuous outputs and shared errors | applied | 09 (Oracle definition: hand-authored manifest entries with `when` predicate, `outcome` (exact / `any` / throws substring) and `witness`; validator; found/expected/baseline separated) |
| 12 | MAJOR | 09 bundles three deliverables and overlaps str-qwua7.10's allowlist ownership | applied (verified with `bd show str-qwua7.10`: it adds `expires:` and issue-cited entries to the same allowlist) | 09 (ratchet only, slug kept), new 14 ts-union-discriminant-literals, new 15 qwua7-10-allowlist-issue-links-note (note-to-existing str-qwua7.10) |
| 13 | MAJOR | 10 adds an unspecified config override (file, glob base, precedence, policy_excluded, subdirectory scans) | applied | 10 (override removed; classification contract anchored to project root, subdirectory-scan equality test, side-effect check on the examples corpus and gauntlet/walkthrough), new 16 source-bucket-config-override (full contract: `shatter.config.json`, root-relative globs, first match wins, cannot override policy_excluded/unsupported, load errors, origin field), 11 (reopen note cites both) |
| 14 | MAJOR | 12's proptest varies inputs, which are already escaped; the defect is in `thrown_error` | applied | 12 (proptest now varies `thrown_error` and other free-text fields with NUL, ESC, DEL, `\r`, multi-line; added an echo-error CLI regression test) |
| 15 | MINOR | 08 describes an insta setup that does not exist | applied | 08 and 03 (snapshot tests described as per-file `assert_snapshot` helpers; no crate depends on `insta`; new suite needs its own explicit update mode and must fail on a missing golden) |

Disputed findings: none.

## Additional corrections found while revising

- "Spec changelog" does not exist as a file. 01 and 02 now point at the `SPEC_SCHEMA_VERSION` doc comment (`shatter-core/src/spec.rs:236-252`), which the bump policy uses as the changelog.
- `explore` has no `--seed` flag (only `scan` does). 05, 08 and 09 use `--max-iterations`, `--parallelism 1` and `--no-seeds` instead, and `--seed` only where supported.
- `fmt2` (06) was an audit scratch file (`edge/plain.ts`), not in any repo; 06 now says to recreate it from its description.
- Evidence HEAD updated to 793f2b0b (code identical to 56c86168).

## Splits and conversions

- 09 known-answer-ratchet-and-ts-discriminants: split. Slug kept for the ratchet gate (retitled). New 14 ts-union-discriminant-literals (TS analyzer fix). New 15 qwua7-10-allowlist-issue-links-note (kind note-to-existing, existing_id str-qwua7.10, P1 to match the target).
- 10 source-bucket-fixture-dir: split. New 16 source-bucket-config-override (blocked by 10). 11 reopen note now `blocked_by: [source-bucket-fixture-dir, source-bucket-config-override]` (filing order only).
- 06 spec-preconditions-from-path-constraints: spec-diff verdict work moved to new 13 spec-diff-symbolic-region-verdicts (blocked by 06).
- New dependencies inside the bucket: 02 and 06 blocked by 05; 08 blocked by 05 and 03 (plus external gauntlet-scan-checker-consumes-json and pin-examples-repo); 09 blocked by pin-examples-repo; 13 blocked by 06; 16 blocked by 10.
- No slugs removed.

## Cross-bucket effects

- shatter-concolic-and-engine-design 02-concolic-early-termination (lines 71, 81, 102) and 12-concolic-early-termination-fix (line 42) cite known-answer-ratchet-and-ts-discriminants for the `computeArea` discriminant widening. That work is now ts-union-discriminant-literals; those references should be retargeted.
- shatter-test-hygiene pin-examples-repo now blocks golden-and-consumer-suite and known-answer-ratchet-and-ts-discriminants.
- shatter-gates-integrity gauntlet-scan-checker-consumes-json still blocks golden-and-consumer-suite (unchanged).
- str-qwua7.10 now receives a note from this bucket (15). If another bucket also adds a note to str-qwua7.10, the filer should merge them.
- shatter-artifacts-correctness retire-snapshot-diff (D2) and this bucket's 05 both touch `shatter-cli/src/commands/diff.rs` (05 moves spec-diff's `SpecInput` reader into core). Whichever lands second rebases.
- shatter-docs spec-s5-contract-table-and-samples is still blocked by 05; the SPEC §5 table should describe the reader's data contract (JSON bundle or bare spec; YAML is output-only).
