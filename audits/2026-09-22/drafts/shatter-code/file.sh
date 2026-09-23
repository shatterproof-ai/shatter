#!/usr/bin/env bash
echo "SUPERSEDED by ../../issues/INDEX.md — do not run file.sh here; use ../../issues/file-all.sh" >&2
exit 1
# Files the audit 2026-09-22 shatter code/design/goals drafts into the shatter beads tracker.
# NOT executed by the drafting agent. Review drafts (INDEX.md) and run a readiness pass first.
# Usage: bash file.sh            (from anywhere; cds into the shatter repo)
#        DRY_RUN=1 bash file.sh  (print commands only)
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="${SHATTER_REPO:-/home/ketan/project/shatter}"
cd "$REPO"
body() { local f="$1" out; out="$(mktemp)"; sed "1,/^<!-- body -->$/d" "$HERE/$f" > "$out"; echo "$out"; }
run() { if [ -n "${DRY_RUN:-}" ]; then echo "+ $*" >&2; echo "DRY-$RANDOM"; else "$@"; fi; }
declare -A ID

# Epic
EPIC=$(run bd create --silent --title 'Epic: Audit 2026-09-22 findings — code, design and goals (shatter)' --type epic --priority P1 --labels audit,epic --body-file "$(body 00-epic.md)")
echo "epic: $EPIC"

ID[01]=$(run bd create --silent --title 'Meta test'\''s `task --list-all --json` writes checksums for 39 tasks, so `task check` and CI skip every stage-2/3 test' --type bug --priority P1 --labels quality-gates,ci,taskfile,audit --parent "$EPIC" --body-file "$(body 01-task-list-json-poisons-checksums.md)")
echo "01: ${ID[01]}"
ID[02]=$(run bd create --silent --title 'CI '\''Full landing gate'\'' has run no product tests since 2026-08-29: add an executed-leaf guard and triage what fails once it runs' --type bug --priority P1 --labels ci,quality-gates,audit --parent "$EPIC" --body-file "$(body 02-ci-hollow-since-0829-guard-and-triage.md)")
echo "02: ${ID[02]}"
ID[03]=$(run bd create --silent --title 'Task `sources:` omit real test inputs (CLI tests/templates/build.rs, embedded frontends, frontend sources for E2E), so gates skip after relevant edits' --type bug --priority P1 --labels quality-gates,taskfile,audit --parent "$EPIC" --body-file "$(body 03-task-sources-omit-inputs.md)")
echo "03: ${ID[03]}"
ID[04]=$(run bd create --silent --title '`task affected` misroutes: .md (incl. askama templates and frontend CLAUDE.md) goes only to the hollow `docs` gate, docs-smoke is never selected, core/frontend changes skip cli:test, shatter-llm falls through to a `check` that doesn'\''t test it' --type bug --priority P2 --labels quality-gates,taskfile,audit --parent "$EPIC" --body-file "$(body 04-affected-gates-routing-gaps.md)")
echo "04: ${ID[04]}"
ID[05]=$(run bd create --silent --title 'Go config loader walks up to `/` (stray /tmp/.shatter breaks tests; ancestor configs can widen policy.allow) and warns on the `defaults` key `shatter init` generates' --type bug --priority P2 --labels go,config,tests,audit --parent "$EPIC" --body-file "$(body 05-go-config-discovery-unbounded.md)")
echo "05: ${ID[05]}"
ID[06]=$(run bd create --silent --title 'shatter-ts handlers.test.ts analyze tests exceed the 30 s jest timeout under machine load' --type bug --priority P3 --labels typescript,tests,flake,audit --parent "$EPIC" --body-file "$(body 06-ts-handlers-test-timeouts-under-load.md)")
echo "06: ${ID[06]}"
ID[07]=$(run bd create --silent --title 'CI runs plain `cargo test` (nextest `[profile.ci]` is dead config) and parity-governed keeps a stale '\''pending str-7jgm.2'\'' fallback' --type chore --priority P3 --labels ci,quality-gates,audit --parent "$EPIC" --body-file "$(body 07-nextest-ci-profile-and-stale-parity-fallback.md)")
echo "07: ${ID[07]}"
run bd comments add str-qwua7.17 -f "$(body 08-drift-patrol-tracker-items-note.md)" >/dev/null; echo "note 08 -> str-qwua7.17"
ID[09]=$(run bd create --silent --title 'Conformance harness: timeouts cascade into misattributed failures, known_drifts can never match, summary arithmetic is wrong, 8/18 cases have no cross-frontend check' --type bug --priority P2 --labels conformance,protocol,parity,audit --parent "$EPIC" --body-file "$(body 09-conformance-harness-correctness.md)")
echo "09: ${ID[09]}"
ID[10]=$(run bd create --silent --title 'Gate telemetry can'\''t tell executed from cached runs; sccache is installed but unused; machine-wide slot covers only shatter gates' --type task --priority P2 --labels quality-gates,performance,sccache,resource-governance,audit --parent "$EPIC" --body-file "$(body 10-gate-telemetry-executed-vs-cached-and-shared-cache.md)")
echo "10: ${ID[10]}"
ID[11]=$(run bd create --silent --title 'Z3 translation declares separate Int and Real constants for one numeric param; solver returns wrong SAT models (0.5<x<1 → x=1.5)' --type bug --priority P1 --labels solver,z3,concolic,audit --parent "$EPIC" --body-file "$(body 11-z3-int-real-sort-split.md)")
echo "11: ${ID[11]}"
ID[12]=$(run bd create --silent --title 'Explore report under-reports discovered paths (report '\''0 path(s)'\'' / '\''1 path(s)'\'' while the batch line and spec show 2-4)' --type bug --priority P1 --labels explorer,report,audit --parent "$EPIC" --body-file "$(body 12-random-explorer-path-undercount.md)")
echo "12: ${ID[12]}"
ID[13]=$(run bd create --silent --title '--setup is silently ignored in concolic mode (all production callers pass setup_context=None), and the random explorer shrinks after teardown' --type bug --priority P1 --labels concolic,setup,orchestrator,parity,audit --parent "$EPIC" --body-file "$(body 13-concolic-setup-teardown-ownership.md)")
echo "13: ${ID[13]}"
ID[14]=$(run bd create --silent --title 'Concolic dynamic mock variation regressed (str-3ky9.4 undone by str-lebv/str-r59s); hidden behind `_`-prefixed params' --type bug --priority P2 --labels concolic,mocking,regression,audit --parent "$EPIC" --body-file "$(body 14-concolic-mock-variation-regression.md)")
echo "14: ${ID[14]}"
ID[15]=$(run bd create --silent --title 'Two value shrinkers: str-55ep/str-ddxe/tuple fixes landed in the dead input_gen copy; live shrink.rs lacks them' --type bug --priority P2 --labels shrinking,shatter-core,refactor,audit --parent "$EPIC" --body-file "$(body 15-duplicate-value-shrinkers.md)")
echo "15: ${ID[15]}"
ID[16]=$(run bd create --silent --title 'Concolic refine phase drops prepare_id/execution_profile and discards the paths it reaches; no shared Execute builder' --type bug --priority P2 --labels concolic,orchestrator,refactor,audit --parent "$EPIC" --body-file "$(body 16-concolic-refine-phase-execute-builder.md)")
echo "16: ${ID[16]}"
ID[17]=$(run bd create --silent --title 'Engines disagree on what a '\''path'\'' is, --max-iterations means different budgets per entry point, and three hand-built orchestrator::ExploreConfig literals diverge' --type task --priority P2 --labels architecture,parity,explorer,orchestrator,audit --parent "$EPIC" --body-file "$(body 17-engine-path-identity-budget-config.md)")
echo "17: ${ID[17]}"
ID[18]=$(run bd create --silent --title '~5,100 lines of production-dead code in shatter-core (recursive, array_mutation, reporter+clustering, sequential scan(), shrink_witness, mutate_mock_values); add a reachability check' --type chore --priority P2 --labels cleanup,shatter-core,tech-debt,audit --parent "$EPIC" --body-file "$(body 18-core-dead-code-removal.md)")
echo "18: ${ID[18]}"
run bd comments add str-qwua7.49 -f "$(body 19-qwua7-49-rescope-note.md)" >/dev/null; echo "note 19 -> str-qwua7.49"
ID[20]=$(run bd create --silent --title 'Invariant inference over-generalises: no minimum support at function level, confidence hard-coded 1.0, only 0-anchored templates' --type bug --priority P2 --labels spec,properties,audit --parent "$EPIC" --body-file "$(body 20-invariant-min-support-templates.md)")
echo "20: ${ID[20]}"
ID[21]=$(run bd create --silent --title 'Z3 has no default per-query timeout (solver can outlive timeout_explore), and `scan --solver-timeout` is silently discarded' --type bug --priority P2 --labels solver,z3,timeout,audit --parent "$EPIC" --body-file "$(body 21-z3-default-timeout.md)")
echo "21: ${ID[21]}"
ID[22]=$(run bd create --silent --title 'Explore auto-resume is keyed only on source fingerprint: --concolic / budget / seed changes silently return stale results labelled with the new mode' --type bug --priority P1 --labels explore,resume,concolic,audit --parent "$EPIC" --body-file "$(body 22-explore-resume-options-key.md)")
echo "22: ${ID[22]}"
run bd comments add str-qwua7.6 -f "$(body 23-explore-with-oracle-size-note.md)" >/dev/null; echo "note 23 -> str-qwua7.6"
ID[24]=$(run bd create --silent --title 'Float constants converted to Z3 via (v*1e6).round() as i64: saturate above ~9.2e12 and become 0 below 5e-7' --type bug --priority P3 --labels solver,z3,audit --parent "$EPIC" --body-file "$(body 24-float-constant-rational-conversion.md)")
echo "24: ${ID[24]}"
ID[25]=$(run bd create --silent --title 'std DefaultHasher used for persisted/reproducible keys (core_sample --seed selection, Rust harness cache dirs)' --type bug --priority P3 --labels rust,cache,audit --parent "$EPIC" --body-file "$(body 25-stable-hash-for-persisted-keys.md)")
echo "25: ${ID[25]}"
ID[26]=$(run bd create --silent --title '`explore -o out.json` / `--spec-out` write an empty no_targets bundle after a successful run, and keep only the first file'\''s bundle for multi-file/glob targets' --type bug --priority P1 --labels cli,explore,artifacts,spec,audit --parent "$EPIC" --body-file "$(body 26-explore-json-output-bundles.md)")
echo "26: ${ID[26]}"
ID[27]=$(run bd create --silent --title '`explore --format text|html` has no effect on stdout (printer branches on --render); --render and --format overlap' --type bug --priority P2 --labels cli,explore,report,audit --parent "$EPIC" --body-file "$(body 27-explore-format-flag-ignored.md)")
echo "27: ${ID[27]}"
ID[28]=$(run bd create --silent --title 'str-qwua7.15 fix is an argv-scanning workaround: `shatter help <cmd>` and 9 non-executing commands still show execution-only globals' --type bug --priority P2 --labels cli,usability,audit --parent "$EPIC" --body-file "$(body 28-help-only-argv-intercept.md)")
echo "28: ${ID[28]}"
ID[29]=$(run bd create --silent --title 'Scan artifact filenames embed the mangled absolute source path; deep checkouts hit ENAMETOOLONG and the scan still exits 0' --type bug --priority P2 --labels scan,artifacts,audit --parent "$EPIC" --body-file "$(body 29-scan-artifact-filenames-abs-path.md)")
echo "29: ${ID[29]}"
ID[30]=$(run bd create --silent --title '`--seed` exists only on scan; str-0m0vn closed although its symptom named explore' --type feature --priority P2 --labels cli,seeds,audit --parent "$EPIC" --body-file "$(body 30-seed-for-explore-and-run.md)")
echo "30: ${ID[30]}"
ID[31]=$(run bd create --silent --title 'Unknown config keys and `--set` key typos are silently ignored' --type feature --priority P2 --labels config,cli,usability,audit --parent "$EPIC" --body-file "$(body 31-unknown-config-keys-warn.md)")
echo "31: ${ID[31]}"
ID[32]=$(run bd create --silent --title 'Behavior-map cache keyed by bare function name: scan never hits on unchanged re-scan, and same-named functions in different files overwrite each other' --type bug --priority P2 --labels cache,behavior-map,scan,audit --parent "$EPIC" --body-file "$(body 32-behavior-map-cache-keys.md)")
echo "32: ${ID[32]}"
ID[33]=$(run bd create --silent --title 'Telemetry KNOWN_SUBCOMMANDS is stale (3 nonexistent, ~15 real commands missing and redacted)' --type bug --priority P3 --labels telemetry,cli,audit --parent "$EPIC" --body-file "$(body 33-telemetry-known-subcommands.md)")
echo "33: ${ID[33]}"
ID[34]=$(run bd create --silent --title 'Mixed-language scan: the second language sub-scan deletes the first'\''s function artifacts and overwrites summary.json; checkpoint lives elsewhere' --type bug --priority P1 --labels scan,artifacts,resume,audit --parent "$EPIC" --body-file "$(body 34-mixed-language-scan-clobbers-artifacts.md)")
echo "34: ${ID[34]}"
run bd comments add str-qwua7.38 -f "$(body 35-spec-diff-false-negative-note.md)" >/dev/null; echo "note 35 -> str-qwua7.38"
ID[36]=$(run bd create --silent --title 'Spec preconditions are sample statistics (often false or vacuous); use the symbolic path constraints already recorded' --type feature --priority P2 --labels spec,spec-diff,audit --parent "$EPIC" --body-file "$(body 36-spec-preconditions-from-path-constraints.md)")
echo "36: ${ID[36]}"
ID[37]=$(run bd create --silent --title 'Three incompatible spec JSON shapes; `compare` rejects the --spec-out bundle (the only clean producer)' --type bug --priority P2 --labels spec,cli,artifacts,audit --parent "$EPIC" --body-file "$(body 37-spec-json-shapes-compare.md)")
echo "37: ${ID[37]}"
ID[38]=$(run bd create --silent --title '`shatter diff` has no producer: no command writes a Snapshot, and diff rejects every JSON Shatter emits' --type bug --priority P1 --labels diff,cli,regression,audit --parent "$EPIC" --body-file "$(body 38-snapshot-producer-for-diff.md)")
echo "38: ${ID[38]}"
ID[39]=$(run bd create --silent --title '`revalidate` ignores return values: changed outputs on replayed inputs are reported as confirmed and exit 0' --type bug --priority P1 --labels regression,cli,behavior-map,audit --parent "$EPIC" --body-file "$(body 39-revalidate-ignores-return-values.md)")
echo "39: ${ID[39]}"
ID[40]=$(run bd create --silent --title 'TS instrumentor data-flow map ignores program point: recorded path constraints contradict the concrete execution' --type bug --priority P1 --labels typescript,instrumentation,concolic,audit --parent "$EPIC" --body-file "$(body 40-ts-flow-map-program-point.md)")
echo "40: ${ID[40]}"
ID[41]=$(run bd create --silent --title 'TS switch/ternary/value-position &&,|| are analyzed but never instrumented; analyze and instrument branch IDs desync and coverage is misattributed' --type bug --priority P1 --labels typescript,instrumentation,coverage,audit --parent "$EPIC" --body-file "$(body 41-ts-switch-ternary-instrumentation.md)")
echo "41: ${ID[41]}"
ID[42]=$(run bd create --silent --title 'TS analyzer resolves shadowed callback parameters to the outer function parameter' --type bug --priority P2 --labels typescript,analyze,audit --parent "$EPIC" --body-file "$(body 42-ts-shadowed-callback-params.md)")
echo "42: ${ID[42]}"
ID[43]=$(run bd create --silent --title 'TS frontend classifies any target error whose message mentions '\''timeout'\'' as timed_out/infrastructure' --type bug --priority P2 --labels typescript,error-handling,audit --parent "$EPIC" --body-file "$(body 43-ts-timeout-classification.md)")
echo "43: ${ID[43]}"
ID[44]=$(run bd create --silent --title 'TS parseRequest validates only the envelope: missing fields crash as internal_error; validCommands hand-maintained' --type bug --priority P2 --labels typescript,protocol,validation,audit --parent "$EPIC" --body-file "$(body 44-ts-request-validation.md)")
echo "44: ${ID[44]}"
run bd comments add str-rf2v -f "$(body 45-ts-fourth-flow-walker-note.md)" >/dev/null; echo "note 45 -> str-rf2v"
ID[46]=$(run bd create --silent --title 'TS protocol round-trip tests are tautological and the builder-parity property test cannot detect drift' --type task --priority P2 --labels typescript,testing,protocol,parity,audit --parent "$EPIC" --body-file "$(body 46-ts-protocol-and-parity-tests-meaningful.md)")
echo "46: ${ID[46]}"
ID[47]=$(run bd create --silent --title 'TS preflight fails dependency-free projects (requires node_modules whenever project_root is set) and one failure blocks every later request' --type bug --priority P3 --labels typescript,install,audit --parent "$EPIC" --body-file "$(body 47-ts-preflight-node-modules.md)")
echo "47: ${ID[47]}"
ID[48]=$(run bd create --silent --title 'TS SymExpr builders collapse common operators to unknown (??, **, shifts, element access, as/!/satisfies, template literals)' --type feature --priority P3 --labels typescript,instrumentation,solver,audit --parent "$EPIC" --body-file "$(body 48-ts-operators-collapse-to-unknown.md)")
echo "48: ${ID[48]}"
ID[49]=$(run bd create --silent --title 'shatter-ts hygiene: async timeout timer leak hidden by jest forceExit, stdin EOF drops in-flight responses, dual lockfiles, tests emitted to dist, js-yaml v3' --type chore --priority P3 --labels typescript,cleanup,audit --parent "$EPIC" --body-file "$(body 49-ts-lifecycle-and-packaging-hygiene.md)")
echo "49: ${ID[49]}"
ID[50]=$(run bd create --silent --title 'Go frontend finds its harness runtime module via the compile-time source path: installed/relocated binaries fail every uncached execute' --type bug --priority P1 --labels go,distribution,install,audit --parent "$EPIC" --body-file "$(body 50-go-harness-runtime-compile-path.md)")
echo "50: ${ID[50]}"
ID[51]=$(run bd create --silent --title 'Documented `go get -tool github.com/shatterproof-ai/shatter/go-tool/...` cannot resolve: module path says go-tool/ but the directory is shatter-go-tool/' --type bug --priority P1 --labels go,distribution,install,docs,audit --parent "$EPIC" --body-file "$(body 51-go-tool-module-path.md)")
echo "51: ${ID[51]}"
ID[52]=$(run bd create --silent --title 'Go rune/char literals are typed as string constants in both SymExpr builders; analyzer strings.Trim mangles escapes and quotes' --type bug --priority P1 --labels go,solver,concolic,audit --parent "$EPIC" --body-file "$(body 52-go-rune-and-escape-literals.md)")
echo "52: ${ID[52]}"
run bd comments add str-qwua7.35 -f "$(body 53-go-flow-builders-note.md)" >/dev/null; echo "note 53 -> str-qwua7.35"
ID[54]=$(run bd create --silent --title '~55 unreachable Go functions incl. the whole reconstruct package; re-scope str-qwua7.48 property tests to live generators' --type chore --priority P2 --labels go,cleanup,testing,audit --parent "$EPIC" --body-file "$(body 54-go-dead-code-and-property-targets.md)")
echo "54: ${ID[54]}"
ID[55]=$(run bd create --silent --title 'shatter-go-tool: non-atomic extract can cache a truncated binary; no HTTP timeouts; GitHub API call on every run; tests cover only arg parsing' --type bug --priority P2 --labels go,distribution,installer,audit --parent "$EPIC" --body-file "$(body 55-go-tool-wrapper-robustness.md)")
echo "55: ${ID[55]}"
ID[56]=$(run bd create --silent --title 'Go frontend small fixes: line-0 records on every execution, generated mock code ignores Unmarshal errors and uses backtick raw strings, dead assignments, unchecked lock-PID writes' --type bug --priority P3 --labels go,cleanup,audit --parent "$EPIC" --body-file "$(body 56-go-small-correctness-tidy.md)")
echo "56: ${ID[56]}"
ID[57]=$(run bd create --silent --title 'Go lint is red on main (10 issues) and no gate runs golangci-lint or gofmt; str-2tyfk and str-qwua7.32 closed with residuals' --type bug --priority P2 --labels go,quality-gates,audit --parent "$EPIC" --body-file "$(body 57-go-lint-red-and-ungated.md)")
echo "57: ${ID[57]}"
ID[58]=$(run bd create --silent --title 'Rust crate-bridge harness shares stdout with user code: any function that prints fails as internal_error' --type bug --priority P1 --labels rust-frontend,crate-bridge,harness,audit --parent "$EPIC" --body-file "$(body 58-rust-crate-bridge-stdout.md)")
echo "58: ${ID[58]}"
ID[59]=$(run bd create --silent --title 'Rust instrumentation emits invalid or stringly-typed constraints: hand-rolled JSON breaks on `1.` floats and control chars, and match arms become string equality on pattern text' --type bug --priority P2 --labels rust-frontend,instrumentation,solver,audit --parent "$EPIC" --body-file "$(body 59-rust-instrument-constraints.md)")
echo "59: ${ID[59]}"
ID[60]=$(run bd create --silent --title 'Request timeout (30 s) is not longer than the build timeout (30/120 s): cold Rust builds surface as a generic request timeout' --type bug --priority P2 --labels timeout,rust-frontend,cli,audit --parent "$EPIC" --body-file "$(body 60-timeout-budget-invariant.md)")
echo "60: ${ID[60]}"
run bd comments add str-qwua7.50 -f "$(body 61-rust-panic-boundary-note.md)" >/dev/null; echo "note 61 -> str-qwua7.50"
ID[62]=$(run bd create --silent --title 'shatter-rust main.rs re-declares the module tree: all 587 inline unit tests compile and run twice' --type chore --priority P2 --labels rust-frontend,tests,performance,audit --parent "$EPIC" --body-file "$(body 62-rust-main-duplicate-module-tree.md)")
echo "62: ${ID[62]}"
ID[63]=$(run bd create --silent --title '38 shatter-rust tests print '\''skipping'\'' and pass when cargo cannot reach the network' --type bug --priority P2 --labels rust-frontend,tests,quality-gates,audit --parent "$EPIC" --body-file "$(body 63-rust-tests-offline-silent-pass.md)")
echo "63: ${ID[63]}"
ID[64]=$(run bd create --silent --title 'shatter-rust: crate type registry rebuilt on every analyze (O(N²) parses) and two independent Axum extractor classifiers' --type refactor --priority P3 --labels rust-frontend,performance,axum,audit --parent "$EPIC" --body-file "$(body 64-rust-frontend-design-dedupe.md)")
echo "64: ${ID[64]}"
ID[65]=$(run bd create --silent --title 'shatter-rust-runtime harness loop swallows malformed requests; branches on user-spawned threads are silently lost' --type bug --priority P3 --labels rust-frontend,runtime,audit --parent "$EPIC" --body-file "$(body 65-rust-runtime-harness-loop.md)")
echo "65: ${ID[65]}"
ID[66]=$(run bd create --silent --title 'shatter-llm hardening: Jev API key reachable via derived Debug; parser ignores int width and first-bracket only; uncapped backoff; no PBT' --type bug --priority P3 --labels llm,security,audit --parent "$EPIC" --body-file "$(body 66-shatter-llm-hardening.md)")
echo "66: ${ID[66]}"
ID[67]=$(run bd create --silent --title 'Parity gates never check dispatch: a command advertised but not dispatched passes every static gate' --type task --priority P2 --labels parity,protocol,quality-gates,audit --parent "$EPIC" --body-file "$(body 67-parity-dispatch-reconciliation.md)")
echo "67: ${ID[67]}"
ID[68]=$(run bd create --silent --title 'Protocol capability facts are hand-replicated in six places, four parity-matrix sections are validated by nothing, and codegen covers 6 of 13 registry enums' --type task --priority P2 --labels parity,protocol,architecture,audit --parent "$EPIC" --body-file "$(body 68-capability-single-source.md)")
echo "68: ${ID[68]}"
ID[69]=$(run bd create --silent --title 'validate-protocol-registry prints a permanent get_invocation_plan '\''may be unimplemented'\'' warning although the matrix marks it optional' --type chore --priority P3 --labels protocol,parity,audit --parent "$EPIC" --body-file "$(body 69-validator-optional-command-warning.md)")
echo "69: ${ID[69]}"
ID[70]=$(run bd create --silent --title 'Build and Release workflow has 0 successes in 200 runs (Windows z3.h, aarch64 openssl-sys); no release exists so install.sh/action.yml cannot install' --type bug --priority P1 --labels release,ci,distribution,audit --parent "$EPIC" --body-file "$(body 70-release-workflow-never-green.md)")
echo "70: ${ID[70]}"
ID[71]=$(run bd create --silent --title 'Snapshot tests self-create missing snapshots and pass; four copy-pasted helpers; whitespace-collapsed comparisons; no CLI output snapshots' --type task --priority P2 --labels testing,report,audit --parent "$EPIC" --body-file "$(body 71-snapshot-test-helpers.md)")
echo "71: ${ID[71]}"
ID[72]=$(run bd create --silent --title 'Test inputs come from an unpinned external examples repo (origin/main, refreshed every 10 min)' --type task --priority P2 --labels testing,examples,audit --parent "$EPIC" --body-file "$(body 72-pin-examples-repo.md)")
echo "72: ${ID[72]}"
ID[73]=$(run bd create --silent --title 'Smoke, walkthrough, gauntlet and E2E user paths never run in CI; the only CI gauntlet path (Perf CI) has 0/13 successes' --type task --priority P2 --labels ci,smoke,walkthrough,gauntlet,audit --parent "$EPIC" --body-file "$(body 73-ci-runs-user-paths.md)")
echo "73: ${ID[73]}"
ID[74]=$(run bd create --silent --title 'Collapse ~20 overlapping gate tiers; stop duplicating task bodies for checksum identity; e2e runs twice in pre-completion-e2e' --type refactor --priority P3 --labels quality-gates,taskfile,audit --parent "$EPIC" --body-file "$(body 74-collapse-test-tiers.md)")
echo "74: ${ID[74]}"
ID[75]=$(run bd create --silent --title 'str-qwua7.4 purge incomplete: a March rapid failfile is still tracked and .gitignore covers only planner/' --type chore --priority P3 --labels go,testing,cleanup,audit --parent "$EPIC" --body-file "$(body 75-rapid-failfile-purge.md)")
echo "75: ${ID[75]}"
ID[76]=$(run bd create --silent --title 'Two duplicate broad-run validation gates, neither scheduled or in check/affected/CI' --type chore --priority P3 --labels quality-gates,cleanup,audit --parent "$EPIC" --body-file "$(body 76-broad-run-gate-duplicates.md)")
echo "76: ${ID[76]}"
ID[77]=$(run bd create --silent --title 'Workflows use deprecated Node-20 action majors, unpinned ubuntu-latest (Ubuntu 26 from 2026-10-19), and setup-go cache misses go.sum' --type chore --priority P3 --labels ci,github-actions,maintenance,audit --parent "$EPIC" --body-file "$(body 77-workflow-action-versions.md)")
echo "77: ${ID[77]}"
ID[78]=$(run bd create --silent --title 'Line-coverage metric inconsistent across languages: Go scan inflates to 100% with 7/18 branches; Rust reports ~54% for fully covered functions' --type bug --priority P1 --labels coverage,go,rust-frontend,parity,audit --parent "$EPIC" --body-file "$(body 78-line-coverage-metric-consistency.md)")
echo "78: ${ID[78]}"
ID[79]=$(run bd create --silent --title 'Turn EXPECTED BRANCHES comments into a known-answer ratchet gate; link gauntlet allowlist entries to issues; preserve TS discriminant literals' --type task --priority P2 --labels testing,gauntlet,typescript,examples,audit --parent "$EPIC" --body-file "$(body 79-known-answer-ratchet-and-ts-discriminants.md)")
echo "79: ${ID[79]}"
ID[80]=$(run bd create --silent --title 'Concolic explorer does not beat the default explorer on the project'\''s hard examples; add a benchmark and investigate early worklist termination' --type task --priority P2 --labels concolic,benchmark,audit --parent "$EPIC" --body-file "$(body 80-concolic-vs-default-benchmark.md)")
echo "80: ${ID[80]}"
ID[81]=$(run bd create --silent --title 'Go explore lacks the cold-build warmup gate scan has; at default parallelism under load Go explores time out en masse' --type bug --priority P3 --labels go,explore,parity,performance,audit --parent "$EPIC" --body-file "$(body 81-go-explore-warmup-gate.md)")
echo "81: ${ID[81]}"
ID[82]=$(run bd create --silent --title 'Rust frontend still generates negative integers for usize params (str-ddxe regression?); deserialization failures appear as behaviors' --type bug --priority P2 --labels rust-frontend,input-generation,regression,audit --parent "$EPIC" --body-file "$(body 82-rust-usize-negative-inputs.md)")
echo "82: ${ID[82]}"
ID[83]=$(run bd create --silent --title 'Pre-commit hook runs the full shatter-core/shatter-cli test suite (incl. E2E) on every commit: ~60 s median, fragile, main driver of --no-verify' --type task --priority P1 --labels git-hooks,quality-gates,audit --parent "$EPIC" --body-file "$(body 83-precommit-hook-fast-hermetic.md)")
echo "83: ${ID[83]}"
ID[84]=$(run bd create --silent --title 'Shatter tests leak per-run directories into shared /tmp (hundreds of crate-bridge harness dirs, GBs) — widen filesystem isolation' --type bug --priority P2 --labels tests,tempdir,rust-frontend,audit --parent "$EPIC" --body-file "$(body 84-tests-leak-tmp-dirs.md)")
echo "84: ${ID[84]}"

# Dependencies (blocked-by direction: first arg is blocked by second)
run bd dep add "${ID[02]}" --blocked-by "${ID[01]}" >/dev/null
run bd dep add "${ID[07]}" --blocked-by "${ID[01]}" >/dev/null
run bd dep add "${ID[10]}" --blocked-by "${ID[01]}" >/dev/null
run bd dep add "${ID[39]}" --blocked-by "${ID[12]}" >/dev/null
run bd dep add "${ID[55]}" --blocked-by "${ID[51]}" >/dev/null
run bd dep add "${ID[73]}" --blocked-by "${ID[01]}" >/dev/null
run bd dep add "${ID[74]}" --blocked-by "${ID[01]}" >/dev/null
run bd dep add "${ID[74]}" --blocked-by "${ID[03]}" >/dev/null
run bd dep add "${ID[80]}" --blocked-by "${ID[22]}" >/dev/null

# Related-issue links (existing ids) are cited in each body; add bd links manually if desired.
echo "done. Run: bd export -o .beads/issues.jsonl (repo policy) and commit via the landing flow."
