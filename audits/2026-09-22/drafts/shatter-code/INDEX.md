# Shatter code/design/goals drafts — audit 2026-09-22

Selection: target_repo=shatter, level in L1/L4/L5, verify verdict not refuted, dedupe relation new /
duplicate-closed-but-unfixed / partially-covered. Findings with relation `related` were treated as new
(no existing issue covers them). Tightly related findings are grouped; duplicates across areas merged.

**Readiness review mode: local-fallback.** Drafts follow the bento issue-readiness-check structure
(problem, current code facts, acceptance, approach, scope, priority/type/labels/deps). No subagent/Task
tool was available in this workflow subagent, so no fresh-reviewer pass was run. Before running `file.sh`,
run a fresh-reviewer readiness pass (at least on P1 drafts) per the skill. Drafts 38 and 74 contain an
explicit maintainer decision.

Files: `NN-slug.md` (header table + `<!-- body -->` + issue body), `file.sh` (filer; not executed).

| # | Title | Pri | Type | Action | Sources |
|---|---|---|---|---|---|
| 00 | Epic: Audit 2026-09-22 findings — code, design and goals (shatter) | P1 | epic | new epic | — |
| 01 | Meta test's `task --list-all --json` writes checksums for 39 tasks, so `task check` and CI skip every stage-2/3 test | P1 | bug | new child | gates-01, tests-ci-01 |
| 02 | CI 'Full landing gate' has run no product tests since 2026-08-29: add an executed-leaf guard and triage what fails once it runs | P1 | bug | new child | gates-02 |
| 03 | Task `sources:` omit real test inputs (CLI tests/templates/build.rs, embedded frontends, frontend sources for E2E), so gates skip after relevant edits | P1 | bug | new child | tests-ci-04, frontend-rust-07 (sources part) |
| 04 | `task affected` misroutes: .md (incl. askama templates and frontend CLAUDE.md) goes only to the hollow `docs` gate, docs-smoke is never selected, core/frontend changes skip cli:test, shatter-llm falls through to a `check` that doesn't test it | P2 | bug | new child | gates-06, tests-ci-05, frontend-rust-07 (selector part) |
| 05 | Go config loader walks up to `/` (stray /tmp/.shatter breaks tests; ancestor configs can widen policy.allow) and warns on the `defaults` key `shatter init` generates | P2 | bug | new child | gates-03, frontend-go-07 |
| 06 | shatter-ts handlers.test.ts analyze tests exceed the 30 s jest timeout under machine load | P3 | bug | new child | gates-05 |
| 07 | CI runs plain `cargo test` (nextest `[profile.ci]` is dead config) and parity-governed keeps a stale 'pending str-7jgm.2' fallback | P3 | chore | new child | gates-08, tests-ci-07 |
| 08 | NOTE on str-qwua7.17: drift-patrol tracker-hygiene is red again (2 stale claims, 4 orphans) | P3 | chore | note → `str-qwua7.17` | gates-09 |
| 09 | Conformance harness: timeouts cascade into misattributed failures, known_drifts can never match, summary arithmetic is wrong, 8/18 cases have no cross-frontend check | P2 | bug | new child | prior-05, protocol-parity-02, gates-10 |
| 10 | Gate telemetry can't tell executed from cached runs; sccache is installed but unused; machine-wide slot covers only shatter gates | P2 | task | new child | gates-11 |
| 11 | Z3 translation declares separate Int and Real constants for one numeric param; solver returns wrong SAT models (0.5<x<1 → x=1.5) | P1 | bug | new child | core-01 |
| 12 | Explore report under-reports discovered paths (report '0 path(s)' / '1 path(s)' while the batch line and spec show 2-4) | P1 | bug | new child | core-02, cli-ux-15, artifacts-03, goals-03 (L6, same root), prior-19 (L6) |
| 13 | --setup is silently ignored in concolic mode (all production callers pass setup_context=None), and the random explorer shrinks after teardown | P1 | bug | new child | core-03, core-09 |
| 14 | Concolic dynamic mock variation regressed (str-3ky9.4 undone by str-lebv/str-r59s); hidden behind `_`-prefixed params | P2 | bug | new child | core-04 |
| 15 | Two value shrinkers: str-55ep/str-ddxe/tuple fixes landed in the dead input_gen copy; live shrink.rs lacks them | P2 | bug | new child | core-05 |
| 16 | Concolic refine phase drops prepare_id/execution_profile and discards the paths it reaches; no shared Execute builder | P2 | bug | new child | core-06, core-16 (context; dup-open str-qwua7.5) |
| 17 | Engines disagree on what a 'path' is, --max-iterations means different budgets per entry point, and three hand-built orchestrator::ExploreConfig literals diverge | P2 | task | new child | core-07, core-14 |
| 18 | ~5,100 lines of production-dead code in shatter-core (recursive, array_mutation, reporter+clustering, sequential scan(), shrink_witness, mutate_mock_values); add a reachability check | P2 | chore | new child | core-08 |
| 19 | NOTE on str-qwua7.49: premise is wrong; real failure is an empty worker pool misreported as task timeouts | P2 | bug | note → `str-qwua7.49` | core-10 |
| 20 | Invariant inference over-generalises: no minimum support at function level, confidence hard-coded 1.0, only 0-anchored templates | P2 | bug | new child | core-11 |
| 21 | Z3 has no default per-query timeout (solver can outlive timeout_explore), and `scan --solver-timeout` is silently discarded | P2 | bug | new child | core-12 |
| 22 | Explore auto-resume is keyed only on source fingerprint: --concolic / budget / seed changes silently return stale results labelled with the new mode | P1 | bug | new child | core-13, cli-ux-16, artifacts-01, goals-05, prior-18 |
| 23 | NOTE on str-qwua7.6: explore_with_oracle keeps growing; add a function-length ratchet | P3 | task | note → `str-qwua7.6` | core-15 |
| 24 | Float constants converted to Z3 via (v*1e6).round() as i64: saturate above ~9.2e12 and become 0 below 5e-7 | P3 | bug | new child | core-19 |
| 25 | std DefaultHasher used for persisted/reproducible keys (core_sample --seed selection, Rust harness cache dirs) | P3 | bug | new child | core-20, frontend-rust-13 |
| 26 | `explore -o out.json` / `--spec-out` write an empty no_targets bundle after a successful run, and keep only the first file's bundle for multi-file/glob targets | P1 | bug | new child | cli-ux-01, artifacts-02, goals-04 |
| 27 | `explore --format text|html` has no effect on stdout (printer branches on --render); --render and --format overlap | P2 | bug | new child | cli-ux-02 |
| 28 | str-qwua7.15 fix is an argv-scanning workaround: `shatter help <cmd>` and 9 non-executing commands still show execution-only globals | P2 | bug | new child | cli-ux-05, prior-07 (L6) |
| 29 | Scan artifact filenames embed the mangled absolute source path; deep checkouts hit ENAMETOOLONG and the scan still exits 0 | P2 | bug | new child | cli-ux-07 |
| 30 | `--seed` exists only on scan; str-0m0vn closed although its symptom named explore | P2 | feature | new child | cli-ux-08 |
| 31 | Unknown config keys and `--set` key typos are silently ignored | P2 | feature | new child | cli-ux-09 |
| 32 | Behavior-map cache keyed by bare function name: scan never hits on unchanged re-scan, and same-named functions in different files overwrite each other | P2 | bug | new child | cli-ux-12, goals-14 |
| 33 | Telemetry KNOWN_SUBCOMMANDS is stale (3 nonexistent, ~15 real commands missing and redacted) | P3 | bug | new child | cli-ux-18 |
| 34 | Mixed-language scan: the second language sub-scan deletes the first's function artifacts and overwrites summary.json; checkpoint lives elsewhere | P1 | bug | new child | artifacts-04, docs-07 (code half) |
| 35 | NOTE on str-qwua7.38: branch-id pairing causes false negatives, not only noise — raise to P1 | P2 | bug | note → `str-qwua7.38` | artifacts-06 |
| 36 | Spec preconditions are sample statistics (often false or vacuous); use the symbolic path constraints already recorded | P2 | feature | new child | artifacts-08, goals-13 |
| 37 | Three incompatible spec JSON shapes; `compare` rejects the --spec-out bundle (the only clean producer) | P2 | bug | new child | artifacts-09, docs-03 (code half) |
| 38 | `shatter diff` has no producer: no command writes a Snapshot, and diff rejects every JSON Shatter emits | P1 | bug | new child | artifacts-12, docs-02, goals-01 |
| 39 | `revalidate` ignores return values: changed outputs on replayed inputs are reported as confirmed and exit 0 | P1 | bug | new child | goals-02 |
| 40 | TS instrumentor data-flow map ignores program point: recorded path constraints contradict the concrete execution | P1 | bug | new child | frontend-ts-01 |
| 41 | TS switch/ternary/value-position &&,|| are analyzed but never instrumented; analyze and instrument branch IDs desync and coverage is misattributed | P1 | bug | new child | frontend-ts-02, prior-17 |
| 42 | TS analyzer resolves shadowed callback parameters to the outer function parameter | P2 | bug | new child | frontend-ts-04 |
| 43 | TS frontend classifies any target error whose message mentions 'timeout' as timed_out/infrastructure | P2 | bug | new child | frontend-ts-05 |
| 44 | TS parseRequest validates only the envelope: missing fields crash as internal_error; validCommands hand-maintained | P2 | bug | new child | frontend-ts-07 |
| 45 | NOTE on str-rf2v: a fourth, already-diverged SSA flow walker lives in executor.ts | P2 | refactor | note → `str-rf2v` | frontend-ts-09 |
| 46 | TS protocol round-trip tests are tautological and the builder-parity property test cannot detect drift | P2 | task | new child | frontend-ts-10, frontend-ts-11, tests-ci-12 |
| 47 | TS preflight fails dependency-free projects (requires node_modules whenever project_root is set) and one failure blocks every later request | P3 | bug | new child | frontend-ts-13 |
| 48 | TS SymExpr builders collapse common operators to unknown (??, **, shifts, element access, as/!/satisfies, template literals) | P3 | feature | new child | frontend-ts-14 |
| 49 | shatter-ts hygiene: async timeout timer leak hidden by jest forceExit, stdin EOF drops in-flight responses, dual lockfiles, tests emitted to dist, js-yaml v3 | P3 | chore | new child | frontend-ts-06, frontend-ts-16, frontend-ts-17 |
| 50 | Go frontend finds its harness runtime module via the compile-time source path: installed/relocated binaries fail every uncached execute | P1 | bug | new child | frontend-go-01 |
| 51 | Documented `go get -tool github.com/shatterproof-ai/shatter/go-tool/...` cannot resolve: module path says go-tool/ but the directory is shatter-go-tool/ | P1 | bug | new child | frontend-go-02 |
| 52 | Go rune/char literals are typed as string constants in both SymExpr builders; analyzer strings.Trim mangles escapes and quotes | P1 | bug | new child | frontend-go-03 |
| 53 | NOTE on str-qwua7.35: four Go SymExpr builders; instrument/flow*.go is unreachable and CLAUDE.md names it as the ite mechanism | P2 | refactor | note → `str-qwua7.35` | frontend-go-04 |
| 54 | ~55 unreachable Go functions incl. the whole reconstruct package; re-scope str-qwua7.48 property tests to live generators | P2 | chore | new child | frontend-go-06, frontend-go-11 |
| 55 | shatter-go-tool: non-atomic extract can cache a truncated binary; no HTTP timeouts; GitHub API call on every run; tests cover only arg parsing | P2 | bug | new child | frontend-go-10 |
| 56 | Go frontend small fixes: line-0 records on every execution, generated mock code ignores Unmarshal errors and uses backtick raw strings, dead assignments, unchecked lock-PID writes | P3 | bug | new child | frontend-go-14, frontend-go-15, prior-22 |
| 57 | Go lint is red on main (10 issues) and no gate runs golangci-lint or gofmt; str-2tyfk and str-qwua7.32 closed with residuals | P2 | bug | new child | prior-08, frontend-go-05 (AGENT context) |
| 58 | Rust crate-bridge harness shares stdout with user code: any function that prints fails as internal_error | P1 | bug | new child | frontend-rust-01 |
| 59 | Rust instrumentation emits invalid or stringly-typed constraints: hand-rolled JSON breaks on `1.` floats and control chars, and match arms become string equality on pattern text | P2 | bug | new child | frontend-rust-02, frontend-rust-03 |
| 60 | Request timeout (30 s) is not longer than the build timeout (30/120 s): cold Rust builds surface as a generic request timeout | P2 | bug | new child | frontend-rust-04 |
| 61 | NOTE on str-qwua7.50: poison-tolerant locks can't help — the handler has no panic boundary | P2 | bug | note → `str-qwua7.50` | frontend-rust-05 |
| 62 | shatter-rust main.rs re-declares the module tree: all 587 inline unit tests compile and run twice | P2 | chore | new child | frontend-rust-06 |
| 63 | 38 shatter-rust tests print 'skipping' and pass when cargo cannot reach the network | P2 | bug | new child | frontend-rust-11 |
| 64 | shatter-rust: crate type registry rebuilt on every analyze (O(N²) parses) and two independent Axum extractor classifiers | P3 | refactor | new child | frontend-rust-12, frontend-rust-14 |
| 65 | shatter-rust-runtime harness loop swallows malformed requests; branches on user-spawned threads are silently lost | P3 | bug | new child | frontend-rust-15 |
| 66 | shatter-llm hardening: Jev API key reachable via derived Debug; parser ignores int width and first-bracket only; uncapped backoff; no PBT | P3 | bug | new child | frontend-rust-16, frontend-rust-17 |
| 67 | Parity gates never check dispatch: a command advertised but not dispatched passes every static gate | P2 | task | new child | protocol-parity-01 |
| 68 | Protocol capability facts are hand-replicated in six places, four parity-matrix sections are validated by nothing, and codegen covers 6 of 13 registry enums | P2 | task | new child | protocol-parity-08, protocol-parity-19 |
| 69 | validate-protocol-registry prints a permanent get_invocation_plan 'may be unimplemented' warning although the matrix marks it optional | P3 | chore | new child | prior-24 |
| 70 | Build and Release workflow has 0 successes in 200 runs (Windows z3.h, aarch64 openssl-sys); no release exists so install.sh/action.yml cannot install | P1 | bug | new child | tests-ci-02, prior-02 |
| 71 | Snapshot tests self-create missing snapshots and pass; four copy-pasted helpers; whitespace-collapsed comparisons; no CLI output snapshots | P2 | task | new child | tests-ci-08 |
| 72 | Test inputs come from an unpinned external examples repo (origin/main, refreshed every 10 min) | P2 | task | new child | tests-ci-09 |
| 73 | Smoke, walkthrough, gauntlet and E2E user paths never run in CI; the only CI gauntlet path (Perf CI) has 0/13 successes | P2 | task | new child | tests-ci-11 |
| 74 | Collapse ~20 overlapping gate tiers; stop duplicating task bodies for checksum identity; e2e runs twice in pre-completion-e2e | P3 | refactor | new child | tests-ci-13, gates-07 (e2e duplication part) |
| 75 | str-qwua7.4 purge incomplete: a March rapid failfile is still tracked and .gitignore covers only planner/ | P3 | chore | new child | tests-ci-15, prior-23 |
| 76 | Two duplicate broad-run validation gates, neither scheduled or in check/affected/CI | P3 | chore | new child | tests-ci-16 |
| 77 | Workflows use deprecated Node-20 action majors, unpinned ubuntu-latest (Ubuntu 26 from 2026-10-19), and setup-go cache misses go.sum | P3 | chore | new child | tests-ci-18 |
| 78 | Line-coverage metric inconsistent across languages: Go scan inflates to 100% with 7/18 branches; Rust reports ~54% for fully covered functions | P1 | bug | new child | goals-06, prior-20 |
| 79 | Turn EXPECTED BRANCHES comments into a known-answer ratchet gate; link gauntlet allowlist entries to issues; preserve TS discriminant literals | P2 | task | new child | goals-07 |
| 80 | Concolic explorer does not beat the default explorer on the project's hard examples; add a benchmark and investigate early worklist termination | P2 | task | new child | goals-08 |
| 81 | Go explore lacks the cold-build warmup gate scan has; at default parallelism under load Go explores time out en masse | P3 | bug | new child | goals-12 |
| 82 | Rust frontend still generates negative integers for usize params (str-ddxe regression?); deserialization failures appear as behaviors | P2 | bug | new child | goals-15 |
| 83 | Pre-commit hook runs the full shatter-core/shatter-cli test suite (incl. E2E) on every commit: ~60 s median, fragile, main driver of --no-verify | P1 | task | new child | sessions-04 |
| 84 | Shatter tests leak per-run directories into shared /tmp (hundreds of crate-bridge harness dirs, GBs) — widen filesystem isolation | P2 | bug | new child | sessions-08 |

## Duplicate-open (no draft; existing issue already covers it)

| Finding | Existing | Note |
|---|---|---|
| gates-04 | str-6nul9 | bench_frontier_ranking swept into core:test-ignored and timing out; fail-fast part folded into draft 07 |
| core-16 | str-qwua7.5 | capture flag hard-coded in both engines incl. random-explorer shrink (scope note is in draft 16) |
| core-17 | str-qwua7.29, .30, .47 | module cycles, missing lints, proptest gaps; drop array_mutation from .47 (in draft 18) |
| frontend-ts-12 | str-qwua7.31 | no ESLint in shatter-ts; 72 typescript-eslint findings (add evidence as comment) |
| prior-15 | str-qwua7.11 | explore --spec-json stdout is markdown then JSON (reproduced at HEAD) |
| prior-16 | str-qwua7.5, .13, .39 | concolic ignores capture flag; hint printed twice; implicit init to stdout (reproduced) |

## Out of this draft set

L2/L3/L6/AGENT findings, and findings targeting bento, bugshot, storystore, shatter-agents or dotfiles,
are drafted in the sibling draft sets. Code halves of some doc findings are included here (docs-02 → 38,
docs-03 → 37, docs-07 → 34).

## Cross-set duplicates (added by the completeness review)

Before running `file.sh`, read §15.1 of `audits/2026-09-22.md`. Drafts in this set that duplicate or overlap a draft in another set:
- 12 = `shatter-docs-ui/02` (same root cause; keep 12).
- 57 = `shatter-agent/25` (golangci-lint gate; keep one).
- 28 overlaps `shatter-docs-ui/07` (str-qwua7.15 help flags; merge).
- 74 overlaps `shatter-docs-ui/21` (e2e runs twice).
- 45 (note on str-rf2v) overlaps `shatter-docs-ui/23`; post one combined note.
- 03/04 overlap `shatter-agent/20`.
