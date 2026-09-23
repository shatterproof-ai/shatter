---
slug: explore-resume-options-key
kind: new
title: "Explore auto-resume is keyed only on the source fingerprint: --concolic, budget and seed changes silently return the prior run's results under the new mode's label"
priority: P1
type: bug
labels: [explore, resume, concolic, artifacts, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Explore auto-resume is keyed only on the source fingerprint: --concolic, budget and seed changes silently return the prior run's results under the new mode's label

## Problem

`shatter explore` resumes a function from prior artifacts when a completed summary entry exists and the source's deep fingerprint matches. Result-affecting options are not part of the key. Suppose you explore a target, then run it again with `--concolic`, a different `--max-iterations`, a different seed or `--no-cache`. The second run does no exploration. It replays the first run's result, and the report labels that result with the *current* explorer mode (`Explorer: concolic (Z3-backed)` on random results). The reverse also happens: a random run replays concolic results without the label.

As a result, every default-vs-concolic comparison in the same artifact dir is silently wrong, including the measurement required by decision D3 (concolic-vs-default-benchmark). The walkthrough's concolic and spec steps also replay the random step. Auto-resume itself is intentional (str-b2my.15), and str-060a fixed only the `--clean` case. The defects are the resume key, the labelling and the missing documentation. Resume stays.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`). Transcripts are under `audits/2026-09-22/` on that branch.

- `shatter-cli/src/commands/explore.rs:1209-1232` `try_resume_function` accepts a prior entry when `function_name` matches, `status == "completed"` and `deep_fingerprint` matches. No option or engine field is compared.
- `explore.rs:5241-5258`: on a hit, the accumulator is merged and `[resumed] <fn>: N branches, Xs (prior run)` is logged at info (:5253). The function is not scheduled.
- `shatter-cli/src/render.rs:143-145` pushes `Explorer: concolic (Z3-backed)` from the *current* `opts.is_concolic`, not from the result's provenance. `finalize_explore` (`explore.rs:3773`, `--from-artifacts`) also takes `use_concolic` from the current flags.
- Partial resume: the ExploreState sidecar (`resume_state_path`, `explore.rs:1237`) can load concolic `hash_branch_path` covered paths into a random run, which uses a different hash space.
- Repro (TS, audit verifier, release binary): `explore 01-arithmetic.ts:classifyNumber`, then `explore 01-arithmetic.ts:classifyNumber --concolic --max-iterations 5`. The second run prints `[info] [resumed] classifyNumber: 3 branches, 7.7s (prior run)`, and stdout says `- *Explorer: concolic (Z3-backed)*` (`artifact-samples/ts-concolic-after-random.{err,md}`, `cli-ux-transcripts/resume-flags.*`). A later `--spec --max-iterations 30` printed `Exploration: 100 iterations`, the first run's budget (`artifact-samples/ts-spec.md`).
- Repro (whole dir): `explore '*.ts' --concolic --no-cache` after a default run resumed all 26 files, and the aggregate is identical to the default run (445/799) (`goals-runs/ts-all-concolic.err`).
- Repro (Go, reverse direction): after a `--concolic` run, `explore mix.go --max-iterations 40` printed `Resumed 2/2 function(s) from prior artifacts` and re-emitted the concolic result (16 Loopy paths) without the concolic label (finding core-13). `explore lit.go:Classify --concolic --max-iterations 200` after a default run resumed and was labelled concolic (finding frontend-go-12).
- Walkthrough: `demo/walkthrough.sh:120` creates one `SHATTER_ARTIFACT_DIR` for the whole run. Steps `concolic-z3` and `spec-generation` (`demo/walkthrough.yaml:64-78`) target the same TS file as step 2, and both log `[resumed] classifyNumber` (`artifact-samples/wt-step8-concolic.err`, `wt-step9-spec.err`).
- SPEC documents resume only for `scan --resume` (SPEC.md §6.3, around :1019). There is no `--no-resume` flag (`unexpected argument`), and explore resume is mentioned only inside the `--clean` help text.
- Downstream memory (`project_kapow_shatter_advise_log.md`) records the same trap: "runs silently RESUME prior results".

## Acceptance criteria

- [ ] Summary entries and resume sidecars store an options hash. The hash covers explorer mode (random/concolic), iteration and time budgets, seeds and seed files, mocks, setup/teardown, solver settings (timeout), spec/invariant flags that change the stored observation, and the engine and frontend version fingerprint. On mismatch the function is re-explored. Full and partial (sidecar) resume both check it.
- [ ] A resumed result keeps its original explorer label. The report header says `(resumed from prior run; --clean to re-run)`, including under `--from-artifacts`.
- [ ] An `[info]` line says why each resume was accepted or rejected, naming the first differing option.
- [ ] CLI test in `shatter-cli/tests/` (fresh temp dir): a random run then a `--concolic` run re-explores and is labelled concolic. The same options twice resumes. A `--max-iterations` change re-explores. At close, show that the test fails on current `main` and passes after the fix (test name plus before/after output in the close note).
- [ ] The walkthrough steps that re-target the same function use separate artifact dirs or `--clean`. The close note includes `task walkthrough` output in which steps 8/9 contain no `[resumed]` line.
- [ ] SPEC §2.1 documents explore auto-resume: what the key covers and how to force a fresh run.
- [ ] `task affected` passes, and its `Gates selected` output is recorded. `cargo test --test e2e_concolic` passes (CLI wiring for explorer mode changed).

## Suggested approach

Add an `options_hash` (a stable hash such as SHA-256 over canonical JSON, not `DefaultHasher`; see stable-hash-persisted-keys) and an `explorer` field to `ExploreSummary` entries and to the ExploreState sidecar. Compute the hash once from the resolved explore config. Check it in `try_resume_function` and in the partial-resume loader. Treat legacy entries without the field as a mismatch (re-explore), the same way legacy summaries without fingerprints already are. Render the explorer label from the stored field. Grep for the parallel path (random `explorer.rs` vs concolic `orchestrator.rs` wiring in `main.rs`/`explore.rs`) so both write the same fields.

## Out of scope

- Scan's behavior-map cache keying (behavior-map-cache-keys) and scan's `--seed` freshness (str-9m9o3).
- Whether concolic finds more than random. That is concolic-vs-default-benchmark (D3), which needs this fix to produce valid numbers.
- The walkthrough step descriptions (str-jd0d1).

## Priority

P1: output is mislabelled, and engine comparisons (including the D3 benchmark) are silently invalid.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-jd0d1 (open; walkthrough labels random runs concolic. This issue fixes the artifact reuse behind it, and str-jd0d1 keeps the step-description half), str-8q1b4 (closed; scan resume report parity, same "resumed results must be comparable" class), str-b2my.15, str-060a, str-9m9o3, concolic-vs-default-benchmark.

## References

Audit 2026-09-22 findings artifacts-01, goals-05, prior-18 (P1), core-13, cli-ux-16, frontend-go-12 (resume part). Source draft: `drafts/shatter-code/22-explore-resume-options-key.md`.
