# Publish output-artifact schemas and a SPEC §5 producer/consumer table; rewrite SPEC §5 samples from real output

- Priority: P2
- Type: task
- Labels: docs,spec,schema,artifacts
- Tracker action: new issue
- Related: str-qwua7.9, str-qwua7.21.1, str-wurp, str-wfqh, str-qwua7.11
- Source findings: audit 2026-09-22 artifacts-10, docs-03, docs-08 (all confirmed)

<!-- body -->
## Problem
None of Shatter's output artifacts has a published schema, and SPEC §5 describes outputs that do not match what the CLI writes. The artifacts affected are spec bundles, the scan report JSON, explore artifacts, scan `summary/manifest/run-status`, behavior maps and snapshots. Consumer commands and plugin skills each hard-code their own guess at the shape and have drifted.

## Evidence / current code facts
- `protocol/schemas/` holds 22 frontend-protocol schemas and no output-artifact schemas.
- `SPEC.md:792-805` (§5.1, "Exploration Report (default)") shows `Explored: classifyNumber / Iterations: 50 / Unique paths: 4 / New paths: [1] …`. Real output is `# Shatter Explore`, `## \`classifyNumber\` *(c.ts:1-6)*`, `**4 path(s)** · **100%** coverage (4/4 lines)`, a `| # | Call | Outcome |` table and `**Summary:**`. The fence is untagged, so docs-smoke cannot see it.
- §5.3 (`SPEC.md:832-856`) shows a bare `FunctionSpec`. `explore --spec-out` actually writes a versioned `FileSpecBundle` `{version,file,functions[]}` (`shatter-core/src/spec.rs:259-300`), and "bundle" is never mentioned in SPEC.
- `shatter compare s.json s.json` on a `--spec-out` bundle exits 2 with `missing field function_name` (`shatter-cli/src/commands/compare.rs:19-21` deserializes `FunctionSpec` directly). `spec-diff` on the same file exits 0.
- §5.6 claims scan JSON contains "per-function behavior maps". The real keys include `discovered_inputs`, `behavior_clusters`, `constraint_stats` and `completion_outcome`.
- `docs/PROJECT-LAYOUT.md:249-254` calls `recorded-mocks/` "the most clearly documented current artifact path" and omits `explore-results/` and `scan-results/`.

## Acceptance criteria
- JSON Schemas are generated from the serde types (e.g. `schemars`) into `protocol/schemas/artifacts/`, covering at least FileSpecBundle, FunctionSpec, the scan report, scan summary/manifest/run-status, the explore per-function artifact, BehaviorMap and Snapshot. A `--check` mode runs in `task parity` or `task docs-smoke` and fails on drift.
- SPEC §5 has a table with one row per artifact: producing command/flag, consuming commands, schema path.
- §5.1, §5.3 and §5.6 samples are regenerated from real output, with tagged fences validated by docs-smoke (at least schema validation of the JSON samples).
- PROJECT-LAYOUT's artifact section matches the real `shatter-artifacts/` tree.
- `compare` either accepts a FileSpecBundle (with `--function` to choose one), or SPEC documents that it needs a bare FunctionSpec and the error message says so.

## Suggested approach
Generate the schemas first, then write the SPEC table from them. The behaviour change to `compare` can be split into its own child issue if it grows.

## Scope
In: schemas, SPEC §5, PROJECT-LAYOUT artifacts section, `compare` input handling. Out: SPEC §6 layout (separate issue), and `shatter diff` having no snapshot producer (L5, tracked separately).
