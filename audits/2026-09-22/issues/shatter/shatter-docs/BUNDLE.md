# Bundle: shatter-docs (audit 2026-09-22)

- **Bucket:** shatter-docs. The theme is the accuracy of user and contributor documentation:
  - the SPEC changelog
  - artifact schemas and SPEC §5/§6
  - facts in the crate CLAUDE.md files
  - how many docs docs-smoke checks
  - the test-tier table
  - stories and drift gates
- **Repo / tracker:** shatter, via `bd` in /home/ketan/project/shatter (prefix `str`).
- **Parent epic:** "Epic: Audit 2026-09-22 findings".
- **Revision:** revised after the Codex cross-check (`issues/crosscheck/shatter-docs.codex.md`); see `REVISION.md` in this directory.
- **Evidence paths:** `audits/2026-09-22/...` files exist on branch `audit-2026-09-22` until the audit directory lands on `main`.
- **Status:** drafts only. Nothing has been filed. Under D6 the maintainer runs one filer script after reconciliation and the Codex cross-check.
- **Verified against:** the `audit-2026-09-22` worktree at 56c86168 (2026-09-23). Line numbers were re-checked there.

## Maintainer decisions (2026-09-23). These override the report and older drafts.

- **D1 Releases.** Keep x86_64-pc-windows-msvc and aarch64-unknown-linux-gnu in the release matrix and fix them: Z3 header/static link on Windows, openssl-sys under cross for aarch64. Release work closes only with a green release-run URL.
- **D2 shatter diff.** Retire snapshot `shatter diff` and the unused Snapshot writer path. spec-diff is the regression tool, and SPEC/README/QUICKSTART are updated to say so. Whether diff-scoped exploration takes the freed `diff` name is left to str-81xiw. The shatter-agents docs for `shatter diff --staged` are corrected to what exists today.
- **D3 Concolic positioning.** Measure first:
  - P1: a controlled default-vs-concolic benchmark, reported per release.
  - P1: fix concolic early termination.
  - A follow-up decision issue, blocked by both, decides the positioning. No doc softening now.
- **D4 Beads hook stall.** Retire the JSONL import and sync the tracker through a Dolt remote:
  - First, verify whether the stale JSONL import has been clobbering newer DB state.
  - AGENTS.md drops `bd sync`.
  - str-qwua7.28 is superseded.
  - Bento's beads-issue-flow gets matching guidance.
  - No hook-timeout env var and no bypass guidance.
- **D5 Git identity.** The leaked `[user]` section in the shared `.git/config` was already removed. Remaining work:
  - Add a `.mailmap` for test@example.com.
  - Add a git-state check (identity override, example.com email, core.bare, hooksPath).
  - Snapshot `.git/config` before and after the fixture entrypoints.
- **D6 Filing.** The maintainer runs one filer script after reconciliation and the Codex cross-check. Agents file nothing.

Decisions that touch this bucket:
- **D2:** artifact-json-schemas generates no Snapshot schema. spec-s5-contract-table-and-samples describes spec-diff as the regression tool and does not describe snapshot diff. The story seed list and the str-wurp note avoid `shatter diff`.
- None of the other decisions changes these drafts.

## Contents

| # | File | Kind | Target | Priority | Title | Blocked by |
|---|---|---|---|---|---|---|
| 01 | 01-spec-changelog-backfill.md | new | - | P2 | SPEC changelog claims §2.8/§2.9/§3.6 updates that were never made; the 09-14 and 09-20 CLI changes have no rows | - |
| 02 | 02-artifact-json-schemas.md | new | - | P2 | Publish JSON Schemas for Shatter's output artifacts, generated from the serde types and checked for drift | - |
| 03 | 03-spec-s5-contract-table-and-samples.md | new | - | P2 | SPEC §5: add an artifact producer/consumer table and replace the obsolete §5.1/§5.3/§5.6 samples with real output | artifact-json-schemas, spec-json-shapes-compare |
| 04 | 04-spec-s6-layout-and-checkpoint.md | new | - | P2 | SPEC §6.1/§6.2 describe stale progress-event fields, scan-id derivation and artifact layout | mixed-language-scan-deletes-artifacts |
| 05 | 05-crate-claude-md-stale-facts.md | new | - | P2 | Correct stale and false facts in the crate CLAUDE.md files (core, ts, go, rust) and the parity-matrix notes they cite | - |
| 06 | 06-docs-smoke-coverage.md | new | - | P3 | Extend docs-smoke to resource-parameters, distribution, execution-adapters, CI-INTEGRATION, PROJECT-LAYOUT and PROTOCOL.md | - |
| 07 | 07-test-tier-docs-overstate-coverage.md | new | - | P3 | CLAUDE.md test-tier table overstates what each tier covers; check-fast's description and the CI snapshot claim are stale | - |
| 08 | 08-qwua7-52-story-seed-list.md | note-to-existing | str-qwua7.52 | P2 | Proposed journey-level seed stories; storystore finds no clap CLI surfaces | - |
| 09 | 09-wurp-changelog-row-and-reverse-check.md | note-to-existing | str-wurp | P2 | Fold the flag-level, reverse and changelog-row checks into str-wurp's acceptance | - |
| 10 | 10-u394l-3-pending-turns-fail.md | note-to-existing | str-u394l.3 | P2 | A PENDING drift-patrol slot older than 60 days should turn FAIL | - |
| 11 | 11-qwua7-25-refresh-note.md | note-to-existing | str-qwua7.25 | P2 | Refreshed line-number and .js evidence for the frontend CLAUDE.md files | - |

Blockers in other buckets:
- spec-json-shapes-compare and mixed-language-scan-deletes-artifacts are code issues in other shatter buckets.
- The reopen note on closed str-qwua7.8 is `docs-first-run-reopen-note`, in bucket shatter-cli-runtime-output. It points to spec-changelog-backfill.

Reconciliation notes:
- spec-s6-layout-and-checkpoint is the SPEC half only. The checkpoint-path code fix belongs to mixed-language-scan-deletes-artifacts.
- crate-claude-md-stale-facts leaves the preflight_failed claims and dangling divergence IDs to str-qwua7.34. One claim from the original draft was dropped because it did not re-verify: PARITY.md's "30 s" Rust timeout.
- crate-claude-md-stale-facts no longer carries items owned by str-qwua7.25 (frontend line numbers, the two `.js` paths) or str-qwua7.24 (TS `ite` claim); the refreshed evidence for .25 is note 11 (qwua7-25-refresh-note). See REVISION.md.
- test-tier-docs-overstate-coverage leaves out the e2e-runs-twice item, which belongs to collapse-test-tiers.
- str-wurp already holds 2026-09-04 notes proposing similar checks. The new note asks for them to be promoted into acceptance, and adds the current drift list.

---

<!-- file: 01-spec-changelog-backfill.md -->

---
slug: spec-changelog-backfill
kind: new
title: "SPEC changelog claims §2.8/§2.9/§3.6 updates that were never made; the 09-14 and 09-20 CLI changes have no rows"
priority: P2
type: bug
labels: [docs, spec, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# SPEC changelog claims §2.8/§2.9/§3.6 updates that were never made; the 09-14 and 09-20 CLI changes have no rows

## Problem

The §8 changelog in `SPEC.md` says some sections were updated when they were not. Several recent CLI-visible changes have no row at all, and the "Last updated" header is stale. Readers and agents who use the changelog to find out what changed are sent to sections that still describe the old behaviour.

str-qwua7.8 was supposed to fix this. It was closed on 2026-09-14 with the bare reason "Closed", and its acceptance criteria were not met. A reopen note on str-qwua7.8 (slug `docs-first-run-reopen-note`, bucket shatter-cli-runtime-output) points to this issue. This issue is the fix; the note is only the record.

## Evidence

Re-verified against `audit-2026-09-22` at 56c86168 (2026-09-23):

- `SPEC.md:3` reads `Last updated: 2026-09-09`. `git log --format='%h %cs %s' -- SPEC.md` shows later SPEC commits that have no §8 row:
  - `8bd5a667` and `21981b1d`, 2026-09-14 (str-qwua7.8)
  - `c8bceb32`, 2026-09-14 (str-qwua7.9.2)
  - `2de05fd9`, 2026-09-20 (str-nfg4y, which corrected the spec-diff help and SPEC text)
- str-qwua7.15 changed help output on 2026-09-14 and also has no row.
- `SPEC.md:1178`, row 2026-08-10 (str-1fwt), claims sections "2.8, 2.9", but neither section describes that change:
  - §2.8 `shatter init` (`SPEC.md:475-493`) does not describe the `.gitignore` block that init manages, or implicit init.
  - §2.9 `shatter doctor` (`SPEC.md:532-536`) describes only the check for a stale embedded frontend.
  - `target/debug/shatter doctor --help` documents `-d, --directory <DIRECTORY>` and a check that "any configured output path (cache, seeds, artifacts, report) … `.gitignore` fails to cover (str-1fwt)", and says it exits non-zero on that condition.
- `SPEC.md:1181`, row 2026-07-18 (str-mktn), claims sections "2.9, 3.6". The only mention of `shatter.config.json` outside the changelog is `SPEC.md:326` (the `run` scope). §3.6 Configuration (`SPEC.md:715`ff.) never names `shatter.config.json` and does not give its precedence relative to `.shatter/config.yaml` and `--set`.

## Acceptance criteria

- [ ] §2.8 documents the `.gitignore` block that `shatter init` manages, and implicit init. It links the str-qwua7.58 decision.
- [ ] §2.9 `shatter doctor` documents `-d/--directory`, the gitignore-coverage check and the exit codes, and matches `shatter doctor --help` at the time of the change.
- [ ] The str-mktn §8 row claims doctor reports which config files are present and their precedence; `doctor --help` does not mention this. Run `shatter doctor` in a project with both `shatter.config.json` and `.shatter/config.yaml` present, paste the output into the close note, and then either document that report in §2.9 (if doctor prints it) or correct the str-mktn row's claim (if it does not). Leaving the row unchanged without that pasted output does not satisfy this item.
- [ ] §3.6 documents `shatter.config.json`: what it holds, and its precedence relative to `.shatter/config.yaml` and `--set`.
- [ ] §8 has rows for str-qwua7.8, str-qwua7.9.2, str-qwua7.15 and str-nfg4y. The `Last updated` header equals the date of the newest row.
- [ ] Every existing §8 row's "Section" column has been spot-checked against `git show <commit> -- SPEC.md` for the commit that row describes. False claims are corrected, and the PR description lists the rows that were checked.
- [ ] A check fails when the `Last updated` date in `SPEC.md` is older than the newest §8 row date. It can live in `scripts/docs-smoke.py` or a small script called from `task docs`.
  - Proof at close: a unit test (for example in `scripts/test_docs_smoke.py`) that fails on a fixture with a stale header and passes on a current one.
  - Proof at close: the check run directly (not through a cached `task` wrapper), with its output pasted into the close note.
- [ ] The close reason lists each acceptance item and how it was verified. A bare "Closed" is what went wrong with str-qwua7.8.

## Suggested approach

1. Rewrite §2.8, §2.9 (doctor) and §3.6 from the current `--help` output and the code: `shatter-cli/src/commands/init.rs`, the doctor command, and config loading.
2. Walk §8 top to bottom with `git log -p -- SPEC.md`, fixing section claims as you go.
3. Add the header-date check.

The broader mechanical gate (every clap flag documented in SPEC, every SPEC flag present in clap, a changelog row required when `args.rs` changes) is str-wurp. Do not build it here.

## Out of scope

- The CLI-surface drift gate (str-wurp).
- SPEC §5 (spec-s5-contract-table-and-samples) and §6 (spec-s6-layout-and-checkpoint).
- Removing `shatter diff` from SPEC §2.6, §2.11 and §5.5. That belongs to retire-snapshot-diff (D2), which adds its own §8 row.

## Dependencies

- Blocked by: none.
- Related: str-qwua7.8 (closed, reopen note), str-wurp, str-nfg4y, str-qwua7.15, str-qwua7.51 (require a close reason at landing), retire-snapshot-diff (also edits SPEC §8).

## Source

Audit 2026-09-22, finding docs-05 (confirmed, P2). Evidence is in `audits/2026-09-22/areas/docs.md` (on branch `audit-2026-09-22` until the audit directory lands on `main`).

---

<!-- file: 02-artifact-json-schemas.md -->

---
slug: artifact-json-schemas
kind: new
title: "Publish JSON Schemas for Shatter's output artifacts, generated from the serde types and checked for drift"
priority: P2
type: task
labels: [docs, schema, artifacts, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Publish JSON Schemas for Shatter's output artifacts, generated from the serde types and checked for drift

## Problem

No output artifact that Shatter writes has a published schema: not the spec bundles, the scan report JSON, the explore per-function artifacts, the scan `summary.json`/`manifest.json`/`run-status.json`, or the checkpoint. `protocol/GOVERNANCE.md` covers only frontend protocol messages. As a result:

- SPEC §5 samples are written by hand and have drifted from real output (spec-s5-contract-table-and-samples).
- Consumer commands each hard-code their own guess at the shape. `compare` rejects the `--spec-out` bundle (spec-json-shapes-compare).
- Plugin skills cite paths and shapes that Shatter never writes. For example, shatter-agents `interpret-shatter-spec` SKILL.md:27 expects `shatter-artifacts/<name>.spec.json`.

## Evidence

Re-verified at 56c86168:

- `protocol/schemas/` holds 22 `*.schema.json` files, all frontend-protocol types (request, response, sym-expr, type-info and so on), plus `test_schema_validation.py`. There are no output-artifact schemas.
- No crate depends on `schemars` (`grep -n schemars Cargo.toml */Cargo.toml` finds 0 matches).
- These are the serde types for the artifacts:
  - `shatter-core/src/spec.rs:259` `FileSpecBundle` (`{version, file, functions[]}`, written by `explore --spec-out`)
  - `shatter-core/src/spec.rs:304` `FunctionSpec`
  - `shatter-core/src/report.rs:1108` `ScanReport`
  - `shatter-core/src/scan_orchestrator.rs:774` `ScanSummary` (`summary.json`)
  - `shatter-core/src/status_export.rs:258` `RunStatus` (`run-status.json`)
  - `shatter-core/src/checkpoint.rs:29` `ScanCheckpoint`
  - `shatter-core/src/behavior.rs:214` `BehaviorMap`
  - the scan manifest writer in `scan_orchestrator.rs`
  - the explore per-function artifact written under `explore-results/` (`shatter-cli/src/commands/explore.rs:1067`)
- The real scan JSON keys include `discovered_inputs`, `behavior_clusters`, `constraint_stats` and `completion_outcome`, but SPEC §5.6 says the JSON holds "per-function behavior maps, coverage, and analysis". Samples are in `audits/2026-09-22/artifact-samples/`.

## Acceptance criteria

- [ ] JSON Schemas (draft 2020-12) are generated from the Rust serde types into `protocol/schemas/artifacts/`, not written by hand. `schemars` derive, or an equivalent, lives behind a feature or in a small generator binary or test. The set covers at least:
  - FileSpecBundle
  - FunctionSpec
  - the scan report
  - scan summary, manifest and run-status
  - the scan checkpoint
  - the explore per-function artifact
  - the staged-pipeline JSON outputs: Stage 1 observation JSON (`observe` / `explore` observation output), Stage 2 analysis JSON (`analyze --output`), and the `solve` output
  - BehaviorMap, if it is still a user-facing artifact after retire-snapshot-diff; otherwise record why it is omitted
- [ ] `protocol/schemas/artifacts/README.md` holds an inventory with one line per JSON artifact that spec-s5-contract-table-and-samples will list: either its schema file, or "no schema" with a reason (for example, no stable serde type yet). Non-JSON outputs (Markdown, HTML, text, YAML specs) are listed as "not applicable". spec-s5-contract-table-and-samples uses this inventory as its schema column, so the two issues cannot disagree.
- [ ] Each schema has `$id`, `title` and a `version` or `const` field that matches the artifact's own version field where one exists (for example `FileSpecBundle.version`).
- [ ] A `--check` mode regenerates into a temporary directory and fails on any difference. It is wired into `task parity` or `task check-static`, and its inputs are listed in that task's `sources:` so checksum caching cannot skip it after a type change.
  - Proof at close: a forced run (`--check` invoked directly, output pasted into the close note) that fails after a deliberate one-field change to `ScanSummary` and passes once the schema is regenerated.
- [ ] A test validates real producer output against the schemas. It runs `explore --spec-out`, a small `scan`, and the staged commands on `examples/` fixtures, then validates each written artifact that has a schema. Because it needs built frontends, it runs in `task e2e` (not `check-static`); name the task step in the close note and list the schema directory and the test file in that task's `sources:`. Proof at close: the validator fails on a hand-edited, non-conforming artifact (paste the failing run) and passes on real output (paste the direct, uncached run).
- [ ] `protocol/GOVERNANCE.md` (or a short `protocol/schemas/artifacts/README.md`) states that output-artifact shape changes require regenerating the schemas and updating the relevant SPEC §5 row.

## Suggested approach

1. Add `schemars` derives next to the existing `Serialize`/`Deserialize` derives on the types above, feature-gated so release builds do not pay for them.
2. Add a generator (a `#[test]` with an `UPDATE_SCHEMAS=1` mode, or a `shatter-core` example binary) plus `--check`.
3. Reuse the validation pattern in `protocol/schemas/test_schema_validation.py` for the producer-output test.

## Out of scope

- The SPEC §5 producer/consumer table and the rewritten samples. That is spec-s5-contract-table-and-samples, which this issue blocks.
- Making `compare` read bundles, and a shared spec reader (spec-json-shapes-compare).
- The `shatter diff` / Snapshot path, which is being retired (retire-snapshot-diff, D2). Do not generate a Snapshot schema.
- Frontend protocol schema cross-validation (str-2fjn, str-ndb.4).
- Pointing shatter-agents skills at the schemas. That is a follow-up in the shatter-agents epic once the schemas exist.

## Dependencies

- Blocked by: none.
- Blocks: spec-s5-contract-table-and-samples.
- Related: retire-snapshot-diff (D2: no Snapshot schema), spec-json-shapes-compare, str-qwua7.9, str-qwua7.21.1, str-wurp, str-2fjn.

## Source

Audit 2026-09-22, finding artifacts-10 (confirmed, P2). The schema half of draft `shatter-docs-ui/05`. Evidence is in `audits/2026-09-22/areas/artifacts.md` (on branch `audit-2026-09-22` until the audit directory lands on `main`).

---

<!-- file: 03-spec-s5-contract-table-and-samples.md -->

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
- [ ] Each table row gives:
  - the producing command and flag;
  - the output location: the default path under `shatter-artifacts/` where one exists, otherwise "stdout" or "caller-selected path (`<flag>`)" (for example the staged commands' `--output`);
  - the consuming commands (or "none");
  - the schema: the path under `protocol/schemas/artifacts/`, or "no schema: <reason>" / "not applicable (Markdown/HTML/text/YAML)", copied from the inventory in `protocol/schemas/artifacts/README.md` that artifact-json-schemas produces. No row may name a schema that does not exist in that directory, and every JSON row must have either a schema path or a stated reason.
- [ ] A docs-smoke (or unit-test) check parses the §5 table and fails if a named schema file is missing or a JSON row has an empty schema cell. Proof at close: the check fails when one schema path in the table is misspelled, and passes on the committed table (direct run, output pasted).
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

Audit 2026-09-22, findings docs-03 (docs half), docs-08 and artifacts-10 (all confirmed, P2). The SPEC half of draft `shatter-docs-ui/05`. Evidence is in `audits/2026-09-22/areas/docs.md` and `areas/artifacts.md` (on branch `audit-2026-09-22` until the audit directory lands on `main`).

---

<!-- file: 04-spec-s6-layout-and-checkpoint.md -->

---
slug: spec-s6-layout-and-checkpoint
kind: new
title: "SPEC §6.1/§6.2 describe stale progress-event fields, scan-id derivation and artifact layout"
priority: P2
type: bug
labels: [docs, spec, scan, artifacts, resume, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [mixed-language-scan-deletes-artifacts]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# SPEC §6.1/§6.2 describe stale progress-event fields, scan-id derivation and artifact layout

## Problem

SPEC §6 "Live Output and Resume" describes an older design:

- The progress-event fields in §6.1 do not match what `scan --progress` emits.
- The scan-id derivation in §6.2 is wrong.
- The artifact layout in §6.2 lists only `checkpoint.json` at a 16-hex prefix.

Anyone writing a CI consumer for `--progress`, or looking for a scan's artifacts, follows the SPEC and gets it wrong.

The code half is owned by mixed-language-scan-deletes-artifacts (P1): the checkpoint is written outside `scan_root()`, and each per-language sub-scan clobbers the others' artifacts. This issue documents the layout that exists after that fix. It is blocked by that issue so it describes one layout, not two.

## Evidence

Re-verified at 56c86168:

- **Scan-id derivation.** `SPEC.md:960-970` (§6.2) shows `shatter-artifacts/scan-results/<scan-id-prefix>/checkpoint.json`. It says the prefix is "the first 16 hex characters of a SHA-256 hash computed from the sorted list of source file paths". The code does not work that way:
  - `compute_scan_id_for_targets` in `shatter-core/src/checkpoint.rs:132` hashes `scan_id_v2:` plus (qualified_id, source_file) pairs.
  - `scan_root` (`scan_orchestrator.rs:557`ff.) resolves `resolve_artifact_root` (`shatter-core/src/harness_storage.rs:77`, which honours `SHATTER_ARTIFACT_DIR`) plus `scan-results/<full 64-hex id>/`.
  - `manifest.json`, `run-status.{json,tsv}`, `summary.json` and `functions/` are written under that `scan_root`.
- **Checkpoint location.** `ScanCheckpoint::default_path` (`shatter-core/src/checkpoint.rs:197`ff., called from `shatter-cli/src/commands/scan.rs:1203`) hard-codes `project_root/shatter-artifacts/scan-results/<first 16 hex>/checkpoint.json`. That is a second directory for the same scan. Observed directory: `scan-results/4dfd2b95…36f8/`.
- **Progress events.** `scan --progress` events carry `function` = `/abs/path/c.ts::classifyNumber` (a qualified id), plus `qualified_id` and `display_name`. SPEC's §6.1 example (`SPEC.md:900-954`) uses a bare function name and documents neither extra field. The auditor observed this by running the command; the verifier checked it at code level only. Re-run `shatter scan examples/ts --progress` to confirm before editing.

## Acceptance criteria

- [ ] §6.1 documents every field of each progress event type as emitted after mixed-language-scan-deletes-artifacts lands. `function` is the qualified id, and `qualified_id` and `display_name` are included. The example is pasted from a real `shatter scan <examples dir> --progress` run.
- [ ] §6.2 documents the full per-scan layout: `scan-results/<full id>/{checkpoint.json, manifest.json, summary.json, run-status.json, run-status.tsv, functions/}`. It also documents the v2 id derivation, the `SHATTER_ARTIFACT_DIR` override, and how a mixed-language scan is laid out.
- [ ] §6.3 resume semantics states what happens to checkpoints written at the old 16-hex location: migrated, or not resumed. This matches what mixed-language-scan-deletes-artifacts implemented.
- [ ] The §6 samples sit in tagged fences that docs-smoke checks. At minimum, the progress-event JSON lines must parse and contain the documented keys. Proof at close: docs-smoke fails when a documented key is removed from the sample. Run it directly and paste the output.
- [ ] A §8 changelog row is added and the `Last updated` header is bumped.

## Suggested approach

After the blocker lands, run a two-language scan with `--progress` and `SHATTER_ARTIFACT_DIR` set to a temp directory. Copy the real event lines and the `find` listing of the scan directory into §6, then trim.

## Out of scope

- The code changes: checkpoint under `scan_root()`, and the shared namespace for per-language sub-scans. Both belong to mixed-language-scan-deletes-artifacts.
- Keying resume by options and resume report parity (str-8q1b4).
- The §5 artifact table (spec-s5-contract-table-and-samples).

## Dependencies

- Blocked by: mixed-language-scan-deletes-artifacts.
- Related: str-8q1b4, str-ck05, spec-s5-contract-table-and-samples.

## Source

Audit 2026-09-22, finding docs-07 (confirmed, P2; docs half). Draft `shatter-docs-ui/06`. Evidence is in `audits/2026-09-22/areas/docs.md` (on branch `audit-2026-09-22` until the audit directory lands on `main`).

---

<!-- file: 05-crate-claude-md-stale-facts.md -->

---
slug: crate-claude-md-stale-facts
kind: new
title: "Correct stale and false facts in the crate CLAUDE.md files (core, ts, go, rust) and the parity-matrix notes they cite"
priority: P2
type: task
labels: [docs, agents, parity, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Correct stale and false facts in the crate CLAUDE.md files (core, ts, go, rust) and the parity-matrix notes they cite

## Problem

Claude Code injects the crate CLAUDE.md files into any agent that reads a file in that subtree, and the root CLAUDE.md tells dispatchers to rely on this. Each of the four files contains claims that are now false. Those claims send agents to the wrong engine, to dead code, or to a contract the code does not follow. The items below are the ones not already owned by the larger restructuring issues (str-qwua7.24 generated tables, str-qwua7.25 slimming), which have had no commits since 2026-09-04. Items those issues already own were removed from this draft; str-qwua7.25 gets a refreshed-evidence note instead (qwua7-25-refresh-note).

## Evidence

All lines were re-verified at 56c86168. Where this issue replaces a line-number reference, use the symbol name instead.

### shatter-core/CLAUDE.md
- `:7` reads "`explorer.rs` — Concolic exploration loop". explorer.rs is the random/hybrid engine. The concolic engine is `orchestrator.rs`, which `:17` describes only as "Multi-round exploration orchestration".
- `:3` ("export logic") and `:11` (`export.rs`) describe dead code. str-qwua7.59 deletes it. If .59 lands first, drop these lines there; otherwise drop them here.
- The Key Modules list has no entry for `solver.rs`, `strategy.rs`, `shrink.rs`, `pipeline_orchestrator.rs` or `planner_consumer.rs`.

### shatter-ts/CLAUDE.md
- `:375` says "str-jeen.40 will refine bucketing". str-jeen.40 is closed.
- The Key Files list names 2 of about 30 `src/` modules.
- **Not in this issue (already owned elsewhere):** the stale line ranges at `:16-17` and the `.js` paths at `:379-380` are str-qwua7.25's acceptance ("Line-number citations replaced by symbol names; the two .js references corrected"); the refreshed evidence goes to that issue as note `qwua7-25-refresh-note`. The "TS is the only frontend that produces `ite`" claim at `:44` is str-qwua7.24's acceptance ("The five stale claims above no longer appear").

### shatter-go/CLAUDE.md (54,640 bytes)
- `:57-58` name `instrument/flow.go` and `instrument/flowwalk.go` as the `ite` mechanism. `deadcode ./...` reports both as unreachable. The live path is `walkBodyForFlow` → `buildSymExprWithFlow` (see matrix `ite-symexpr-production-partial`).
- `:273` says `planner.ResolveMockSpecs` emits MockSpecs. `deadcode` reports it as unreachable.
- `str-8v66` and `str-ruw0` are cited in CLAUDE.md and in `shatter-go/planner/plan.go:100,128`. `bd show` returns "no issue found" for both.
- `:318` mentions `SHATTER_HARNESS_CACHE`, but no Go source reads that variable.
- `:138` reads "TS and Rust currently declare `outcome` only". This is stale (protocol-parity-13). str-qwua7.24 fixes the same claim in the TS and Rust files only; this Go-file sentence is not in its list. For example, TS's handshake `SUPPORTED_CAPABILITIES` (`shatter-ts/src/handlers.ts:56`ff.) declares analyze, execute, instrument, prepare, setup, teardown, generate and many `complex_type:*` entries. Restate which *Go-only* capabilities TS and Rust decline, and point to the matrix.
- The outcome status list omits `preflight_failed`. `:240` mentions `skipped_by_policy` separately, so check whether the list itself includes it.

### shatter-rust/CLAUDE.md, plus the matrix entries it points to
- `:109` and `protocol/parity-matrix.yaml:887` (`adapter_capabilities.async_runtime`) say `tokio::runtime::Runtime::new().block_on(...)`. The generated harness uses `tokio::runtime::Builder::new_current_thread()` at `shatter-rust/src/executor.rs:2547, 2845, 4942, 6801`, and does so deliberately.
- `:79` and `:93` cite a "single-file constraint". `shatter-rust/src/analyzer.rs:834-952` resolves same-crate cross-file types, and CLAUDE.md `:86` itself describes a cross-file enum E2E.
- (`:234-235` stale `handler.rs` line numbers are str-qwua7.25's line-number item; see note `qwua7-25-refresh-note`.)
- `protocol/parity-matrix.yaml:496` marks `rust: captured` for `console_output`. The same entry's notes (`:491-492`) and `protocol/PARITY.md:120` say the crate-bridge harness does not capture it, and `PARITY.md:109` still shows ✅. Mark it partial, with the crate-bridge exception.
- The matrix's axum adapter note (around `:897-907`) does not list the `Multipart` extractor, which the adapter handles.

### Dropped from the original draft
The claim that "PARITY.md:96 says 30 s but the Rust timeout fallback is 120 s" did not re-verify. `executor.rs:1017` `DEFAULT_BUILD_TIMEOUT_SECS = 120` is a build timeout. `PARITY.md:133-135` lists Rust's execution timeout default as 5 s, and "30" appears nowhere in PARITY.md.

## Acceptance criteria

- [ ] Every item under Evidence is corrected, or deleted in favour of a pointer to `protocol/parity-matrix.yaml`/`PARITY.md`. The close note lists each item with its disposition.
- [ ] Every line this issue edits refers to code by symbol name, not source line number. Removing the remaining line-number citations across the three frontend files is str-qwua7.25; do not expand into it here.
- [ ] `task parity` passes after the matrix edits (console_output partial, async_runtime flavour, axum Multipart). Run the validator directly and paste the output, because `task` results can be served from the checksum cache.
- [ ] A check fails when a crate CLAUDE.md cites a `src/…` path that does not exist, or a `str-*` ID that the tracker cannot resolve. Its home is the agent-rules drift lint, str-u394l.4, or a `task docs` step if .4 is not ready.
  - Proof at close: a unit test that fails on a fixture CLAUDE.md citing a nonexistent `src/…` path and `str-8v66`, and passes on the committed crate CLAUDE.md files (if str-qwua7.25 has not yet fixed the `.js` paths, the real files will still fail; land after .25's TS PR or allowlist those two paths with a pointer to .25).
  - Divergence-ID resolution is str-qwua7.34's check; do not duplicate it.

## Suggested approach

Work file by file. Verify each claim against `rg`/`deadcode`/`bd show`, then correct it or replace it with a pointer. Keep the edits minimal so they do not conflict with str-qwua7.25's later slimming.

## Out of scope

- Generating capability tables from the matrix (str-qwua7.24).
- Slimming the files to 5 KB, frontend line-number citations and the two `.js` paths (str-qwua7.25).
- The TS `ite` and TS/Rust "outcome only" claims (str-qwua7.24).
- The false "does not emit `preflight_failed`" claims (shatter-go/CLAUDE.md:206, `shatter-go/protocol/constants.go:16-19`, `types.go:584-590`) and the dangling divergence IDs (`loop-body-states-typescript-only`, `error-code-preflight-failed-typescript-only`, `rust-side-effects-not-captured`). These are already str-qwua7.34's acceptance; coordinate so they are not edited twice.
- Adding `.rs` to the `explore --help` extension list (`shatter-cli/src/args.rs:501`). That is a CLI help change, not a CLAUDE.md fact.

## Dependencies

- Blocked by: none.
- Related: str-qwua7.24, str-qwua7.25 (see note qwua7-25-refresh-note), str-qwua7.34, str-qwua7.59, str-u394l.4.

## Source

Audit 2026-09-22, findings core-21, frontend-ts-15, frontend-go-09, frontend-rust-08 and protocol-parity-13 (docs-24 goes to str-qwua7.25 via qwua7-25-refresh-note) (all confirmed). Draft `shatter-docs-ui/22`. Evidence is in `audits/2026-09-22/areas/{core-engine,frontend-ts,frontend-go,frontend-rust,protocol-parity,docs}.md` (on branch `audit-2026-09-22` until the audit directory lands on `main`).

---

<!-- file: 06-docs-smoke-coverage.md -->

---
slug: docs-smoke-coverage
kind: new
title: "Extend docs-smoke to resource-parameters, distribution, execution-adapters, CI-INTEGRATION, PROJECT-LAYOUT and PROTOCOL.md"
priority: P3
type: task
labels: [docs, smoke, quality-gates, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Extend docs-smoke to resource-parameters, distribution, execution-adapters, CI-INTEGRATION, PROJECT-LAYOUT and PROTOCOL.md

## Problem

`task docs-smoke` runs the examples in the docs against the built CLI to catch stale flags, removed commands and invalid JSON/YAML. It checks only the four docs it was created with. User-facing and contributor docs added since then are not checked, and PROTOCOL.md's JSON examples, which frontend authors copy, are never validated.

## Evidence

Re-verified at 56c86168:

- `scripts/docs-smoke.yaml:20-24` lists exactly `README.md`, `QUICKSTART.md`, `SPEC.md` and `docs/INDEX.md`.
- The `docs-smoke` task in `Taskfile.yml` (around `:334`) repeats the same four files in its `sources:`. Because task results are checksum-cached, a doc added to the YAML but not to `sources:` would not trigger a re-run when it changes.
- `docs/INDEX.md` lists these docs, none of which is covered (Audience per INDEX: PROJECT-LAYOUT "Users and contributors", distribution "Users and CI maintainers", resource-parameters "Users and contributors", execution-adapters "Contributors and architects", CI-INTEGRATION "Contributors"):
  - `docs/PROJECT-LAYOUT.md` (`:11`)
  - `docs/distribution.md` (`:12`)
  - `docs/resource-parameters.md` (`:25`)
  - `docs/execution-adapters.md` (`:29`)
  - `docs/CI-INTEGRATION.md` (`:26`)
- The auditor ran docs-smoke with resource-parameters, distribution, execution-adapters and PROJECT-LAYOUT added, and all four passed (4 + 4 + 3 blocks checked). Adding them is free today. The verifier did not re-run this.
- `PROTOCOL.md` has 22 ```` ```json ```` fences, none of which is validated against `protocol/schemas/`.

## Acceptance criteria

- [ ] `docs/resource-parameters.md`, `docs/distribution.md`, `docs/execution-adapters.md`, `docs/PROJECT-LAYOUT.md`, `docs/CI-INTEGRATION.md` and `PROTOCOL.md` are added to `scripts/docs-smoke.yaml` and to the `docs-smoke` task's `sources:`. `python3 scripts/docs-smoke.py` passes when run directly; paste the output into the close note.
- [ ] The `docs-smoke` task's `sources:` also include `protocol/schemas/*.schema.json`, so a schema-only change invalidates the checksum cache once PROTOCOL.md examples are validated against the schemas. Proof at close: after a passing run, touch one field in a schema and show `task docs-smoke` re-executes (not "up to date").
- [ ] Each PROTOCOL.md JSON example is either validated against a schema or explicitly skipped. Selection is by an explicit fence tag, not by guessing from keys: for example ```` ```json protocol=request ```` / ```` ```json protocol=response ```` (or a specific per-message schema name), and ```` ```json illustrative ```` for examples that are deliberately partial. Note that requests carry `command` while responses carry `status` (for example `PROTOCOL.md:18-19`), so a single key-based selector cannot work. An untagged ```` ```json ```` fence in PROTOCOL.md fails docs-smoke. Proof at close: docs-smoke fails when a required field is removed from one validated request example and from one validated response example (paste both failing runs), and passes on the committed text.
- [ ] A unit test in `scripts/test_docs_smoke.py` checks coverage by rules that the committed list can satisfy:
  - every doc in `docs/INDEX.md` whose Audience cell contains "Users" (case-insensitive) is in the docs-smoke list, unless it is in an explicit `exclude:` list in `docs-smoke.yaml` with a reason (subset rule, not equality);
  - the docs-smoke list may contain additional docs (for example `docs/INDEX.md` itself, `docs/execution-adapters.md`, `docs/CI-INTEGRATION.md`, `PROTOCOL.md`) that are not user-audience docs;
  - every doc in the docs-smoke list is also in the `docs-smoke` task's `sources:`.
  Proof: the test fails on a fixture INDEX that adds an uncovered Users-audience doc, and fails on a fixture Taskfile missing one listed doc from `sources:`; it passes on the committed files.

## Suggested approach

Add the docs first, since that is free. Then add a `schema:` mode to docs-smoke for PROTOCOL.md, driven by the fence tag, that loads the schemas with `jsonschema` (already used by `protocol/schemas/test_schema_validation.py`). Validating output-artifact samples in SPEC §5 is handled by spec-s5-contract-table-and-samples and can reuse this mode.

## Out of scope

- Artifact schemas and SPEC §5 samples (artifact-json-schemas, spec-s5-contract-table-and-samples).
- The CLI-surface drift gate (str-wurp).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.9, str-qwua7.9.1, str-qwua7.44, spec-s5-contract-table-and-samples.

## Source

Audit 2026-09-22, finding docs-19 (confirmed, P3). Draft `shatter-docs-ui/29`. Evidence is in `audits/2026-09-22/areas/docs.md` (on branch `audit-2026-09-22` until the audit directory lands on `main`).

---

<!-- file: 07-test-tier-docs-overstate-coverage.md -->

---
slug: test-tier-docs-overstate-coverage
kind: new
title: "CLAUDE.md test-tier table overstates what each tier covers; check-fast's description and the CI snapshot claim are stale"
priority: P3
type: task
labels: [docs, quality-gates, taskfile, agents, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# CLAUDE.md test-tier table overstates what each tier covers; check-fast's description and the CI snapshot claim are stale

## Problem

The Test Tiers table in the root `CLAUDE.md` is the main thing agents use to pick a gate. It says what each tier is for, but not what it runs, and some of those claims overstate coverage. The `check-fast` tier describes itself as the pre-push gate, but it is not, and CLAUDE.md does not mention it. CLAUDE.md also claims CI verifies snapshots by running the full `task check`. That claim is false while CI's `task check` executes no test leaves, which is the checksum-poisoning bug.

## Evidence

Re-verified at 56c86168:

- **Standard tier.** The table (`CLAUDE.md:20-30`) labels Standard (`task test-standard`) "Before committing". In `Taskfile.yml:103-111`, `test-standard` is `deps: [frontends-built, workspace-clippy]` plus `task: workspace-test`, which is `cargo test --workspace` over core, cli and llm. It runs no TypeScript, Go or rust-frontend unit tests.
- **CI snapshot claim.** `CLAUDE.md:13` says "Regression snapshots are checked into the repo and verified in CI by `.github/workflows/ci.yml` (runs the full `task check` landing gate …)". While the task checksum-poisoning bug stands (str-qwua7.3, audit slug task-list-json-poisons-checksums), CI's `task check` reports its test leaves "up to date" and executes none of them.
- **check-fast.** `Taskfile.yml:188`ff. describes `check-fast` as "Fast quality gate (pre-push: clippy + tests + TS + Go; …)". `scripts/setup-hooks.sh:164-172` selects only `check` (`SHATTER_FULL_PUSH=1` or gate rank 2) or `affected` for pre-push, never `check-fast`. `check-fast` is absent from CLAUDE.md and appears only in the heavyweight-slot list at `AGENTS.md:540`. The body of str-35vtk.25 (open) also calls `task check-fast` the feature-ref pre-push policy.

## Acceptance criteria

- [ ] The tier table gains a "Covers" column listing, for each tier, the crates/frontends and test kinds it runs (unit, proptest, E2E, snapshot, clippy).
- [ ] `scripts/test_test_tier_wiring.py` (it already exists and parses task bodies) validates the "Covers" column for the **fixed** tiers only: Quick, Standard, Full, E2E, Parity (and check-fast if it is kept). Its expansion from a tier to leaf tasks must follow all three ways this Taskfile invokes work, or it will miss real coverage:
  - `deps:` lists (for example `test-standard` → `frontends-built`, `workspace-clippy`);
  - `cmds:` entries of the form `- task: <name>` (for example `test-standard` → `workspace-test`, `check-governed` → `check-static`, `check-unit`, `check-integration`);
  - shell commands of the form `bash scripts/gate-wrapper.sh <gate> task <name>` (for example `check` → `check-governed`, `e2e` → `e2e-governed`). Any other shell command in a tier's expansion is mapped to a leaf by an explicit table in the test, and an unmapped command fails the test, so new wiring cannot be silently ignored.
  Included per-crate Taskfiles (`core:`, `cli:`, `ts:`, `go:`, `rust-fe:`) are followed through their namespaces.
- [ ] The Affected tier's row does not claim fixed coverage. Its "Covers" cell says it is selected from the diff by `scripts/affected-gates.py` (falling back to Full on unknown paths) and points there; the wiring test asserts that this cell names `affected-gates.py` and does not list crates.
- [ ] Proof at close: the test fails when `workspace-test` is removed from `test-standard`, fails when the table claims a frontend that a fixed tier does not reach, and fails on a fixture tier that invokes an unmapped shell command; it passes on the committed table. Paste the direct `python3 -m unittest scripts/test_test_tier_wiring.py` runs.
- [ ] `check-fast` is resolved one of two ways:
  - It is documented in the CLAUDE.md table with an accurate description, and its `desc:` no longer says "pre-push".
  - Or it is removed, together with its `gate-wrapper.sh` wiring and the AGENTS.md:540 mention.

  Either way, str-35vtk.25's body is corrected to match what `scripts/setup-hooks.sh` actually selects.
- [ ] The CI/snapshot sentence at `CLAUDE.md:13` states what CI actually runs today. Restore the stronger claim only when ci-executed-leaf-guard has landed and a CI run URL shows test leaves executing.

## Suggested approach

Extend the existing wiring test to parse `Taskfile.yml` and the included per-crate Taskfiles, expand each fixed tier through `deps`, `task:` cmds and `gate-wrapper.sh` invocations to leaf tasks, and map leaf names to a short coverage vocabulary. Keep the table's "Covers" cells in that vocabulary so the test can compare them.

## Out of scope

- E2E running twice in `pre-completion-e2e`. That belongs to collapse-test-tiers (report §15.1).
- Collapsing or renaming tiers (collapse-test-tiers). If that lands first, write the "Covers" column for the new tier set.
- Fixing the checksum poisoning itself (str-qwua7.3) and the CI executed-leaf guard (ci-executed-leaf-guard).

## Dependencies

- Blocked by: none. Only the CI-sentence restoration waits on ci-executed-leaf-guard.
- Related: collapse-test-tiers, ci-executed-leaf-guard, str-qwua7.2, str-qwua7.3, str-35vtk.25.

## Source

Audit 2026-09-22, findings gates-07 (partially confirmed; verifier corrected it to P3) and tests-ci-17 (confirmed, P3). Draft `shatter-docs-ui/21`, minus its e2e-duplication item. Evidence is in `audits/2026-09-22/areas/tests-ci.md` (on branch `audit-2026-09-22` until the audit directory lands on `main`).

---

<!-- file: 08-qwua7-52-story-seed-list.md -->

---
slug: qwua7-52-story-seed-list
kind: note-to-existing
title: "Note on str-qwua7.52: proposed journey-level seed stories, and the storystore inventory finds no clap CLI surfaces"
priority: P2
type: task
labels: [docs, stories, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.52
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.52 (open, P2: "Adopt storystore: initialise docs/stories, seed CLI stories, land str-u394l.3")

Target: `str-qwua7.52`. Post the text below as a comment with `bd comments add str-qwua7.52`. Do not change the issue's status or priority.

## Comment text

> **Audit 2026-09-22 (findings docs-11, goals-19): still unstarted, plus a proposed seed list.**
>
> `docs/stories/` still does not exist (`ls docs/stories` gives ENOENT at 56c86168). This issue has had no activity since the 2026-09-06 "Adopt" decision, and drift-patrol's `docs-stories` slot is still PENDING under str-u394l.3. Several 2026-09-22 findings are journey-level breakages that no per-command test catches: goals-01, 02 and 04; sandbox safety; the regression baseline; the resume layout. Story-level evidence checks are meant to surface exactly these.
>
> **Proposed seed stories.** This list is the auditor's proposal, not a maintainer decision. Trim or reorder freely. Each should cite existing evidence (walkthrough.yaml steps, e2e tests) and have an executable check.
>
> 1. **First exploration.** `shatter explore` on one function, then read the report (TS, Go and Rust).
> 2. **Safe execution of targets.** The default-deny host-write guard and the sandbox remedies, per frontend.
> 3. **Baseline, then change, then detect** (the core regression journey). Save a baseline with `explore --spec-out`, change the code, then detect the regression with `spec-diff` / `stale` locally and in CI. Per decision D2 (2026-09-23), spec-diff is the regression tool and snapshot `shatter diff` is being retired, so this story must not cite `shatter diff`.
> 4. **CI scan with failure thresholds.** `scan --fail-on-failures`, reports and exit codes.
> 5. **Resume an interrupted scan.** `--resume`; the checkpoint and the artifact layout.
> 6. **Functions that use live resources** (docs/resource-parameters.md).
> 7. **Offline staged pipeline.** analyze, then solve, then observe.
> 8. **Cross-language compare.** Compare the TS and Go implementations of one function.
> 9. **init / config / doctor.** First-time project setup.
> 10. **Agent-driven usage** through the shatter-agents plugin (run-shatter, interpret-shatter-spec).
> 11. **HTML report review.**
>
> **Tooling limitation (not a blocker for this issue or for str-u394l.3's basic gate).** storystore's inventory currently finds **0 `cli-command` surfaces** in shatter. Its only CLI extractor matches commander.js `.command('name')`. On this repo it detects go, javascript, rust and typescript but extracts only javascript and typescript, so Shatter's clap subcommands in `shatter-cli/src/args.rs` are invisible to it. The consequence is narrow: automatic *CLI-surface completeness* reporting from `stories-coverage` will show nothing uncovered until the storystore issue **clap-cobra-extractors** (storystore epic "Audit 2026-09-22 findings (storystore)") lands. Everything else can proceed now: `stories-init`, seeding, and str-u394l.3's recorded acceptance (directory and INDEX exist, the index is fresh, active stories carry evidence fields, patrol wiring). drift-patrol's `check_docs_stories` (`scripts/drift-patrol.py:453`ff.) already checks the index without any CLI extraction. Until the extractor lands, list the clap subcommands manually in the stories README if CLI completeness is wanted.
>
> Also link `docs/stories/INDEX.md` from `docs/INDEX.md` when it exists, as this issue already requires.

## Why a note and not a new issue

str-qwua7.52 already owns stories-init, seeding, the INDEX link and landing str-u394l.3. The audit adds a concrete journey list and records one external tooling limitation that affects only automatic CLI-surface completeness.

## Source

Audit 2026-09-22, findings docs-11 (confirmed, P2) and goals-19 (confirmed, P3). Both are duplicate-open of str-qwua7.52. Related: str-u394l.3, and the storystore clap-cobra-extractors issue (plugins-05).

---

<!-- file: 09-wurp-changelog-row-and-reverse-check.md -->

---
slug: wurp-changelog-row-and-reverse-check
kind: note-to-existing
title: "Note on str-wurp: fold the flag-level, reverse (SPEC to clap) and changelog-row checks into its acceptance; flag-level drift keeps returning"
priority: P2
type: task
labels: [docs, spec, governance, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-wurp
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-wurp (open, P2: "Mechanical CLI-surface drift gate: SPEC.md and gauntlet coverage vs clap definitions")

Target: `str-wurp`. Post the text below as a comment. The maintainer may also choose to move the acceptance items into the description. Do not change status.

Context for the filer: str-wurp's **description** still has only command-level acceptance. The 2026-09-04 audit appended an "Acceptance checks (to append)" block in NOTES, covering flag-level, reverse-direction and changelog-row checks, but the description was never updated and nothing has landed.

## Comment text

> **Audit 2026-09-22 (finding docs-06): third audit in a row to find flag-level SPEC drift.**
>
> Verified at 56c86168, the drift this gate would flag today:
> - `SPEC.md:634` (§2.11 exit codes) still names a nonexistent `--failure-threshold`. `shatter scan --help` has only `--fail-on-failures [<PERCENT>]` and `--fail-on-setup-error`. This is the **reverse** direction: a flag named in SPEC that clap does not define.
> - §2.9 `shatter doctor` (`SPEC.md:532-536`) omits `-d/--directory`.
> - §2.9 `shatter list-targets` (`SPEC.md:499-505`) omits `--scope`.
> - SPEC's `Last updated: 2026-09-09` header and §8 changelog miss the 09-14 and 09-20 SPEC and CLI changes (str-qwua7.8, str-qwua7.9.2, str-qwua7.15, str-nfg4y). The prose fixes are filed separately as spec-changelog-backfill.
>
> Please make these part of this issue's **acceptance criteria**, not just notes:
> 1. **Forward check:** every non-hidden long flag of every subcommand appears as a backticked token in that command's SPEC §2 section, or in an allowlist (for example "shared with explore").
> 2. **Reverse check:** every backticked `--flag` in SPEC §2 and §2.11 that is presented as a *Shatter* flag exists in clap for that command. This would have caught `--failure-threshold`. It must be context-aware, because SPEC legitimately names other tools' flags: for example `SPEC.md:176` (the explore flag table) says `--memory-limit` maps to Node's `--max-old-space-size`. Either (a) check only flag-table first columns and `shatter <cmd> …` usage lines, or (b) check every backticked flag but keep an explicit `external_flags:` allowlist (flag, SPEC location, owning tool, reason), seeded with `--max-old-space-size`. Unknown flags fail; allowlisted ones do not.
> 3. **Changelog-row rule**, as two separate requirements so neither can be satisfied alone:
>    - (a) *Row required:* if `shatter-cli/src/args.rs` or SPEC §2 changed relative to `origin/main`, SPEC §8 must gain at least one new row in that diff. Editing only the `Last updated` header does **not** satisfy this.
>    - (b) *Header consistent:* the `Last updated` date must equal the newest §8 row's date (a static check, which can run everywhere). Several rows may share a date, so same-day changes simply add another row with the same date; the header is unchanged in that case and still passes.
>    - Exemption: a commit trailer such as `Spec-Changelog: none (<reason>)` waives (a) only, never (b). The gate prints the reason.
>    (a) is diff-relative, so it belongs in `task affected` / pre-push and PR CI, not in static `check-static`. (b) can live in `check-static` (spec-changelog-backfill adds a first version of it in docs-smoke; reuse it rather than duplicate).
> 4. Take the inventory from clap itself (a `#[test]` walking `Cli::command()` subcommands and arguments), not by parsing help text.
>
> **Proof at close:** on the pre-fix HEAD, the gate reports **at least** the drift listed above (`--failure-threshold`, doctor `-d/--directory`, list-targets `--scope`), and every other item it reports is either real drift fixed in the same change or added to an allowlist with a reason — list them in the close note. It does not flag `--max-old-space-size`. It passes after the fix. A fixture diff that changes `args.rs` and only the `Last updated` header fails rule (a); a fixture with a new row but a stale header fails rule (b). drift-patrol's `cli-surface-drift` slot (`scripts/drift-patrol.py:437`) turns from PENDING into a real check. Show all of these with forced, direct runs, not cached `task` results.
>
> Note on D2 (2026-09-23): snapshot `shatter diff` is being retired (retire-snapshot-diff). The allowlist must not carry a `diff` entry once that lands.

---

<!-- file: 10-u394l-3-pending-turns-fail.md -->

---
slug: u394l-3-pending-turns-fail
kind: note-to-existing
title: "Note on str-u394l.3: drift-patrol PENDING slots unimplemented for 3+ months; a PENDING slot older than 60 days should turn FAIL"
priority: P2
type: task
labels: [quality-gates, drift-patrol, stories, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-u394l.3
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-u394l.3 (open, P2: "Stories coverage gate")

Target: `str-u394l.3`. Post the text below as a comment. Do not change status.

## Comment text

> **Audit 2026-09-22 (finding prior-21): the PENDING placeholders never expire.**
>
> drift-patrol still reports two placeholder slots, both open for more than 3 months:
> - `docs-stories`: PENDING, tracked by **this issue** (open since 2026-06-17; `scripts/drift-patrol.py:453`ff., `check_docs_stories`).
> - `cli-surface-drift`: PENDING, tracked by str-wurp (open since 2026-06-12; `scripts/drift-patrol.py:437`ff.).
>
> `docs/stories/` is still absent (verified at 56c86168), and str-qwua7.52 ("Adopt storystore", decided 2026-09-06) is still unstarted.
>
> By design, PENDING does not fail the patrol (`scripts/drift-patrol.py:25-32`). A PENDING slot only becomes FAIL in two cases: under `--strict-pending`, or when its tracking issue is *closed* (`:290-307`). A slot whose issue simply stays open is therefore a reminder that never escalates. Combined with the scheduled patrol not running (audit finding prior-01), nobody sees even the reminder.
>
> **Proposal (auditor's, for the maintainer to accept or reject):** add an age limit, with these semantics:
> - **Age source and precedence.** Each PENDING slot may carry a `pending_since: YYYY-MM-DD` in `drift-patrol.py`. If present, it is the age source and overrides the tracking issue's `created_at` (this is how a maintainer extends a slot: bump `pending_since` in a commit, leaving a visible record). If absent, the tracking issue's `created_at` is used.
> - **Boundary.** Age is whole days between the source date and the patrol run date (UTC). Age ≤ 60 stays PENDING; age ≥ 61 reports FAIL, with a message naming the issue, the age source and the age.
> - **Unavailable data.** If there is no `pending_since` and the tracker cannot be read (bd missing, issue not found), the slot stays PENDING with an explicit "age unknown: <reason>" message, and FAILs under `--strict-pending` as today. It never silently passes.
> - Acceptance: unit tests cover age 60 (PENDING), age 61 (FAIL), `pending_since` overriding an older `created_at` (PENDING), and tracker-unavailable without `pending_since` (PENDING with "age unknown"; FAIL under `--strict-pending`).
> - Acceptance: `docs/DRIFT-PATROL.md` documents the rule, including precedence and the extension procedure.
> - Result at close: under this rule, both slots above would fail today. That is the intended pressure, and it is resolved by landing this issue and str-wurp, or by explicitly re-dating them with `pending_since`.
>
> The rule is patrol-wide, not stories-specific. If the maintainer prefers, move it to a small child issue of the drift-patrol work (str-u394l). It is posted here because this issue owns one of the two stale slots.
>
> Not a blocker for this issue: storystore's inventory finds 0 clap CLI surfaces in shatter (see the note on str-qwua7.52 and the storystore issue clap-cobra-extractors). That limits only automatic CLI-surface completeness reporting. This issue's recorded acceptance (docs/stories with README and INDEX, three observed-mode stories, a check that fails on a missing directory or stale index, patrol wiring, contributor instructions) needs no CLI extraction, and `check_docs_stories` already validates the index. It can proceed now.

---

<!-- file: 11-qwua7-25-refresh-note.md -->

---
slug: qwua7-25-refresh-note
kind: note-to-existing
title: "Note on str-qwua7.25: refreshed line-number and .js evidence for the frontend CLAUDE.md files"
priority: P2
type: task
labels: [docs, agents, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.25
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.25 (open, P2: "Slim each frontend CLAUDE.md to ≤5 KB of rules; move per-issue history to docs/frontends/<lang>.md")

Target: `str-qwua7.25`. Post the text below as a comment with `bd comments add str-qwua7.25`. Do not change status or priority.

## Comment text

> **Audit 2026-09-22 (findings frontend-ts-15, frontend-rust-08, docs-24): still open, evidence refreshed at 56c86168.**
>
> This issue's acceptance already owns "line-number citations replaced by symbol names; the two .js references corrected". The re-audit found the same items still present, with drifted locations:
> - `shatter-ts/CLAUDE.md:16-17` cite "Lines ~278-352" (`buildSymExprWithFlow`) and "Lines ~860-951" (`buildSymExpr`). The real definitions are around `instrumentor.ts:874` and `:1862`. Replace with the symbol names.
> - `shatter-ts/CLAUDE.md:379-380` cite `src/browser-globals-recognizer.js` and `src/handlers.js`; the files are `.ts`.
> - `shatter-rust/CLAUDE.md:234-235` cite `src/handler.rs:552, 620` and "line 803". `last_file` is set at `handler.rs:693` and `:767` and read at `:842` and `:970`. Replace with symbol names.
>
> **Proof at close (suggested):** `rg -n ':[0-9]{2,}|Lines? ~?[0-9]' shatter-{ts,go,rust}/CLAUDE.md` returns nothing, pasted into the close note, and the three `.ts`/`.rs` paths resolve.
>
> The non-overlapping stale facts in the crate CLAUDE.md files (core explorer/orchestrator mislabel, Go dead `flow.go`/`ResolveMockSpecs` references, unresolved `str-8v66`/`str-ruw0`, Rust `Runtime::new()` vs `new_current_thread()`, matrix console_output/axum notes) are filed separately as crate-claude-md-stale-facts. Coordinate so the same lines are not edited twice.

## Why a note and not a new issue

The Codex cross-check pointed out that str-qwua7.25's committed acceptance already owns these items; excluding "slimming" from a new issue does not transfer them.

## Source

Audit 2026-09-22, findings frontend-ts-15, frontend-rust-08 and docs-24 (confirmed). Split from crate-claude-md-stale-facts during the cross-check revision.
