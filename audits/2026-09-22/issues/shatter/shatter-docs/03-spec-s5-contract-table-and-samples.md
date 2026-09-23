---
slug: spec-s5-contract-table-and-samples
kind: new
title: "SPEC §5: add an artifact producer/consumer table and replace the obsolete §5.1/§5.3/§5.6 samples with real output"
priority: P2
type: task
labels: [docs, spec, artifacts, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [artifact-json-schemas, spec-json-shapes-compare]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# SPEC §5: add an artifact producer/consumer table and replace the obsolete §5.1/§5.3/§5.6 samples with real output

## Problem

SPEC §5 "Output Formats" describes outputs that the CLI no longer writes. It also never says which command produces each artifact or which commands consume it. Users and agents copy the samples and get shapes the tools reject. `docs/PROJECT-LAYOUT.md` has the same problem for the artifact tree.

## Evidence

Re-verified at 56c86168:

- **§5.1 default report.** §5.1 "Exploration Report (default)" (`SPEC.md:792-805`) shows `Explored: classifyNumber / Iterations: 50 / Unique paths: 4 / Lines covered … / New paths: [1] …`, in an untagged fence that docs-smoke never inspects. Real `shatter explore` output looks like this:
  - `# Shatter Explore`
  - `` ## `classifyNumber` *(c.ts:1-6)* ``
  - `**4 path(s)** · **100%** coverage (4/4 lines)`
  - a `| # | Call | Outcome |` table
  - a `**Summary:**` line

  Sample: `audits/2026-09-22/artifact-samples/ts-explore.md`.
- **§5.3 spec JSON.** §5.3 "Behavioral Specification (JSON)" (`SPEC.md:832-856`) shows a bare `FunctionSpec`. `explore --spec-out` actually writes a versioned `FileSpecBundle` `{version, file, functions[]}` (`shatter-core/src/spec.rs:259`), and SPEC never mentions the bundle.
  - `shatter compare s.json s.json` on a `--spec-out` file exits 2 with ``missing field `function_name` ``, because `shatter-cli/src/commands/compare.rs:19-22` deserializes `FunctionSpec` directly.
  - `spec-diff` on the same file exits 0.
- **§5.6 scan reports.** §5.6 (`SPEC.md:887-896`) says scan JSON contains "per-function behavior maps, coverage, and analysis". The real keys include `discovered_inputs`, `behavior_clusters`, `constraint_stats` and `completion_outcome`.
- **§5.5 snapshot.** §5.5 "Behavior Snapshot (JSON)" (`SPEC.md:864`ff.) documents the snapshot artifact of `shatter diff`. That command is being retired (D2, retire-snapshot-diff).
- **PROJECT-LAYOUT.** `docs/PROJECT-LAYOUT.md:249-252` says "The most clearly documented current artifact path in code is: `shatter-artifacts/recorded-mocks/`", and it omits `explore-results/` and `scan-results/`.

## Acceptance criteria

- [ ] §5 opens with a table that has one row per artifact. The rows cover:
  - the analysis, solve and observation JSON produced by the staged commands
  - FunctionSpec and FileSpecBundle
  - the Markdown and YAML specs
  - the scan report (md/json/html/txt)
  - the scan summary, manifest and run-status
  - the checkpoint
  - the explore per-function artifacts
  - behavior maps
- [ ] Each table row gives the producing command and flag, the default path under `shatter-artifacts/`, the consuming commands, and the schema path under `protocol/schemas/artifacts/` (from artifact-json-schemas).
- [ ] Regression checking is described as `shatter spec-diff` over `--spec-out` bundles (D2 decision). §5 does not describe a snapshot artifact or `shatter diff`. If retire-snapshot-diff has not yet removed §5.5 when this lands, remove or replace §5.5 here and coordinate the §8 row.
- [ ] §5.1, §5.3 and §5.6 samples are regenerated from real output on an `examples/` fixture. They sit in tagged fences (for example ```` ```json shatter-artifact=file-spec-bundle ````) that docs-smoke validates. At minimum, JSON samples are validated against their schema. Proof at close: docs-smoke fails when one field is deleted from the §5.3 sample, and passes on the committed text. Run `scripts/docs-smoke.py` directly and paste the output.
- [ ] §5.3 documents the FileSpecBundle, and the input shapes `compare`, `spec-diff` and `stale` accept, matching what spec-json-shapes-compare implements.
- [ ] `docs/PROJECT-LAYOUT.md`'s artifact section matches the real `shatter-artifacts/` tree: `explore-results/`, `scan-results/<id>/…`, `recorded-mocks/`. The §6.2 scan layout itself is described by spec-s6-layout-and-checkpoint; link to it rather than duplicating it.
- [ ] A §8 changelog row is added and the `Last updated` header is bumped.

## Suggested approach

Wait for the schemas and the shared spec reader to land. Generate every sample by running the CLI on `examples/ts/` fixtures and pasting the output, then write the table from the schema directory listing.

## Out of scope

- Generating the schemas (artifact-json-schemas).
- Changing `compare`'s input handling (spec-json-shapes-compare).
- Removing the `shatter diff` command and code (retire-snapshot-diff).
- SPEC §6 (spec-s6-layout-and-checkpoint).
- `--spec-json` stdout purity (str-qwua7.11).

## Dependencies

- Blocked by: artifact-json-schemas, spec-json-shapes-compare.
- Coordinate with: retire-snapshot-diff, which owns §2.6, §2.11 and §5.5 removal of `shatter diff` (D2).
- Related: str-qwua7.9, str-qwua7.9.1, str-wfqh, str-qwua7.11.

## Source

Audit 2026-09-22, findings docs-03 (docs half), docs-08 and artifacts-10 (all confirmed, P2). The SPEC half of draft `shatter-docs-ui/05`. Evidence is in `audits/2026-09-22/areas/docs.md` and `areas/artifacts.md`.
