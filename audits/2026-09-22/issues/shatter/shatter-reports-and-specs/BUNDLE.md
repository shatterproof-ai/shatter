# Bundle: shatter-reports-and-specs (audit 2026-09-22)

- **Bucket:** shatter-reports-and-specs. Report and spec rendering/contract quality: invariants, YAML tags, scan headline, branch metric, spec shapes and preconditions, golden outputs.
- **Repo / tracker:** shatter; bd in /home/ketan/project/shatter (prefix str). Parent epic for new issues: "Epic: Audit 2026-09-22 findings".
- **Status:** final drafts, revised after the Codex cross-check (see REVISION.md). Nothing is filed. The maintainer runs one filer script after reconciliation and the Codex cross-check (D6).
- **Evidence paths** under `audits/2026-09-22/` exist on branch `audit-2026-09-22` (worktree `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`) until the audit reports land. Code line numbers were re-checked on 2026-09-23 at HEAD 56c86168 and again during the cross-check revision at audit-worktree HEAD 793f2b0b (code under `shatter-*` identical to 56c86168).

## Maintainer decisions (2026-09-23)

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix and fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64). Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot-diff command and the unused Snapshot writer path; spec-diff is THE regression tool. SPEC/README/QUICKSTART are updated to match. Whether diff-scoped exploration (str-81xiw) takes the `diff` name is left to str-81xiw; the name becomes free. The shatter-agents plugin's `shatter diff --staged` docs are corrected to what exists today.
- **D3 Concolic positioning:** measure first. P1 controlled default-vs-concolic benchmark and P1 fix for concolic early termination; a follow-up decision issue (blocked by both) re-decides the "concolic-first" positioning. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import in shatter and move tracker sync to a Dolt remote; first verify whether importing the stale JSONL has been clobbering newer DB state; AGENTS.md drops `bd sync`; str-qwua7.28 is superseded; bento's beads-issue-flow gets matching guidance. No hook-timeout env var and no hook-bypass guidance.
- **D5 Git identity:** the leaked `[user]` section was already removed. Drafts: a `.mailmap` for the test identities (no history rewrite), a git-state check (local identity override / example.com email / core.bare=true / hooksPath override), and a `.git/config` snapshot in test_git_fixture_isolation.py.
- **D6 Filing:** one filer script, run by the maintainer. No agent files anything.

Decisions that touch this bucket: D2 (spec-diff is the only regression tool: raises the weight of 06, 07 and 13; old spec bundles must stay readable, so 05 owns a versioned shared reader and 02/06 each add an upgrade step and a legacy fixture, with no "drop backward reading" option).

## Contents

| # | Slug | Kind | Target | P | Blocked by |
|---|---|---|---|---|---|
| 01 | invariant-markdown-blank-subjects | new | - | P2 | - |
| 02 | spec-yaml-custom-tags | new | - | P2 | spec-json-shapes-compare |
| 03 | scan-report-headline-and-paths | new | - | P2 | - |
| 04 | branch-metric-counts-sites | new | - | P2 | - |
| 05 | spec-json-shapes-compare | new | - | P2 | - (blocks spec-s5-contract-table-and-samples, spec-yaml-custom-tags, spec-preconditions-from-path-constraints) |
| 06 | spec-preconditions-from-path-constraints | new | - | P2 | spec-json-shapes-compare |
| 07 | qwua7-38-spec-diff-false-negative | note-to-existing | str-qwua7.38 | P2 | - |
| 08 | golden-and-consumer-suite | new | - | P2 | gauntlet-scan-checker-consumes-json, spec-json-shapes-compare, scan-report-headline-and-paths, pin-examples-repo |
| 09 | known-answer-ratchet-and-ts-discriminants | new | - | P2 | pin-examples-repo |
| 10 | source-bucket-fixture-dir | new | - | P3 | - |
| 11 | source-bucket-reopen-note | reopen-note | str-9awj | P3 | source-bucket-fixture-dir, source-bucket-config-override (filing order only) |
| 12 | control-bytes-in-reports | new | - | P3 | - |
| 13 | spec-diff-symbolic-region-verdicts | new | - | P2 | spec-preconditions-from-path-constraints |
| 14 | ts-union-discriminant-literals | new | - | P2 | - |
| 15 | qwua7-10-allowlist-issue-links-note | note-to-existing | str-qwua7.10 | P1 | - |
| 16 | source-bucket-config-override | new | - | P3 | source-bucket-fixture-dir |

Cross-check revision (2026-09-23): 09 was split into 09 (ratchet gate, same slug), 14 ts-union-discriminant-literals and 15 qwua7-10-allowlist-issue-links-note (note on str-qwua7.10, which already owns the allowlist schema); 10 was split into 10 (heuristic) and 16 source-bucket-config-override; 06 no longer owns spec-diff verdicts, which moved to the new 13 spec-diff-symbolic-region-verdicts (blocked by 06); 05 now owns the versioned shared reader and blocks 02 and 06; 08 is additionally blocked by 05, 03 and pin-examples-repo; 09 is blocked by pin-examples-repo. Earlier re-verification changes: 03 adds the HTML Paths=branches fix (artifacts-13); 07 corrects stale spec_diff.rs line numbers; 12 corrects the root cause to unescaped thrown-error messages (report.rs:2479). Full table in REVISION.md.

---

<!-- file: 01-invariant-markdown-blank-subjects.md -->

---
slug: invariant-markdown-blank-subjects
kind: new
title: "Spec markdown renders invariants with blank subjects (\"-  != null [1] (100/100)\"), so input and output invariants look like duplicates"
priority: P2
type: bug
labels: [spec, report, invariants, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Spec markdown renders invariants with blank subjects ("-  != null [1] (100/100)"), so input and output invariants look like duplicates

## Problem

`shatter explore --invariants --spec ts/01-arithmetic.ts:classifyNumber` prints this in the markdown spec:

```
**Function invariants:**
-  != null [1] (100/100)
-  != null [1] (100/100)
-  is non-empty [1] (100/100)
```

The subject is missing, so the reader cannot tell what is non-null. The first two lines look identical, but they are different invariants: one is about the input and one is about the output. The JSON output of the same run carries the correct subject-qualified text in `label` (`input is non-null`, `output is non-null`, `output is non-empty string`). The markdown renderer prints `invariant.description` instead, and for a scalar parameter or return value that description starts with an empty path.

The feature is opt-in (`--invariants`), which is why this is P2: the audit finding artifacts-05 was filed at P1 and the verifier lowered it to P2.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (`/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`, HEAD 793f2b0b; code under `shatter-core/` and `shatter-cli/` is identical to 56c86168):

- `shatter-core/src/spec.rs:568-577` (function-wide invariants) and `shatter-core/src/spec.rs:617-626` (per-class invariants) both format `"- {} [{}] ({}/{})"` from `ci.invariant.description, ci.confidence, ci.satisfied_count, ci.total_count`.
- `shatter-core/src/invariants.rs:143-146`: `format_path` is `path.join(".")`. For a scalar parameter or a return value the path is empty, so `description` starts with an empty subject (for example `" != null"`).
- `shatter-core/src/invariants.rs:630-645`: `ClassifiedInvariant.label` holds the subject-qualified text. The markdown renderer does not use it.
- The spec.rs unit tests use synthetic descriptions such as `x > 0`, so no test runs the real detector through the markdown renderer.
- Matching markdown and JSON from one input, captured during this revision with `target/release/shatter` built from the audit worktree:
  - `shatter explore --allow-host-writes --invariants --spec <examples>/standalone/ts/01-arithmetic.ts:classifyNumber` prints the three blank-subject lines above.
  - `shatter explore --allow-host-writes --invariants --spec --spec-json <same target>` gives function-wide invariants with `description` / `label` pairs `" != null"` / `input is non-null`, `" != null"` / `output is non-null`, and `" is non-empty"` / `output is non-empty string`. Per-class invariants follow the same pattern (for example `" == 0"` / `input == 0` in the "zero" class).
- Correction to the earlier draft: the audit sample `audits/2026-09-22/artifact-samples/ts-spec-invariants.json` describes `categorizeUser`, not `classifyNumber`, so it is not the JSON of the same run as `ts-spec-invariants.md`. Use the commands above as the evidence.

Repro: run the two commands above (`<examples>` is `/home/ketan/project/examples`; `--allow-host-writes` is needed when the sandbox is unavailable), then compare the "Function invariants" markdown block with the `invariants[].label` values in the JSON.

## Acceptance criteria

- [ ] Both markdown render sites in `spec.rs` (function-wide and per-class) print `ClassifiedInvariant.label`, not `invariant.description`. For `classifyNumber`, the function-wide block contains exactly three lines, with the subjects `input is non-null`, `output is non-null` and `output is non-empty string`. No rendered invariant line starts with a space after the `- ` bullet.
- [ ] A test runs the real invariant detector (not a hand-built `ClassifiedInvariant`) on `classifyNumber` through the markdown renderer and asserts the three labelled lines above. This can be a CLI test that runs `explore --invariants --spec` on the example, or a core test that feeds recorded executions to the detector and then to `format_spec_markdown`. Close-time proof: the test output failing on the unfixed code (blank subjects) and passing after the fix, both pasted into the close comment.
- [ ] The `[1] (n/n)` confidence suffix follows str-qwua7.61 (remove the confidence score). Whichever issue lands second rebases onto the other; the close comment says which landed first.
- [ ] The fix changes only the markdown renderer: the diff touches no serde attribute, JSON/YAML view struct or serializer, and the existing spec JSON/YAML tests pass unchanged. If a JSON or YAML change turns out to be needed, the spec schema version is bumped under the bump policy in `shatter-core/src/spec.rs:236-252` (its doc comment is the schema changelog).

## Suggested approach

Switch both render sites to `ci.label`. Put the test next to the existing spec tests in `shatter-core`, or in the CLI golden suite if golden-and-consumer-suite has landed.

## Out of scope

- Removing the confidence score itself (str-qwua7.61).
- Suppressing invariants that only restate a class precondition (for example `input == 0` inside the "zero" class). File separately if wanted; it depends on spec-preconditions-from-path-constraints.
- Fixing the blank-subject `description` field in JSON (consumers should read `label`).
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
blocked_by: [spec-json-shapes-compare]
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

Python `yaml.safe_load` rejects it: `ConstructorError: could not determine a constructor for the tag '!AllEqual'`. Any consumer that uses a safe YAML loader cannot read Shatter's spec or properties YAML. The JSON form of the same data is `{"AllEqual":{...}}`, which parses fine but uses a different shape from the rest of the spec model (`SymConstraint`, for example, is already internally tagged with `kind`).

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 793f2b0b; code identical to 56c86168):

- `shatter-core/src/equivalence.rs:85-105`: `pub enum Precondition { AllPositive{..}, AllNegative{..}, AllZero{..}, AllEqual{..}, SameType{..} }` with plain `#[derive(Serialize, Deserialize)]`, which makes it externally tagged. serde_yaml renders externally tagged enum variants as YAML custom tags.
- `shatter-core/src/execution_record.rs:17-25`: `SymConstraint` already uses `#[serde(tag = "kind", rename_all = "snake_case")]`, the target shape.
- `shatter-core/src/spec.rs:675-786`: the YAML view structs (`SpecClassYaml` has `preconditions: &'a Vec<Precondition>` at :691), `format_spec_yaml` (:765) and `format_file_spec_yaml` (:773) serialize those enums directly. The views are serialize-only; no Shatter command reads YAML back.
- The YAML tests in spec.rs (for example `yaml_bundle_includes_version` at :2077) use string-contains assertions and never parse the output with a standard loader.
- Captured sample: `audits/2026-09-22/artifact-samples/properties.yaml` has `- !AllEqual` at lines 14, 49 and 88 (on branch `audit-2026-09-22` until the audit reports land).

Repro: `shatter properties <examples>/standalone/ts/01-arithmetic.ts:classifyNumber > p.yaml && python3 -c 'import yaml; yaml.safe_load(open("p.yaml"))'`.

## Compatibility contract (maintainer decision D2)

`spec-diff` is the only regression tool, and CI baselines are spec JSON written by older builds. Old JSON must stay readable; there is no "drop backward reading" option.

- This change bumps `SPEC_SCHEMA_VERSION` by one (to N+1, where N is the value when this lands) and adds one upgrade step to the shared spec reader from spec-json-shapes-compare (this issue is blocked by it). The step rewrites externally tagged enum values from version N into the new internally tagged form. A plain `#[serde(untagged)]` fallback on the enum is not enough on its own, because the reader must know which version it upgraded from.
- Legacy fixture: the version-N bundle is added to `shatter-core/tests/fixtures/spec-legacy/` before the shape changes.
- Mixed versions: `spec-diff old(vN).json new(vN+1).json` gives the same classes, verdicts and exit code as `spec-diff` on two vN+1 files produced from the same sources, and additionally reports the existing `SpecVersionMismatch` note.
- YAML is output-only. It gets the new shape with no compatibility path.

## Acceptance criteria

- [ ] Spec YAML and properties YAML load with a strict safe loader. A test runs PyYAML `safe_load` (or a Rust YAML parser configured to reject unknown tags) on the YAML output for TS `classifyNumber`, Go `ClassifyNumber` and Rust `classify_number`, and on a synthetic spec that contains every `Precondition` variant. Close-time proof: that test failing on current code and passing after the fix, both pasted into the close comment.
- [ ] Every enum reachable from `FunctionSpec` / `FileSpecBundle` / the YAML views that serializes externally tagged today uses `#[serde(tag = "kind", rename_all = "snake_case")]` (for example `{kind: all_equal, param_index: 0, value: 0}`) in both JSON and YAML. The close comment lists each enum changed; a test fails if any YAML output contains a `!` tag.
- [ ] `SPEC_SCHEMA_VERSION` is bumped by one, and its doc comment (`shatter-core/src/spec.rs:236-252`, which is the schema's changelog under the bump policy there; there is no separate spec changelog file) gets a `- vN+1:` line naming this issue.
- [ ] Legacy reading: `spec-diff` on the version-N legacy fixture against a fresh bundle of the same source exits 0 with no ADDED, REMOVED, PRECOND or CHANGED rows, and prints the version-mismatch note. `compare` on the legacy fixture and a fresh bundle reports 100% equivalent. Both tests are in the test suite, not only run once.
- [ ] `task affected` passes, and its `Gates selected` output is recorded in the close comment.

## Suggested approach

Change the serde attributes, add the upgrade step to the shared reader, and add a YAML-parse test helper that shells out to `python3 -c 'import yaml,sys; yaml.safe_load(sys.stdin)'` or uses a strict Rust parser. If spec-preconditions-from-path-constraints is in flight at the same time, land one first; the second bumps the version again and adds its own upgrade step and fixture.

## Out of scope

- Redesigning what the preconditions mean (spec-preconditions-from-path-constraints).
- Reading YAML specs.

## Related

str-qwua7.38 (its description notes that preconditions are externally tagged enums). Blocked by spec-json-shapes-compare (shared versioned reader). Source finding: artifacts-14 (confirmed).

---

<!-- file: 03-scan-report-headline-and-paths.md -->

---
slug: scan-report-headline-and-paths
kind: new
title: "Scan HTML report: headline shows 100% for 1 of 12 functions and 'Paths Found' counts branches; absolute temp paths in rendered tables, all-zero rows, undefined 'Interesting Inputs'"
priority: P2
type: bug
labels: [report, scan, html, ux, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Scan HTML report: headline shows 100% for 1 of 12 functions and "Paths Found" counts branches; absolute temp paths in rendered tables, all-zero rows, undefined "Interesting Inputs"

## Problem

A mixed TS+Go scan discovered 12 functions, attempted 5, completed 1 and failed 4 (7 were not attempted because the total budget ran out). The reports disagree about what happened:

1. **HTML headline misleads.** The HTML tiles read "Functions 1" (completed only), "Paths Found 3" and an unqualified "Coverage 100%". There is no discovered, attempted or failed count. The markdown report is correct: it leads with discovered/attempted/completed/failed and labels the 100% as "(completed-functions subset)". Only the HTML misleads.
2. **HTML "Paths" is really branches.** The HTML "Paths Found" tile and per-function "Paths" column show `branches_covered`. The markdown and stdout of the same run show 4 paths; the HTML shows 3. (Finding artifacts-13, folded in here.)
3. **Absolute temp paths everywhere.** Every table, the JSON `file_path` and `qualified_id`, and the artifact names contain absolute `/tmp/...` paths: 6 in the markdown, 29 in the JSON. This issue fixes the rendered tables only; the JSON identity fields stay as they are (see Out of scope).
4. **All-zero rows.** The Source Set Summary prints seven buckets, six of them `0 | 0`.
5. **"Interesting Inputs" has no rule.** It lists 2 of the 4 inputs (`0 -> "zero"`, `-1 -> "negative"`) with no stated selection rule.
6. **Two different summaries.** With `-o`, the stdout `# Scan Results` table uses a single `/abs/path::fn` column while the written file uses separate Function/File columns.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 793f2b0b; code identical to 56c86168):

- `shatter-core/templates/scan_report.html:13-18`: the stat row has only Functions (`total_fn`), Paths Found (`total_paths`), Coverage (`overall_cov_bar_html`) and Skipped.
- `shatter-core/src/html_templates.rs:382`: `let total_paths: usize = report.functions.iter().map(|f| f.branches_covered).sum();` and `:428`: `paths_count: f.branches_covered,`.
- Captured run (on branch `audit-2026-09-22` until the audit reports land): `audits/2026-09-22/artifact-samples/scan-mix.{md,html,json,stdout}`. `scan-mix.md:3-9` has the correct discovered/attempted/completed/failed header; `scan-mix.md:14-22` has six all-zero Source Set rows; `scan-mix.md:71-76` is the Interesting Inputs block; `scan-mix.stdout` shows the `/tmp/...::classifyNumber | 4 | 100%` table.
- Interesting Inputs selection: `shatter-core/src/report.rs:2465-2468` keeps discovered inputs that threw or that `is_boundary_value` accepts. The rule exists in code but the report never states it.
- Existing HTML snapshot test (`shatter-core/tests/html_snapshots.rs` with files under `shatter-core/tests/snapshots/`; a hand-written file-comparison helper, not the insta crate) renders a synthetic report and pins the current Paths=branches output, so it does not catch this.
- `shatter-core/src/report.rs:448-460`: `qualified_id` is documented as the stable function identifier (`"<source_file>::<bare_name>"`), the same ID the call graph emits as `function_id` and the scan orchestrator uses to key `analysis_map`, `file_map` and `behavior_maps`. It is an identity key, not a display string, so this issue must not change it.
- Coverage gap in the audit: the HTML was read as text only, never rendered in a browser.

## Acceptance criteria

- [ ] The HTML headline leads with "N of M functions completed" and shows discovered, attempted, failed and skipped counts. Coverage is labelled with its basis (completed subset or all discovered), matching the markdown wording.
- [ ] The HTML "Paths" tile and column show path counts, not `branches_covered`. If branch coverage is also shown, it is labelled as branches.
- [ ] Rendered text uses project-relative display paths: every path shown in the markdown, HTML and stdout tables is relative to the project root (falling back to the absolute path only for files outside it). The JSON identity fields `file_path` and `qualified_id` keep their current values and meaning; the JSON may add a `project_root` field and a separate display field, but no existing field changes. A test asserts that the markdown and HTML for a scan under a temp directory contain no occurrence of the temp directory prefix, and that `qualified_id` values are unchanged from the pre-fix JSON.
- [ ] Zero rows and empty sections are omitted from markdown and HTML.
- [ ] "Interesting Inputs" either states its selection rule in the report (for example "one per distinct outcome") or is removed.
- [ ] The stdout summary and the written file summary share one table shape.
- [ ] A new test builds one `ScanReport` (from a real scan of known-answer examples, not a synthetic struct) and asserts that the HTML, markdown and JSON agree on discovered/attempted/completed/failed counts and path counts. It fails on current code; record the failing and passing runs in the close comment. The HTML snapshot file is regenerated in the same commit, and the diff of that snapshot is shown in the close comment.
- [ ] The rendered HTML is reviewed visually: a screenshot (or a bugshot gallery) of the before and after report for the mixed scan is attached to the close comment.

## Suggested approach

Pass the discovered/attempted/failed counts that `report.rs` already computes for markdown into the HTML template context. Fix `total_paths`/`paths_count` to use the path count field. Compute display paths once (a helper that takes the report's project root and a `file_path`) and call it from each renderer; do not rewrite the stored identity fields. Reproduce the mixed scan with a small TS+Go directory that includes at least one Go function that fails or times out.

## Out of scope

- The artifact filename scheme and ENAMETOOLONG (separate finding cli-ux-07).
- Why the Go functions timed out.
- Making `qualified_id` or JSON `file_path` project-relative. That changes a stable identity key used by the call graph, the scan orchestrator and downstream consumers (`report.rs:448-460`), and needs its own migration issue with a schema bump and consumer inventory. File it separately if wanted.

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

Re-checked on 2026-09-23 in the audit worktree (HEAD 793f2b0b; code identical to 56c86168):

- Concolic: `shatter-core/src/orchestrator.rs:2847` sets `branches_covered: Some(discoveries.len())` in the `ExploreProgressSnapshot`.
- Random explorer: `shatter-core/src/explorer.rs:1430` and `:2537` set `branches_covered: Some(aggregator.discoveries_count())`.
- `shatter-core/src/explorer.rs:375-380`: the snapshot documents `total_branches` as "total branches reported by static analysis" and `branches_covered` as "distinct branches covered so far (unique branch IDs with recorded discoveries)". Both count branch IDs (sites), not `(id, taken)` pairs.
- Audit evidence: `audits/2026-09-22/areas/artifacts.md:127`, `areas/frontend-go.md` (go-15 row) and findings core-18 and frontend-go-12 in `audits/2026-09-22/findings.json` (on branch `audit-2026-09-22` until the audit reports land).

## Decision taken in this issue

The metric becomes **branch-side coverage**: the numerator counts distinct `(branch_id, taken)` pairs observed, and the denominator is 2 × the branch points reported by static analysis. The alternative of keeping the site count and renaming it ("branch points reached") is rejected: it would keep showing `2/2` for the Go `Classify` case above, which is the misleading output this issue exists to fix. Where static analysis gives no branch count (`total_branches` is `None`), the denominator is omitted and the line says `N branch sides`, not a fraction.

## Acceptance criteria

- [ ] Progress lines and reports (markdown, HTML, JSON) show covered branch sides out of 2 × branch points, labelled as sides (for example `3/4 branch sides`). The words "N/M branches" for the site count no longer appear in any output; a grep over `shatter-core/src` and `shatter-cli/src` for the old format string finds nothing.
- [ ] JSON: the scan and explore report schemas gain side-count fields (for example `branch_sides_covered`, `branch_sides_total`), and their schema versions are bumped under the existing bump policies. Existing fields keep their meaning, or are removed with a version bump; the close comment lists which.
- [ ] Known-answer tests run in both engines (random explorer, `explorer.rs`, and concolic orchestrator, `orchestrator.rs`; see CLAUDE.md "parallel parity") on a fixture with two branch points where exactly one side is never taken (the Go `Classify` nested `x > 0.5` / `x < 1` shape, or a TS equivalent with a fixed iteration budget). Each test asserts the exact progress value `3/4 branch sides`. Close-time proof: both tests failing on current code (showing `2/2`) and passing after the fix, pasted into the close comment.
- [ ] A second fixture with every side taken (TS `classifyNumber`) asserts `6/6 branch sides`, so the fix does not simply undercount.
- [ ] Any stopping or scheduling logic that reads `branches_covered` (plateau detection, worklist exhaustion, frontier ranking) is listed in the close comment with file:line, and each is either switched to sides or left on sites with a one-line reason.
- [ ] `task e2e` (TS, Go and Rust concolic E2E) passes with forced execution (not a cached "up to date"); the output is recorded in the close comment.

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
title: "compare rejects the --spec-out bundle (the only clean producer); add one versioned spec reader in core that owns legacy-schema reading"
priority: P2
type: bug
labels: [spec, cli, artifacts, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# `compare` rejects the `--spec-out` bundle (the only clean producer); add one versioned spec reader in core that owns legacy-schema reading

## Problem

Shatter writes spec JSON in two shapes, and its consumers disagree about which they accept:

- `explore --spec-out` writes a versioned `FileSpecBundle` (`{version, file, functions: [...]}`).
- `explore --spec-json` on stdout emits a bare `FunctionSpec`, after the markdown report (str-qwua7.11).
- `properties` (and `specify --yaml`) emit YAML through serialize-only view structs. No command reads that YAML back.

`compare` only deserializes a bare `FunctionSpec`, so it cannot read the `--spec-out` file, which is the only producer that writes a clean file. Users who follow the documented flow (explore TS and Go with `--spec-out`, then `compare`) get a parse error. `spec-diff` has its own private reader (`SpecInput` in `shatter-cli/src/commands/diff.rs`) that accepts both JSON shapes and keeps the bundle `version` for its schema-mismatch report, but `compare` does not share it. Given a TS and a Go bundle, `spec-diff` prints "Added functions: ClassifyNumber / Removed functions: classifyNumber" with no hint that `compare` is the right tool.

This issue also makes the shared reader the single owner of legacy-schema reading. spec-yaml-custom-tags and spec-preconditions-from-path-constraints both change the spec JSON schema; maintainer decision D2 makes `spec-diff` the only regression tool, so a spec written by an older build must stay readable. Both of those issues are blocked by this one and plug their upgrade step into this reader.

This is the code half of finding docs-03. The docs half (SPEC §5 producer/consumer table) is spec-s5-contract-table-and-samples, which this issue blocks.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 793f2b0b; code identical to 56c86168):

- `shatter-cli/src/commands/compare.rs:19-22`: both inputs are parsed with `serde_json::from_str::<shatter_core::spec::FunctionSpec>`.
- `shatter-cli/src/commands/diff.rs:43-89`: `enum SpecInput { Function(FunctionSpec), Bundle(FileSpecBundle) }` detects the shape by a top-level `functions` array, exposes `version()` (bundle version, `None` for a bare spec) and flattens with `into_specs()`. `SpecVersionMismatch` (:91-104) reports an old/new version difference without failing.
- `shatter-core/src/spec.rs:252` (`SPEC_SCHEMA_VERSION = 1`; bundles without the field deserialize as version 0), `:259` (`pub struct FileSpecBundle`), `:304` (`pub struct FunctionSpec`).
- `shatter-core/src/spec.rs:675-714`: the YAML views (`SpecClassYaml`, `FunctionSpecYaml`) are `#[derive(Serialize)]` only and replace `ClassifiedInvariant` with `SpecInvariant`. The YAML is therefore a different, lossy representation, not a second encoding of the JSON model.
- Captured output (on branch `audit-2026-09-22` until the audit reports land):
  - `audits/2026-09-22/artifact-samples/compare-ts-go.txt`: `Error: failed to parse spec A '.../ts-spec-out.json': missing field `function_name` at line 172 column 1` (exit 2).
  - `compare-ts-go-bare.txt`: with hand-extracted bare specs, `4 of 4 shared behaviors match — 100% equivalent`.
- The verifier reproduced `compare s.json s.json` on a `--spec-out` bundle exiting 2 while `spec-diff s.json s.json` exits 0.

Repro: `shatter explore --spec-out ts.json <examples>/standalone/ts/01-arithmetic.ts:classifyNumber`, the same for `go/01-arithmetic.go:ClassifyNumber` into `go.json`, then `shatter compare ts.json go.json`.

## Data contract for the shared reader

The reader must not flatten away information that consumers use. It returns a document, not a bare `Vec<FunctionSpec>`:

- `SpecDocument { source_path, shape: Bundle | BareFunction, schema_version: Option<u32>, file: Option<String>, functions: Vec<FunctionSpec> }`. `schema_version` is the bundle's `version` (0 for pre-versioning bundles) and `None` for a bare spec. `file` is the bundle's `file`.
- Functions are addressed by `(file, function_name)`. When two entries in one document share a name, selecting by bare name is an error that lists the qualified candidates. Nothing is silently chosen or merged.
- Input is JSON only. A YAML file (for example `properties` output) is rejected with an error that says YAML spec output is write-only and names the JSON producers (`explore --spec-out`, `explore --spec-json`).
- Version handling: versions `0..=SPEC_SCHEMA_VERSION` are read, upgrading older versions in memory through an ordered list of upgrade steps (empty today; spec-yaml-custom-tags and spec-preconditions-from-path-constraints each add one). A version newer than the binary's `SPEC_SCHEMA_VERSION` is an error naming both versions, exit code 2. The original `schema_version` is kept on the document after upgrade so `spec-diff` can still report `SpecVersionMismatch`.
- Legacy fixtures: `shatter-core/tests/fixtures/spec-legacy/` holds one real bundle and one bare spec per supported version (v0 = the current bundle with the `version` field removed, v1 = current). Each later schema bump adds its predecessor's fixture there.

## Acceptance criteria

- [ ] `shatter-core` exposes the reader described above (for example `spec::read_spec_document(path) -> Result<SpecDocument, SpecReadError>`). `SpecInput` is removed from `diff.rs`; `compare`, `spec-diff`, `stale` and `revalidate` (wherever they read spec JSON) all call the core reader. The close comment lists each consumer switched, with file:line.
- [ ] `compare` accepts bundles and bare specs. With one function per side, no flag is needed. With several, `compare --function A[=B]` selects them (qualified `file::name` accepted); an ambiguous or missing name is an error listing the candidates.
- [ ] `spec-diff` output and exit codes are unchanged for every existing `shatter-cli/tests` spec-diff test, and it still reports `SpecVersionMismatch` for a v0-vs-v1 pair (test on the legacy fixtures).
- [ ] `spec-diff` suggests `compare` when the only added and removed functions differ by case or by language naming convention (for example `classifyNumber` / `ClassifyNumber` / `classify_number`).
- [ ] A CLI round-trip test runs `explore --spec-out` on TS `classifyNumber` and Go `ClassifyNumber` with a fixed `--max-iterations` budget and `--no-seeds` (`explore` has no `--seed` flag today; only `scan` does), then `compare` on the two files, and asserts exit 0 and "4 of 4". Close-time proof: the test failing on current code (parse error) and passing after the fix, both pasted into the close comment.
- [ ] Reader unit tests cover: v0 bundle, v1 bundle, bare spec, a version newer than the binary (error names both versions), a YAML file (error names the JSON producers), duplicate function names in one bundle (bare-name selection errors), and malformed JSON (error names the accepted shapes). A proptest checks that serializing any generated `FileSpecBundle` and reading it back yields the same functions, `file` and version.

## Suggested approach

Move `SpecInput` into `shatter-core/src/spec.rs` (or a new `spec_io.rs`), extend it with the fields above, and switch the consumers. Keep the upgrade-step list as a plain `match` on version so each schema bump adds one arm and one fixture.

## Out of scope

- Reading YAML specs.
- The markdown-before-JSON stdout mixing of `--spec-json` (str-qwua7.11).
- SPEC documentation of the artifact table (spec-s5-contract-table-and-samples).
- The retired snapshot `shatter diff` command (maintainer decision D2; retire-snapshot-diff).

## Related

str-wfqh, str-nq20, str-qwua7.11; blocks spec-s5-contract-table-and-samples, spec-yaml-custom-tags and spec-preconditions-from-path-constraints. Source findings: artifacts-09 (confirmed), docs-03 (code half).

---

<!-- file: 06-spec-preconditions-from-path-constraints.md -->

---
slug: spec-preconditions-from-path-constraints
kind: new
title: "Spec preconditions are sample statistics (often false or vacuous); export each class's symbolic path condition, with explicit unknown parts"
priority: P2
type: feature
labels: [spec, spec-diff, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [spec-json-shapes-compare]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Spec preconditions are sample statistics (often false or vacuous); export each class's symbolic path condition, with explicit unknown parts

## Problem

Class preconditions come from statistics over the observed samples (`AllEqual param[0] == -1`, `param[0] > 0`, `SameType number`), not from the path condition. They are sometimes false and often vacuous:

- In `fmt2`, the `n > 10` class shows the precondition `param[0] > 0`, although `n = 5` throws. The throw class gets `typeof param[0] == "number"`.
- In `regress/base.json`, class `returns "negative"` has `AllEqual{param_index: 0, value: -1}` with `sample_count: 20`: 20 samples of the same value become an equality precondition.

The executions already record the symbolic constraint of every branch decision, but the spec drops them: equivalence classes are keyed on a `BranchPath` of `(branch_id, taken)` pairs only. This issue carries those constraints into the spec so that each class states the input region it covers. It is also the prerequisite for spec-diff-symbolic-region-verdicts, which uses the regions to report real regressions that spec-diff misses today (maintainer decision D2 makes spec-diff the only regression tool). The closed issue str-0oc promised constraint-derived preconditions; the prior audit (2026-09-04) raised this again.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 793f2b0b; code identical to 56c86168):

- `shatter-core/src/equivalence.rs:218` `fn derive_preconditions(all_inputs: &[Vec<serde_json::Value>])` builds `AllEqual` (:236), `AllZero` (:249), `AllPositive` (:255), `AllNegative` (:261) and `SameType` (:272) from input values only.
- `shatter-core/src/equivalence.rs:31-40`: `BranchPath::from_decisions` keeps only `branch_id` and `taken` from each `BranchDecision`; the `constraint` field is discarded when classes are built.
- `shatter-core/src/execution_record.rs:17-25,50-64`: `BranchDecision.constraint` is a `SymConstraint`, which is either `Expr { expr }` or `Unknown { hint }`, and defaults to `Unknown` when a frontend omits it. `taken` records which side ran, so the class condition for a not-taken decision is the negation of `expr`.
- `shatter-core/src/spec.rs:451-518` `build_spec_class` copies `ec.common_preconditions` into `SpecClass.preconditions` (:518). Provenance is `Proven` only if every branch in the path was discovered by Z3.
- Captured evidence (on branch `audit-2026-09-22` until the audit reports land): `audits/2026-09-22/goals-runs/regress/base.json` (AllEqual preconditions with sample_count 20/20/7/13, all provenance `observed`); `audits/2026-09-22/artifact-samples/edge-plain-spec.md` (the fmt2 `param[0] > 0` class); `sd-v2.json` (`fail` gets `SameType number`).

## Contract

- **Retention.** When executions are grouped into a class, the class keeps the ordered list of `(branch_id, taken, SymConstraint)` for its path. If executions in one class recorded different constraints for the same branch step (for example a loop or a frontend that emits `Unknown` on some runs), the class keeps the step as `unknown` with a hint naming the disagreement. Nothing is dropped silently.
- **Negation.** Each step contributes `expr` when `taken` is true and `not(expr)` when false. The conjunction of the steps is the class's `path_condition`.
- **Incomplete knowledge.** A step whose constraint is `Unknown` appears in `path_condition` as an explicit `unknown` conjunct carrying its hint. The class gets `path_condition_status`: `complete` (every step symbolic), `partial` (some unknown) or `none` (all unknown, or no branches). A `partial` condition over-approximates the region and must be labelled as such in every renderer.
- **Rendering.** Markdown and text render `path_condition` as source-like text using parameter names from the analysis (for example `n < 0`, `!(n < 0) && !(n === 0) && n % 2 === 0`), with unknown conjuncts shown as `<unknown: hint>`.
- **Statistics.** The current sample statistics move to a separate `observed_inputs` field and are rendered under that name, not as preconditions. Statistics shared by every class are not rendered.
- **Schema.** This bumps `SPEC_SCHEMA_VERSION` by one and adds an upgrade step to the shared reader from spec-json-shapes-compare (this issue is blocked by it): a legacy bundle upgrades with `path_condition_status: none` and its old `preconditions` moved to `observed_inputs`. The previous-version bundle is added to `shatter-core/tests/fixtures/spec-legacy/` first.

## Acceptance criteria

- [ ] The retention, negation, incomplete-knowledge, rendering, statistics and schema rules above are implemented. The JSON class carries `path_condition` (structured, reusing the `SymExpr` representation) and `path_condition_status`.
- [ ] Known-answer tests pin the path condition of every class of `classifyNumber` for TS, Go and Rust under both engines (random and `--concolic`; CLAUDE.md "parallel parity"), and of `fmt2`. `fmt2` was an audit scratch file (`edge/plain.ts`, not checked in): recreate it as a test fixture from its description in `audits/2026-09-22/areas/artifacts.md` (F2: `n > 10` returns "big", `n < 0` returns "neg", otherwise throws `Error("bad")`); the "big" class must not claim `n > 0`. For each language and engine the test pins what is actually produced, including `partial`/`none` where a frontend emits `Unknown`; the close comment gives the per-language, per-engine status table. Each test fails on current code (no `path_condition`); record both runs.
- [ ] A test with a class whose executions disagree on one step's constraint shows that step as `unknown` and the class as `partial`.
- [ ] A proptest over generated decision lists checks that `path_condition` has exactly one conjunct per branch step, that not-taken steps are negated, and that the status is `complete` if and only if no conjunct is unknown.
- [ ] Legacy reading: `spec-diff` and `compare` on the previous-version fixture against a fresh bundle of the same source give the same verdicts as before this change (spec-diff-symbolic-region-verdicts has not landed yet, so no region verdicts are expected).
- [ ] `task e2e` passes with forced execution (not a cached "up to date"); output recorded in the close comment.

## Suggested approach

Carry the constraint list next to `BranchPath` in the equivalence class (keep `BranchPath` itself as the grouping key so class identity does not change). Write a small `SymExpr` pretty-printer keyed on parameter names from the analysis.

## Out of scope

- Any change to spec-diff verdicts. Region-overlap regression verdicts are spec-diff-symbolic-region-verdicts; behavior pairing of renumbered classes is str-qwua7.38.
- The YAML tag shape (spec-yaml-custom-tags).
- Simplifying or minimizing the conjunction (for example with Z3). A literal conjunction is acceptable.

## Related

str-0oc (closed, promised this), str-dcz, str-qwua7.38. Blocked by spec-json-shapes-compare; blocks spec-diff-symbolic-region-verdicts. Source findings: artifacts-08 (confirmed), goals-13 (confirmed).

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
- Pairing code today (re-checked 2026-09-23, audit worktree HEAD 793f2b0b, code identical to 56c86168): `shatter-core/src/spec_diff.rs:134-141` builds `old_by_path`/`new_by_path` keyed on `&c.branch_path` and matches with `old_by_path.get(&new_class.branch_path)`; removed classes at `:201`. The `~93-97` line reference in this issue's description is stale.

What spec-diff can and cannot establish from these two files: v1 recorded "pass" only at input 50 and v2 recorded "overflow" only at input 101, and spec-diff does not execute targets. The files alone therefore cannot prove that a specific input moved from "pass" to "overflow". They do prove that v2 has a behavior ("overflow") that no v1 class has, and that the v1 "fail" and v2 "overflow" classes have different postconditions even though their branch paths match.

Proposed additions to this issue's acceptance criteria (pairing only):

- An exact `BranchPath` match must not by itself pair two classes whose postconditions differ (under the existing nondeterminism-aware comparison). Such classes are left unpaired, so the v2 class is reported ADDED and the v1 class REMOVED.
- Known-answer test using `sd-v1.json` / `sd-v2.json` (copy them into the test fixtures): the output contains an `[ADDED]` row for `returns "overflow"`, contains no `[PRECOND]` or `[INCONCLUSIVE]` row that pairs "fail" with "overflow", and the command exits non-zero. It fails on current code (the word `overflow` does not appear); record the failing run.

Not part of this issue: the concrete verdict "inputs > 100 changed from pass to overflow". That needs each class's symbolic input region and a solver check, and is owned by the new audit issue spec-diff-symbolic-region-verdicts (blocked by spec-preconditions-from-path-constraints), which uses the same grade pair as its known-answer test. That issue does not change pairing; whichever of the two lands second rebases and re-runs both sets of tests.

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
blocked_by: [gauntlet-scan-checker-consumes-json, spec-json-shapes-compare, scan-report-headline-and-paths, pin-examples-repo]
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

No test tier owns output correctness. Output tests do exist, but they do not catch these bugs: the snapshot tests in `shatter-core/tests/` (hand-written file-comparison helpers, not the insta crate) render synthetic in-memory reports, so they pin whatever the renderer does today, including current bugs. For example, the HTML snapshot pins "Paths Found" = sum of `branches_covered`. None of them runs the CLI on a known-answer example, compares counts across formats, or feeds one command's output to its consumer.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 793f2b0b; code identical to 56c86168):

- Snapshot tests: `shatter-core/tests/html_snapshots.rs`, `outcome_md_snapshots.rs`, `run_markdown_ordering_snapshots.rs`, `source_set_summary_snapshots.rs`, with files in `shatter-core/tests/snapshots/`. They build reports in memory. Each file defines its own `fn assert_snapshot(path, actual)` (for example `html_snapshots.rs:63`); no crate in the workspace depends on `insta`.
- `shatter-cli/tests/scan_seed_reproducibility.rs:1-30` records that `--seed` does not make a scan fully deterministic: parallel scheduling and wall-clock timeouts still leak nondeterminism, so that test asserts per-function coverage rather than byte-identical reports. `explore` has no `--seed` flag; `scan` does.
- They pin a known bug: `shatter-core/src/html_templates.rs:382` computes `total_paths` as the sum of `branches_covered`, and the HTML snapshot records that output.
- `task golden-test` (Taskfile.yml, "Run cross-frontend parity golden tests") covers protocol goldens only.
- `shatter-cli/tests/` has contract tests (`json_stdout_contract.rs`, `exit_code_conventions.rs`, and others) but no terminal-output or report-output goldens for explore/scan/spec-out/spec-diff/compare.
- The snapshot helpers also silently create a missing snapshot and pass (`shatter-core/tests/outcome_md_snapshots.rs:45-51`); that is owned by snapshot-test-helpers, not by this issue.

## Reproducibility contract

Exploration results (which inputs are found, which classes form) are not byte-stable across runs, so the suite must not golden-test them byte for byte. The contract:

- **Fixtures.** The examples corpus is read at the SHA pinned by pin-examples-repo (this issue is blocked by it). The fixture functions are TS `classifyNumber`, Go `ClassifyNumber` and Rust `classify_number`, whose branches are all reachable from mined literals within a small budget.
- **Budgets.** Every run uses an iteration budget (`--max-iterations`), never only a wall-clock limit, plus `--parallelism 1`, `--no-seeds`, `--no-cache` (or a fresh `--cache-dir` per test), a fresh temp project directory, and `--seed` where the command has one (`scan`).
- **What is compared byte for byte:** only normalized structure that does not depend on search luck: section headings and order, table columns, labels, and the set of class postconditions for the fixture functions (these are fully reachable, so the set is stable). Normalization strips timings, iteration counts, absolute and temp paths, and example inputs.
- **What is compared as facts, not bytes:** counts (functions discovered/attempted/completed/failed, path counts, class counts) must be equal across the formats of one run, and must equal the known answer for the fixture functions (4 classes for `classifyNumber`).
- **Stability proof.** Before the suite is added to `task check`, it is run 10 times in a row on one machine with zero differences; the close comment records the command and the result.
- **Baseline policy.** Goldens change only through the explicit update command, in the same commit as the code change that causes them, and the commit message says why.

## Acceptance criteria

- [ ] A new task (for example `task golden-cli`) runs `explore`, `scan`, `explore --spec-out`, `spec-diff` and `compare` on the fixture functions under the reproducibility contract above, compares normalized output against checked-in goldens, and has an explicit update command.
- [ ] Cross-format assertion: for the same scan run, function counts (discovered/attempted/completed/failed), path counts and class counts are equal across markdown, JSON and HTML. (Passes only after scan-report-headline-and-paths, which is why this issue is blocked by it.)
- [ ] Consumer round-trips: `explore --spec-out` → `compare` (TS vs Go `classifyNumber`, asserts 4 of 4; needs spec-json-shapes-compare); `explore --spec-out` → `spec-diff` on two runs of the same source (asserts exit 0, no ADDED/REMOVED/CHANGED); scan JSON → the gauntlet checker (as rewritten by gauntlet-scan-checker-consumes-json).
- [ ] Goldens that still encode a known open bug when this lands are listed in a checked-in file with the tracker id of that bug, so fixing the bug updates the golden deliberately. The three blocking bugs above may not appear in that list.
- [ ] `scripts/affected-gates.py` selects the new task for changes to `shatter-core/src/report.rs`, `spec*.rs`, `html_templates.rs`, `shatter-cli/src/render.rs`, `shatter-core/templates/` and `demo/`. A selector test proves it.
- [ ] The new task is part of `task check`. Proof at close: a forced run (not a cached "up to date") showing the task executed and passed; the 10-run stability result; and one deliberately broken renderer change (for example reverting the HTML Paths fix) that makes the task fail.

## Suggested approach

Reuse the pattern of the existing snapshot helpers (compare whitespace-normalized output against a file under a `snapshots/` directory; today they regenerate when the file is deleted), but give the new suite an explicit update mode (for example an environment variable read by the task's update command), invoke the built CLI binary on the fixture functions instead of rendering in-memory reports, and run a normalizer over the output first. Unlike the existing helpers, a missing golden must fail, not be written silently. Add the cross-format and round-trip checks as ordinary tests in the same task.

## Out of scope

- Fixing the individual output bugs (filed separately in this bucket and others).
- Snapshot helper hygiene, such as failing on a missing snapshot under CI (snapshot-test-helpers).

## Dependencies

- gauntlet-scan-checker-consumes-json: the scan JSON → gauntlet checker round-trip needs the rewritten checker.
- spec-json-shapes-compare: the `--spec-out` → `compare` round-trip cannot pass until `compare` reads bundles.
- scan-report-headline-and-paths: the HTML/markdown path-count equality cannot pass until the HTML shows paths.
- pin-examples-repo: the fixture functions must come from a pinned examples revision.

## Related

str-qwua7.10, str-qwua7.53, str-qwua7.9, str-wurp, str-7jgm.3. Source finding: artifacts-18 (partially confirmed; the verifier noted that snapshot tests exist but are synthetic and pin current bugs, so this extends the pattern rather than creating a tier from scratch).

---

<!-- file: 09-known-answer-ratchet-and-ts-discriminants.md -->

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

Re-checked on 2026-09-23 in the audit worktree (HEAD 793f2b0b; code identical to 56c86168):

- `shatter-core/src/source_bucket.rs:253-267`: `FIXTURE_DIRS = ["testdata", "test-data", "fixtures", "fixture", "examples", "example", "samples", "sample", "__fixtures__"]`, and `is_fixture_sample` returns true if **any** path segment matches. So `internal/fixture/loader.go`, `pkg/sample/...` and `internal/example/...` are all classified as fixtures, wherever they sit.
- `shatter-core/src/source_bucket.rs:145-170` (`classify_path`): classification is path-only and works on whatever path string the caller passes, lower-cased and split on `/`. Callers include `report.rs:908`, `:1440`, `:6188` and `status_export.rs:955`, `:1115`; the implementer must check which of them pass absolute paths (the zolem report used absolute `/tmp/...` paths elsewhere) and relativize before classifying. Precedence is policy_excluded > generated > unsupported > declaration_only > fixture_sample > test_spec > production_ish.
- Audit evidence (on branch `audit-2026-09-22` until the audit reports land): `audits/2026-09-22/goals-runs/zolem-fixture-default.json` has `codebase.source_set.fixture_sample = {file_count: 10, line_count: 1279}`, `production_ish = {0, 0}` and `codebase.productionish_source_lines = 0` (re-read 2026-09-23); write-up in `audits/2026-09-22/areas/goals.md` item 16 (goals-16).

## Classification contract

- The classifier works on the path relative to the project root (the root Shatter already detects; `--project-dir` overrides it). A scan started in a subdirectory classifies each file the same way as a scan started at the root. Files outside the project root keep today's behavior.
- A file is `fixture_sample` when any of these holds: a segment is `testdata`, `test-data` or `__fixtures__`; a segment is `fixtures`, `fixture`, `samples` or `sample` **and** an earlier segment is a test directory (`test`, `tests`, `__tests__`); the first segment (relative to the project root) is `examples`, `example`, `samples` or `sample`.
- Otherwise a directory named `fixture`, `fixtures`, `sample`, `samples`, `example` or `examples` below a production source root is not a fixture by itself, in Go, TS or Rust.
- Precedence between buckets does not change.

## Acceptance criteria

- [ ] The contract above is implemented in `source_bucket.rs`, and the module docs describe it.
- [ ] A table test classifies at least: `internal/fixture/loader.go` → production_ish; `pkg/sample/x.go` → production_ish; `internal/example/x.ts` → production_ish; `testdata/a.go`, `x/testdata/a.go`, `src/__fixtures__/a.ts`, `tests/fixtures/a.rs`, `examples/a.ts` → fixture_sample; `internal/specs/x.go` → production_ish (the str-9awj regression). The rows that change fail on current code; record both runs in the close comment.
- [ ] A test runs classification for the same file from the project root and from a subdirectory (scan root below the project root) and gets the same bucket.
- [ ] A proptest checks that inserting a `fixture`/`sample`/`example` segment into a production path below its first segment never changes the bucket, unless a test directory precedes it.
- [ ] Re-running the zolem `internal/fixture` scan gives a non-zero `production_ish` count; the close comment records the before/after `source_set`.
- [ ] Side effect check: the examples corpus lives at `/home/ketan/project/examples`, so today an absolute path through it contains an `examples` segment and may be bucketed `fixture_sample`. The close comment records the `source_set` of a scan of `standalone/ts/` before and after, and `task gauntlet` and `task walkthrough` pass (output recorded); any change in their reports is explained.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Out of scope

- A user-configurable bucket override per path glob (source-bucket-config-override).
- Other source-bucket categories (generated, declaration_only) unless they share the same any-segment rule.

## Related

str-9awj (closed; same class for `specs`, gets a reopen-note pointing here), str-jeen.37, str-jeen.38, source-bucket-config-override. Source finding: goals-16 (confirmed).

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
blocked_by: [source-bucket-fixture-dir, source-bucket-config-override]
existing_id: str-9awj
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on closed str-9awj: the segment-name misclassification recurs for production packages named `fixture`

Target: **str-9awj** (closed, P2, "Specs bucket mislabels prod"). Action: add the comment below. Do not reopen; the follow-up work is the new issues source-bucket-fixture-dir and source-bucket-config-override (the filer substitutes their ids). `blocked_by` above only means the new issues must be filed first so the comment can cite their ids. str-9awj status (closed) verified with `bd show` on 2026-09-23.

## Comment text

**Audit 2026-09-22 note (finding goals-16): the same class of bug recurs for `fixture`.**

This issue fixed production files under `internal/specs` being bucketed as `test_spec`. The fixture rule still uses the same any-segment heuristic: `shatter-core/src/source_bucket.rs:253-267` treats a path as `fixture_sample` if any segment is `testdata`, `test-data`, `fixtures`, `fixture`, `examples`, `example`, `samples`, `sample` or `__fixtures__`.

A zolem scan of its production package `internal/fixture` (a fixture loader and selector) reported `fixture_sample {10 files, 1279 lines}`, `production_ish {0, 0}` and `productionish_source_lines: 0`, so the production denominator for that package was zero.

Follow-up: <source-bucket-fixture-dir id> (convention-based classification anchored to the project root, and a regression test for `internal/fixture/loader.go`). A per-glob config override is a separate follow-up: <source-bucket-config-override id>. Please keep the `internal/specs` regression test from this issue when that change lands.

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

Re-checked on 2026-09-23 in the audit worktree (HEAD 793f2b0b; code identical to 56c86168):

- `audits/2026-09-22/goals-runs/zolem-fixture-default.md` (on branch `audit-2026-09-22` until the audit reports land): a byte count gives 6 NUL and 3 ESC. Every one is inside a thrown-error message: lines 836-847 (`"\u0000"` and `"a\u0000b"` inputs, CEL error `token recognition error at: '<NUL>'` plus the echoed source line ` | <NUL>`) and lines 849-853 (`"\u001b[31m"` input, echoed as ` | <ESC>[31m`).
- Writer: `shatter-core/src/report.rs:2479` in the Interesting Inputs section: `writeln!(out, "- {inputs_str} -> **error:** {err}")`. `inputs_str` goes through `format_json_compact_list` (escaped); `err` (`thrown_error`) is written unescaped. Other places that print `thrown_error` or error messages into markdown, text or HTML likely do the same; list them at pickup (`/usr/bin/grep -rn thrown_error shatter-core/src shatter-cli/src`).
- The explore renderer's `value_short` (`shatter-cli/src/render.rs:277-284`) uses `serde_json::Value::to_string()`, which escapes controls, so values are not the problem there. But it truncates with `&s[..37]` (`render.rs:280`), a byte slice that panics when byte 37 falls inside a multi-byte UTF-8 character. The other `format_value_short` helpers (`compare.rs:265`, `explorer.rs:3259`, `export.rs:789`) should be checked for the same pattern.

Repro: scan zolem's `internal/fixture` package (or any function whose error message echoes a string input), then run `python3 -c 'import sys;b=open(sys.argv[1],"rb").read();print(b.count(b"\x00"), b.count(b"\x1b"))' report.md`.

## Acceptance criteria

- [ ] Error messages and any other free text from the target program (thrown errors, stdout/stderr captures, return strings shown unquoted) are escaped before they are written to markdown, text or HTML reports. The close comment lists every writer changed.
- [ ] All markdown, text and HTML renderers escape C0 controls other than `\n` and `\t`, and DEL, as `\u00XX` (HTML additionally entity-escapes as it does today). Multi-line error messages are indented or fenced so they stay inside their list item.
- [ ] A proptest varies the target-supplied free-text fields, not the inputs (inputs are already JSON-escaped): it builds reports whose `thrown_error`, captured stdout/stderr and other free-text fields are arbitrary strings, with the generator biased to include NUL (0x00), ESC (0x1b, including `\u001b[31m`), DEL (0x7f), other C0 controls, `\r`, and multi-line text. It asserts that the rendered markdown, text and HTML are valid UTF-8, contain no byte in 0x00-0x08, 0x0b-0x1f or 0x7f, and that every line of a multi-line error stays inside its list item (indented or fenced). It fails on current code; record both runs in the close comment.
- [ ] A CLI regression test runs a scan on a small TS fixture whose function throws `new Error("bad input: " + s)` for a string argument `s`, with seeded inputs `"\u0000"`, `"a\u0000b"` and `"\u001b[31m"` (for example through `--seeds-dir` or a unit-level report built from recorded executions), and asserts the written markdown and HTML contain no NUL or ESC byte. It fails on current code.
- [ ] Truncation helpers truncate on a char boundary; a test with a multi-byte string longer than the limit does not panic.

## Suggested approach

Add one shared `escape_display(&str) -> Cow<str>` in core and use it in every place that prints target-program text (error messages first, starting at `report.rs:2479`) or values into a report. Make the truncation helpers use `char_indices` or `floor_char_boundary`.

## Out of scope

JSON output (serde_json already escapes control characters).

## Related

Source finding: goals-17 (confirmed).

---

<!-- file: 13-spec-diff-symbolic-region-verdicts.md -->

---
slug: spec-diff-symbolic-region-verdicts
kind: new
title: "spec-diff misses real regressions: report CHANGED when old and new classes cover overlapping input regions with different outcomes (Z3 witness), INCONCLUSIVE when regions are unknown"
priority: P2
type: feature
labels: [spec-diff, spec, solver, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [spec-preconditions-from-path-constraints]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# spec-diff misses real regressions: report CHANGED when old and new classes cover overlapping input regions with different outcomes (Z3 witness), INCONCLUSIVE when regions are unknown

## Problem

`spec-diff` reads two spec files and never executes the target. It can only compare what the files record, and today each class records a branch path, sample statistics and one or a few example inputs. When a behavior change moves inputs from one class to another, the two files usually have no example input in common, so spec-diff cannot state what changed. Two audit cases:

- **grade.** v1: `n < 0` → "invalid", `n < 50` → "fail", else "pass". v2 inserts `n > 100` → "overflow" before the `< 50` test. The only real change is that inputs above 100 now return "overflow" instead of "pass". spec-diff reports `2 added, 1 removed, 1 precondition(s) changed, 1 inconclusive`, and the word `overflow` never appears. v1 recorded "pass" only at input 50, and v2 recorded "overflow" only at 101, so no shared example exists.
- **even/odd swap.** `classifyNumber` with the "positive-even"/"positive-odd" results swapped: spec-diff reports `[PRECOND] Class 3 ... - param[0] == 2 + param[0] == 1` and `[INCONCLUSIVE]`, not a changed outcome. Base recorded "positive-even" at input 2; new recorded it at input 1.

Once spec-preconditions-from-path-constraints lands, each class carries its symbolic `path_condition` with a `complete`/`partial`/`none` status. With complete conditions on both sides, spec-diff can decide the question from the files alone: an old class O and a new class N whose conditions are jointly satisfiable, but whose postconditions differ, are a behavior change for every input in that intersection, and Z3 can produce a concrete witness. For grade, v1 "pass" (`!(n < 0) && !(n < 50)`) intersects v2 "overflow" (`!(n < 0) && n > 100`), witness 101. Where a condition is `partial` or `none`, the region is not known and the verdict must stay INCONCLUSIVE; spec-diff must never report "no change" for it.

Maintainer decision D2 makes spec-diff the only regression tool, so its false negatives are false negatives for Shatter's whole regression story.

## Division of work with str-qwua7.38

- **str-qwua7.38** owns class *pairing*: which old class corresponds to which new class when branch IDs are renumbered, and the rule (added by the audit note qwua7-38-spec-diff-false-negative) that a path match alone must not pair classes whose postconditions differ.
- **This issue** owns *region verdicts*: for every old/new class pair whose path conditions intersect (whether or not the pairing step paired them), report CHANGED with a witness when the postconditions differ. It does not change pairing.
- Whichever lands second rebases onto the other; the second one re-runs both issues' known-answer tests.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 793f2b0b; code identical to 56c86168):

- `shatter-core/src/spec_diff.rs:134-141`: classes are paired only by exact `branch_path`; no field of `SpecClass` holds a symbolic condition today.
- `shatter-cli/src/commands/diff.rs` (spec-diff command): reads files only; the help text says "Matched classes are only compared when both sides recorded a comparable canonical example; otherwise the pair is reported as inconclusive".
- Captured evidence (on branch `audit-2026-09-22` until the audit reports land): `audits/2026-09-22/artifact-samples/sd-v1.json`, `sd-v2.json`, `sd-diff.txt` (grade; class examples `[[-1]]`, `[[0]]`, `[[50]]` in v1 and `[[-1]]`, `[[0]]`, `[[101]]`, `[[100]]` in v2); `audits/2026-09-22/goals-runs/regress/base.json`, `new.json` (even/odd swap; "positive-even" example `[[2]]` in base and `[[1]]` in new). Write-up: `audits/2026-09-22/areas/artifacts.md` F5.

## Acceptance criteria

- [ ] For each function present in both files, spec-diff checks every (old class, new class) pair where both `path_condition`s are `complete` and the postconditions differ (using the existing nondeterminism-aware postcondition comparison). If the conjunction of the two conditions is satisfiable under Z3, it reports `[CHANGED]` naming both postconditions, the intersected condition as text, and a Z3 witness input. The exit code is non-zero.
- [ ] If either side of an otherwise overlapping pair is `partial` or `none`, or the two functions' parameter lists differ, the pair is reported `[INCONCLUSIVE]` with the reason (`unknown region`, `signature changed`). Such a pair never produces "no change". A Z3 `unknown` result or timeout is also INCONCLUSIVE.
- [ ] Known-answer test **grade**: TS sources for v1 and v2 (as described above) are checked in as test fixtures; the test explores both with `explore --spec-out` and a fixed `--max-iterations`, runs `spec-diff`, and asserts a `[CHANGED]` row from "pass" to "overflow" whose witness satisfies `n > 100`, and exit non-zero. Close-time proof: the test failing on the code before this issue (with spec-preconditions-from-path-constraints landed) and passing after.
- [ ] Known-answer test **even/odd swap**: same shape for `classifyNumber` with the two positive results swapped; asserts `[CHANGED]` rows between "positive-even" and "positive-odd" with witnesses of the right parity.
- [ ] Negative control: spec-diff of two independent explorations of the same unchanged source reports no `[CHANGED]` row (run it on `classifyNumber` for TS, Go and Rust).
- [ ] Legacy input: spec-diff of a legacy bundle (no `path_condition`, upgraded to status `none` by the shared reader) against a new bundle reports INCONCLUSIVE for overlapping pairs and does not crash.
- [ ] `--json` output carries the verdict, both class labels, the intersected condition and the witness; a round-trip test covers it.
- [ ] `task e2e` passes with forced execution; output recorded in the close comment.

## Suggested approach

Reuse the core Z3 translation of `SymExpr` that the concolic solver already uses. Bound each satisfiability check with a small timeout and treat timeouts as INCONCLUSIVE. Pairs can be pruned by postcondition first (only differing postconditions need a solver call).

## Out of scope

- Pairing renumbered classes (str-qwua7.38).
- Replaying recorded examples against the other version's source. spec-diff stays file-only.
- Producing symbolic conditions (spec-preconditions-from-path-constraints).

## Related

str-qwua7.38, str-nfg4y (canonical-input guard), str-0oc. Blocked by spec-preconditions-from-path-constraints. Audit note on str-qwua7.38: qwua7-38-spec-diff-false-negative. Source findings: artifacts-06, artifacts-08, goals-13.

---

<!-- file: 14-ts-union-discriminant-literals.md -->

---
slug: ts-union-discriminant-literals
kind: new
title: "TS analyzer widens discriminated-union tag fields to plain str, so computeArea never gets kind = circle/rectangle/triangle (0/6 branches)"
priority: P2
type: bug
labels: [typescript, frontend-ts, analyzer, examples, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# TS analyzer widens discriminated-union tag fields to plain `str`, so `computeArea` never gets `kind` = circle/rectangle/triangle (0/6 branches)

(Split from known-answer-ratchet-and-ts-discriminants during the cross-check revision.)

## Problem

`computeArea(shape: Shape)` in the examples repo's `standalone/ts/05-unions.ts` switches on `shape.kind`, a discriminated union of `"circle"`, `"rectangle"` and `"triangle"` variants. The TS analyzer reports the `kind` field of every variant as plain `str`, dropping the literal type. The generator then produces `kind` values such as `""`, `"0"`, `" "` and random strings, never a real tag, and emitted `{"kind":"true","radius":2.0}`. No case matches, so exploration stays at 0/6 branches.

The analyzer already emits `enum_values` for literal-union parameters and string enums (`shatter-ts/src/analyzer.test.ts:792-810`), so the missing piece is the object-field case inside union variants.

## Evidence

- `/home/ketan/project/examples/standalone/ts/05-unions.ts:9-15` (EXPECTED BRANCHES for `computeArea`: six outcomes across three `kind` values) and `:17` onward (`switch (shape.kind)`).
- The audit's analysis output shows `{"kind":"str"}` in all three union variants; its concolic run reached 21 iterations, 1 path, 0/6 branches (`audits/2026-09-22/areas/goals.md` item 7, on branch `audit-2026-09-22` until the audit reports land).
- `shatter-ts/src/protocol.ts:467`: `enum_values?: (string | number | boolean)[]` exists on the type-info shape; `shatter-ts/src/analyzer.test.ts:792` ("emits enum_values for a literal-union alias parameter") and `:803` (string enum parameter) cover parameters only.
- `demo/gauntlet-scan-allowlist.yaml:32-36` allowlists `computeArea` with the reason "coverage limited by union-input synthesis" and no issue id.

## Acceptance criteria

- [ ] For an object type inside a union whose field has a string, number or boolean literal type, the TS analyzer emits that field with its literal value (as `enum_values` with one value, or a const literal type; the close comment says which). A unit test in `shatter-ts/src/analyzer.test.ts` covers `Shape` from `05-unions.ts` and fails on current code.
- [ ] A known-answer test explores `computeArea` with a fixed `--max-iterations` budget and asserts that all three `kind` values appear in recorded inputs and that at least the three non-throwing outcomes are reached (both engines, random and `--concolic`). It fails on current code; record both runs in the close comment.
- [ ] Parity: if the analysis JSON changes shape or gains a field for this case, `protocol/parity-matrix.yaml` and `shatter-ts/CLAUDE.md` are updated, and `task parity` + `task conformance` pass (output recorded). If the Go or Rust analyzers already handle tagged variants, the close comment says so; if not, a follow-up issue is filed and linked.
- [ ] The `computeArea` entry is removed from `demo/gauntlet-scan-allowlist.yaml` if it now passes, or its reason is updated with this issue's id. `task gauntlet` passes (output recorded).
- [ ] `cargo test --test e2e_concolic` (TS E2E) passes with forced execution.

## Out of scope

- `routeRequest` (also allowlisted for union-input synthesis) beyond checking whether this fix helps it; the close comment reports its before/after branch count.
- The known-answer ratchet gate (known-answer-ratchet-and-ts-discriminants).

## Related

known-answer-ratchet-and-ts-discriminants, concolic-early-termination (shatter-concolic-and-engine-design bucket; attributes part of the `computeArea` loss to this). Source finding: goals-07 (confirmed).

---

<!-- file: 15-qwua7-10-allowlist-issue-links-note.md -->

---
slug: qwua7-10-allowlist-issue-links-note
kind: note-to-existing
title: "Note on str-qwua7.10: require a tracker issue on every gauntlet allowlist entry, not only the two new expiring ones"
priority: P1
type: bug
labels: [gauntlet, quality-gates, audit-2026-09-22]
parent_epic: ""
blocked_by: []
existing_id: str-qwua7.10
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.10: require a tracker issue on every gauntlet allowlist entry, not only the two new expiring ones

Target: **str-qwua7.10** (open, P1, "Demo gates must fail on 0% coverage, all-throwing functions and lifecycle-helper clusters"; verified with `bd show` on 2026-09-23). Action: append the comment below. Do not change the priority. This note replaces the allowlist half of the earlier draft known-answer-ratchet-and-ts-discriminants, because str-qwua7.10 already owns the allowlist schema (it adds a required `expires:` date and two entries that cite their issue ids), and a second issue changing the same file's schema would split ownership.

## Comment text

**Audit 2026-09-22 note (finding goals-07): extend the allowlist schema change to every entry.**

This issue adds a required `expires:` field to `demo/gauntlet-scan-allowlist.yaml` and two new entries that cite their issue ids. The 2026-09-22 audit found that the existing entries have the same problem the expiry is meant to fix: the file has 113 lines and one `str-` reference, it has not changed since 2026-05-08 (commits 8734407f and 398e4a7e), and entries such as `computeArea` (:33) and `routeRequest` (:38) give "union-input synthesis" as the reason with no issue.

Proposed additions to the acceptance checks:

- Every `expected_failures` entry (existing and new) carries an `issue:` field with a tracker id, alongside `expires:`. The checker fails on an entry without one. For the existing entries, file or reuse issues (the 2026-09-22 audit filed ts-union-discriminant-literals for `computeArea`).
- A checker test covers an entry with no `issue:` field and shows the gate failing.

Related new audit issues: known-answer-ratchet-and-ts-discriminants (a separate ratchet gate over the examples' expected outcomes, which does not touch the allowlist schema) and ts-union-discriminant-literals.

---

<!-- file: 16-source-bucket-config-override.md -->

---
slug: source-bucket-config-override
kind: new
title: "Let projects override the source bucket per path glob in shatter.config.json"
priority: P3
type: feature
labels: [scan, report, classification, config, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [source-bucket-fixture-dir]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Let projects override the source bucket per path glob in `shatter.config.json`

(Split from source-bucket-fixture-dir during the cross-check revision, so the heuristic fix is not held up by a new configuration feature.)

## Problem

The source-bucket classifier (`shatter-core/src/source_bucket.rs`) is a path heuristic. source-bucket-fixture-dir makes it follow conventions, but any heuristic will misclassify some layouts (the zolem `internal/fixture` production package was one). Today a project has no way to correct a wrong bucket, so its production denominator and coverage summaries stay wrong until Shatter's heuristic changes.

## Evidence

- `shatter-core/src/source_bucket.rs:145-170`: `classify_path` is path-only with a fixed precedence (policy_excluded > generated > unsupported > declaration_only > fixture_sample > test_spec > production_ish) and no configuration input.
- `shatter-core/src/config.rs:461-475`: `shatter.config.json` holds **scan-global** settings (file discovery, output, caching, limits, parallelism); `.shatter/config.yaml` holds per-function settings. Source bucketing is a file-discovery/report property, so it belongs in `shatter.config.json`.
- Audit evidence: `audits/2026-09-22/goals-runs/zolem-fixture-default.json` (goals-16), on branch `audit-2026-09-22` until the audit reports land.

## Contract

- **Location.** A new optional `source_buckets` array in `shatter.config.json`: `[{ "glob": "internal/fixture/**", "bucket": "production_ish" }, ...]`. It is not read from `.shatter/config.yaml`, and there is no CLI flag.
- **Glob base.** Globs match the file path relative to the directory that contains `shatter.config.json` (the project root), using `/` separators on every OS. A scan started in a subdirectory uses the same config and the same base, so the result does not depend on where the scan started. Files outside the project root are never matched.
- **Precedence.** Entries are tried in array order; the first match wins. A matching override replaces the heuristic buckets `generated`, `declaration_only`, `fixture_sample`, `test_spec` and `production_ish`. It does not override `policy_excluded` or `unsupported`: a file that policy excludes or that no frontend can read keeps that bucket, and `shatter doctor` warns about an override that targets such a file.
- **Values.** `bucket` must be one of the overridable bucket names; an unknown name or an invalid glob is a config error at load time, naming the entry index.
- **Visibility.** Scan JSON records, per file, whether its bucket came from an override (for example `source_bucket_origin: "override" | "heuristic"`), and the scan report schema version is bumped. `shatter doctor` lists active overrides.

## Acceptance criteria

- [ ] The contract above is implemented. The README "Project Configuration" section and the SPEC config section document `source_buckets` with one example.
- [ ] Tests: an override reclassifies `internal/fixture/loader.go` to production_ish; first-match-wins order; an override on a policy-excluded path is ignored with a doctor warning; the same result for a scan started at the root and in a subdirectory; config load errors for an unknown bucket and an invalid glob. Each new test fails on the code before this change.
- [ ] A proptest checks that with an empty `source_buckets` list the classification of any generated path equals the heuristic's.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Out of scope

- Changing the heuristic itself (source-bucket-fixture-dir).
- Per-function bucket overrides.

## Related

source-bucket-fixture-dir, str-9awj, str-jeen.37, str-jeen.38. Source finding: goals-16 (confirmed).
