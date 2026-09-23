# Bundle: shatter-reports-and-specs (audit 2026-09-22)

- **Bucket:** shatter-reports-and-specs. Report and spec rendering/contract quality: invariants, YAML tags, scan headline, branch metric, spec shapes and preconditions, golden outputs.
- **Repo / tracker:** shatter; bd in /home/ketan/project/shatter (prefix str). Parent epic for new issues: "Epic: Audit 2026-09-22 findings".
- **Status:** final drafts. Nothing is filed. The maintainer runs one filer script after reconciliation and the Codex cross-check (D6).
- **Evidence paths** under `audits/2026-09-22/` exist on branch `audit-2026-09-22` (worktree `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`) until the audit reports land. Code line numbers were re-checked on 2026-09-23 at HEAD 56c86168.

## Maintainer decisions (2026-09-23)

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix and fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64). Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot-diff command and the unused Snapshot writer path; spec-diff is THE regression tool. SPEC/README/QUICKSTART are updated to match. Whether diff-scoped exploration (str-81xiw) takes the `diff` name is left to str-81xiw; the name becomes free. The shatter-agents plugin's `shatter diff --staged` docs are corrected to what exists today.
- **D3 Concolic positioning:** measure first. P1 controlled default-vs-concolic benchmark and P1 fix for concolic early termination; a follow-up decision issue (blocked by both) re-decides the "concolic-first" positioning. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import in shatter and move tracker sync to a Dolt remote; first verify whether importing the stale JSONL has been clobbering newer DB state; AGENTS.md drops `bd sync`; str-qwua7.28 is superseded; bento's beads-issue-flow gets matching guidance. No hook-timeout env var and no hook-bypass guidance.
- **D5 Git identity:** the leaked `[user]` section was already removed. Drafts: a `.mailmap` for the test identities (no history rewrite), a git-state check (local identity override / example.com email / core.bare=true / hooksPath override), and a `.git/config` snapshot in test_git_fixture_isolation.py.
- **D6 Filing:** one filer script, run by the maintainer. No agent files anything.

Decisions that touch this bucket: D2 (spec-diff is the only regression tool: raises the weight of 06 and 07, and 02/05/06 must keep spec-diff reading old bundles).

## Contents

| # | Slug | Kind | Target | P | Blocked by |
|---|---|---|---|---|---|
| 01 | invariant-markdown-blank-subjects | new | - | P2 | - |
| 02 | spec-yaml-custom-tags | new | - | P2 | - |
| 03 | scan-report-headline-and-paths | new | - | P2 | - |
| 04 | branch-metric-counts-sites | new | - | P2 | - |
| 05 | spec-json-shapes-compare | new | - | P2 | - (blocks spec-s5-contract-table-and-samples) |
| 06 | spec-preconditions-from-path-constraints | new | - | P2 | - |
| 07 | qwua7-38-spec-diff-false-negative | note-to-existing | str-qwua7.38 | P2 | - |
| 08 | golden-and-consumer-suite | new | - | P2 | gauntlet-scan-checker-consumes-json |
| 09 | known-answer-ratchet-and-ts-discriminants | new | - | P2 | - |
| 10 | source-bucket-fixture-dir | new | - | P3 | - |
| 11 | source-bucket-reopen-note | reopen-note | str-9awj | P3 | source-bucket-fixture-dir (filing order only) |
| 12 | control-bytes-in-reports | new | - | P3 | - |

Changes from the old drafts found while re-verifying: 03 adds the HTML Paths=branches fix (artifacts-13), a rendered-HTML check, and the code location of the Interesting Inputs rule; 06 fixes a stale cross-reference ("draft 38") to the right slugs; 07 corrects stale spec_diff.rs line numbers (now 134-141) and keeps P2; 08 moves the missing-snapshot item to snapshot-test-helpers and states that the existing insta tests are synthetic and pin current bugs; 09 notes that the examples corpus is the sibling repo /home/ketan/project/examples; 12 corrects the root cause: the control bytes come from unescaped thrown-error messages (report.rs:2479), not from input values, and adds a UTF-8 byte-slice panic in render.rs:280.

---

<!-- file: 01-invariant-markdown-blank-subjects.md -->

---
slug: invariant-markdown-blank-subjects
kind: new
title: "Spec markdown renders invariants with blank subjects and duplicate lines (\"-  != null [1] (100/100)\")"
priority: P2
type: bug
labels: [spec, report, invariants, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Spec markdown renders invariants with blank subjects and duplicate lines ("-  != null [1] (100/100)")

## Problem

`shatter explore --invariants --spec ts/01-arithmetic.ts:classifyNumber` prints this in the markdown spec:

```
**Function invariants:**
-  != null [1] (100/100)
-  != null [1] (100/100)
-  is non-empty [1] (100/100)
```

The subject is missing, so the reader cannot tell what is non-null. Two lines are identical. The JSON output of the same run has usable labels (for example `input.age is non-null`), and YAML renders `input is non-null`. Only the markdown renderer is broken. The feature is opt-in (`--invariants`), which is why this is P2: the audit finding artifacts-05 was filed at P1, and the verifier lowered it to P2.

## Evidence

Re-checked against the audit worktree (`/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`, HEAD 56c86168) on 2026-09-23:

- `shatter-core/src/spec.rs:568-577` (function-wide invariants) and `shatter-core/src/spec.rs:617-626` (per-class invariants) both format `"- {} [{}] ({}/{})"` from `ci.invariant.description, ci.confidence, ci.satisfied_count, ci.total_count`.
- `shatter-core/src/invariants.rs:144-146`: `format_path` is `path.join(".")`. For a scalar parameter or a return value the path is empty, so the description starts with an empty subject.
- `shatter-core/src/invariants.rs:638`: `ClassifiedInvariant.label` exists and holds the subject-qualified text. The markdown renderer does not use it.
- The spec.rs unit tests use synthetic descriptions such as `x > 0`, so no test runs the real detector through the markdown renderer.
- Captured sample: `audits/2026-09-22/artifact-samples/ts-spec-invariants.md:21-24` (the three lines above). The JSON of the same run is `ts-spec-invariants.json`. These paths are on branch `audit-2026-09-22` until the audit reports land.

Repro: `shatter explore --invariants --spec <examples>/standalone/ts/01-arithmetic.ts:classifyNumber`, then read the "Function invariants" block.

## Acceptance criteria

- [ ] The markdown renderer uses `ClassifiedInvariant.label` (or an equivalent subject-qualified string). For a scalar-parameter function every invariant line names its subject (`input`, `return`, or the parameter name).
- [ ] Duplicate invariant lines are removed. Invariants that only restate the class precondition (for example `input == 0` inside the zero class) are suppressed.
- [ ] A golden test runs the real invariant detector on `classifyNumber` with `--invariants --spec` and pins the markdown output. The test fails on current code (blank subject, duplicate line) and passes after the fix. Record the failing run in the close comment.
- [ ] The `[1] (n/n)` confidence suffix follows str-qwua7.61 (remove the confidence score). Whichever lands second rebases onto the other; the close comment says which.
- [ ] JSON and YAML invariant output are unchanged, or the change is recorded in the spec changelog.

## Suggested approach

Switch both render sites in `spec.rs` to `ci.label`. De-duplicate by label before rendering. Add a precondition-restatement filter keyed on the class's preconditions. Put the golden test next to the existing spec tests, or in the CLI golden suite if golden-and-consumer-suite has landed.

## Out of scope

- Removing the confidence score itself (str-qwua7.61).
- Changing invariant detection.

## Related

str-qwua7.61 (confidence score removal), str-qwua7.47. Source finding: artifacts-05 (confirmed; P1 lowered to P2 by the verifier).

---

<!-- file: 02-spec-yaml-custom-tags.md -->

---
slug: spec-yaml-custom-tags
kind: new
title: "Spec/properties YAML uses serde custom tags (!AllEqual) that standard YAML loaders reject"
priority: P2
type: bug
labels: [spec, serialization, properties, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Spec/properties YAML uses serde custom tags (`!AllEqual`) that standard YAML loaders reject

## Problem

`shatter properties ts/01-arithmetic.ts:classifyNumber` emits YAML like this:

```yaml
- !AllEqual
  param_index: 0
  value: 0
```

Python `yaml.safe_load` rejects it: `ConstructorError: could not determine a constructor for the tag '!AllEqual'`. Any consumer that uses a safe YAML loader cannot read Shatter's spec or properties YAML. The JSON form of the same data is `{"AllEqual":{...}}`, which parses fine.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 56c86168):

- `shatter-core/src/equivalence.rs:85-105`: `pub enum Precondition { AllPositive{..}, AllNegative{..}, AllZero{..}, AllEqual{..}, SameType{..} }` with plain `#[derive(Serialize, Deserialize)]`, which makes it externally tagged. serde_yaml renders externally tagged enum variants as YAML custom tags.
- `shatter-core/src/spec.rs:680-786`: the YAML view structs (`SpecClassYaml` has `preconditions: &'a Vec<Precondition>` at :691), `format_spec_yaml` (:765) and `format_file_spec_yaml` (:773) serialize those enums directly.
- The YAML tests in spec.rs (for example `yaml_bundle_includes_version` at :2077) use string-contains assertions and never parse the output with a standard loader.
- Captured sample: `audits/2026-09-22/artifact-samples/properties.yaml` has `- !AllEqual` at lines 14, 49 and 88 (on branch `audit-2026-09-22` until the audit reports land).

Repro: `shatter properties <examples>/standalone/ts/01-arithmetic.ts:classifyNumber > p.yaml && python3 -c 'import yaml; yaml.safe_load(open("p.yaml"))'`.

## Acceptance criteria

- [ ] Spec YAML and properties YAML load with a strict safe loader. A test runs PyYAML `safe_load` (or a Rust YAML parser with no custom-tag support) on the YAML output for every known-answer example. It fails on current code and passes after the fix; record both runs in the close comment.
- [ ] Every serialized enum in the spec model (`Precondition`, and any other externally tagged enum reachable from `FunctionSpec`/`FileSpecBundle`) uses an internally tagged form, for example `{kind: all_equal, param_index: 0, value: 0}`, in both JSON and YAML.
- [ ] The spec bundle schema version is bumped and the spec changelog has a row for the change.
- [ ] `spec-diff` (now the only regression tool, maintainer decision D2) still reads bundles written with the old shape, proven by a test that diffs an old-shape fixture against a new-shape one. If backward reading is dropped instead, the changelog and the error message say so.

## Suggested approach

Add `#[serde(tag = "kind", rename_all = "snake_case")]` to the enums, plus a compatibility deserializer (untagged fallback) for the old externally tagged form. Coordinate the version bump with spec-preconditions-from-path-constraints and spec-json-shapes-compare so the schema changes once, not three times.

## Out of scope

Redesigning what the preconditions mean (see spec-preconditions-from-path-constraints).

## Related

str-qwua7.38 (its description notes that preconditions are externally tagged enums). Source finding: artifacts-14 (confirmed).

---

<!-- file: 03-scan-report-headline-and-paths.md -->

---
slug: scan-report-headline-and-paths
kind: new
title: "Scan HTML report: headline shows 100% for 1 of 12 functions and 'Paths Found' counts branches; absolute temp paths, all-zero rows, undefined 'Interesting Inputs'"
priority: P2
type: bug
labels: [report, scan, html, ux, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Scan HTML report: headline shows 100% for 1 of 12 functions and "Paths Found" counts branches; absolute temp paths, all-zero rows, undefined "Interesting Inputs"

## Problem

A mixed TS+Go scan discovered 12 functions, attempted 5, completed 1 and failed 4 (7 were not attempted because the total budget ran out). The reports disagree about what happened:

1. **HTML headline misleads.** The HTML tiles read "Functions 1" (completed only), "Paths Found 3" and an unqualified "Coverage 100%". There is no discovered, attempted or failed count. The markdown report is correct: it leads with discovered/attempted/completed/failed and labels the 100% as "(completed-functions subset)". Only the HTML misleads.
2. **HTML "Paths" is really branches.** The HTML "Paths Found" tile and per-function "Paths" column show `branches_covered`. The markdown and stdout of the same run show 4 paths; the HTML shows 3. (Finding artifacts-13, folded in here.)
3. **Absolute temp paths everywhere.** Every table, the JSON `file_path` and `qualified_id`, and the artifact names contain absolute `/tmp/...` paths: 6 in the markdown, 29 in the JSON.
4. **All-zero rows.** The Source Set Summary prints seven buckets, six of them `0 | 0`.
5. **"Interesting Inputs" has no rule.** It lists 2 of the 4 inputs (`0 -> "zero"`, `-1 -> "negative"`) with no stated selection rule.
6. **Two different summaries.** With `-o`, the stdout `# Scan Results` table uses a single `/abs/path::fn` column while the written file uses separate Function/File columns.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 56c86168):

- `shatter-core/templates/scan_report.html:13-18`: the stat row has only Functions (`total_fn`), Paths Found (`total_paths`), Coverage (`overall_cov_bar_html`) and Skipped.
- `shatter-core/src/html_templates.rs:382`: `let total_paths: usize = report.functions.iter().map(|f| f.branches_covered).sum();` and `:428`: `paths_count: f.branches_covered,`.
- Captured run (on branch `audit-2026-09-22` until the audit reports land): `audits/2026-09-22/artifact-samples/scan-mix.{md,html,json,stdout}`. `scan-mix.md:3-9` has the correct discovered/attempted/completed/failed header; `scan-mix.md:14-22` has six all-zero Source Set rows; `scan-mix.md:71-76` is the Interesting Inputs block; `scan-mix.stdout` shows the `/tmp/...::classifyNumber | 4 | 100%` table.
- Interesting Inputs selection: `shatter-core/src/report.rs:2465-2468` keeps discovered inputs that threw or that `is_boundary_value` accepts. The rule exists in code but the report never states it.
- Existing HTML insta snapshot (`shatter-core/tests/html_snapshots.rs`, `shatter-core/tests/snapshots/`) renders a synthetic report and pins the current Paths=branches output, so it does not catch this.
- Coverage gap in the audit: the HTML was read as text only, never rendered in a browser.

## Acceptance criteria

- [ ] The HTML headline leads with "N of M functions completed" and shows discovered, attempted, failed and skipped counts. Coverage is labelled with its basis (completed subset or all discovered), matching the markdown wording.
- [ ] The HTML "Paths" tile and column show path counts, not `branches_covered`. If branch coverage is also shown, it is labelled as branches.
- [ ] Reports use project-relative paths. The JSON stores `project_root` once, and `file_path`/`qualified_id` are relative to it.
- [ ] Zero rows and empty sections are omitted from markdown and HTML.
- [ ] "Interesting Inputs" either states its selection rule in the report (for example "one per distinct outcome") or is removed.
- [ ] The stdout summary and the written file summary share one table shape.
- [ ] A new test builds one `ScanReport` (from a real scan of known-answer examples, not a synthetic struct) and asserts that the HTML, markdown and JSON agree on discovered/attempted/completed/failed counts and path counts. It fails on current code; record the failing and passing runs in the close comment. The HTML insta snapshot is updated.
- [ ] The rendered HTML is reviewed visually: a screenshot (or a bugshot gallery) of the before and after report for the mixed scan is attached to the close comment.

## Suggested approach

Pass the discovered/attempted/failed counts that `report.rs` already computes for markdown into the HTML template context. Fix `total_paths`/`paths_count` to use the path count field. Relativize paths once in the report model, not in each renderer. Reproduce the mixed scan with a small TS+Go directory that includes at least one Go function that fails or times out.

## Out of scope

- The artifact filename scheme and ENAMETOOLONG (separate finding cli-ux-07).
- Why the Go functions timed out.

## Related

str-3f27b, str-73pl, str-9q1z, str-qwua7.57; golden-and-consumer-suite (cross-format counts in general). Source findings: artifacts-15 (partially confirmed; narrowed to the HTML headline by the verifier), artifacts-13 (confirmed).

---

<!-- file: 04-branch-metric-counts-sites.md -->

---
slug: branch-metric-counts-sites
kind: new
title: "Progress and report 'N/N branches' counts branch sites, not sides; shows 2/2 while a side was never taken"
priority: P2
type: bug
labels: [report, progress, coverage, ux, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Progress and report "N/N branches" counts branch sites, not sides; shows 2/2 while a side was never taken

## Problem

The explore progress line and the reports say `2/2 branches` or `3/3 branches` even when one side of a branch was never taken. Users read that as full branch coverage. Observed examples:

- Go `Classify(x float64)` with nested `x > 0.5` / `x < 1`: progress `2/2 branches`, line coverage 80% (4/5). The true side of `x < 1` (`return "low"`) was never reached. Default and concolic engines behave the same.
- Go `lit.go:Classify`, concolic: `24 iters, 2 paths, 3/3 branches`, while 2 of 4 arms were never taken.
- TS `classifyNumber`: `30 iters, 4 paths, 3/3 branches` (`areas/artifacts.md:127`); here the count happens to be right, which hides the problem.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 56c86168):

- Concolic: `shatter-core/src/orchestrator.rs:2847` sets `branches_covered: Some(discoveries.len())` in the `ExploreProgressSnapshot`.
- Random explorer: `shatter-core/src/explorer.rs:1430` and `:2537` set `branches_covered: Some(aggregator.discoveries_count())`.
- `shatter-core/src/explorer.rs:375-380`: the snapshot documents `total_branches` as "total branches reported by static analysis" and `branches_covered` as "distinct branches covered so far (unique branch IDs with recorded discoveries)". Both count branch IDs (sites), not `(id, taken)` pairs.
- Audit evidence: `audits/2026-09-22/areas/artifacts.md:127`, `areas/frontend-go.md` (go-15 row) and findings core-18 and frontend-go-12 in `audits/2026-09-22/findings.json` (on branch `audit-2026-09-22` until the audit reports land).

## Acceptance criteria

- [ ] Progress lines and reports (markdown, HTML, JSON) show covered branch sides out of 2 × branch points, for example `3/4 branch sides`. Alternatively, the metric is renamed "branch points reached" everywhere, including SPEC and all reports. The close comment says which option was chosen.
- [ ] Known-answer tests in both engines (random explorer and concolic orchestrator; see CLAUDE.md "parallel parity") use a fixture with one untaken side and assert the shown count is below the total. They fail on current code; record both runs.
- [ ] Any stopping logic (plateau detection, worklist exhaustion) that keys on this metric is identified; if it exists, it uses sides. The close comment lists what was checked.
- [ ] `task e2e` (TS, Go and Rust concolic E2E) passes with forced execution (not a cached "up to date"); record the output.

## Suggested approach

Track `(branch_id, taken)` pairs in the discovery aggregator and the orchestrator's discovery set, and carry a sides total alongside `total_branches`. Update the SPEC wording for the metric in the same change.

## Out of scope

- Line-coverage denominator issues (finding goals-06, tracked separately).
- The resume-ignores-explorer-mode part of frontend-go-12 (handled separately) and its init empty-path part (str-qwua7.39).

## Related

str-9q1z, str-4o07, str-cii2. Source findings: core-18 (confirmed), frontend-go-12 (branch-metric part only).

---

<!-- file: 05-spec-json-shapes-compare.md -->

---
slug: spec-json-shapes-compare
kind: new
title: "Three incompatible spec JSON shapes; compare rejects the --spec-out bundle (the only clean producer)"
priority: P2
type: bug
labels: [spec, cli, artifacts, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Three incompatible spec JSON shapes; `compare` rejects the `--spec-out` bundle (the only clean producer)

## Problem

Shatter writes specs in three shapes:

- `explore --spec-out` writes a versioned `FileSpecBundle` (`{version, file, functions: [...]}`).
- `explore --spec-json` on stdout emits a bare `FunctionSpec`, after the markdown report (str-qwua7.11).
- `properties` emits a YAML list of bundles.

`compare` only deserializes a bare `FunctionSpec`, so it cannot read the `--spec-out` file, which is the only producer that writes a clean file. Users who follow the documented flow (explore TS and Go with `--spec-out`, then `compare`) get a parse error. `spec-diff` accepts bundles but, given a TS and a Go bundle, prints "Added functions: ClassifyNumber / Removed functions: classifyNumber" with no hint that `compare` is the right tool.

This is the code half of finding docs-03. The docs half (SPEC §5 producer/consumer table) is spec-s5-contract-table-and-samples, which this issue blocks.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 56c86168):

- `shatter-cli/src/commands/compare.rs:19-22`: both inputs are parsed with `serde_json::from_str::<shatter_core::spec::FunctionSpec>`.
- `shatter-core/src/spec.rs:252` (bundle version constant), `:259` (`pub struct FileSpecBundle`), `:304` (`pub struct FunctionSpec`).
- Captured output (on branch `audit-2026-09-22` until the audit reports land):
  - `audits/2026-09-22/artifact-samples/compare-ts-go.txt`: `Error: failed to parse spec A '.../ts-spec-out.json': missing field `function_name` at line 172 column 1` (exit 2).
  - `compare-ts-go-bare.txt`: with hand-extracted bare specs, `4 of 4 shared behaviors match — 100% equivalent`.
- The verifier reproduced `compare s.json s.json` on a `--spec-out` bundle exiting 2 while `spec-diff s.json s.json` exits 0.

Repro: `shatter explore --spec-out ts.json <examples>/standalone/ts/01-arithmetic.ts:classifyNumber`, the same for `go/01-arithmetic.go:ClassifyNumber` into `go.json`, then `shatter compare ts.json go.json`.

## Acceptance criteria

- [ ] One shared spec reader in `shatter-core` accepts a bundle, a bare spec and a bundle list. `compare`, `spec-diff`, `stale` and `revalidate` (where they read specs) all use it; the close comment lists each consumer switched.
- [ ] `compare --function A[=B]` selects functions from multi-function bundles. With one function per side, no flag is needed.
- [ ] `spec-diff` suggests `compare` when the added and removed function names differ only by case or language convention.
- [ ] A CLI round-trip test runs `explore --spec-out` for a TS and a Go known-answer function, then `compare` on the two files, and asserts success and "4 of 4". It fails on current code; record both runs.
- [ ] Error messages for an unrecognized shape name the accepted shapes.

## Suggested approach

Add `spec::read_specs(path) -> Vec<FunctionSpec>` (or similar) in core that detects the shape, then switch the consumers. Coordinate any schema-version change with spec-yaml-custom-tags and spec-preconditions-from-path-constraints.

## Out of scope

- The markdown-before-JSON stdout mixing of `--spec-json` (str-qwua7.11).
- SPEC documentation of the artifact table (spec-s5-contract-table-and-samples).
- The retired snapshot `shatter diff` command (maintainer decision D2; retire-snapshot-diff).

## Related

str-wfqh, str-nq20, str-qwua7.11; blocks spec-s5-contract-table-and-samples. Source findings: artifacts-09 (confirmed), docs-03 (code half).

---

<!-- file: 06-spec-preconditions-from-path-constraints.md -->

---
slug: spec-preconditions-from-path-constraints
kind: new
title: "Spec preconditions are sample statistics (often false or vacuous); derive them from the symbolic path constraints already recorded"
priority: P2
type: feature
labels: [spec, spec-diff, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Spec preconditions are sample statistics (often false or vacuous); derive them from the symbolic path constraints already recorded

## Problem

Class preconditions come from statistics over the observed samples (`AllEqual param[0] == -1`, `param[0] > 0`, `SameType number`), not from the path condition. They are sometimes false and often vacuous:

- In `fmt2`, the `n > 10` class shows the precondition `param[0] > 0`, although `n = 5` throws. The throw class gets `typeof param[0] == "number"`.
- In `regress/base.json`, class `returns "negative"` has `AllEqual{param_index: 0, value: -1}` with `sample_count: 20`: 20 samples of the same value become an equality precondition.

This also weakens `spec-diff`, which is now the only regression tool (maintainer decision D2). After swapping the even/odd outputs of a function, `spec-diff` reports `[PRECOND] Class 3 ... - param[0] == 2 + param[0] == 1` and `[INCONCLUSIVE]`, not "input 2 now returns positive-odd". The closed issue str-0oc promised constraint-derived preconditions; the prior audit (2026-09-04) raised this again.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 56c86168):

- `shatter-core/src/equivalence.rs:218` `fn derive_preconditions(all_inputs: &[Vec<serde_json::Value>])` builds `AllEqual` (:236), `AllZero` (:249), `AllPositive` (:255), `AllNegative` (:261) and `SameType` (:272) from input values only.
- `shatter-core/src/spec.rs:451-518` `build_spec_class` copies `ec.common_preconditions` into `SpecClass.preconditions` (:518). Provenance is `Proven` only if every branch in the path was discovered by Z3.
- Every raw result's `branch_path` already carries constraints, for example `{op: gt, left: {param n}, right: {const 10}}`.
- Captured evidence (on branch `audit-2026-09-22` until the audit reports land): `audits/2026-09-22/goals-runs/regress/base.json` (AllEqual preconditions with sample_count 20/20/7/13, all provenance `observed`); `audits/2026-09-22/artifact-samples/edge-plain-spec.md` (the fmt2 `param[0] > 0` class); `sd-v2.json` (`fail` gets `SameType number`). The verifier re-ran `shatter spec-diff base.json new.json` and got the `[PRECOND]` + `[INCONCLUSIVE]` output quoted above.

## Acceptance criteria

- [ ] Each class's precondition is the conjunction of its branch constraints, rendered as source-like text (for example `n < 0`, `n > 10 && n % 2 == 0`). Provenance marks these as symbolic.
- [ ] Sample statistics move to a separate "observed inputs" field. Preconditions shared by every class are suppressed.
- [ ] `spec-diff` re-executes, or cross-references, old examples against the new classes, so the even/odd swap is reported as a changed postcondition for a concrete input (`input 2: "positive-even" -> "positive-odd"`), not as a precondition change plus inconclusive.
- [ ] Known-answer tests pin the preconditions for `classifyNumber` (TS, Go, Rust) and for `fmt2`. They fail on current code; record both runs.
- [ ] A spec-diff known-answer test covers the even/odd swap and fails on current code.
- [ ] The spec schema version is bumped and the spec changelog has a row, and spec-diff still reads old bundles (or says clearly that it cannot).

## Suggested approach

Render the class's recorded path constraints through a small constraint printer (param names from the analysis). Keep the current statistics under a new `observed_inputs` field. Bump the schema once, together with spec-yaml-custom-tags and spec-json-shapes-compare. Coordinate the spec-diff pairing change with str-qwua7.38.

## Out of scope

- Behavior-based class pairing in spec-diff (str-qwua7.38).
- The YAML tag shape (spec-yaml-custom-tags).

## Related

str-0oc (closed, promised this), str-dcz, str-qwua7.38. Source findings: artifacts-08 (confirmed), goals-13 (confirmed).

---

<!-- file: 07-qwua7-38-spec-diff-false-negative.md -->

---
slug: qwua7-38-spec-diff-false-negative
kind: note-to-existing
title: "Note on str-qwua7.38: exact-BranchPath pairing causes false negatives, not only noise"
priority: P2
type: bug
labels: [spec-diff, audit-2026-09-22]
parent_epic: ""
blocked_by: []
existing_id: str-qwua7.38
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.38: exact-BranchPath pairing causes false negatives, not only noise

Target: **str-qwua7.38** (open, P2, "spec-diff: pair classes by behavior when branch paths are renumbered, not by exact BranchPath"). Action: append the comment below. Do not change the priority (stays P2; the audit finding artifacts-06 was filed at P1 and the verifier lowered it to P2 because the tool still exits non-zero in this example).

## Comment text

**Audit 2026-09-22 note (finding artifacts-06): pairing by exact BranchPath also hides real changes.**

This issue describes renumbered branch IDs as noise (unchanged behavior shows up as ADDED + REMOVED). The 2026-09-22 audit found that the same pairing can also pair two different behaviors and hide the real regression.

Weight: under maintainer decision D2 (2026-09-23), the snapshot `shatter diff` command is being retired and `spec-diff` is the only regression tool. A false negative in spec-diff is therefore a false negative in Shatter's whole regression story, which makes this issue more important than when it was filed.

Evidence (files under `audits/2026-09-22/artifact-samples/` on branch `audit-2026-09-22` until the audit reports land):

- v1 `grade`: `n < 0` -> invalid, `n < 50` -> fail, else pass. v2 inserts `n > 100` -> overflow.
- v1 `fail` and v2 `overflow` share `BranchPath [(0,F),(1,T)]`, so they are paired (`sd-v1.json`, `sd-v2.json`).
- `spec-diff` output (`sd-diff.txt`): `Summary: 2 added, 1 removed, 1 precondition(s) changed, 1 class(es) with insufficient comparison evidence`, then `[ADDED] Class 2 — returns "fail"`, `[ADDED] Class 4 — returns "pass"`, `[REMOVED] Class 3 — returns "pass"`, a `[PRECOND]` change and `[INCONCLUSIVE] Class 2`. The word `overflow` never appears, so the real regression (inputs > 100 now return `overflow` instead of `pass`) is not reported. The command still exits non-zero here only because of the unrelated REMOVED class.
- Pairing code today (re-checked 2026-09-23, HEAD 56c86168): `shatter-core/src/spec_diff.rs:134-141` builds `old_by_path`/`new_by_path` keyed on `&c.branch_path` and matches with `old_by_path.get(&new_class.branch_path)`; removed classes at `:201`. The `~93-97` line reference in this issue's description is stale.

Proposed additions to the acceptance criteria:

- Add this v1/v2 `grade` pair as a known-answer spec-diff test that must report a CHANGED postcondition for inputs > 100 (`pass` -> `overflow`). It fails on current code; record the failing run.
- Behavior pairing must check that paired classes agree on outcomes for shared concrete examples before reporting them as the same class; a path match alone must not suppress a postcondition change.
- Related new issue: spec-preconditions-from-path-constraints (spec-diff reports an even/odd output swap as a precondition change plus inconclusive, the same class of false negative).

---

<!-- file: 08-golden-and-consumer-suite.md -->

---
slug: golden-and-consumer-suite
kind: new
title: "Producer/consumer contract suite for CLI artifacts: CLI-driven golden outputs, cross-format counts, consumer round-trips"
priority: P2
type: task
labels: [testing, artifacts, report, quality-gates, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [gauntlet-scan-checker-consumes-json]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Producer/consumer contract suite for CLI artifacts: CLI-driven golden outputs, cross-format counts, consumer round-trips

## Problem

Most output bugs found by the 2026-09-22 audit are drift between a producer and its consumer:

- the gauntlet checker's regex against the scan summary (gauntlet-scan-checker-consumes-json);
- `compare` rejecting the `--spec-out` bundle (spec-json-shapes-compare);
- SPEC §5 samples vs real output (spec-s5-contract-table-and-samples);
- HTML "Paths Found" showing branches while markdown shows paths (scan-report-headline-and-paths);
- `explore -o x.json` writing an empty bundle (explore-o-json-empty-bundle);
- plugin skills documenting flags the CLI does not have.

No test tier owns output correctness. Output tests do exist, but they do not catch these bugs: the insta snapshot tests in `shatter-core/tests/` render synthetic in-memory reports, so they pin whatever the renderer does today, including current bugs. For example, the HTML snapshot pins "Paths Found" = sum of `branches_covered`. None of them runs the CLI on a known-answer example, compares counts across formats, or feeds one command's output to its consumer.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 56c86168):

- Snapshot tests: `shatter-core/tests/html_snapshots.rs`, `outcome_md_snapshots.rs`, `run_markdown_ordering_snapshots.rs`, `source_set_summary_snapshots.rs`, with files in `shatter-core/tests/snapshots/`. They build reports in memory.
- They pin a known bug: `shatter-core/src/html_templates.rs:382` computes `total_paths` as the sum of `branches_covered`, and the HTML snapshot records that output.
- `task golden-test` (Taskfile.yml, "Run cross-frontend parity golden tests") covers protocol goldens only.
- `shatter-cli/tests/` has contract tests (`json_stdout_contract.rs`, `exit_code_conventions.rs`, and others) but no terminal-output or report-output goldens for explore/scan/spec-out/spec-diff/compare.
- The snapshot helpers also silently create a missing snapshot and pass (`shatter-core/tests/outcome_md_snapshots.rs:45-51`); that is owned by snapshot-test-helpers, not by this issue.

## Acceptance criteria

- [ ] A new task (for example `task golden-cli`) runs `explore`, `scan`, `explore --spec-out`, `spec-diff` and `compare` on known-answer examples (TS `classifyNumber`, Go `ClassifyNumber`, Rust `classify_number`). It compares normalized output (paths made relative, timings and absolute temp dirs stripped) against checked-in goldens, and has an explicit update command.
- [ ] Cross-format assertion: for the same run, function counts (discovered/attempted/completed/failed), path counts and class counts are equal across markdown, JSON and HTML.
- [ ] Consumer round-trips: `explore --spec-out` -> `compare`; `explore --spec-out` -> `spec-diff`; scan JSON -> the gauntlet checker (as rewritten by gauntlet-scan-checker-consumes-json).
- [ ] Goldens that encode a known open bug are marked with the tracker id of that bug, so fixing the bug updates the golden deliberately.
- [ ] `scripts/affected-gates.py` selects the new task for changes to `shatter-core/src/report.rs`, `spec*.rs`, `html_templates.rs`, `shatter-cli/src/render.rs`, `shatter-core/templates/` and `demo/`. A selector test proves it.
- [ ] The new task is part of `task check`. Proof at close: a forced run (not a cached "up to date") showing the task executed and passed, plus one deliberately broken renderer change that makes it fail.

## Suggested approach

Build on the existing insta setup instead of adding a new framework: add CLI-driven snapshot tests that invoke the built binary on the examples repo, run a normalizer over the output, and snapshot the result. Add the cross-format and round-trip checks as ordinary tests in the same task.

## Out of scope

- Fixing the individual output bugs (filed separately in this bucket and others).
- Snapshot helper hygiene, such as failing on a missing snapshot under CI (snapshot-test-helpers).

## Dependencies

Blocked by gauntlet-scan-checker-consumes-json (the scan JSON -> gauntlet checker round-trip needs the rewritten checker).

## Related

str-qwua7.10, str-qwua7.53, str-qwua7.9, str-wurp, str-7jgm.3. Source finding: artifacts-18 (partially confirmed; the verifier noted that insta snapshot tests exist but are synthetic and pin current bugs, so this extends them rather than creating a tier from scratch).

---

<!-- file: 09-known-answer-ratchet-and-ts-discriminants.md -->

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

---

<!-- file: 10-source-bucket-fixture-dir.md -->

---
slug: source-bucket-fixture-dir
kind: new
title: "Source-bucket classifier labels production packages named 'fixture' as fixture_sample, zeroing the production denominator"
priority: P3
type: bug
labels: [scan, report, classification, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Source-bucket classifier labels production packages named `fixture` as fixture_sample, zeroing the production denominator

## Problem

Scanning zolem's `internal/fixture` package, which is production code (a fixture loader and selector), reports `source_set.fixture_sample {file_count: 10, line_count: 1279}`, `production_ish {0, 0}` and `productionish_source_lines: 0`. Every function gets `source_bucket: fixture_sample`. The production coverage denominator becomes zero, and coverage summaries for the package are meaningless.

This is the same class of bug as closed str-9awj, which fixed the same segment-name heuristic for `specs` directories. The heuristic is still applied to other names.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 56c86168):

- `shatter-core/src/source_bucket.rs:253-267`: `FIXTURE_DIRS = ["testdata", "test-data", "fixtures", "fixture", "examples", "example", "samples", "sample", "__fixtures__"]`, and `is_fixture_sample` returns true if **any** path segment matches. So `internal/fixture/loader.go`, `pkg/sample/...` and `internal/example/...` are all classified as fixtures, wherever they sit.
- Audit evidence (on branch `audit-2026-09-22` until the audit reports land): `audits/2026-09-22/goals-runs/zolem-fixture-default.json` has `codebase.source_set.fixture_sample = {file_count: 10, line_count: 1279}`, `production_ish = {0, 0}` and `codebase.productionish_source_lines = 0` (re-read 2026-09-23); write-up in `audits/2026-09-22/areas/goals.md` item 16 (goals-16).

## Acceptance criteria

- [ ] Classification follows conventions instead of any-segment names: `testdata/`, `__fixtures__/`, `fixtures/` inside a test tree, `*_test.go`, top-level `examples/`. A package directory named `fixture`, `sample` or `example` below a production source root, in Go, TS or Rust, is not a fixture by itself.
- [ ] `shatter.config.json` / `.shatter/config.yaml` can override the bucket per path glob, and the override is documented (README or SPEC config section).
- [ ] A unit test classifies a production `internal/fixture/loader.go` as `production_ish`, and a property or table test covers the remaining names. The test fails on current code; record both runs.
- [ ] The str-9awj regression (`internal/specs`) stays covered.

## Suggested approach

Anchor the fixture rule to test roots and the repo root (or the scan root), not to any segment. Add a config override map from glob to bucket, resolved before the heuristic.

## Out of scope

Other source-bucket categories (generated, declaration_only) unless they share the same any-segment rule.

## Related

str-9awj (closed; same class for `specs`, gets a reopen-note pointing here), str-jeen.37, str-jeen.38. Source finding: goals-16 (confirmed).

---

<!-- file: 11-source-bucket-reopen-note.md -->

---
slug: source-bucket-reopen-note
kind: reopen-note
title: "Note on closed str-9awj: the segment-name misclassification recurs for production packages named 'fixture'"
priority: P3
type: bug
labels: [scan, classification, audit-2026-09-22]
parent_epic: ""
blocked_by: [source-bucket-fixture-dir]
existing_id: str-9awj
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on closed str-9awj: the segment-name misclassification recurs for production packages named `fixture`

Target: **str-9awj** (closed, P2, "Specs bucket mislabels prod"). Action: add the comment below. Do not reopen; the follow-up work is the new issue source-bucket-fixture-dir (the filer substitutes its id). `blocked_by` above only means the new issue must be filed first so the comment can cite its id.

## Comment text

**Audit 2026-09-22 note (finding goals-16): the same class of bug recurs for `fixture`.**

This issue fixed production files under `internal/specs` being bucketed as `test_spec`. The fixture rule still uses the same any-segment heuristic: `shatter-core/src/source_bucket.rs:253-267` treats a path as `fixture_sample` if any segment is `testdata`, `test-data`, `fixtures`, `fixture`, `examples`, `example`, `samples`, `sample` or `__fixtures__`.

A zolem scan of its production package `internal/fixture` (a fixture loader and selector) reported `fixture_sample {10 files, 1279 lines}`, `production_ish {0, 0}` and `productionish_source_lines: 0`, so the production denominator for that package was zero.

Follow-up: <source-bucket-fixture-dir id> (convention-based classification, a per-glob config override, and a regression test for `internal/fixture/loader.go`). Please keep the `internal/specs` regression test from this issue when that change lands.

---

<!-- file: 12-control-bytes-in-reports.md -->

---
slug: control-bytes-in-reports
kind: new
title: "Markdown and HTML reports embed raw NUL and ESC bytes from generated inputs"
priority: P3
type: bug
labels: [report, security, ux, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Markdown and HTML reports embed raw NUL and ESC bytes from generated inputs

## Problem

A zolem scan's markdown report (`zolem-fixture-default.md`) contains 6 NUL bytes and 3 ESC (0x1b) bytes. `grep` treats the file as binary ("binary file matches"). The input values themselves are escaped correctly (`"\u0000"`), but the **error messages** thrown by the function under test echo the input back, and the report writes those messages raw. An input of `"\u001b[31m"` puts a real ANSI color escape into the report, so printing the report with `cat` can inject terminal escape sequences. Multi-line error messages also break the markdown list structure (continuation lines such as ` | ^` start at column 1). The verifier noted that the terminal-injection angle could justify P2; it is filed at P3 as the audit recommended.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 56c86168):

- `audits/2026-09-22/goals-runs/zolem-fixture-default.md` (on branch `audit-2026-09-22` until the audit reports land): a byte count gives 6 NUL and 3 ESC. Every one is inside a thrown-error message: lines 836-847 (`"\u0000"` and `"a\u0000b"` inputs, CEL error `token recognition error at: '<NUL>'` plus the echoed source line ` | <NUL>`) and lines 849-853 (`"\u001b[31m"` input, echoed as ` | <ESC>[31m`).
- Writer: `shatter-core/src/report.rs:2479` in the Interesting Inputs section: `writeln!(out, "- {inputs_str} -> **error:** {err}")`. `inputs_str` goes through `format_json_compact_list` (escaped); `err` (`thrown_error`) is written unescaped. Other places that print `thrown_error` or error messages into markdown, text or HTML likely do the same; list them at pickup (`/usr/bin/grep -rn thrown_error shatter-core/src shatter-cli/src`).
- The explore renderer's `value_short` (`shatter-cli/src/render.rs:277-284`) uses `serde_json::Value::to_string()`, which escapes controls, so values are not the problem there. But it truncates with `&s[..37]` (`render.rs:280`), a byte slice that panics when byte 37 falls inside a multi-byte UTF-8 character. The other `format_value_short` helpers (`compare.rs:265`, `explorer.rs:3259`, `export.rs:789`) should be checked for the same pattern.

Repro: scan zolem's `internal/fixture` package (or any function whose error message echoes a string input), then run `python3 -c 'import sys;b=open(sys.argv[1],"rb").read();print(b.count(b"\x00"), b.count(b"\x1b"))' report.md`.

## Acceptance criteria

- [ ] Error messages and any other free text from the target program (thrown errors, stdout/stderr captures, return strings shown unquoted) are escaped before they are written to markdown, text or HTML reports. The close comment lists every writer changed.
- [ ] All markdown, text and HTML renderers escape C0 controls other than `\n` and `\t`, and DEL, as `\u00XX` (HTML additionally entity-escapes as it does today). Multi-line error messages are indented or fenced so they stay inside their list item.
- [ ] A proptest asserts that rendered reports (markdown, text, HTML) for arbitrary string inputs are valid UTF-8 and contain no C0 control bytes other than newline and tab. It fails on current code; record both runs.
- [ ] Truncation helpers truncate on a char boundary; a test with a multi-byte string longer than the limit does not panic.

## Suggested approach

Add one shared `escape_display(&str) -> Cow<str>` in core and use it in every place that prints target-program text (error messages first, starting at `report.rs:2479`) or values into a report. Make the truncation helpers use `char_indices` or `floor_char_boundary`.

## Out of scope

JSON output (serde_json already escapes control characters).

## Related

Source finding: goals-17 (confirmed).
