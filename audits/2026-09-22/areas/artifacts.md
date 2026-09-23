# Area review: generated artifacts and reports (L5, L6)

Audit 2026-09-22, branch `audit-2026-09-22` at `16794cef`. Reviewer scope: explore and scan
markdown/JSON/HTML/text reports, spec markdown/JSON/YAML, spec-diff, compare, stale,
the on-disk artifact tree (`shatter-artifacts/`), and walkthrough/gauntlet output checking.

Binary: `target/release/shatter` built from this worktree (`cargo build --release -p shatter-cli`,
20 min under load average 120-200). Rust frontend built with
`cargo build --release` in `shatter-rust/`. Examples: `/tmp/shatter-examples-main/standalone`
(external examples repo, commit `49984f4`). All runs used `SHATTER_ALLOW_HOST_WRITES=1`.
Samples: `audits/2026-09-22/artifact-samples/` (file names cited below).

**Environment caveat.** The host ran at load average 80-200 throughout (other reviewers
building). Go builds in a mixed scan and the Rust frontend build both timed out, so
no Rust report samples exist and the Go scan sample shows only failures. TS and single-target
Go explore runs completed. Findings that depend on timing are marked as such.

## Summary verdict

| Artifact | Human (L6) | Agent/machine (L6) | Correct (L5) | Note |
|---|---|---|---|---|
| explore markdown (stdout) | Good | Fair | **Poor** | clean table, but path count can be 0 or 1 when 3 paths were found (F2) |
| explore `-o x.json` | n/a | **Broken** | **Broken** | writes an empty `no_targets/unclassified` bundle for a successful run (F1) |
| explore `--spec-json` (stdout) | n/a | **Broken** | ok | markdown report precedes the JSON (str-qwua7.11, still true) |
| explore `--spec-out` bundle | n/a | Good | Fair | versioned, well shaped; `compare` cannot read it (F6) |
| spec markdown | Good | Fair | Poor | preconditions wrong/vacuous (F7); invariant lines have blank subjects (F4) |
| spec YAML (`properties`) | Fair | **Poor** | Fair | custom `!AllEqual` tags; PyYAML `safe_load` rejects it (F10) |
| spec-diff text/JSON | Fair | Fair | **Poor** | misses the real regression, pairs "fail" with "overflow" (F5) |
| compare | Good | Good | ok | only accepts bare FunctionSpec, not the `--spec-out` bundle (F6) |
| stale | Good | Good | ok | clear Fresh/Untracked output |
| scan markdown | Fair | Fair | Fair | headline 100% with 1 of 12 functions completed; abs paths; noisy sections (F12) |
| scan JSON | n/a | Good | Fair | rich, versioned (v9), undocumented schema, absolute paths (F11) |
| scan HTML | Good styling | n/a | **Poor** | "Paths" is branches_covered (F9); failures omitted (str-hds8u); highlight bug (str-8uomd) |
| explore HTML | Good styling | n/a | Fair | non-executable lines look uncovered; zero-width bars (str-77803) |
| on-disk scan tree | n/a | Poor | **Poor** | mixed-language scan: second language deletes the first's artifacts and summary (F3) |
| walkthrough output | Good | n/a | **Poor** | concolic and spec steps replay the random step's resumed result (F0) |
| gauntlet checker | n/a | n/a | **Broken** | scan-failure regex matches a format removed 2026-05-13 (F8) |

## Positives worth keeping

- The explore markdown table (`# | Call | Outcome`) reads in ten lines, carries no ANSI when
  piped, and TS and Go produce the same shape for the same function
  (`ts-explore.md`, `go-explore-spec.md`).
- The TS and Go spec bundles for `classifyNumber` have identical structure, identical classes,
  identical branch paths and examples (`ts-spec-out.json`, `go-spec.json`). Cross-language
  output parity at the spec layer is real.
- `compare` output is clear: "**4 of 4 shared behaviors match — 100% equivalent**" with
  a list of matching behaviors (`compare-ts-go-bare.txt`).
- `stale` output is clear and correctly separates Fresh and Untracked
  (`stale.txt`).
- Artifacts carry schema versions: `FileSpecBundle.version` (v1) with a written bump policy
  (`shatter-core/src/spec.rs:233-251`), `ScanReport.version` 9
  (`shatter-core/src/report.rs:79`), `run-status.tsv` has a `schema_version` column,
  explore artifacts carry `version`.
- The HTML report does have a coherent style sheet (`shatter-core/templates/includes/style.html`,
  72 lines, system fonts, stat tiles, coverage colour thresholds, collapsible functions,
  inline annotated source). The 2026-09-04 audit's statement that there is "no `<style`
  token anywhere in `shatter-core/src`" missed `shatter-core/templates/`; the page is
  styled. It still lacks dark mode and `max-width`.
- The HTML escaper and the Askama split mean `<input> & 'quote'` in thrown messages render
  safely (checked in template code, `html_templates.rs:20-25`).
- Scan JSON separates `failed[]` with structured `error_type` and `failed_at`
  (`scan-mix.json`), which makes failures machine-actionable.

## Status of 2026-09-04 findings in this area

| Prior finding | Issue | Status 2026-09-22 | Evidence |
|---|---|---|---|
| `--spec-json` stdout is markdown then JSON | str-qwua7.11 | **Still true** | `ts-specjson-stdout.out` line 1 `# Shatter Explore`; `json.load` → `JSONDecodeError: Expecting value: line 1 column 1`; code: `explore.rs:3610` `should_print_report` ignores `spec_as_json` |
| "Created …" init lines on stdout, "detected language: unknown" | str-qwua7.39 | **Still true** | `ts-explore.md` lines 1-4 |
| spec-diff renumbering noise | str-qwua7.38 | **Still true and worse than described** (F5) | `sd-diff.txt` |
| Invariant confidence constant 1.0 | str-qwua7.61 | Still true; rendering also broken (F4) | `ts-spec-invariants.md` |
| Four names for one concept | str-qwua7.57 | Still true: "path(s)", "Class N", "Cluster N", "Paths Found" | `ts-explore.md`, `ts-spec.md`, `scan-mix.md`, `scan-mix.html` |
| Demo gates check exit codes, not results | str-qwua7.10 | Still true, and the scan-failure half of the checker has been dead since May (F8) | `demo/gauntlet_check_output.py:28-30` |
| Tautological preconditions | not filed | Still true; now shown to be wrong, not only tautological (F7) | `edge-plain-spec.md`, `ts-spec-invariants.json` |
| Scan prints absolute `::` paths; two report shapes | not filed | Still true (F12) | `scan-mix.stdout`, `scan-mix.md` |
| `--format text` still contains markdown | not filed | Still true for explore | `edge-text.out` identical to `edge-md.out` |
| HTML has no dark mode / responsive rules | not filed | Partly wrong (styles exist), dark mode still absent | `templates/includes/style.html` |

Issues filed on 2026-09-13 that this review confirms and does not re-list: str-8uomd (HTML path
highlight uses a document-wide `querySelector`, `scan_report.html:73`), str-77803 (zero-width
coverage fills), str-6prkb (keyboard access), str-hds8u (HTML omits failed functions; confirmed:
`scan-mix.html` shows Functions 1 / Skipped 7 and no trace of the 4 failures listed in
`scan-mix.md` "Failed Functions"), str-jd0d1 (walkthrough labels random runs concolic),
str-wlban (Unicode panic in spec builder), str-gr41w, str-cl330.

## Findings

### F0 (P1, L5) explore silently resumes prior results regardless of flags; the walkthrough's concolic and spec steps show the random step's result

`try_resume_function` (`shatter-cli/src/commands/explore.rs:1209-1233`) reuses a prior artifact
when the function name, status and deep fingerprint match. The key has no explorer mode,
iteration budget, seed, or other config. Reproduction:

```
explore ts/01-arithmetic.ts:classifyNumber                         # random, 100 iters
explore --concolic --max-iterations 20 ts/01-arithmetic.ts:classifyNumber
  stderr: [info] [resumed] classifyNumber: 3 branches, 12.3s (prior run)
  stdout: ... - *Explorer: concolic (Z3-backed)*
```
(`ts-concolic-after-random.md/.err`). A later `--spec --max-iterations 30` run printed
"**Exploration:** 100 iterations" (`ts-spec.md`), the first run's budget. The stdout report
never says "resumed"; only an `[info]` line on stderr does.

The walkthrough exports one `SHATTER_ARTIFACT_DIR` for all steps (`demo/walkthrough.sh:120`),
step 2 explores all walkthrough TS targets, step 8 (`concolic-z3`) and step 9
(`spec-generation`) re-explore the first one. Replaying those steps with a shared artifact dir
reproduces it: `wt-step8-concolic.err` and `wt-step9-spec.err` both contain
`[resumed] classifyNumber: 3 branches, 7.6s (prior run)`, and step 8's stdout carries
"Explorer: concolic (Z3-backed)". The demo's Z3 step demonstrates nothing. Even a fresh
`--concolic` run on this example gets every branch from `user_provided` seeds
(`discoveries: [[2,'user_provided'],[1,'user_provided'],[0,'user_provided']]`,
`solver_guided_inputs: 0`), so no class is ever `[proven]` (`ts-concolic-fresh-spec.md`).

Related but distinct: str-9m9o3 (scan cache ignores `--seed`), str-060a (closed, `--clean`).

Recommendation: include an exploration-config hash (explorer mode, iterations, seeds, mocks,
solver settings) in the explore summary entry and resume only on an exact match. Print
`(resumed from <date>, <mode>, <iters> iters)` in the stdout report when a result is reused.
Give each walkthrough step its own artifact dir or pass `--clean`. Pick a walkthrough concolic
example whose branches seeds cannot reach, so `[proven]` appears.

### F1 (P1, L5/L6) `explore -o out.json` writes an empty "no_targets" bundle for a successful run

```
rm -rf shatter-artifacts; explore -o ts-explore-fresh.json --max-iterations 30 ts/01-arithmetic.ts:classifyNumber
[batch 1/1] classifyNumber: 30 iters, 4 paths, 3/3 branches, 12.3s (ok)
[warn] JSON output for explore writes spec bundle; use --spec-out for explicit spec output
[info] Wrote no-target spec marker (reason=unclassified) to .../ts-explore-fresh.json
exit 0
```
File contents: `{"version":1,"file":"ts/01-arithmetic.ts","functions":[],"status":"no_targets","no_target_reason":"unclassified"}`.
Stdout is only `# Shatter Explore` and a blank line (`ts-explore-o.stdout`).
Cause: specs are only built when `opts.show_spec || opts.detect_invariants`
(`explore.rs:3665`); `-o *.json` routes into the spec-bundle writer with no specs, and the
empty bundle is labelled "no targets". SPEC §2.1 says `-o` writes "a report; format inferred from
extension (`.html`, `.md`, `.json`, `.txt`)". The same "no_targets/unclassified" marker is
written when the target timed out (`rust-spec.json` after `build timed out`). A CI job reading
this file concludes the file has no functions.

Recommendation: make `-o x.json` write the explore report JSON (the per-function data already
in the explore artifact), or imply `--spec` for `.json`. Never write `no_targets` when targets
were attempted; record `status: failed` with the failure class instead. Add a CLI test that
`explore -o f.json` on a known-answer example has one function with four classes.

### F2 (P1, L5) The explore report under-reports paths (0 or 1 shown while 3 were found)

A 3-path TS function (`n>10` → "big", `n<0` → "neg", else throws):

```
[batch 1/1] fmt2: 30 iters, 3 paths, 2/2 branches, 1.3s (ok)
**1 path(s)** · **100%** coverage (5/5 lines)
| 1 | `fmt2(10.0)` | throws `Error: bad` |
```
A second identical run showed only `fmt2(11) returns "big"`. A third run
printed `**0 path(s)** · **100%** coverage (5/5 lines)` with an empty table, while the spec printed
in the same stdout says "**Behavioral classes:** 3" with all three classes
(`edge-plain-spec.md`). The artifact's `observation.unique_paths` is 1 and `new_path_executions`
has one entry, while `raw_results` contains all three branch paths.
The report (`shatter-cli/src/render.rs:105-137`) prints `unique_paths` and
`new_path_executions`; the `[batch]` line computes paths elsewhere. `classifyNumber` is not
affected, so this is input-dependent and nondeterministic. Root cause not isolated (confidence
medium on the mechanism, high on the symptom).

Recommendation: derive the report's path count and table from the same equivalence classes
the spec uses, and assert in an E2E test that report path count equals spec class count
for the known-answer examples. Add a known-answer fixture whose first branch sits on line 2
of the file with a throw fall-through.

### F3 (P1, L5) Mixed-language scan: the second language's sub-scan deletes the first's artifacts and overwrites summary.json

`scan mix` over 3 TS files and 2 Go files (`scan-mix.err`) logged Go artifacts
`functions/00001_…01-arithmetic.go__ClassifyNumber.json` … `00004_…04-errors.go__SafeDivide.json`,
then TS artifacts `00001` … `00008` in the same `scan-results/<id>/functions/` dir. After the
run only the eight TS files exist, and `summary.json` reports `total_functions: 8, failed: 0`,
while the report says 12 discovered, 4 failed (`scan-summary.json`, `scan-mix.json`).
The Go failures are gone from the resumable/inspectable artifact tree. Indexes restart at 00001
per language. Separately, the checkpoint lives in `scan-results/<first 16 hex>/checkpoint.json`
(`shatter-core/src/checkpoint.rs:196-203`, which also ignores `SHATTER_ARTIFACT_DIR`) while
function artifacts go to `scan-results/<full 64 hex>/` via `HarnessStorage::resolve_artifact_root`
(`scan_orchestrator.rs:557-565`): one scan, two directories.

Recommendation: namespace per-language sub-scans (or merge them into one summary with global
indexes); run the stale-artifact cleanup once per scan, not per language; put the checkpoint in
the same scan root through `HarnessStorage`. Add a two-language scan test that asserts every
function in the report has an artifact and a summary entry.

### F4 (P1, L6) Invariant rendering in spec markdown is broken: blank subjects, duplicate lines, "[1]"

`explore --invariants --spec ts/01-arithmetic.ts:classifyNumber` (`ts-spec-invariants.md`):

```
**Function invariants:**
-  != null [1] (100/100)
-  != null [1] (100/100)
-  is non-empty [1] (100/100)
```
For a scalar parameter or return, `format_path` joins an empty path (`invariants.rs:144-146`),
and the markdown uses `ci.invariant.description` and prints the f64 confidence
(`spec.rs:570-576`, `619-625`), dropping the input/output target, so the input and the output
`!= null` lines are identical. The JSON already carries a good `label`
("input.age is non-null") and YAML renders `property: input is non-null`
(`ts-spec-invariants.json`, `properties.yaml`); only markdown is wrong. Unit tests pass because
they use synthetic descriptions such as `"x > 0"` (`spec.rs:1737`).

Recommendation: render `label` in markdown, drop `[1] (n/n)` (with str-qwua7.61), and add a
golden test of `--invariants --spec` output for a scalar function.

### F5 (P1, L5) spec-diff misses the real behavior change and mispairs classes

v1 `grade`: `<0` invalid, `<50` fail, else pass. v2 inserts `n > 100 → "overflow"` before the
`<50` test. Classes (`sd-v1.json`, `sd-v2.json`):

```
v1 Class 2 "fail" path [(0,F),(1,T)]      v2 Class 3 "overflow" path [(0,F),(1,T)]
v1 Class 3 "pass" path [(0,F),(1,F)]      v2 Class 2 "fail" [(0,F),(1,F),(2,T)]; Class 4 "pass" [(0,F),(1,F),(2,F)]
```
spec-diff output (`sd-diff.txt`):
```
Summary: 2 added, 1 removed, 1 precondition(s) changed, 1 class(es) with insufficient comparison evidence
[ADDED]   Class 2 — returns "fail"
[ADDED]   Class 4 — returns "pass"
[REMOVED] Class 3 — returns "pass"
[PRECOND] Class 2 — returns "fail"   - typeof param[0] == "number"  + param[0] > 0
[INCONCLUSIVE] Class 2 — returns "fail": insufficient comparison evidence
```
The only real change (inputs above 100 went from "pass" to "overflow") is not reported:
"overflow" is not ADDED, and the fail→overflow pairing (same `BranchPath`) is shown under the old
label "fail" and downgraded to INCONCLUSIVE by the canonical-input guard (str-nfg4y). "fail" is
reported ADDED, PRECOND and INCONCLUSIVE in the same diff. This extends str-qwua7.38
(renumbering noise) to a false negative; it should be P1, not P2.

Recommendation: key classes on behavior (postcondition plus symbolic path condition), not on
branch ids; when two classes share a path but differ in postcondition, report CHANGED
with both labels; add this v1/v2 pair as a spec-diff known-answer test.

### F6 (P2, L6/L4) Three incompatible spec JSON shapes; `compare` rejects the only clean one

- `--spec-json` on stdout: bare `FunctionSpec`, no `version`, preceded by markdown.
- `--spec-out` / `-o`: `FileSpecBundle` `{version, file, functions[]}`.
- `properties` / `specify --yaml`: a YAML list of bundles.

`compare ts-spec-out.json go-spec.json` → `Error: failed to parse spec A …: missing field
function_name at line 172` exit 2 (`compare-ts-go.txt`). With the bare function extracted by hand
it works (`compare-ts-go-bare.txt`). `spec-diff` accepts bundles. So the documented
cross-language workflow (SPEC §2.6 "Typical use: verify a TypeScript and a Go implementation…")
has no clean producer: the bundle is rejected and the bare form only exists on a polluted stdout.
spec-diff of a TS bundle vs a Go bundle reports "Added functions: ClassifyNumber / Removed
functions: classifyNumber" (name-case mismatch) rather than suggesting `compare`.

Recommendation: one reader for all spec consumers that accepts bundle, bare spec, and
bundle list; `compare` gains `--function A[=B]` for bundles. Publish the spec JSON schema (F11).

### F7 (P2, L5) Spec preconditions are sample statistics, often wrong or vacuous; the solver's constraints are not used

Classes built from 30 iterations (`edge-plain-spec.md`, `ts-spec-invariants.json`, `sd-v1.json`):
- `fmt2` "big" (`n > 10`): precondition `param[0] > 0`, which is false (n=5 throws).
- throw class: `typeof param[0] == "number"`; all three `categorizeUser` classes:
  `SameType object`. These do not distinguish any class from another.
- `classifyNumber` "zero": `param[0] == 0` from 33 identical samples. The explorer re-ran the same
  input 33 times (also visible as `idle 88` in `ts-explore.err`).

The `branch_path` of every raw result already carries the symbolic constraint
(`{"op":"gt","left":{"param":"n"},"right":{"const":10}}` in `edge_sym.ts` artifact). The markdown
labels these `[observed]`, but a reader takes a precondition as a rule. Not filed after the
2026-09-04 audit (its recommendation 11 was merged into the spec-diff issue).

Recommendation: emit the path condition (the conjunction of branch constraints, rendered as
source-like text) as the precondition, keep sample statistics as a separate "observed inputs"
line, and suppress preconditions that every class shares.

### F8 (P1, AGENT/L5) The gauntlet's scan-failure check has matched nothing since 2026-05-13

`demo/gauntlet_check_output.py:28-30` flags `Scan complete: … N error(s)`. Commit `00124c84`
(2026-05-13, str-izhn) changed the summary to `Scan complete: **1 completed**, **4 failed**, …`
(`scan_orchestrator.rs:6120`). Running the checker on today's scan stdout with 4 failed and
7 interrupted functions: `python3 demo/gauntlet_check_output.py --allowlist
demo/gauntlet-scan-allowlist.yaml --output scan-mix.stdout` → exit 0, no output. Failed
functions never appear as `| FAIL |` rows (the Function Summary lists completed functions only)
and emit no `[error]` line (`grep -c '\[error\]\|\[warn\]' scan-mix.err` = 0). The checker's own
tests (`demo/test_gauntlet_check_output.py:31,70,82`) pin the dead format. The allowlist's
`expected_scan_errors: count: 2` for `11-opaque-types.ts`/`12-external-deps.ts` refers to
files that are no longer in the examples checkout, and root `CLAUDE.md` still documents that
allowlist. The fix for str-jeen.57 (closed 2026-05-08) lasted five days.
Four copies of the step checker exist (`walkthrough.sh:261`, `walkthrough-docker.sh:144`,
`gauntlet.sh:353-377`, `gauntlet-docker.sh:167`); the Docker gauntlet has no FAIL/summary check.

Recommendation: have scan emit a machine-readable summary line (or use the `-o *.json`
`codebase.failed_functions`) and make the checker parse that; generate checker test fixtures by
running the CLI instead of hand-writing strings; one shared checker for all four scripts. Fold
into str-qwua7.10.

### F9 (P2, L6) HTML "Paths" is branches covered

`html_templates.rs:382` `total_paths = sum(branches_covered)` and `:428`
`paths_count: f.branches_covered`. `scan-mix.html`: tile "Paths Found 3" and table "Paths 3"
directly above a table listing 4 paths; stdout and markdown say 4. Recommendation: count
discovered inputs or classes, and label branches as branches.

### F10 (P2, L6) Spec YAML is not portable YAML

`properties.yaml` encodes preconditions with serde_yaml external tags (`- !AllEqual`).
`yaml.safe_load` → `ConstructorError: could not determine a constructor for the tag
'!AllEqual'`. JSON uses `{"AllEqual": {...}}`. Recommendation: internally-tagged
`{kind: all_equal, param_index, value}` in both formats (a spec schema bump), and a test that
parses YAML output with a strict YAML 1.2 loader.

### F11 (P2, L3/L2) No published schema for any output artifact; SPEC §5 is stale

- `protocol/schemas/` has 22 schemas for the frontend protocol, none for spec bundles, scan
  report JSON (v9), explore artifacts, `summary.json`, `manifest.json`, or `run-status.tsv`.
- SPEC §5.1 shows `Explored: classifyNumber / Iterations: 50 / New paths: [1] …`; the real
  report is a markdown table (`ts-explore.md`).
- SPEC §5.3 shows a bare FunctionSpec; the `--spec-out` file is a versioned bundle.
- SPEC §5.6 says scan JSON holds "per-function behavior maps, coverage, and analysis"; the file
  has `discovered_inputs`, `behavior_clusters`, `constraint_stats`, `completion_outcome` and no
  behavior map or analysis (`scan-mix.json`).
- SPEC §6.2 documents `scan-results/<16 hex>/checkpoint.json` only; runs write
  `scan-results/<64 hex>/{manifest.json,summary.json,run-status.json,run-status.tsv,functions/}`.
- `docs/PROJECT-LAYOUT.md:249-254` says the "most clearly documented current artifact path"
  is `recorded-mocks/` and describes the rest as "design direction"; `explore-results/` and
  `scan-results/`, written by every run, are not mentioned.
- The shatter plugin's `interpret-shatter-spec` skill expects
  `shatter-artifacts/<name>.spec.json`, a path Shatter never writes.

Recommendation: generate JSON Schemas from the serde types (`schemars`) into
`protocol/schemas/artifacts/`, check them in a drift test (like the protocol registry), link them
from SPEC §5 and from the plugin skills, and rewrite SPEC §5.1/§5.6/§6.2 from real output
(docs-smoke can execute them).

### F12 (P2, L6) Scan report: misleading headline, absolute paths, two different summaries

`scan-mix.md`: 12 discovered, 1 completed, 4 failed, 7 not attempted, yet "**Overall coverage
(completed-functions subset):** 100.0%" and the HTML tile "Coverage 100%" (no subset label).
Paths are absolute temp paths in every table, in the JSON `file_path`/`qualified_id`, and in
artifact file names (`00001_tmp_claude-1000_-home-ketan-…__classifyNumber.json`, 150+
characters), so reports cannot be diffed across machines or checkouts. With `-o`, stdout gets a
`# Scan Results` table keyed by `/abs/path::fn` while the file gets a `Function | File` table.
"Interesting Inputs" lists 2 of 4 discovered inputs with no stated criterion. The "Low-Coverage
Buckets" section and the seven-row "Source Set Summary" (six rows of zeros, snake_case bucket
ids) print on every report.

Recommendation: headline "N of M functions completed" before any coverage figure and show
coverage over all discovered functions; relative paths (to project root) everywhere including
JSON (keep an absolute `project_root` once); omit zero rows and empty sections; state the
Interesting Inputs rule or drop the section.

### F13 (P2, AGENT, target shatter-agents) The plugin ships a `shatter-diff` skill for a CLI that does not exist

`shatter-agents/catalog/skills/shatter-diff/SKILL.md` (shipped in `plugins/claude` and
`plugins/codex`) drives `shatter diff [<base-ref>] [--staged] [--include-tests] --format json
--output-dir`. The real `shatter diff` compares two behavior snapshots:
`shatter diff --staged` → `error: unexpected argument '--staged' found … Usage: shatter diff
[OPTIONS] <SNAPSHOT> <CURRENT>`. The plugin issue sa-tyb was closed "landed on main" when the
skill text landed; the Shatter-side feature is still open as str-81xiw ("diff-explore"), under a
different name. Recommendation: pull the skill from the released plugin until
str-81xiw lands, rename it to the eventual command, and add a plugin CI check that every
`shatter <cmd> --flag` in a skill parses against `shatter <cmd> --help` of the pinned binary.

### F14 (P2, L5/L2) Nothing produces a behavior snapshot, so `shatter diff` has no input

`Snapshot::write_to_file` and `Snapshot::from_behavior_map(s)` have no callers outside
`shatter-core/src/snapshot.rs`; no command's `--help` mentions "snapshot" except `diff`
(`grep -ci snapshot` = 0 for explore/scan/run/observe/analyze/specify/properties). QUICKSTART:160
recommends `shatter diff snapshots/shipping.json current/shipping.json` and SPEC §5.5 documents the
format, but neither says how to make one. Recommendation: add `--snapshot-out` to explore/scan,
or retire `diff` in favour of `spec-diff`, and fix QUICKSTART.

### F15 (P3, L6) Smaller output defects

- `explore --format text` output is byte-identical to the markdown (`edge-text.out` vs
  `edge-md.out`). The scan text path's `strip_markdown_text` (`report.rs:1918-1946`) deletes every
  `*`, `#` prefix and `|`, which would corrupt Go pointer types (`*Order`), TS unions and values
  such as `"a | b"` if it were applied.
- `explore -o file.html` leaves `# Shatter Explore` plus a blank line on stdout
  (`ts-explore-o.stdout`).
- `--spec --spec-out F` silently drops the markdown spec from stdout.
- `explore --analyze-only` prints `params: 1, branches: 3` with no names, types or conditions
  (`analyze-only-ts.out`), while walkthrough step 1 promises "Discover parameters, types, and
  branch conditions". It also refuses to run without a sandbox, though it executes nothing
  (`analyze-only-ts.err`).
- HTML source view marks non-executable lines (signature, closing braces) "uncovered" next to a
  "7/7 lines" label (`ts-explore-o.html`).
- Explore failure table labels a Rust function's language as `any` (`rust-explore.md`).
- All four demo scripts print "complete with errors" in green (`demo/walkthrough.sh:388` etc.).

## Agent-system observations

1. **No producer/consumer contract tests.** The gauntlet checker, `compare`, the plugin skills
   and SPEC §5 each hard-code an output shape; none of them runs against real output. Every
   format change (str-izhn, spec bundle versioning) silently broke a consumer. A golden-output
   suite (explore/scan/spec/spec-diff on known-answer examples, TS+Go+Rust) checked by
   `task e2e` would have caught F1, F2, F4, F5, F8 and F9.
2. **Demo gates validate process health, not results** (str-qwua7.10). F0 shows that the
   walkthrough step can be a no-op without any signal; a check that concolic steps report
   `solver_guided_inputs > 0` or a `[proven]` class would expose it.
3. **Issue closure without end-to-end verification.** str-jeen.57 (checker) and sa-tyb (skill)
   closed on text changes; neither closure required running the consumer against the current
   binary.
4. **SPEC §5 is not executed.** docs-smoke (str-qwua7.9) validates config/snapshot examples; it
   does not compare §5 report examples with real output. Extend it to render the known-answer
   example and diff the shape.
