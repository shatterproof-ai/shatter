# Cross-check review

- **Reviewer:** claude (DEGRADED same-runtime fallback)
- **Artifact type:** issue
- **Mode:** degraded

> **DEGRADED REVIEW.** The counterpart runtime was unavailable, so this review came from an independent agent of the *same* runtime. It shares the original author's model and blind spots; weight it accordingly.

## Findings

# Cross-check review (DEGRADED: same-runtime fallback) -- bundle shatter-concolic-and-engine-design

**Mode:** DEGRADED. The Codex counterpart failed identity validation (exit 4), so a Claude reviewer did this review read-only.
**Evidence base checked:** audit worktree `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22` (HEAD 56c86168, confirmed), `~/project/holdout`, `~/project/shatter-effectiveness`, and bd in `~/project/shatter`.

## Spot-verification summary

I checked most file:line citations against HEAD 56c86168 and found them accurate. That includes:

- args.rs:117/505/970-979/992/1379;
- explore.rs:5138-5169 (seed None, max_executions = max_iterations);
- scan_orchestrator.rs:3080-3102 and :3144-3150 (5x);
- observe.rs:107-130 (5x, plateau 20, empty mocks);
- orchestrator.rs:203/864-871/1621-1646/2147/2490/2557/2643;
- explorer.rs:566-571/1012;
- pipeline.rs:951;
- e2e_concolic.rs:1553-1558, plus the grep counts 1/0/0;
- the dead-code module sizes (675/397/1326/530, and scan() at 482 lines, total 3,410) with no non-test callers;
- the core_sample.rs and shatter-rust executor.rs DefaultHasher sites, and no rust-toolchain file;
- Cargo.toml:44, bench_frontier_ranking.rs:34-35/304/381, and the Taskfile bench targets;
- function lengths 1372/1346/965/476 (brace-matched);
- the holdout summary.json values and the shatter-effectiveness state/commit/line counts.

All 21 concolic artifacts do report `worklist_exhausted` with `solver_guided_inputs: 0`. The referenced bd IDs exist with the stated statuses. I found no open duplicates for drafts 01, 02, 04, 07 or 08.

## Findings

### 01 concolic-vs-default-benchmark

- **MAJOR -- "Equal execution budget" cannot be met through the entry point the draft recommends, yet the issue is marked unblocked.** The suggested approach says to drive `shatter scan --seed N [--concolic]` because scan is the only command with `--seed`. But scan's concolic arm gets `max_executions = 5 x max_iterations` (`concolic_scan_max_executions`, scan_orchestrator.rs:3144-3150), and no CLI flag sets it independently. So the "documented, equal execution budget" criterion is unreachable without a code change owned by engine-path-identity-budget-config, which the draft puts out of scope and does not list as a blocker. Pick one fix: add a blocker, allow a documented unequal budget, or add a budget override to scope.
- **MAJOR -- `--clean` does not exist on `scan`.** The acceptance criteria require a fresh artifact dir "plus `--clean`", but `--clean` exists only on `explore` (args.rs:673). ScanArgs has `--no-cache` (args.rs:1016). Following the draft's scan-based approach, a fresh agent cannot satisfy the criterion as worded. Name the correct scan flag (`--no-cache` plus a fresh dir) or allow either one.
- **MINOR -- A garbled evidence bullet contradicts the acceptance criteria.** "Unlike those, this benchmark calls `orchestrator::explore_with_oracle` directly" is describing the *existing* frontier bench, but it reads as if it describes the new one. The acceptance criteria then forbid calling `orchestrator::` directly. Reword it to "the existing frontier bench bypasses the CLI; this one must not".
- **MINOR -- Hidden context.** The draft cites "Kapow agent memory (2026-07-02)" with no path. It also leaves the downstream project choice (kapow/zolem/pickpackit) open with no selection criterion.
- **MINOR -- Scope is heavy for one issue.** It covers the harness, a three-language corpus, a downstream subset, a per-release checklist change and release notes. Consider splitting the release-process wiring into a follow-up.

### 02 concolic-early-termination

- **MINOR -- Part of one criterion is already satisfied.** A test already maps each `TerminationReason` to `StopReason` (pipeline.rs:~2160-2180), and the mapping at pipeline.rs:918-941 is 1:1. The likely defect is upstream: the loop's `termination_reason` stays `WorklistExhausted` at orchestrator.rs:2557 even when executions reach `max_executions`. The criterion should target the orchestrator loop's reason assignment (a run with `total_executions == max_executions` must report `MaxExecutions`), not the mapping.
- **MINOR -- Evidence is incomplete.** The list of 100-iteration functions omits `stripInlineComment` (100 iterations) and `sortPreferences` (95). The draft also misses an informative signal: for several 100-iteration runs, `len(raw_results)` is far below `iterations` (validateJwt 100 vs 22, validateEmail 100 vs 27, parseDotenv 100 vs 27, parseSemver 100 vs 50). That suggests the fuzz phase consumes the execution budget without recording results, which bears directly on the root cause.
- Otherwise the draft is well scoped and has a good repro command and checkable acceptance criteria.

### 03 concolic-positioning-decision

- **MINOR -- A related open issue is not linked.** str-jd0d1 ("Walkthrough labels random runs concolic", open P2) is a live instance of the same positioning mismatch and should be linked.
- The draft is ready. Its blockers are correct, and the decision options each require a metric and threshold.

### 04 effectiveness-benchmark-holdout

- **MAJOR -- One issue carries several deliverables.** It bundles a location decision, a first benchmark slice (with two alternative designs), the holdout fix-or-archive, and conditional tracker initialization plus filing the remaining plan tasks. The draft's own suggested approach says to split. File it as a parent with children, or file the location decision first, so that "done" is not conditional on a decision made inside the issue.
- The claims are verified (the summary.json totals, the 401 skipped equal to 401 errors, and the shatter-effectiveness contents and commit).

### 05 engine-path-identity-budget-config

- **MINOR -- Heavy overlap with str-qwua7.6.2 (open P1), left unresolved.** The config-unification half largely duplicates str-qwua7.6.2 ("Unify explorer::ExploreConfig and orchestrator::ExploreConfig behind a shared base"). "Either do the work there ... or here" leaves ownership open. Decide before filing: file only the path-identity and budget-semantics parts here, and append the scan/observe literals to str-qwua7.6.2 as a note.
- **MINOR -- A call site is missing.** The explorer.rs `hash_branch_path` call sites include :1886 as well as :1704/:1788/:1833.
- **MINOR -- The Loopy fixture has no repo path.** It exists only in the audit notes, so the parity criterion should say where to add it (for example `examples/go/` or a test fixture dir).
- **MINOR -- A criterion may be unachievable by design.** "Equal path counts under random and concolic" for Loopy may conflict with the random explorer's intentional loop-bucketing. The target identity (bucketed or raw) should be stated.

### 06 engine-parity-e2e

- **MAJOR -- Four tasks in one issue.** It combines a three-language E2E suite, a new lint, a CLAUDE.md rule, and bento cross-linking. The draft itself says to "split into child tasks when claimed". File them as children now so each has its own acceptance criteria and proof.
- **MINOR -- The lint's cleanup cost is not sized.** A rough grep finds about 92 `_`-prefixed let/param bindings across shatter-core/src and shatter-cli/src (including test modules). "Existing hits are fixed or annotated" is sizeable, and the non-test filter is not specified.

### 07 core-dead-code-removal

- The claims are verified: sizes, no non-test callers, the scan() doc at :1223, and the test at :7510.
- **MINOR -- Reference checking is incomplete.** The grep regex in the check does not cover `shatter_core::<mod>` uses in other workspace crates (shatter-llm, benches, the tests/ dirs). A workspace-wide grep on my side found no uses, but the stated check should cover the whole workspace.

### 08 stable-hash-persisted-keys

- The draft is ready. All cited sites were verified.
- **MINOR -- The toolchain is unpinned.** With no toolchain pin, the golden-value tests will only catch algorithm changes on the CI toolchain in use. That is acceptable, but worth noting.

### 09 qwua7-6-function-length-ratchet (note)

- The draft is accurate: lengths verified and str-qwua7.6 is open. It is ready to append.

### 10 qwua7-43-bench-dev-dep-cycle (note)

- The draft is accurate: str-qwua7.43 is open, and the imports, Cargo.toml:44 and Taskfile targets were verified. It is ready to append.

## Verdict

**Mostly ready. Fix 01 before filing, and split 04 and 06.** The factual accuracy of the bundle is high: I found no incorrect file or line claims beyond small omissions.

Top fixes:

1. **Draft 01:** reconcile the equal-budget and `--clean` criteria with the scan-based approach. Either add engine-path-identity-budget-config as a blocker, or scope a budget override and use scan's `--no-cache`.
2. **Drafts 04 and 06:** file as parents with children (or split now) instead of multi-deliverable single issues.
3. **Draft 02:** retarget the termination-accounting criterion at the orchestrator loop (the MaxExecutions reason is lost), and add the raw_results-vs-iterations evidence.
4. **Draft 05:** settle ownership versus str-qwua7.6.2 before filing.
