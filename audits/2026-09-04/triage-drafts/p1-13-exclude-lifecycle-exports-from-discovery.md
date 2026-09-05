---
repo: shatter
type: bug
priority: 1
labels: discovery, scan, typescript, setup
existing: none
---
# Exclude setup/teardown lifecycle exports from target discovery

## Problem
Scans treat exported `setup`/`teardown` lifecycle helpers as ordinary targets and fuzz them; `teardown` throws on mismatched scope, producing 390 `Teardown scope mismatch` clusters across three gauntlet steps (87+ garbage clusters per run) and burying real results. Either discovery must exclude lifecycle exports, or the `*.shatter.setup.ts` convention must be enforced/documented so example files like `setup-file-level.ts` are never discovered as targets.

## Current code facts
- `shatter-core/src/discovery.rs:490-532` — `SESSION_SETUP_ROOT_PREFIX`, `FILE_SETUP_INFIX`, `SetupConfigOverride`, `discover_setup_files()` (str-0s76.3): recognised setup files are `shatter.setup.{ext}`, `.shatter/setup.{ext}`, `<stem>.shatter.setup.{ext}`. A plain `setup-file-level.ts` does not match, so its exports are targets.
- Gauntlet: `audits/2026-09-04/gates/gauntlet-excerpt.txt` — clusters under `### teardown` from `setup-file-level.ts` in Step 6 (133), Step 23 (132), Step 55 (125); `teardown` reports 75–100% coverage so no threshold fires.
- Walkthrough shows the same clusters (`gates/walkthrough-excerpt.txt:956-957, 2049-2059`).
- `demo/gauntlet-scan-allowlist.yaml` — no entry for this class.
- The example file lives in the external examples repo (matched by `demo/gauntlet-scan-allowlist.yaml:111-112` for other files), so the fix may need a discovery rule rather than a rename.

## Acceptance checks
- Discovery skips functions named `setup`/`teardown`/`beforeAll`/`afterAll`/`beforeEach`/`afterEach` (exact list decided and documented) when exported from a file that also exports a recognised setup-shaped API, OR the example is renamed to the `*.shatter.setup.ts` convention and `discover_setup_files()` gains a warning when a file exports lifecycle names without matching the convention. State which in the fix.
- Unit test in `discovery.rs` for the chosen rule; gauntlet Steps 6/23/55 show zero `Teardown scope mismatch` clusters.
- SPEC/README setup-file section documents the rule.

## Scope
In: discovery rule or convention enforcement + warning, tests, docs sentence, gauntlet re-run.
Out: the gate checker changes (item 8).

## Size
Small.

## Provenance
Audit 2026-09-04, section 11, action item 13; evidence audits/2026-09-04/usability-ui.md (§"Demo gates" Teardown, rec. 7), gates/gauntlet-excerpt.txt, gates/walkthrough-excerpt.txt.
