# Shatter — Usability & UI Review

Read-only review of CLI help, captured and live outputs (`target/release/shatter 0.1.0`,
run from `scratchpad/ui/proj`), spec/JSON artifacts, demo gate logs, `shatter-vs`,
HTML report, and contributor inner loop. Live captures are in `scratchpad/ui/live-*.{out,err,json}`.
Date: 2026-09-04.

## 1. Help text consistency

**Flag-name drift across 24 subcommands** (from `help-*.txt`):

| Concern | Spellings in use |
|---|---|
| Per-function time cap | explore: `--per-function-timeout` **and** `--timeout-explore`; scan: `--timeout-per-fn` **and** `--timeout-explore`; run/observe/properties: `--timeout` |
| Whole-run time cap | explore `--time-limit`; scan `--timeout-total`; run `--timeout` ("Overall timeout") |
| Machine-readable output | `--json` (compare, diff, spec-diff, specify, discover-deps); `--spec-json` (explore, analyze); `--format json` (scan, list-targets); `--output-format` (properties = "currently only 'yaml' is supported", revalidate, stale); `--yaml` (specify) |
| Output path | `-o/--output` (explore, scan, analyze, solve, specify, list-targets); `--output-dir` (run); `--spec-out` (explore, "implies --spec-json"); `--observe-output`, `--persist-stages` |
| Project dir | global `--project-dir`; doctor/init `-d/--directory`; discover-deps `--working-dir`; list-targets positional `[DIRECTORY]` |

- **P1** Every subcommand — including `init`, `cache`, `telemetry`, `spec-diff`, `doctor` — repeats the six global timing flags plus `--allow-host-writes`, `--set`, `--color`, `--render`, `--project-dir`. `shatter spec-diff --help` is 60 lines, 13 flags, of which 2 are its own. `--allow-host-writes` (a 5-line paragraph about sandboxing target execution) is shown on `spec-diff`, which executes nothing. Floor of 13 flags for a leaf command is pure noise.
- **P1** `explore` has 79 flags, `scan` 69; `--help` for explore is 15 KB. Interleaving is unordered: `--log-level`, then `--max-iterations`, `--per-function-timeout`, `-v`, `-q`, `--timeout-explore`, `--time-limit`, `--timing`… (global flags are shuffled into command flags because clap renders them by declaration order). No `help_heading` grouping ("Budget", "Output", "Strategy", "LLM").
- **P2** Tracker IDs leak into user-facing help: "(str-gg9v)", "(str-jeen.13)", "See str-frc.3 / str-frc.6", "(str-izhn)", "str-v01r / str-p2rz", "(str-o09e)", "(str-1fwt)". 9 occurrences across help text. Meaningless to users.
- **P2** Defaults documented two ways: `[default: 10]` for `--exec-timeout` *and* "Default: 10s" in prose; `--max-iterations` says "[default: 100]" in prose only (no clap default) on explore but `[default: 50]` on run.
- **P2** `--concolic` described as "instead of the random explorer" (explore), "instead of random exploration" (observe), and "Routes every function through the concolic orchestrator…" (run). No guidance on when to choose it; QUICKSTART §4 silently adds `--concolic` for spec generation without explaining why.
- **P2** Top-level command list has no progression. 24 commands, alphabetical-ish, mixing user commands (`explore`, `scan`, `run`), pipeline internals (`observe`, `analyze`, `solve`, `specify`), dev tools (`bench`, `build-frontend`, `discover-deps`, `workspace`) and maintenance (`cache`, `telemetry`, `doctor`). `explore`/`scan`/`run` overlap is not explained: `run` says "Unlike `scan`, `run` exposes no CLI flags for scope filters".
- **P3** `--render plain` labelled "(deprecated)" in help of every command; `--format text` claims "markdown with formatting stripped" but live output still contains `#` headings, `**bold**` and `|` tables (`live-text.out`).

## 2. Happy-path output

Explore (markdown, default) is good: one heading per function, a 3-column table `# | Call | Outcome`, coverage summary. Captured `ts-explore.out` is scannable in 10 lines. No ANSI escapes leak when piped (`cat -v` shows 0 `^[` sequences for both `--format text` and default). `NO_COLOR` honored.

- **P1 (agent-breaking)** `shatter explore --spec-json <target>` writes the **markdown explore report followed by the JSON spec on the same stdout**. Live: `live-specjson.out` starts with `# Shatter Explore … | 1 | classifyNumber(0) | …` then `{ "function_name": …`. `python3 json.load` → `JSONDecodeError: Expecting value: line 1 column 1`. The pre-captured `ts-spec-v1.json` has the same contamination. Only `--spec-out FILE` or `-o file.json` produce clean JSON. QUICKSTART §4 recommends `--spec-json --spec-out`, but nothing warns that `--spec-json` alone is unparseable.
- **P1** Exit codes contradict SPEC §2.11 ("`2` = usage or tool error … a malformed input file"). Live:
  - `explore nope.ts:foo` → `Error: file not found: 'nope.ts'` **exit 1** (spec says 2)
  - `explore t.py:f` → `Error: unsupported file extension '.py'` **exit 1**
  - `explore arithmetic-v1.ts:doesNotExist` → `[error] Analyze error (FunctionNotFound)…` then `Error: explore: all 1 attempted target(s) failed (build_failed=0, runtime_failed=0, timed_out=0)` **exit 1**
  - `spec-diff bad.json v1.spec.json` → `Error: failed to parse spec 'bad.json': EOF while parsing…` **exit 1** (SPEC §2.11 gives exactly this case as an example of exit 2)
  - host-write refusal → **exit 1**
  - Only clap usage errors exit 2. A CI job cannot distinguish "regression found" from "spec file corrupt".
- **P1** `scan .` and `run .` on the same directory disagree about a missing Rust frontend: `scan` warns, skips the `.rs` file and exits **0**; `run` aborts with `Error: shatter-rust frontend not found…` and exits **1** with no report at all. Same 600-character remediation paragraph is printed 2× per scan (once as `[warn]`, once as `STATUS skipped_by_unavailable_frontend … hint=`).
- **P2** `scan` prints **two consecutive reports** to stdout: `# Scan Results` (table) immediately followed by `# Shatter Scan Report` (bullets + Source Set Summary + per-function sections + Uncovered Branches + Interesting Inputs), ~150 lines for 3 functions. The table uses absolute paths with `::` separator (`/tmp/…/proj/arithmetic-v1.ts::classifyNumber`) while explore uses `file:function` and relative paths.
- **P2** Contradictory line in scan report: `` `Categorize`: 0 uncovered branch(es) (58.3% coverage) `` and `` `classifyNumber`: 0 uncovered branch(es) (90.9% coverage) `` — branch coverage 100% but line coverage <100% is rendered as if a branch gap exists.
- **P2** Terminology is not unified: explore → "**path(s)**"; spec → "**Class N**", "Behavioral classes"; scan → "**Cluster N**"; JSON → `classes`, `equivalence_classes`, `behaviors`; help → "equivalence classes, behavior map". A user sees four names for one concept in one session.
- **P2** `--timeout-explore 1` on a 6-path function exits 0 with no indication in stdout or stderr that the budget fired (`live-timeout.*` contain no "timeout"/"stopped" text). `TerminationReason` exists in core but is not surfaced.
- **P3** Stderr `[info]` lines contain 200+ character absolute artifact paths (`Wrote explore artifact for classifyNumber -> /tmp/…/00011_classifyNumber.json`) on every run; scan emits one per function. Should be debug-level or relative.
- **P3** `doctor` exits 1 in a source checkout because the embedded Go frontend is "stale" — every `cargo build --release` produces a stale doctor unless `-p shatter-cli` is rebuilt; users installing from release never see this, but contributors see `doctor` red by default.

**Error actionability:** good in the cases that matter. Host-write refusal (verbatim, `live-firstrun.err`):

```
Error: refusing to execute target functions without a sandbox.
…
Choose one:
  • Configure an OS sandbox (recommended):
      export SHATTER_SANDBOX_BACKEND=docker    # or: bwrap
  • Opt into unsandboxed execution (targets still run in a throwaway working directory):
      shatter … --allow-host-writes
      # or, once per shell/CI job:
      export SHATTER_ALLOW_HOST_WRITES=1
```
That is a model error message. But **P1**: QUICKSTART §2's first command, `./target/release/shatter explore shipping.ts:calculateShipping`, is exactly the command this refuses. The quickstart never mentions `--allow-host-writes` or a sandbox (QUICKSTART.md:62-80). First-run experience = refusal.

## 3. Spec markdown / JSON for agents

Markdown spec (`ts-spec.md`) is clear: per-class Preconditions `[observed]`, Postcondition, Example block, sample count. JSON spec is well-shaped and stable-looking: `function_name`, `location`, `classes[] {label, branch_path, preconditions, postcondition{kind,value}, side_effects, examples[], sample_count, *_provenance}`.

- **P2** Preconditions are single-sample tautologies: with 20 iterations the spec says `param[0] == 0`, `== -1`, `== 2`, `== 1` for four classes whose real preconditions are `n == 0`, `n < 0`, `n > 0 && even`, `n > 0 && odd`. JSON encodes as `{"AllEqual": {"param_index": 0, "value": 2}}` / `{"AllPositive": …}` — an externally-tagged enum that agents must special-case per variant. Symbolic constraints (`SymExpr`) already exist in core but are not exported to the spec.
- **P2** `label` embeds a 1-based index (`"Class 3 — returns \"positive-even\""`). `spec-diff` then keys on it: `specdiff.txt` reports `[ADDED] Class 3 — returns "positive-even"` **and** `[REMOVED] Class 3 — returns "positive-even"` for the same behavior because a new class shifted numbering. Diff of 1 real change reports 3 added / 2 removed.
- **P3** Spec markdown is emitted *after* the explore report on the same stream (`ts-spec.md` = explore report + spec) — same concatenation problem as JSON.
- Good: `spec-diff --json` is clean JSON with `diffs/added_functions/removed_functions`; exit 0/1 works for same/changed.

## 4. Exported tests

No CLI surface exists. `export.rs` has `generate_jest_tests`/`generate_vitest_tests`/`generate_go_tests`, but SPEC §8 changelog says "removed the deleted test-export output" and `args.rs` has no export flag; `shatter-cli/src/commands/explore.rs:2128` only mentions Jest for file-name conventions. **P2**: README §"How Shatter Works" step 4 still promises "optional specs **or tests**". Either wire `--export-tests {jest,vitest,go}` or remove the claim and the dead generators.

## 5. Visual surfaces

- `styles/` is a Vale prose-lint vocabulary directory (`styles/Vocab`), not UI styles.
- `shatter-report/` is a checked-in sample artifact: `scan-report.md` shows a run with **0 functions explored, 36 skipped (fingerprint match)** — as a showcase it demonstrates nothing. Last touched March 2026.
- HTML report (`report.rs`/`html_templates.rs`, gauntlet writes `explore.html`, `scan.html`): no `prefers-color-scheme`, `@media`, `font-family`, `max-width`, or `<style` token anywhere in `shatter-core/src` → **P3** no dark mode, no responsive rules, no typographic system; visual quality cannot be assessed beyond "unstyled or inline".
- `shatter-vs`: `package.json` contributes 8+ commands with duplicated titles (`"Autotest"` ×2, `"Regression test"` ×2, `"Reset Shatter workspace"` ×2 — one of them on `shatterReviewTestcasesFromTreeView`, a copy-paste error), empty `description`, version 0.0.1, no README, activates only `onLanguage:typescript`. Extension code has **zero** references to the `shatter` CLI, `.shatter/`, or specs; its `src/core/{generator,hybridize,seed}.ts` is the v1 in-process fuzzer. **P2**: incoherent with the CLI product; should be deleted or archived.

## 6. Contributor UX

- README quickstart is complete for install + build prereqs (libclang, Z3, go-task, pyyaml/jsonschema listed). **P1** carry-over: first explore command refused (see §2).
- Inner loop: `task test-quick` = "cargo workspace tests only" with `deps: [frontends-built]`; 60+ Taskfile targets (`check`, `check-fast`, `check-fast-governed`, `check-governed`, `affected`, `affected-governed`, `*-governed` variants ×6). **P2** The `-governed` duplication doubles the target list; a newcomer cannot tell which gate is required.
- Test names (30 sampled): Rust names are descriptive and behavior-first (`user_seeds_consumed_before_literals`, `overlay_resolves_serde_rename_all_camel_case_field`, `flaky_when_code_unchanged_but_path_differs`); a few are meaningless (`fn default()`, `fn new()`). TS names good (`does not capture console output when capture is false`). Go names good but tracker-tagged names appear in TS (`… (str-ya5dx)`). Rating: Good.
- Flakiness evidence: 74 `#[ignore]` in Rust, all with reasons (`"subprocess E2E; run via task e2e-ts"`, `"documentation helper"`); Go `t.Skip("go toolchain unavailable")` ×6+; no `retry` wrappers found in test code or CI. TS Jest suite: 976 tests, 125 s, ends with "Force exiting Jest: … async operations that kept running" (`gate-ts-test.log`) — **P3** leaked handles, latent flakiness.

## 7. Agent integration

- Exit codes: documented in SPEC §2.11 (good) but **not implemented** for tool errors (all 1, see §2). README/QUICKSTART don't mention exit codes at all.
- Structured errors: stderr is `[level] text`; no JSON error envelope. Explore JSON report has `status`/`no_target_reason` fields (good), scan JSON has `functions/codebase/test_order` (good).
- JSON free of noise: `-o file.json` yes; `--spec-json` to stdout **no** (P1 above); `--format json` on scan — stdout clean. `[info]` goes to stderr, correct.
- `spec-diff` in CI: exit 0/1 works; docs/CI-INTEGRATION.md is about **this repo's own** Taskfile gates, not user CI — there is no user-facing "run shatter in CI and gate on spec-diff" recipe. `action.yml` only installs the binary. README:334 is one bullet. **P2**.
- Rust targets: release install claims `shatter-rust` "ships as a sibling binary", but a source build leaves `scan` silently skipping `.rs` (exit 0) and `run` hard-failing; the 600-char hint is repeated on every invocation.

## Demo gates: are the walkthrough/gauntlet hiding product bugs?

**Rust deserialization failures (walkthrough Step 7, `walkthrough.log:2570-2620`).** `classify_number(0)` and `safe_divide(0.0, 0.0)` throw `runtime_error: input 0 deserialization failed: invalid type: integer 0, expected a string`; both report **0% coverage** and are marked `(ok)`. The analyzer typed the params numerically (it generated ints/floats) while the generated harness (`shatter-rust/src/executor.rs:2471/2484/2498`) deserializes into `String`. That is an analyzer/harness type disagreement in the Rust frontend — a **product bug**, not an explorer limit. `negotiate_language` shows 0/102 lines with 2/13 branches, also `(ok)`.
Does the gate catch it? No. `walkthrough.sh:261` `error_pattern='\[error\]|failed to deserialize|panic|SIGSEGV|error: exploration error'` — the message is "deserialization failed", which does **not** match `failed to deserialize`. Coverage is never checked. `EXIT=0`, "All steps passed."

**Teardown scope mismatch (gauntlet, 390 lines).** All 390 occur in three scan steps (Step 6 "Scan Standalone TypeScript" 133, Step 23 "Scan Total Timeout" 132, Step 55 "HTML Scan Report" 125), always under `### teardown` from `setup-file-level.ts`. Shatter is treating the example's exported `setup`/`teardown` lifecycle helpers as ordinary targets and fuzzing them; `teardown` throws on mismatched scope. That is a **scope-policy bug** (setup/teardown exports should be excluded from target discovery, or the example is mis-specified) producing 87+ garbage clusters per run. `gauntlet_check_output.py` `PROCESS_ERROR_RE = \[error\]|failed to deserialize|panic|SIGSEGV|error: exploration error` does not match `throws Error: Teardown scope mismatch`; the `| FAIL |` row check only fires on coverage-threshold failures and the function reaches 100%. The allowlist (`gauntlet-scan-allowlist.yaml`) does not list it — and would not need to, because nothing flags it. `EXIT=0`, "All steps passed."

Verdict: both gates test "did the process exit 0 and avoid a panic", not "did Shatter produce a sane result". Neither the checker nor the allowlist would ever flag 0% coverage, all-throwing functions, or lifecycle helpers explored as targets.

## 8A. Contributor summary

| Area | Human | Agent | Note |
|---|---|---|---|
| Build/install docs | Good | Good | prereqs complete |
| First run (quickstart) | **Poor** | **Poor** | refused by default-deny; flag undocumented in quickstart |
| Inner loop (`task test-quick`) | Fair | Fair | 60+ targets, `-governed` duplicates |
| Test naming | Good | Good | few `default()`/`new()` |
| Flakiness signals | Fair | Fair | Jest force-exit; ignores are reasoned |
| Demo gates as QA | **Poor** | **Poor** | exit-0 with 0% coverage and 390 spurious throws |
| `doctor` in checkout | Fair | Fair | red after normal build |
| Help text | Fair | Poor | 13-flag floor, tracker IDs, 79-flag explore |

## 8B. End-user summary

| Area | Human | Agent | Note |
|---|---|---|---|
| Explore report (md) | Good | Good | scannable, no ANSI leak |
| Scan report | Fair | Fair | two reports, absolute `::` paths, contradictory coverage line |
| Spec markdown | Good | Fair | tautological preconditions |
| Spec JSON | Fair | **Poor** on stdout | markdown prefix breaks `--spec-json`; index-bearing labels |
| spec-diff | Fair | Fair | numbering churn; JSON clean; exit 0/1 ok |
| Exit codes | Fair | **Poor** | SPEC says 2 for tool errors; observed 1 |
| Error messages | Good | Fair | sandbox refusal exemplary; frontend-missing hint repeated |
| Flag consistency | Poor | Poor | 5 spellings for "timeout", 5 for "json" |
| scan vs run parity | Poor | Poor | opposite behavior on missing frontend |
| Exported tests | Missing | Missing | promised in README, no command |
| HTML report / VS ext | Poor | n/a | no dark/responsive; extension unrelated to CLI |
| CI recipe | Fair | Fair | action installs only; no gate example |

## Issue-ready recommendations

P1
1. **Make `--spec-json`/`--spec` stdout exclusive**: when a spec format is requested on stdout, suppress the explore report (or move it to stderr / require `-o`). Add a test that `explore --spec-json` stdout parses as JSON.
2. **Implement SPEC §2.11 exit codes**: map `FileNotFound`, unsupported extension, `FunctionNotFound`, spec parse errors, and sandbox refusal to exit 2; keep 1 for fired gates. Add CLI integration tests per class.
3. **Fix QUICKSTART first-run**: show `--allow-host-writes` (or `SHATTER_SANDBOX_BACKEND`) in §2 and explain why, before the first `explore`.
4. **Align `scan`/`run` on missing frontends**: same skip-with-warning default, same `--require-rust` opt-in; print the remediation paragraph once.
5. **Demo gates check results, not exit codes**: extend `gauntlet_check_output.py` with (a) `deserialization failed` in `PROCESS_ERROR_RE`, (b) a per-function floor (coverage 0% with all-throw ⇒ flag), (c) `throws Error: Teardown scope mismatch`-class lifecycle failures. Apply the same checker to the walkthrough.
6. **Rust frontend**: fix analyzer/harness param-type disagreement (int/float typed as `String` in generated harness) — the walkthrough's own Rust examples are 0% covered.
7. **Exclude setup/teardown exports from target discovery** (or document that `setup-file-level.ts`-style files must use the `*.shatter.setup.ts` convention) so scans stop producing hundreds of throw clusters from lifecycle helpers.

P2
8. **Flag vocabulary**: one `--timeout-per-function`, `--timeout-total`, `--format {md,json,html,text,yaml}`, `-o` across all commands; deprecate aliases with hidden `alias =`. Use clap `help_heading` to group Budget / Output / Strategy / LLM / Advanced; hide `--allow-host-writes`, `--set`, timing flags on commands that don't execute targets.
9. **Strip tracker IDs** (`str-…`) from help text and user-facing messages.
10. **Unify terminology**: pick "behavior class" (or "path") and use it in explore, scan, spec, JSON keys, and help.
11. **Stable spec labels**: key classes by branch-path hash, not "Class N"; make `spec-diff` match on hash so renumbering doesn't produce add/remove pairs. Export symbolic preconditions (`n > 0 && n % 2 == 0`) instead of `AllEqual{value:2}`.
12. **Surface termination reason** in explore/scan output ("stopped: timeout 1s" / "plateau") so users know whether coverage is a limit or a budget.
13. **Test export**: either add `explore --export-tests <jest|vitest|go> -o dir` or delete `export.rs` and the README claim.
14. **User-facing CI doc**: one page with `explore --spec-out baseline.json` on main, `spec-diff baseline.json new.json` in PR, exit-code table, and the GitHub action.
15. **Archive `shatter-vs` and `shatter-report/`**; fix or delete the duplicated command titles.
16. **Scan output**: one report, relative paths, `file:function`, drop the "0 uncovered branch(es) (58.3% coverage)" line when branches are fully covered.

P3
17. Demote `Wrote … artifact -> <abs path>` to debug; print a single relative artifact dir at the end.
18. Add dark-mode + responsive CSS to the HTML report; fix `--format text` to actually strip markdown.
19. `doctor`: distinguish "stale embed in a dev checkout" (warn) from real install problems (fail).
20. Resolve Jest open-handle force-exit; rename `default()`/`new()` tests.
