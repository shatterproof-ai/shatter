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

`shatter explore` resumes a function from prior artifacts when a completed summary entry exists and the source's deep fingerprint matches. Result-affecting options are not part of the key. Suppose you explore a target, then run it again with `--concolic`, a different `--max-iterations` or a different seed. The second run does no exploration. It replays the first run's result, and the report labels that result with the *current* explorer mode (`Explorer: concolic (Z3-backed)` on random results). The reverse also happens: a random run replays concolic results without the label.

As a result, every default-vs-concolic comparison in the same artifact dir is silently wrong, including the measurement required by decision D3 (concolic-vs-default-benchmark). The walkthrough's concolic and spec steps also replay the random step. Auto-resume itself is intentional (str-b2my.15), and str-060a fixed only the `--clean` case. The defects are the resume key, the labelling and the missing documentation. Resume stays.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`). Transcripts are under `audits/2026-09-22/` on that branch.

- `shatter-cli/src/commands/explore.rs:1209-1232` `try_resume_function` accepts a prior entry when `function_name` matches, `status == "completed"` and `deep_fingerprint` matches. No option or engine field is compared.
- `explore.rs:5241-5258`: on a hit, the accumulator is merged and `[resumed] <fn>: N branches, Xs (prior run)` is logged at info (:5253). The function is not scheduled.
- `shatter-cli/src/render.rs:143-145` pushes `Explorer: concolic (Z3-backed)` from the *current* `opts.is_concolic`, not from the result's provenance. `finalize_explore` (`explore.rs:3773`, `--from-artifacts`) also takes `use_concolic` from the current flags.
- Partial resume has no validation at all. `PersistedExploreState` (`explore.rs:67-70`) stores only `covered_paths` and `discovery_inputs`: no source fingerprint, no options, no explorer mode. `read_resume_state` (`explore.rs:1264-1273`) loads the sidecar (`resume_state_path`, :1236) whenever it parses. So a sidecar left by an interrupted run is reused after the source is edited (stale covered paths and inputs), and concolic `hash_branch_path` covered paths can be loaded into a random run, which uses a different hash space.
- Repro (TS, audit verifier, release binary): `explore 01-arithmetic.ts:classifyNumber`, then `explore 01-arithmetic.ts:classifyNumber --concolic --max-iterations 5`. The second run prints `[info] [resumed] classifyNumber: 3 branches, 7.7s (prior run)`, and stdout says `- *Explorer: concolic (Z3-backed)*` (`artifact-samples/ts-concolic-after-random.{err,md}`, `cli-ux-transcripts/resume-flags.*`). A later `--spec --max-iterations 30` printed `Exploration: 100 iterations`, the first run's budget (`artifact-samples/ts-spec.md`).
- Repro (whole dir): `explore '*.ts' --concolic --no-cache` after a default run resumed all 26 files, and the aggregate is identical to the default run (445/799) (`goals-runs/ts-all-concolic.err`). The resume here came from `--concolic` not being in the key. `--no-cache` on explore is documented as "Disable behavior map caching entirely" (`shatter-cli/src/args.rs:557-559`, `ExploreArgs`); it does not govern explore resume, and this issue does not change that (see acceptance).
- Repro (Go, reverse direction): after a `--concolic` run, `explore mix.go --max-iterations 40` printed `Resumed 2/2 function(s) from prior artifacts` and re-emitted the concolic result (16 Loopy paths) without the concolic label (finding core-13). `explore lit.go:Classify --concolic --max-iterations 200` after a default run resumed and was labelled concolic (finding frontend-go-12).
- Walkthrough: `demo/walkthrough.sh:120` creates one `SHATTER_ARTIFACT_DIR` for the whole run. Steps `concolic-z3` and `spec-generation` (`demo/walkthrough.yaml:64-78`) target the same TS file as step 2, and both log `[resumed] classifyNumber` (`artifact-samples/wt-step8-concolic.err`, `wt-step9-spec.err`).
- SPEC documents resume only for `scan --resume` (SPEC.md §6.3, around :1019). There is no `--no-resume` flag (`unexpected argument`), and explore resume is mentioned only inside the `--clean` help text.
- Downstream memory (`project_kapow_shatter_advise_log.md`) records the same trap: "runs silently RESUME prior results".

## Acceptance criteria

- [ ] Summary entries store an options hash. The hash covers explorer mode (random/concolic), iteration and time budgets, seeds and seed files, mocks, setup/teardown, solver settings (timeout), spec/invariant flags that change the stored observation, and the engine and frontend version fingerprint. `--no-cache` and output-only flags (`-o`, `--format`, `--spec-out`) are not in the hash; SPEC says so and names `--clean` as the way to force a fresh run. On mismatch the function is re-explored.
- [ ] The partial-resume sidecar (`PersistedExploreState`) stores the function's deep fingerprint, the options hash and the explorer mode. `read_resume_state` (or its caller) discards the sidecar when any of them differs from the current run, and discards legacy sidecars that lack the fields. Full and partial resume share one validation helper.
- [ ] A resumed result keeps its original explorer label. The report header says `(resumed from prior run; --clean to re-run)`, including under `--from-artifacts`.
- [ ] An `[info]` line says why each resume (full or partial) was accepted or rejected, naming the first differing field (fingerprint, or the first differing option).
- [ ] CLI tests in `shatter-cli/tests/` (fresh temp dir each):
  - a random run then a `--concolic` run re-explores and is labelled concolic;
  - the same options twice resumes;
  - a `--max-iterations` change re-explores;
  - partial resume after a source edit: leave a resume sidecar for a function (run with a budget that leaves it incomplete, or write a sidecar fixture with a matching path), edit the function body, re-run; the sidecar is rejected with the fingerprint reason and the result reflects the edited source;
  - partial resume after a mode change: a sidecar written by a `--concolic` run is rejected by a random run.

  At close, show the tests failing on current `main` and passing after the fix (test names plus before/after output in the close note).
- [ ] The walkthrough steps that re-target the same function use separate artifact dirs or `--clean`. The close note includes `task walkthrough` output in which steps 8/9 contain no `[resumed]` line.
- [ ] SPEC §2.1 documents explore auto-resume: what the key covers, what it does not (`--no-cache`), and how to force a fresh run.
- [ ] `task affected` passes, and its `Gates selected` output is recorded. `task --force e2e-ts` passes (CLI wiring for explorer mode changed). The task passes `--include-ignored`; a bare `cargo test --test e2e_concolic` skips the ignored cases and does not count. The close note includes the `test result:` line showing the ignored cases ran.

## Suggested approach

Add an `options_hash` (a stable hash such as SHA-256 over canonical JSON, not `DefaultHasher`; see stable-hash-persisted-keys) and an `explorer` field to `ExploreSummary` entries, and add `deep_fingerprint`, `options_hash` and `explorer` to `PersistedExploreState`. Compute the hash once from the resolved explore config. Check all of them in one helper called from `try_resume_function` and from the partial-resume loader. Treat legacy entries without the field as a mismatch (re-explore), the same way legacy summaries without fingerprints already are. Render the explorer label from the stored field. Grep for the parallel path (random `explorer.rs` vs concolic `orchestrator.rs` wiring in `main.rs`/`explore.rs`) so both write the same fields.

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
