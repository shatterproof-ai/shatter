# Revision: shatter-concolic-and-engine-design (2026-09-23)

Inputs: Codex review `issues/crosscheck/shatter-concolic-and-engine-design.codex.md` (primary), same-runtime review `issues/crosscheck/shatter-concolic-and-engine-design.md` (secondary). Code claims re-verified against the audit worktree. Tracker claims verified with `bd show` / `bd search` in `/home/ketan/project/shatter` on 2026-09-23. Nothing filed (D6).

## Codex findings

| # | Sev | Finding (one line) | Action | Files |
|---|---|---|---|---|
| 1 | BLOCKER | 02: artifacts contain `z3` discoveries, which contradicts "zero solver contribution"; `ExploreResultAccumulator` drops `stop_reason`/`solver_guided_inputs` | **Applied.** Verified: `explore.rs:190-330` builds the output with `..Default::default()`, and `StopReason`'s `#[default]` is `WorklistExhausted`. 6 of 21 artifacts have `z3` discoveries. The reporting bug is a new P1 issue (11). 02 was rewritten as a diagnosis blocked by 11, with the false claim retracted. The fix moved to a new issue (12). | 02, 11 (new), 12 (new) |
| 2 | MAJOR | 01-03: reproduction evidence (`goals-runs/`) is not tracked; command, SHA and config are missing | **Applied.** Verified: `audits/2026-09-22/goals-runs/` is in `.git/info/exclude`. The full 21-row per-function table (iters, raw_results, paths, branches, discoveries by method) is now inline in 02, and 02 states that the command and examples SHA were not recorded. 02 gives a reconstructed repro command. 01 and 15 require full provenance in every result file. Draft 01's kapow memory citation now gives the real path. | 01, 02, 15 |
| 3 | MAJOR | 01: `scan` has no `--clean`; the fuzz RNG ignores the seed (`orchestrator.rs:3060`); scan's 5× budget breaks equal budgets | **Applied.** All three verified. 01 now names `--no-cache`, an empty `--cache-dir` and fresh artifact directories, and adds a harness reproducibility check. It is blocked by new issues 13 (explore-budget-semantics, explicit execution budget) and 14 (concolic-fuzz-rng-unseeded), plus 11. | 01, 13 (new), 14 (new) |
| 4 | MAJOR | 02: the "exceeds 21 executions" criterion rewards extra executions, and the escape hatch is weak | **Applied.** The fix issue (12) asserts a reproducible missed branch, `z3` provenance, an accurate `stop_reason`, and `solver_guided_inputs >= 1` under a fixed seed and bounded budget. It explicitly does not assert a minimum execution count. The diagnosis (02) must end with a defect/expected verdict and a named fixture. | 02, 12 |
| 5 | MAJOR | 01/03: nobody owns the post-fix benchmark run | **Applied.** New issue 15 (concolic-benchmark-postfix-run), blocked by 01 and 12. 03 is now blocked only by 15. | 03, 15 (new) |
| 6 | MAJOR | 05: equal path counts do not prove equal path identity | **Applied.** 05 now tests identity on identical `ExecuteResult` traces (same bucket, different bucket, empty `branch_path` fallback, nested scopes). Engine-level expectations are "both return behaviours reached", with no path-count equality. | 05, 06 |
| 7 | MAJOR | 05: overlaps str-qwua7.6.2 (config) and str-qwua7.29 (`hash_branch_path` relocation) | **Applied.** Verified with bd: both are open. str-qwua7.6.2 names only `explore.rs:5138-5171`. str-qwua7.29 moves `hash_branch_path` to a leaf module. The config-literal scope became note 21 on str-qwua7.6.2. 05 is narrowed to identity semantics and call sites, and defers module location to str-qwua7.29. Budget semantics were split to 13. | 05, 13 (new), 21 (new) |
| 8 | MAJOR | 04, 06 and 07 each bundle independent deliverables | **Applied.** 04 was split into 04 (benchmark), 16 (holdout disposition) and 17 (shatter-effectiveness tracker and backlog). 06 was split into 06 (engine_parity suite), 18 (`_`-binding lint) and 19 (close-reason rule). 07 was split into 07 (deletions) and 20 (reachability gate). | 04, 06, 07, 16-20 (new) |
| 9 | MAJOR | 04: "bug-finding effectiveness" has no scoring oracle | **Applied.** 04 now has a "Scoring oracle" section. It defines the seeded-fault manifest (at least 20 faults, at least 2 languages), detection as an input-level outcome difference against the unmutated source, a denominator that excludes unsupported targets, a false-positive control run, handling of equivalent mutants, and an oracle self-test. | 04 |
| 10 | MAJOR | 06: "expected-fail" has no executable semantics | **Applied.** 06 specifies per-assertion `expect_divergence` markers: all assertions still execute, an unexpected pass fails the suite, and unmarked assertions must pass. The harness self-test covers all three cases. There is no `#[ignore]`. | 06 |
| 11 | MAJOR | 07: counting references from outside the file is not production reachability | **Applied.** In 20: defined roots (binary `main`s plus cross-crate non-test uses), item-level graph traversal via rustdoc JSON, or a documented heuristic with regression cases for mutually referencing dead modules and same-file live helpers, plus stale-allowlist detection. 07's evidence grep now covers the whole workspace (secondary-review minor). | 07, 20 (new) |
| 12 | MAJOR | 08: "every persisted hash" exceeds the stated inventory (`helpers.rs:711`, `run.rs:1041`) | **Applied.** Verified both. 08 now has a full workspace `DefaultHasher` inventory in three classes (must migrate, must classify, may keep), which adds `external_audit_cache_root`, `scope_hash` and `ExecutionRecord.input_hash`/`detect_mock_misses`. It also requires a final table that matches the grep exactly. | 08 |

No Codex finding was disputed.

## Secondary (same-runtime) review items

| Item | Action |
|---|---|
| 01 budget, `--clean` and bench-bullet wording | Applied (same as Codex 3). The frontier-bench sentence was reworded as "the existing bench bypasses the CLI; this one must not". |
| 01 hidden kapow memory context; downstream project choice has no criterion | Applied. Real memory path cited. Selection criterion: the project with the most functions scan completes without unsupported-target errors. |
| 02 "retarget the criterion at the orchestrator loop (MaxExecutions lost at `:2557`)" | **Disputed (superseded).** The orchestrator returns the correct `TerminationReason` at `orchestrator.rs:1621-1647` and assigns it at `:3179`. `:2557` is only the initial value. The loss is in the CLI accumulator (Codex 1), so the criterion targets the accumulator (11). |
| 02 raw_results vs iterations evidence; missing stripInlineComment and sortPreferences | Applied. Full 21-row table with `raw`, and a criterion requiring the gap to be explained. |
| 03 link str-jd0d1 | Applied. Verified open P2 with bd. |
| 05 missing call site `explorer.rs:1886`; no repo path for Loopy; bucketed vs raw identity | Applied (all three). |
| 06 lint cleanup cost not sized, no non-test filter | Applied in 18: explicit exclusions, and non-test hit count recorded before wiring. |
| 08 toolchain unpinned | Noted in 08's out-of-scope. |

## Split and convert notes

- **02 concolic-early-termination** (slug kept) is now diagnosis only. New: **11 explore-stop-reason-accounting** (P1 bug; blocks 01 and 02) and **12 concolic-early-termination-fix** (P1; blocked by 02).
- **01 concolic-vs-default-benchmark** is now blocked by 11, **13 explore-budget-semantics** (P1, new) and **14 concolic-fuzz-rng-unseeded** (P1, new).
- **15 concolic-benchmark-postfix-run** (P1, new): blocked by 01 and 12; blocks 03.
- **03 concolic-positioning-decision**: blocked_by changed from [01, 02] to [15].
- **04 effectiveness-benchmark-holdout** (slug kept) is now the benchmark only. New: **16 holdout-disposition** and **17 effectiveness-repo-tracker-backlog**, both blocked by 04.
- **05 engine-path-identity-budget-config** (slug kept) is now path identity only. Budget semantics moved to 13. Config literals moved to **21 qwua7-6-2-scan-observe-config-literals** (note-to-existing on str-qwua7.6.2).
- **06 engine-parity-e2e** (slug kept) is now the suite only. New: **18 underscore-binding-lint** and **19 pipeline-close-reason-rule**.
- **07 core-dead-code-removal** (slug kept) is now deletions only. New: **20 core-reachability-gate** (blocked by 07).
- 09 and 10 (notes) are unchanged. Codex had no findings on them.
- No slug was removed or converted.

## Cross-bucket effects

- `shatter-engine-correctness/02-float-probe-paths-uncounted` cites engine-path-identity-budget-config for "unifying path identity". That is still correct, because the slug now means path identity only.
- `shatter-engine-correctness/07-duplicate-value-shrinkers` cites core-dead-code-removal for "a crate-wide dead-code check". That check is now **core-reachability-gate**.
- `shatter-frontend-ts/09-ts-branchtype-known-answer-fixtures` and `02-ts-switch-ternary-instrumentation` cite engine-parity-e2e for the suite plus the `_`-param lint and checklist rule. The lint is now **underscore-binding-lint** and the rule is **pipeline-close-reason-rule**.
- `shatter-cli-flags-and-help/05-seed-for-explore-and-run` says concolic-vs-default-benchmark needs fixed seeds. The benchmark now uses `scan --seed` and is blocked by **concolic-fuzz-rng-unseeded**. It needs seed-for-explore-and-run only if the harness uses `explore`.
- Any bucket that references concolic-early-termination as "the fix" should now reference **concolic-early-termination-fix**. concolic-early-termination is the diagnosis.
- New note-to-existing target: str-qwua7.6.2 (21).
