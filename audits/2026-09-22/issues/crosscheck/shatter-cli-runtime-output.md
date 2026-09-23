# Cross-check review

- **Reviewer:** claude (DEGRADED same-runtime fallback)
- **Artifact type:** issue
- **Mode:** degraded

> **DEGRADED REVIEW.** The counterpart runtime was unavailable, so this review came from an independent agent of the *same* runtime. It shares the original author's model and blind spots; weight it accordingly.

## Findings

# Cross-check review (DEGRADED: same-runtime fallback, Claude): bundle shatter-cli-runtime-output

Codex review failed identity validation (exit 4). An independent Claude reviewer did this review read-only against audit worktree HEAD 56c86168 and the live bd tracker.

## Claims verified as accurate
- 01: `host_writes.rs` `sandbox_backend_configured()` (55-63), `execution_permitted()` (77-79) and the `setup()` early `return Ok(None)` (142-145) match the draft. The only other reader of `SHATTER_SANDBOX_BACKEND` is `shatter-go/sandbox/runner.go:16`. The README 309-310/321-329, QUICKSTART 83-85 and SPEC 606-624 wording, `refusal_message()` and the `args.rs:170-171` help all match. `shatter-go/CLAUDE.md` confirms that Go ignores `SHATTER_HOST_WRITE_DIR` when the sandbox is enabled.
- 03: the post-hoc loop at `scan.rs:1412-1424`, the `eprintln!` JSON at 1335-1365, no progress handling in `run.rs`, the worker-count string at `explore.rs:4468`, and the `rust-explore3.err` sample (16 workers for 1 target, starting 2/3 before 1/3) are all confirmed. `run.err` is 0 bytes.
- 05: `RUST_FRONTEND_INSTALL_HINT` at `helpers.rs:418-424`, the runtime lookup and error at `shatter-rust/src/executor.rs:1200-1223`, `build_frontend.rs:607-630`, and doctor running only `check_embedded_frontend` plus `check_generated_paths_ignored` are confirmed. `SHATTER_RUNTIME_PATH` does not appear in README, QUICKSTART or SPEC.
- 06: `value_short` byte-slices `&s[..37]` (`render.rs:277-284`). serde_json does not escape non-ASCII, so the panic claim is sound. The Mocks extras are at `render.rs:141`, `format_exploration_report` is at `explorer.rs:2950`, and `LIFECYCLE_EXPORT_NAMES` is at `discovery.rs:605-617`.
- 07: `render_validity_markdown` is printed before `print_summary_report` (`run.rs:672-679`), the degraded text is at `run.rs:1711`, and the H1 is at `scan.rs:2034`.
- 08: `args.rs:41` marks plain as deprecated, the `coverage_metrics.rs:277/328` lines exist, and `render.rs` has no `stop_reason`.
- The referenced bd ids exist with the stated open or closed status (str-qwua7.8, str-7pkp.5, str-qwua7.40/.13/.20.2/.60/.51/.56/.57/.15/.10, str-joyqu, str-02i70, str-jeen.5, str-zt4v, str-4ad5).

## Findings

**MAJOR: rust-runtime-path-and-doctor duplicates two open issues without resolving the overlap.**
str-qwua7.40 (open, P2) is "`shatter doctor`: report whether shatter-rust is resolvable, from where, and its version/protocol match", which is the draft's first doctor AC bullet. str-qwua7.13 (open, P1) is "scan and run must agree on a missing frontend; print the remediation once", which overlaps the "hint printed once per run" AC. The draft only says "coordinate with it or fold it in". Before filing, decide which issue owns each item: block on them, or narrow this draft to the runtime-crate error, the env var docs and the doctor sandbox/toolchain checks.

**MAJOR: rust-runtime-path-and-doctor puts about six deliverables in one issue.**
The draft covers four new doctor checks with a new exit-code contract, a hint rewrite, per-run error dedup, a failure-table language-label fix, env var docs, and two out-of-tree tests. It is hard to estimate, and it crosses the two open issues above. Split it: (a) runtime-crate error, dedup, label and docs; (b) the doctor readiness checks and exit codes.

**MAJOR: run-report-verdict-and-coverage-metrics combines a layout bug with a product decision.**
Moving the verdict below the H1 and rewriting it in plain language is a well-bounded bug. Picking one headline coverage metric for explore, scan and run is a SPEC-level decision, and the draft itself says it should be made "together with branch-metric-counts-sites". It also contains an open-ended investigation ("identify the degraded cause"). As written, the whole issue cannot close until that decision lands. Split the metric unification into its own issue, blocked by or paired with branch-metric-counts-sites.

**MINOR: rust-runtime-path-and-doctor gives a wrong root cause for the missing test coverage.**
The draft says `e2e_concolic_rust.rs` "runs from the workspace root, where the runtime crate is found automatically". In fact it sets `SHATTER_RUNTIME_PATH` explicitly in the frontend config (`shatter-core/tests/e2e_concolic_rust.rs:122-125`, and also `tests/support/rust_frontend_harness.rs:77`). Auto-discovery also walks up from the frontend *executable*, not the cwd. The conclusion (the out-of-tree path is untested) still holds, but the stated reason is wrong.

**MINOR: rust-runtime-path-and-doctor's doctor exit-code AC is ambiguous under default-deny.**
"Non-zero when a check that blocks execution fails" would make `doctor` exit 1 on every fresh install and in every CI job without `SHATTER_ALLOW_HOST_WRITES`, because default-deny blocks execution. State whether host-write readiness counts as a fail or a warn.

**MINOR: markdown-drops-render-plain-info misquotes the walkthrough-review rubric.**
The draft says rubric items 2, 4 and 7 treat branch coverage, discovery method and termination reason as essential. In `.claude/skills/walkthrough-review/SKILL.md`, item 2 is input conditions and item 4 is concrete examples. Only item 7 (exploration completeness) supports the termination-reason line. Fix the citation, or rest the rationale on the plain-vs-markdown information gap alone.

**MINOR: sandbox-backend-disables-guard leaves out one doc that its fix would make stale.**
`shatter-go/CLAUDE.md:257` says "the CLI never sets `SHATTER_HOST_WRITE_DIR` in that case". Suggested approach step 1 makes that statement false. Add it to the docs AC. The AC that says "decide explicitly whether a TS/Rust-only execution passes the gate" is a design choice left inside the acceptance criteria. The recommendation is fine, but the filer should confirm it or make it a hard requirement.

**MINOR: scan-progress-post-hoc bundles the explore header and counter fixes with the scan/run progress fix.**
It is coherent under the shared-sink approach, but the explore worker-count header (`explore.rs:4468`) is an independent one-line fix. `--progress` is currently a bool flag (`args.rs:961`), so the "`--progress json`" spelling in the AC means a CLI surface change. Say so, and route it through the flag-unification bucket if one exists.

**MINOR: per-language-outcome-rendering prescribes a Go wording with no rationale and mixes in unrelated concerns.**
`errors: <msg>` is a new invented verb. Consider `returns error "<msg>"` so it follows the rubric's "error value" framing. Rust `main` exclusion and the Mocks-line placement are separate concerns that the verifier did not re-verify. They could be split, or at least marked as needing confirmation.

**MINOR: run-report-verdict-and-coverage-metrics has an off-by-a-few test line reference.**
The existing tests are at `run.rs:3484-3513`, not 3490-3513.

## Verdict
No BLOCKERs. 01 (sandbox-backend-disables-guard), 02, 04 and 08 are ready to file after the minor fixes. 03 and 06 are fileable. Before filing, 05 (rust-runtime-path-and-doctor) needs its overlap with the open str-qwua7.40 and str-qwua7.13 resolved and should be split. 07 (run-report-verdict-and-coverage-metrics) should have the metric-unification decision split out.

Top fixes:
1. Resolve 05's duplication with str-qwua7.40 and str-qwua7.13, and split 05.
2. Split 07 into the verdict-placement bug and the metric-unification decision.
3. Correct 05's e2e root-cause claim and 08's rubric citation, and add `shatter-go/CLAUDE.md` to 01's docs AC.
