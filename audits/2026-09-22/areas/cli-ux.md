# CLI design and UI review (area: cli-ux) — 2026-09-22

Scope: `shatter-cli` design (L1, L4) and terminal UI (L6): `--help` for all 24 subcommands, and live runs of
explore, scan, run, spec-diff, doctor, init, list-targets, telemetry and cache on TS, Go and Rust examples.

Binary: `target/debug/shatter` built from `audit-2026-09-22` (HEAD `16794cef`). The `run-heavy` build took 8 minutes
because the host load average was about 200. I copied the binary to a private scratch directory and checked that its
help output was byte-identical to the primary checkout's `target/debug/shatter` (built today). The Rust frontend was
built with `cargo build --manifest-path shatter-rust/Cargo.toml` and placed on `PATH`.

Examples: TS/Go/Rust `standalone/*` from the external examples checkout (`/tmp/shatter-examples-main`, the same source
`demo/walkthrough.sh` uses via `scripts/examples_checkout.py`) and `demo/fixtures/arithmetic-v{1,2}.ts`.
All runs used `SHATTER_ALLOW_HOST_WRITES=1` except the first-run refusal test.

Transcripts: `audits/2026-09-22/cli-ux-transcripts/` (the `help/` subdirectory has `--help` and `-h` for every subcommand).

Caveat: the host load was very high (60–220) throughout. Wall times are not meaningful. One re-scan hit 125 s
per-function timeouts, which I treat as environmental.

## 0. Status of the assigned prior issues

| Issue | Tracker | Observed at HEAD | Evidence |
|---|---|---|---|
| str-qwua7.11 `--spec-json` stdout must be pure JSON | open | **Still true** | `ts-specjson.out` starts `# Shatter Explore`; `json.load` → `Expecting value: line 1 column 1`. `spec-diff --help` still says specs are "as produced by `explore --spec-json`". |
| str-qwua7.12 SPEC §2.11 exit codes | open | **Effectively fixed**: close it. prior-audit-regress.md shows `error_exit_code` predates the 09-04 audit | missing file / `.py` / unknown fn / bad `--set` / fn glob / spec-diff bad JSON / missing spec / host-write refusal all exit **2**. spec-diff regression exits 1, same exits 0. Remaining nit: `helpers.rs:347` `print_stdout` exits **1** on a stdout I/O error. SPEC.md:640-644 still says "str-qwua7.12 tracks bringing the remaining commands into line". |
| str-qwua7.13 scan/run missing frontend, hint printed once | open | **Still true, and worse than filed** | `explore 04_errors.rs` with no shatter-rust prints the ~600-char hint twice (a `STATUS … hint=` line and then `Error: …`), 1,345 bytes of stderr, `rust-explore.err`. Following the hint (shatter-rust on PATH) then fails with `cannot locate shatter-rust-runtime crate; set SHATTER_RUNTIME_PATH` (`rust-explore2.err`), which no user doc mentions (see F10). |
| str-qwua7.15 hide execution-only globals | **closed** | **Partially fixed** | `spec-diff --help` 48 lines (was 60), and `init/doctor/cache/telemetry` are clean. But `shatter help spec-diff` (78 lines) still shows `--allow-host-writes` because the fix intercepts only `-h/--help` argv (`main.rs:70`). Nine other commands that SPEC §2.10 calls non-executing (analyze, solve, specify, stale, diff, compare, list-targets, nondeterminism, workspace) still show it. See F5. |
| str-qwua7.20 CLI argument-surface epic | open | Unchanged | explore 79 flags / 307 lines / 15.7 KB; scan **70** (+1 since 09-04: `--seed`); run 30. `help_heading` still 0 uses in args.rs. Global flags are still interleaved (`scan.short.txt:10-30`). |
| str-qwua7.39 implicit-init "Created" lines on stdout; language "unknown" | open | **Still true, and it breaks the scan JSON contract** | `ts-firstrun`/`go-explore`/`rust-explore` all print `Created .shatter/config.yaml (detected language: unknown)` on **stdout** (`init.rs:82,91,143`), even for a Go-only or Rust-only directory. New: `scan . --format json` in a fresh dir → stdout is not JSON (`scan-json.out`: 4 "Created/Initialized" lines, then `{`). See F4. |
| str-qwua7.57 "behavior class" terminology | open | Still true | Four names in one session: explore "**4 path(s)**", scan "Cluster 0..N", spec "Class 1 — …", `--show-clusters` "behavior clusters". Top-level help says "equivalence classes, behavior map". |
| str-qwua7.58 `--no-init` / `SHATTER_NO_INIT` | open | Not started | No `no-init` in any help page; no `SHATTER_NO_INIT` in shatter-cli/src. |

## 1. Findings (new unless marked)

### F1 [P1 L1/L6] `explore <target> -o out.json` writes an empty `no_targets` bundle after a successful run
- `explore 01-arithmetic.ts:classifyNumber --clean -o explore-bundle3.json` explored 4 paths at 100% and exited 0.
  The file it wrote:
  ```json
  {"version":1,"file":"01-arithmetic.ts","functions":[],"status":"no_targets","no_target_reason":"unclassified"}
  ```
  stderr: `[warn] JSON output for explore writes spec bundle; use --spec-out for explicit spec output` and then
  `[info] Wrote no-target spec marker (reason=unclassified)`. `--spec -o x.json` behaves the same (`explore-bundle4.json`).
- Code: `commands/explore.rs:6643-6665`. `file_spec_bundles` is only populated on the `--spec-json`/`--spec-out` path.
  Otherwise the fallback built for "analyze/preflight failed" (str-ni32 comment) runs on a successful run too.
- Help (`args.rs` ExploreStdoutFormat doc, `help/explore.txt` `-o`) calls `-o <file>.json` *the* JSON path for explore.
  A CI job that uses it gets a file that says nothing was explored, with exit 0.
- Fix: `-o *.json` builds the spec bundle unconditionally, or rejects `.json` and points to `--spec-out`.
  The no-target marker should be written only when there were actually no targets. Add a CLI test: explore a
  known-answer function with `-o x.json` and assert `functions.len() > 0`.

### F2 [P1 L1/L6] `explore --format text|html` does nothing on stdout; two overlapping format flags
- `explore … --format html` → stdout is markdown (`format-html.out`). `--format text` → markdown with `#`/`**`
  (`format-text.out`). `--format text -o r.md --stdout` → still markdown (`format-text-with-o.out`).
- Cause: the streaming printer branches on `output_format` (the `--render md|plain` enum), not on `format`
  (`--format`). See `commands/explore.rs:4780-4790` and `:3853-3865`. `--format` is honored only by the post-run
  `-o`+`--stdout` replay (`:4029-4038`, `:6688-6697`).
- Explore therefore has `--render {md,plain}`, `--format {markdown,html,text}`, `--color` and `-o` extension
  inference, and they interact silently. No test covers `explore --format` (only `explore … --format json` as a clap
  rejection in `tests/json_stdout_contract.rs:250`).
- `strip_markdown_text` (`shatter-core/src/report.rs:1917-1945`) also strips every `*`, `` ` ``, `|` and leading `#`
  from **data values**, so an outcome like `"2*3=6 | a`b`"` would be corrupted in text mode. I could not show this
  live because text mode is not reachable on stdout.
- Fix: one `--format` that governs stdout for every value it accepts. Remove `--render plain` (after porting its
  extra information, see F14). Strip markdown structurally, from the AST or by rendering from data, not with a
  character filter.

### F3 [P1 L6] `-o FILE --stdout` prints the explore report twice, and `-q -o FILE` leaks the header to stdout
- `explore 01-arithmetic.ts -o x.json --stdout`: stdout holds the full streamed report, then a replay of it with
  extra `## classifyNumber / **Status:** completed / exploration completed` blocks (`explore-o2.out`).
  The streaming path always prints, and the replay (`explore.rs:4029`, `:6688`) prints again.
- `explore … -o report.html -o bundle.json -q` (no `--stdout`) → stdout is `# Shatter Explore\n\n` alone
  (`explore-o.out`, 19 bytes). With `-o` and no `--stdout`, stdout should be empty.
- Fix: choose a single stdout emitter per run, either streaming or replay, and gate it on (`-o` absent or
  `--stdout`). Add a golden test for both combinations.

### F4 [P1 L6] (escalates str-qwua7.39) First-run `scan --format json` stdout is not JSON
- Fresh directory: `scan . --format json` → stdout begins `  Created  .shatter/` … `Initialized Shatter project at …`
  and then `{`. `json.load` → `Expecting value: line 1 column 3` (`scan-json.out`).
- The prior audit graded scan JSON stdout "clean" only because its directory was already initialized.
  `tests/json_stdout_contract.rs` does not test a fresh directory.
- Fix: land .39 (setup lines → stderr) as P1, and add a fresh-dir case to `json_stdout_contract.rs` for
  scan/list-targets/spec-diff/diff JSON.

### F5 [P2 L1/L4] The str-qwua7.15 fix is an argv-scanning workaround with gaps
- `main.rs:70` `maybe_print_non_executing_help` scans raw argv for `-h/--help`, guesses the subcommand path
  (`args.rs:402 resolve_subcommand_path`), and renders help from a separately mutated clap `Command`
  (`args.rs:280-420`). The commit message says the natural fix (`mut_arg(...hide)`) corrupted clap's parsing.
- Gaps: (a) `shatter help spec-diff` goes through clap's own `help` subcommand and still shows `--allow-host-writes`,
  `--set` and the `--timing*` flags (`help/help-subcommand-spec-diff.txt`, 78 vs 48 lines). (b)
  `NON_EXECUTING_COMMAND_PATHS` lists 5 commands. SPEC §2.10 names `analyze, solve, specify, stale, diff, spec-diff,
  compare, list-targets, doctor` as exempt from execution, and 9 of those still show the flags
  (`grep -l allow-host-writes help/*.txt`). (c) The execution flags still *parse* on those commands:
  `spec-diff --allow-host-writes a b` is accepted.
- The structural fix is the one the epic already proposes (str-qwua7.20.1). Move `--allow-host-writes`, `--set` and
  `--timing*` from `global = true` on `Cli` into a flattened `ExecOptions` included only by executing commands.
  That removes the second clap tree and the argv guesser.

### F6 [P2 L6] Progress reporting is fake in scan, JSON-lines-only when enabled, and absent in run
- Default `scan` prints `[info] [1/10] … (26.0s elapsed)` … `[10/10] … (26.0s elapsed)`. All ten lines appear
  **after** the scan finishes and carry the same total elapsed time (`ts-scan.err`). Code:
  `commands/scan.rs:1413-1424`, a loop over `result.function_results` after the merge.
- `scan --progress` emits raw JSON objects (`{"type":"progress","status":"started",…}`) on stderr, interleaved with
  human `[info]` lines (`scan-progress.err`). That is neither readable nor cleanly parseable.
- `run .` was silent on stderr for 74 s and printed nothing until the final report (`run.err` is 0 bytes).
- `explore` prints three lines per function (`[progress] starting`, `[progress] completed`, `[batch]`). The counters
  mean different things: `starting 2/3: compute_stats` means scheduled index 2, while `completed 1/3: main` means the
  first completion. The header says `(16 parallel worker(s))` for one target on one frontend session.
- Fix: one progress renderer shared by explore/scan/run. On a TTY, live human lines to stderr by default; `--progress
  json` for machines, JSON only. Delete the post-hoc loop. Label counters ("done 1/3").

### F7 [P2 L1] Scan artifact filenames embed the absolute source path and hit ENAMETOOLONG
- Artifact names look like `00001_tmp_claude-1000_-home-ketan-project-shatter_<uuid>_scratchpad_proj_01-arithmetic.ts__classifyNumber.json`
  (`ts-scan.err`), from `scan_artifact_path` (`shatter-core/src/scan_orchestrator.rs:585-591`).
  `sanitize_artifact_component` is applied to the qualified `/abs/path::fn`.
- A file about 230 characters deep gives `[warn] failed to write scan artifact temp file for …::f: File name too long (os error 36)`.
  The scan still exits 0 (`scan-deep.err`), and the artifact that resume and `--from-artifacts` depend on is lost.
  Deep monorepo checkouts on CI runners reach this length.
- Fix: name artifacts by project-relative path plus a short hash (for example `<rel-dir>/<fn>-<hash8>.json`).
  Treat a failed artifact write as a counted scan error, not a log warning.

### F8 [P2 L4, AGENT] `--seed` exists only on `scan`; str-0m0vn closed with its explore symptom unaddressed
- `--seed` appears only in `help/scan.txt`, not in explore or run. The str-0m0vn symptom reads "There is no way to
  make `shatter scan` or `shatter explore` reproducible". Its close reason covers scan only and files two scan
  follow-ups (str-pbqyr, str-9m9o3). No follow-up exists for explore or run.
- CLAUDE.md's parity rule ("When adding a new … CLI flag … grep for the parallel code path") was not applied to the
  explore/scan/run trio.
- Fix: add `--seed` to explore and run through the shared options struct (str-qwua7.20.1), and add explore to the
  str-0m0vn reproducibility test.
- Agent root cause: the parity rule lists `buildSymExpr`, explorer/orchestrator and `--concolic` wiring, but not
  "explore/scan/run flag sets". The closure checklist does not ask whether every surface named in the symptom was
  fixed.

### F9 [P2 L4/L6] Unknown config keys and `--set` typos are silently ignored
- `explore … --set defaults.max_iteratons=5` → exit 0, no warning. `--set foo.bar=1` is accepted too.
  A type error (`--set defaults.max_iterations=abc`) is reported, so values are validated but keys are not.
- `shatter-core/src/config.rs:4180-4182` has a test comment that pins this: "ShatterConfig derives Deserialize
  without [deny_unknown_fields], so unknown keys are silently ignored."
- Fix: warn on unknown keys (via `serde_ignored`, or `deny_unknown_fields` plus a migration list) in both
  `.shatter/config.yaml` and `--set`, with a "did you mean" suggestion. Make it an error under `--strict-config`.

### F10 [P2 L6/L2] The Rust-frontend remediation is incomplete and aimed at contributors
- The hint reads "this is the expected state after `cargo build --release --bin shatter` … install it on PATH with
  `cargo install --path shatter-rust`". That is source-checkout guidance shown to every user, and README.md:89-111
  says the same.
- After putting shatter-rust on PATH, explore runs outside the checkout fail 3/3 with
  `execute error (FileNotFound): cannot locate shatter-rust-runtime crate; set SHATTER_RUNTIME_PATH`
  (`rust-explore2.err`). `SHATTER_RUNTIME_PATH` is not in README, QUICKSTART, SPEC or any help text (only
  `build_frontend.rs:607-630` and an old plan doc).
- `doctor` reports all green in that state (`doctor.out`). It checks neither shatter-rust, nor the runtime crate,
  nor node/go, nor whether a sandbox backend or host-write opt-in is configured. Without one of those, every
  execution command refuses to run.
- The same failure produces three identical `[error]` lines and a "Failure impact" table whose Lang column says
  `any`, although the target is `.rs`.
- Fix: until str-qwua7.60 embeds the frontend, extend `doctor` (str-qwua7.40) to check the runtime crate, toolchains
  and sandbox/host-write readiness, each with a fix command. Make the missing-frontend hint two lines that point at
  `shatter doctor`. Document `SHATTER_RUNTIME_PATH` in the env-var table (str-qwua7.20.2).

### F11 [P2 L6] Error outcomes render differently per language
- TS: ``throws `Error: division by zero` `` (`ts-safedivide--concolic.out`).
- Go: ``throws `function_error: division by zero` `` (`go-explore.out`). Go does not throw, and `function_error` is
  an internal category name.
- Rust: ``returns `{"Err":"division by zero"}` `` and ``returns `{"Ok":{"avg":2.0,"flag":null,"max":2....` ``, a
  raw serde JSON envelope truncated mid-token (`rust-explore3.out`).
- Rust sections also end with a stray list item `- *Mocks: to_string*` after the table, and `main` is explored as a
  target.
- The walkthrough-review skill says the output must show "the error value in languages where errors are scalars
  (e.g., Go's `error` string, Rust's enum variant)".
- Fix: a per-language outcome formatter in core, for example `errors: division by zero` for Go/Rust `Err`,
  `returns Ok(…)` with the value pretty-printed and elided at a token boundary, and `throws Error: …` for TS.
  Add a parity golden across the three `04-errors` examples.

### F12 [P2 L5, regression] Scan's incremental behavior-map cache never hits
- The same directory scanned twice with no edits (`scan-cache-1/2`) → both runs report `0 expected skipped` and
  re-explore everything (9.4 s vs 134.9 s under load). The `shatter-report/` sample from March showed
  "36 skipped (fingerprint match)", so this used to work.
- Likely cause: scan looks up `cache.is_fresh(func_name, dfp)` / `cache.load(func_name)` with the qualified id
  (`/abs/path::fn`, per the str-fuhw comment at `scan_orchestrator.rs:4270-4276`). It stores with
  `cache.store(&behavior_map)` (`:3360`, `:4931`), which keys on `BehaviorMap.function_id`. On disk that id is the
  bare name: `.shatter-cache/behavior-maps/classifyNumber.json` has `"function_id": "classifyNumber"`.
  `store` also skips the fingerprint, and `cache.rs:127-133` says scan should use `store_with_fingerprint`.
- A side effect of bare-name keys: `01-arithmetic.ts`, `arithmetic-v1.ts` and `arithmetic-v2.ts` all write the same
  `classifyNumber.json`, so any bare-name lookup can return another file's behavior map as a mock.
- Confidence: high for the symptom, medium for the root cause (the code reading is not fully traced).
- Fix: key the cache by one qualified function id on both the store and load sides, and store with the fingerprint.
  Add a CLI test: scan twice, assert `expected_skipped == n` on the second run.

### F13 [P2 L6] The `run` report opens with a confusing "degraded" verdict; three commands use three coverage metrics
- `run .` over 6 all-supported TS files (`run.out`) prints `## Report Validity: degraded` with
  `represented_source_percent=61.5 below high threshold 75.0` **before** the `# Shatter Run Report` H1
  (`commands/run.rs:1996`). A user cannot tell what is unrepresented in a fully supported directory.
- Coverage headlines disagree: explore reports **line** coverage ("100% (7/7 lines)"), scan's headline is **branch**
  coverage ("Overall coverage … 92.5%", 37/40 branches), and run's total is **lines** again ("80/99 81%").
  run also uses a different default budget (50 vs 100 iterations), so for the same function run says 31% and scan
  says 63% (computeStats).
- Scan prints `# Scan Results` and then `# Shatter Scan Report` (prior finding, unfiled). The "Interesting Inputs"
  section lists 50 lines for `transformString` and 18 identical-outcome inputs for `categorizeUser`, which is not
  curated.
- Fix: put the H1 first and a one-line verdict with a plain-language cause. Name the metric on every headline
  ("branch coverage", "line coverage") and use the same one in all three commands. Cap "Interesting Inputs" at one
  per distinct outcome.

### F14 [P2 L5/L6, AGENT] The default markdown report omits information the deprecated `--render plain` shows
- `--render plain` (labelled "(deprecated)" in every help page) prints `Branches: 3/3 (100%)`,
  `[random: 3 (100%)]` (discovery method) and `Symbolic: 3/3 constraints (100%)` (`render-plain.out`).
  The default markdown shows only paths and line coverage (`render-md-color.out`).
- Neither mode shows what `.claude/skills/walkthrough-review/SKILL.md` §§2, 4, 6 and 7 list as what a human needs:
  per-path input constraints ("`b = 0`, `n < 0`"), arguments with parameter names, and whether exploration was
  complete or budget-limited. The data exists (`stop_reason` is in the artifact JSON, and path constraints are
  collected).
- Agent root cause: walkthrough-review is an advisory skill with a good rubric, but no gate checks the rubric,
  and the walkthrough gate checks only exit codes and panics (str-qwua7.10).
- Fix: port the plain renderer's branch/discovery lines into markdown. Add a "stopped: worklist exhausted /
  iteration budget / timeout" line, and a per-row constraint column once SymExpr pretty-printing exists. Turn the
  walkthrough-review rubric into golden assertions on one TS example.

### F15 [P1 L5/L6] (corroborates core-engine F2) Explore under-reports paths, with TS and Rust repros
- TS `04-errors.ts:safeDivide --clean --no-cache`: stderr `[batch 1/1] safeDivide: 100 iters, 3 paths, 3/3 branches`,
  stdout `**1 path(s)**`. The artifact `raw_results` holds 3 distinct branch paths (10/51/49 executions) but
  `unique_paths: 1` (`ts-safedivide.*`).
- A fresh directory with `markup.ts` gives stderr `2 paths` and stdout `**0 path(s)** · **100%** coverage` with an
  empty table (`fresh-markup.*`, artifact `unique_paths: 0`, `raw_results` 105 across 2 paths).
- Rust `safe_divide`: stderr `2 paths`, stdout `1 path(s)` (`rust-explore3.*`).
- `--concolic` on the same TS function reports 3 (`ts-safedivide--concolic.out`).
- The path count is the headline the walkthrough-review rubric names first, and it is wrong on the default explorer.
  See core-engine.md F2 for the float-probe root cause.

### F16 [P2 L4/L6] (corroborates core-engine F14) Auto-resume silently ignores changed flags
- After a default run, `explore 01-arithmetic.ts:classifyNumber --concolic --max-iterations 7` printed
  `[info] [resumed] classifyNumber: 3 branches, 65.7s (prior run)` and the old 100-iteration random result
  (`resume-flags.*`). `--render plain` was resumed the same way.
- UI angle: resume is on by default, is announced only at `[info]` level among five lines of noise, and is
  documented only inside the `--clean` help text. Fix: show `(resumed from <date>; pass --clean to re-run)` in the
  report header, and invalidate on option-hash change (core F14).

### F17 [P2 AGENT] Tracker IDs in help text are growing, and the prior audit's UI P2/P3 items were never filed
- 13 distinct `str-…` IDs across 21 help pages (`grep -o 'str-[a-z0-9.]*' help/*.txt`): `str-gg9v` ×21, `str-jeen.13`,
  `str-v01r`, `str-p2rz`, `str-frc.3/.5/.6`, `str-izhn`, `str-d6hj`, `str-1wcl`, `str-o09e`, `str-1fwt`, and
  **`str-0m0vn` added on 09-05, after the audit flagged the pattern** (`args.rs:977`).
- The 09-04 report's usability items 9 (strip tracker IDs), 12 (surface termination reason), 16 (scan double
  report / absolute paths), 17 (`Wrote … artifact -> <abs path>` at info), 18 (`--format text` does not strip) and
  20 have no tracker issue. `bd search` for "tracker id", "termination", "Scan Results", "absolute path",
  "artifact path" and "format text" returns nothing, and str-qwua7 has no children for them.
- Fix: file them now (dedupe against str-9ee5 for flags). Add a unit test that renders every subcommand's help and
  fails on `str-[a-z0-9]+`. Add a line to rust-conventions: "No tracker IDs in `///` on clap args; they become
  user-facing help."
- Agent root cause: /audit Phase 10 filed only the P1 and selected P2 items (str-qwua7.22 is about audit filing),
  and nothing lints help text.

### F18 [P3 L1] Telemetry's subcommand allow-list is stale
- `shatter-core/src/telemetry.rs:53-65` `KNOWN_SUBCOMMANDS` = explore, scan, **export**, **spec**, run, analyze, init,
  stale, telemetry, help, **version**. Three of those do not exist, and 15 real ones (spec-diff, diff, doctor,
  list-targets, observe, …) are missing, so their names are redacted from `command_run` events.
  Telemetry is on by default (`telemetry.rs:209`, `telemetry status` → enabled).
- Fix: derive the list from `Cli::command().get_subcommands()` and add a test that binds the two.

### F19 [P3 L6] Error-message polish
- `explore arithmetic-v1.ts:doesNotExist` → `[error] Analyze error (FunctionNotFound): …` and then
  `Error: explore: all 1 attempted target(s) failed (build_failed=0, runtime_failed=0, timed_out=0)`. The breakdown
  is all zeros because analyze failures are not a category, and there is no list of available functions or
  "did you mean".
- `--dry-run` help says "Requires --output", but `explore … --dry-run` without `-o` works (exit 0, prints
  `No existing spec … all 1 function(s) are stale.`) (`args.rs:676`).
- The explore target help says "(.ts = TypeScript, .go = Go)" and omits `.rs` (`args.rs:501`).
- `--analyze-only` prints un-headed plain text (`classifyNumber (arithmetic-v1.ts:11)\n params: 1, branches: 3`),
  not markdown.

### F20 [P2 AGENT] Parallel reviewer subagents share one scratchpad directory
- The workflow gave every reviewer the same scratchpad
  (`/tmp/claude-1000/-home-ketan-project-shatter/<session>/scratchpad`). At 12:39 another agent replaced my
  `scratchpad/proj` with its own git fixture, deleting my TS example copies mid-run. I had run `rm -rf scratchpad/proj`
  at 12:15, which may have removed another reviewer's directory. I switched to `scratchpad/cliux/`.
- Fix (workflow script / bento swarm prompts): give each subagent `scratchpad/<area>/` and say "only write inside
  it"; never suggest generic names like `proj/`.

## 2. Positives worth preserving
- The host-write refusal (`ts-firstrun.err`) is an exemplary error: it states the risk, lists ranked remedies with
  copy-pasteable commands, and exits 2.
- Exit codes now follow SPEC §2.11 across every error class I tried. `error_exit_code` uses a typed `GateFailure`
  downcast (`main.rs:1475`).
- `list-targets` text and JSON output are clean, relative-path, and well structured. It is the model for other
  commands.
- `spec-diff --json` is valid JSON, and its exits of 0/1/2 are correct.
- No ANSI leaks when piped or under `NO_COLOR`. `--color always` renders good termimad tables.
- The explore markdown table (`# | Call | Outcome`) is scannable in a few lines when the path count is right.
- The HTML report has a coherent inline stylesheet (system font stack, cards, coverage bars). It lacks dark
  mode and `@media` rules (prior finding).

## 3. Grades
| Level | Grade | Note |
|---|---|---|
| L1 (CLI code) | C | the `-o json` and `--format` no-ops, the argv-scanning help hack, ENAMETOOLONG, and a dead scan cache; `explore.rs` 11.3k lines and `main.rs` dispatch repeats `finalize_exit_code` 23× (tracked in qwua7.6) |
| L4 (CLI design) | C- | 79/70/30 flags; `--render` vs `--format`; seed/progress/coverage metric differ across explore/scan/run; resume ignores options |
| L6 (terminal UI) | C | explore output is good when correct, but the headline path count is wrong, progress is fake, per-language rendering is inconsistent, JSON stdout is contaminated on first run, and absolute paths are everywhere |
