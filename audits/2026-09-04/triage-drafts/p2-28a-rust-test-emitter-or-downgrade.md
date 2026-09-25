---
repo: shatter
type: chore
priority: 2
labels: rust, core, docs
existing: none
---
# Delete the dead test emitters (export.rs) and the README "or tests" claim
## Decision (2026-09-06)
Delete the dead test emitters. The CLI export surface was removed in str-tlnt and nothing outside `shatter-core/src/export.rs` calls `generate_jest_tests`/`generate_vitest_tests`/`generate_go_tests`. Remove export.rs (1,743 lines) and its tests, the `export.rs — Test generation from behavior maps` line in shatter-core/CLAUDE.md, and README step 4's "or tests". No SPEC test-export column is needed because the feature no longer exists.

## Problem
SPEC §1.3 lists TypeScript, Go and Rust all as "Supported" and says other docs treat it as canonical. `export.rs` generates Jest, Vitest and Go tests but has no Rust generator, and no test compiles or runs any emitted file. Either the emitter exists for every supported language or the status table should say what is missing.

## Current code facts
- `shatter-core/src/export.rs`: `generate_jest_tests` (:239), `generate_jest_tests_with_annotations` (:254), `generate_go_tests` (:301), `generate_go_tests_with_annotations` (:315), `generate_vitest_tests` (:612), `generate_vitest_tests_with_annotations` (:626). No `generate_rust_tests`.
- Unit tests assert string content only; emitted Go/Jest runnability is not verified in-crate.
- The CLI export flag was removed (str-tlnt, commit 87cad05b); README.md "How Shatter Works" step 4 still says "optional specs or tests"; `shatter-core/CLAUDE.md:11` lists `export.rs — Test generation from behavior maps`.
- SPEC.md:56-64 status table; §7.1 lists Rust parity gaps but not test export.

## Options
1. **Downgrade now (proposed default)**: SPEC §1.3 gains a "Test export" column (TS Y, Go Y, Rust N) and a §7 limitation row; README step 4 says "specs (and, via the library, Jest/Vitest/Go tests)"; file a follow-up feature "Rust test emitter + compile checks for all emitters".
2. **Emitter now**: `generate_rust_tests` (+ `_with_annotations`) emitting `#[test]` fns with `assert_eq!`/`should_panic` from a `BehaviorMap`, plus compile-check tests (`rustc --crate-type lib` on a temp file; `go vet`/`tsc --noEmit` for the others).
3. Delete `export.rs` and the README claim (usability rec 13) — only if the CLI export surface is not coming back.

## Acceptance checks
- Decision recorded; SPEC §1.3/§7 and README consistent with it; if option 2, compile checks exist for all four emitters.

## Scope
In: export.rs + SPEC/README wording. Out: re-adding the CLI `--export-tests` flag (separate decision).

## Size
small (downgrade) / medium (emitter).

## Provenance
Audit 2026-09-04, section 11, action item 28; evidence audits/2026-09-04/design-foundation.md §1 (Test generation), rec 6; usability-ui.md §4.
