# Revision: shatter-artifacts-correctness (2026-09-23)

Applies the Codex cross-check (`crosscheck/shatter-artifacts-correctness.codex.md`, primary) and the earlier same-runtime review (`crosscheck/shatter-artifacts-correctness.md`, secondary). Code claims re-verified against the audit worktree (== main `56c86168` plus audit files). Tracker: `bd show str-qwua7.11` and `bd show str-qwua7.39` confirm both are open (P1 and P2). Nothing filed (D6).

## Codex findings

| # | Sev | Finding (one line) | Action | Files |
|---|---|---|---|---|
| C1 | MAJOR | 01: partial resume (`PersistedExploreState`/`read_resume_state`) has no source-fingerprint check | applied: verified at `explore.rs:67-70`, `:1264-1273`; sidecar now must store and validate deep fingerprint + options hash + explorer mode via one shared helper; added source-edit and mode-change partial-resume tests | 01 |
| C2 | MAJOR | 02: "record failed functions with a failure class" needs an undefined schema change | applied by **split**: 02 now covers only the success case (no schema change); new 15 `explore-spec-bundle-failed-functions` defines `failed_functions`/`FailedFunction`/closed `failure_class` enum, `Failed` status, `SPEC_SCHEMA_VERSION` bump, spec-diff behaviour, and separates exploration failures from target exceptions | 02, 15 (new) |
| C3 | MAJOR | 03: spec-diff matches by bare `function_name`; multi-file collisions | applied: verified `diff_spec_collections` (`diff.rs:223-262`); acceptance now requires (file, name) matching and a same-name-in-two-files test that fails against a name-keyed implementation | 03 |
| C4 | MAJOR | 03: `compare` support is larger and undefined; overlaps spec-json-shapes-compare | applied: `compare` removed from 03's acceptance; ownership assigned to spec-json-shapes-compare (other bucket), which must accept the shape 03 chooses | 03 |
| C5 | MAJOR | 04: checkpoint criterion contradicts CLI modes (no checkpoint without `--resume`; `--resume PATH` is explicit) | applied: verified `scan.rs:1186-1224`; scoped to `--resume auto`, pinned unchanged no-resume/off/PATH behaviour, test enables `--resume auto` explicitly | 04 |
| C6 | MAJOR | 05: proposed `error` mask prefix is wrong; real vocabulary is `thrown_error` | applied: verified `nondeterminism.rs:487-600` (`return`, `return.<path>`, `thrown_error`, `<outcome>`); criteria use that vocabulary, compare errors by type+message only (never stack), and add producer/consumer mask tests | 05, 06 |
| C7 | MAJOR | 11: denominator must include branch-decision lines (match-arm patterns via `branch_hit`) | applied: verified `merge_lines_executed` (`shatter-rust-runtime/src/lib.rs:291-309`) and `line_of(&arm.pat)` (`instrument.rs:421`); count is now the union of all probe lines, with multi-line match E2E, unit and proptest | 11 |
| C8 | MAJOR | 01/10/11: bare `cargo test --test e2e_*` skips `#[ignore]`d tests | applied: verified 26/23/13 ignored tests and `Taskfile.yml:577-646`; replaced with `task --force e2e-ts/e2e-go/e2e-rust` and require the named tests' `... ok` lines in the close note | 01, 10, 11 |
| C9 | MAJOR | 14: explore `--spec-json` case depends on open str-qwua7.11 without ordering | applied: case moved out of .39 and assigned to str-qwua7.11; .39 gains an independent fresh-dir explore markdown case (no init lines on stdout) | 14 |
| C10 | MINOR | 02: `finalize_explore -o .json` writes nothing when empty; the marker is the `--spec-out` branch | applied: evidence corrected (`explore.rs:4006-4021` vs `:4040-4074`); tests now cover each sink separately, live and `--from-artifacts` | 02 |
| C11 | MINOR | 03: spec-diff loader accepts bundle or single `FunctionSpec`, not a list | applied | 03 |
| C12 | MINOR | 11: `instrumentable_line_count` is an Instrument-response field, not Execute | applied: restricted to Instrument responses; noted shatter-rust's flat response struct | 11 |
| C13 | MINOR | 07: `Snapshot::` search matches `SourceFileSnapshot::` | applied: tightened `rg` pattern (verified hit list on `56c86168`), excluded historical `docs/perf/inventories/**`, named the unrelated hits that must stay | 07 |
| C14 | MINOR | 14: `compare --json` and `revalidate` JSON omitted from inventory | applied: added `compare --json` and `revalidate --output-format json`; `specify --json` and `discover-deps --json` explicitly out of scope with reasons | 14 |

No Codex finding disputed.

## Secondary (same-runtime) findings

| # | Sev | Finding | Action | Files |
|---|---|---|---|---|
| S1 | MAJOR | 07 `rg` criterion unpassable; CONTRIBUTING.md:130 and perf inventory missing | applied (with C13): CONTRIBUTING.md and docs-smoke docstrings added; perf inventory excluded as historical rather than edited | 07 |
| S2 | MAJOR | 05 "ExpectedDrift fails exit by default" is an undecided product change | applied: drift is reported separately from confirmed; exit policy for drift unchanged (exit 0 when outputs match); any change needs a maintainer decision | 05, 06 |
| S3 | MINOR | 01 `--no-cache` listed as result-affecting but not in the key | applied: `--no-cache` removed from the problem statement, explicitly excluded from the hash and documented | 01 |
| S4 | MINOR | 02 finalize_explore evidence overstated | applied (same as C10) | 02 |
| S5 | MINOR | 03 SpecInput shape wrong | applied (same as C11) | 03 |
| S6 | MINOR | 10 title leads with unverified zolem numbers | applied: retitled around the verified clamp and span-override defects; slug unchanged | 10 |
| S7 | MINOR | 12 `source_file` field is a persisted-format change | applied: noted `BehaviorMap` has no version, field is `serde(default)` optional, legacy maps ignored, no parity-matrix change | 12 |
| S8 | MINOR | filer must resolve ids against bd, not JSONL | no draft change (filer concern); noted here | - |
| S9 | MINOR | 04 mixed-language `--resume` behaviour unspecified | applied: one-namespace requirement holds with and without `--resume`; test runs `--resume auto` twice | 04 |

## Splits and conversions

- **Split:** `explore-o-json-empty-bundle` (02) -> 02 (success case, no schema change) + new `explore-spec-bundle-failed-functions` (15, P2, blocked by 02). No slugs removed or converted.
- 10's title changed; slug `go-scan-coverage-clamp` unchanged.

## Cross-bucket effects

- spec-json-shapes-compare (shatter-reports-and-specs): owns `compare` reading bundles and multi-file inputs; its shared reader must accept the multi-file shape chosen in multi-file-spec-bundle-first-only (relate or order the two).
- artifact-json-schemas / spec-s5-contract-table-and-samples (shatter-docs): should describe `failed_functions` and the new bundle status from explore-spec-bundle-failed-functions, in addition to the multi-file shape.
- str-qwua7.11 (existing, open): the fresh-directory whole-stdout-JSON case for explore `--spec-json` now belongs to it, not to the str-qwua7.39 note.
