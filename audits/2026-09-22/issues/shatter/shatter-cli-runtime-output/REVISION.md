# Revision: shatter-cli-runtime-output

Applied the Codex cross-check (`issues/crosscheck/shatter-cli-runtime-output.codex.md`, primary) and the degraded same-runtime review (`issues/crosscheck/shatter-cli-runtime-output.md`, secondary). Code re-checked against the audit worktree (code identical to 56c86168). Tracker state checked live with `bd show str-qwua7.40` and `bd show str-qwua7.13` (both OPEN) on 2026-09-23. Nothing is filed (D6). None of D1-D6 conflicts with this bucket; no draft proposes timeout env vars or hook bypass (D4).

## Codex findings

| # | Sev | Finding (one line) | Action | Files |
|---|---|---|---|---|
| C1 | MAJOR | 01 leaves the TS/Rust execution policy undecided; tests must prove execution, not just a clean cwd | applied: policy table chosen (backend is Go-only; TS/Rust fail closed unless `--allow-host-writes`; mixed runs skip TS/Rust with reason); tests assert paths > 0 and marker in throwaway dir, and a refused run fails the allow cases | 01 |
| C2 | MAJOR | 03 regression test (stderr before stdout) already passes on the post-hoc loop | applied: test now gates a second function on a release file and requires the first completion line while the child still runs; explicitly says an ordering-only test does not count | 03, 04 |
| C3 | MAJOR | 03 worker-count formula `min(targets, workers)` does not match the per-function batch scheduler | applied: header reports sessions, runnable functions, and `min(effective_workers, runnable functions)` labelled "up to"; fallback to labelled configured capacity if one function can run concurrent batches; evidence cites explore.rs:4339, 4467-4472, 5399-5468 | 03 |
| C4 | MAJOR | 05 misdiagnoses runtime discovery (exe ancestors, not cwd; E2E sets `SHATTER_RUNTIME_PATH`); proposed test can pass already | applied: evidence corrected (executor.rs:1198-1223, e2e_concolic_rust.rs:122-125, rust_frontend_harness.rs:77); test relocates `shatter-rust`, strips the env var, uses >= 3 functions, asserts one error; positive control with the var set | 05 |
| C5 | MAJOR | 05 duplicates str-qwua7.40 / str-qwua7.13 without settling ownership | applied: split. Doctor Rust section + runtime-crate check -> note on str-qwua7.40 (keeps its version/protocol and require-flag criteria; flags the `--require-rust` vs `--require-frontend` spelling conflict). Hint dedup/length -> note on str-qwua7.13 (adds explore coverage). 05 narrowed to runtime error dedup, `rust` failure-impact row, env-var docs | 05, 09 (new), 10 (new) |
| C6 | MAJOR | 05 does not define which missing prerequisites make doctor fail | applied: new 11 defines in-use / required per language (warn vs fail), host-write readiness never fails, invalid backend value fails; tests include a TS-only project with go absent exiting 0 | 11 (new) |
| C7 | MAJOR | 06 Rust `Result` heuristic can mislabel ordinary maps | applied: `Ok(...)`/`Err(...)` only from frontend-supplied return-type metadata (executor's `is_result_return_shape`); analyzer's generic `Union` noted as insufficient; negative tests for a `{"Err":..}` map and an `Ok` field struct; protocol/parity steps if a field is added | 06 |
| C8 | MAJOR | 06 combines rendering with Rust `main` discovery and mock-recording | applied: split into 12 (Rust `main` exclusion, reproduce-first AC, modeled on Go `isMainEntrypointDecl`, analyzer.go:465-490) and 13 (mock-source diagnosis, closes with a written finding). Mocks-line placement stays in 06 | 06, 12 (new), 13 (new) |
| C9 | MAJOR | 07 has two incompatible coverage contracts; naming-only snapshot cannot verify unification | applied: metric work split to 15 with one contract (line coverage headline everywhere, branch only as a named secondary line); test compares headline denominators across the three commands on one fixture and fails on main | 07, 15 (new) |
| C10 | MAJOR | 08 "every fact" exceeds scope and tests | applied: narrowed to three required lines with tests; "every fact" replaced by a close-time parity inventory table (enumerates MC/DC, stubbed imports, float probes, abandoned frontiers, opaque suggestions, perf, GA); removal of `--render plain` explicitly out of scope | 08 |
| C11 | MAJOR | Bundle stderr requirements conflict (03 JSON-only vs 01/05 human text) | applied: 03 defines the machine-mode contract (`--progress` stays a bool; warn/error records become `{"type":"log",...}` with the human text as `message`; info suppressed) and tests a warning under `--progress`; 01 and 05 reference that contract | 01, 03, 05 |
| C12 | MINOR | 02 wrongly says README has no Go-only caveat (README:309 says "(Go frontend)") | applied in 02 comment text and 01 evidence | 01, 02 |
| C13 | MINOR | 06 and 08 misidentify renderer paths (markdown = render.rs via explore.rs:3620-3648; core formatter = legacy plain) | applied; 06 also names the second `&s[..37]` site (explorer.rs:3262) | 06, 08 |

No Codex finding is disputed.

## Secondary (same-runtime) findings

| Sev | Finding | Action | Files |
|---|---|---|---|
| MAJOR | 05 duplicates str-qwua7.40 / .13 | applied (same as C5) | 05, 09, 10 |
| MAJOR | 05 bundles ~six deliverables | applied: split into 05, 09, 10, 11 | 05, 09, 10, 11 |
| MAJOR | 07 combines layout bug with metric decision and open-ended diagnosis | applied: 07 layout/wording only; 14 diagnosis; 15 metric | 07, 14, 15 |
| MINOR | 05 wrong E2E root cause | applied (C4) | 05 |
| MINOR | doctor exit code ambiguous under default-deny | applied: host-write readiness is warn, never fail | 11 |
| MINOR | 08 misquotes rubric items 2/4/7 | applied: cites "7. Exploration completeness" and criterion "J. Completeness signal" | 08 |
| MINOR | 01 omits `shatter-go/CLAUDE.md:257` | applied: added to evidence and docs AC | 01 |
| MINOR | 03 bundles explore header; `--progress json` is a new CLI surface | partially: header kept in 03 (shared sink covers it); no new spelling, `--progress` bool is machine mode | 03 |
| MINOR | 06 Go wording `errors: <msg>` is invented | applied: ``returns error `<msg>` `` | 06 |
| MINOR | 07 test line ref 3490 -> 3484 | applied | 07 |

## Splits and conversions

- 05 rust-runtime-path-and-doctor: slug kept, retitled and narrowed. New: 09 doctor-rust-runtime-note (note-to-existing str-qwua7.40), 10 rust-hint-once-note (note-to-existing str-qwua7.13), 11 doctor-execution-readiness (new, blocked_by sandbox-backend-disables-guard).
- 06 per-language-outcome-rendering: slug kept, rendering only. New: 12 rust-main-default-exclusion, 13 rust-mocks-to-string-diagnosis.
- 07 run-report-verdict-and-coverage-metrics: slug kept for stability, retitled to the verdict layout bug. New: 14 run-validity-degraded-cause-diagnosis, 15 coverage-headline-metric-unification.
- No slug removed or converted. Cross-bucket references: 15 and 08 relate to branch-metric-counts-sites (shatter-reports-and-specs) and explore-format-flag-ignored (shatter-cli-flags-and-help) as "related", not as blockers. explore-format-flag-ignored (other bucket) references markdown-drops-render-plain-info, whose slug is unchanged; 08 now says it does not remove `--render plain` and hands that decision (with a parity inventory) to explore-format-flag-ignored. `issues/INDEX.md` and `issues/MANIFEST.md` need the seven new slugs 09-15.
- Decisions made in drafts that the maintainer may want to confirm before filing: 01's fail-closed policy for TS/Rust under backend-only; 15's choice of line coverage as the headline metric.
