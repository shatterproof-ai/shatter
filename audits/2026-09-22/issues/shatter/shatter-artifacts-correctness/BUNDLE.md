# Bundle: shatter-artifacts-correctness

- **Audit:** Shatter audit 2026-09-22. These are final issue drafts. Nothing has been filed. Under D6 the maintainer files them with one filer script.
- **Bucket:** shatter-artifacts-correctness: artifacts that are wrong or missing (explore resume, `-o` JSON bundles, mixed-language scan, revalidate, retiring snapshot diff, coverage metrics, file names).
- **Repo:** shatter
- **Tracker:** bd in /home/ketan/project/shatter (prefix str)
- **Parent epic for new issues:** "Epic: Audit 2026-09-22 findings"
- **Evidence base:** line numbers were re-checked against `56c86168` (branch `audit-2026-09-22`). Paths under `audits/2026-09-22/` exist only on that branch until the audit reports land.

## Maintainer decisions (2026-09-23)

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix and fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64). Do not drop them. Release work closes only with a green release-run URL. *(No draft in this bucket.)*
- **D2 shatter diff:** retire the snapshot-diff command (`shatter diff`) and the unused Snapshot writer path. spec-diff is the regression tool. Update SPEC, README and QUICKSTART. str-81xiw decides whether diff-scoped exploration takes the freed `diff` name. The shatter-agents plugin's `shatter diff --staged` docs are corrected to what exists today. *(Applied in 07, 08 and 09. 14 drops the `diff --json` contract case. 03 and 15 extend spec-diff, the remaining regression tool.)*
- **D3 Concolic positioning:** measure first. P1 default-vs-concolic benchmark and P1 concolic early-termination fix. A follow-up decision issue then decides README/SPEC positioning, with no doc softening now. *(Not in this bucket. 01 notes that the benchmark needs correct resume keying.)*
- **D4 Beads hook stall:** retire the JSONL import, move tracker sync to a Dolt remote, drop `bd sync` from AGENTS.md, and supersede str-qwua7.28. No hook-timeout env var and no bypass guidance. *(No draft in this bucket.)*
- **D5 Git identity:** the leaked `[user]` section has already been removed. Add `.mailmap`, a git-state drift check and a fixture `.git/config` snapshot test. *(No draft in this bucket.)*
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. Agents file nothing.

## Drafts

| # | Slug | Kind | P | Existing | Blocked by | Title |
|---|---|---|---|---|---|---|
| 01 | explore-resume-options-key | new | P1 | - | [] | Explore auto-resume is keyed only on the source fingerprint: --concolic, budget and seed changes silently return the prior run's results under the new mode's label |
| 02 | explore-o-json-empty-bundle | new | P1 | - | [] | `explore -o out.json` writes an empty no_targets/unclassified spec bundle after a successful run and exits 0 |
| 03 | multi-file-spec-bundle-first-only | new | P1 | - | [explore-o-json-empty-bundle] | Multi-file and glob explore write only the first file's spec bundle to -o *.json / --spec-out; a glob writes a no_targets marker |
| 04 | mixed-language-scan-deletes-artifacts | new | P1 | - | [] | Mixed-language scan: each per-language sub-scan deletes the previous one's function artifacts and overwrites summary.json; the `--resume auto` checkpoint is written to a different directory |
| 05 | revalidate-return-values | new | P1 | - | [] | `shatter revalidate` ignores return values: changed outputs on replayed inputs are reported as confirmed and exit 0 |
| 06 | revalidate-reopen-note | reopen-note | P1 | str-kab3 | [revalidate-return-values] | Reopen-note on closed str-kab3 (and str-3lob): revalidate ignores return values; see revalidate-return-values |
| 07 | retire-snapshot-diff | new | P1 | - | [] | Retire the snapshot `shatter diff` command and the unused Snapshot module; make spec-diff the documented regression tool |
| 08 | snapshot-diff-reopen-note | reopen-note | P1 | str-6k6.1 | [retire-snapshot-diff] | Reopen-note on closed str-6k6.1: only the snapshot reader shipped; snapshot `shatter diff` is being retired (D2); see retire-snapshot-diff |
| 09 | diff-name-freed-note | note-to-existing | P2 | str-81xiw | [retire-snapshot-diff] | Note on open epic str-81xiw: snapshot `shatter diff` is being retired, so the `diff` subcommand name becomes free (informational) |
| 10 | go-scan-coverage-clamp | new | P1 | - | [] | Line coverage clamps the denominator up to the covered count and the concolic scan path bypasses the instrumentable count; Go scan reported 15/15 lines for a 67-line function with 7/18 branches |
| 11 | rust-instrumentable-line-count | new | P1 | - | [] | Rust frontend never reports instrumentable_line_count: fully covered functions show ~54% line coverage (Go was fixed in str-szcn3) |
| 12 | behavior-map-cache-keys | new | P2 | - | [] | Behavior-map cache is stored under the bare function name but looked up by qualified id: unchanged re-scans never hit, and same-named functions in different files overwrite each other |
| 13 | scan-artifact-filenames-abs-path | new | P2 | - | [] | Scan artifact filenames embed the mangled absolute source path; deep checkouts hit ENAMETOOLONG and the scan still exits 0 |
| 14 | qwua7-39-json-stdout-first-run | note-to-existing | P1 | str-qwua7.39 | [] | Note on str-qwua7.39 (raise P2 -> P1): first-run `scan --format json` stdout is not JSON; widen to every JSON stdout command; implicit init can print an empty path |
| 15 | explore-spec-bundle-failed-functions | new | P2 | - | [explore-o-json-empty-bundle] | Spec bundles cannot say a function was attempted and failed: a failed explore writes the same no_targets/unclassified marker as an empty file |

Dependency edges inside the bucket: explore-o-json-empty-bundle blocks multi-file-spec-bundle-first-only and explore-spec-bundle-failed-functions (15, split from 02 after the 2026-09-23 cross-check). The reopen-notes on str-kab3/str-3lob (06) and str-6k6.1 (08) and the note on str-81xiw (09) are filed after the new issue they point to, so its real id can be substituted for the placeholder. Cross-bucket: mixed-language-scan-deletes-artifacts blocks spec-s6-layout-and-checkpoint (shatter-docs bucket). multi-file-spec-bundle-first-only defines the multi-file spec shape that spec-json-shapes-compare's shared reader (shatter-reports-and-specs bucket) must accept; `compare` support is owned there, not here. See REVISION.md for the 2026-09-23 cross-check changes.

---

<!-- file: 01-explore-resume-options-key.md -->

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

---

<!-- file: 02-explore-o-json-empty-bundle.md -->

---
slug: explore-o-json-empty-bundle
kind: new
title: "`explore -o out.json` writes an empty no_targets/unclassified spec bundle after a successful run and exits 0"
priority: P1
type: bug
labels: [cli, explore, artifacts, spec, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# `explore -o out.json` writes an empty no_targets/unclassified spec bundle after a successful run and exits 0

## Problem

A successful single-function `shatter explore <file>:<fn> -o out.json` explores every path, exits 0, and writes this to `out.json`:

```json
{"version":1,"file":"01-arithmetic.ts","functions":[],"status":"no_targets","no_target_reason":"unclassified"}
```

The file claims nothing was explored. Batch tooling that reads `-o *.json` gets a false no-target result. SPEC §2.1 (SPEC.md:160) documents `-o PATH` as "Write a report; format inferred from extension (`.html`, `.md`, `.json`, `.txt`)". SPEC also says JSON on stdout is not offered, and tells users to write `-o file.json` instead (SPEC.md:162). That makes this the documented JSON path.

The cause: per-file spec bundles are collected only when the `--spec-out` path is set. The `-o *.json` branch then finds no bundle and falls back to the str-ni32 "analyze failed" no-target marker, even though analysis and exploration succeeded.

This issue covers the single-file success case (the empty bundle). It needs no spec schema change. Two follow-ups build on it: recording attempted-but-failed functions in the bundle instead of the no-target marker (explore-spec-bundle-failed-functions, which needs a schema change), and multi-file and glob targets keeping only the first file's bundle (multi-file-spec-bundle-first-only).

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-cli/src/commands/explore.rs:6459-6480`: per-target bundles are pushed into `file_spec_bundles` only `if output_path.is_some()`, where `output_path` is the `--spec-out` destination.
- `explore.rs:6643-6682`: the `-o <path>` JSON branch logs `[warn] JSON output for explore writes spec bundle; use --spec-out for explicit spec output`, then writes `file_spec_bundles.first()`. The list is empty when `--spec-out` is not given, so it writes `build_no_target_spec_bundle(...)` with reason `unclassified` (the str-ni32 fallback, whose comment says it is for "analyze/preflight failed before any spec was produced").
- `finalize_explore` (`--from-artifacts`, `explore.rs:3773`) has two separate sinks and does not share the live-path bug. Its `-o *.json` branch (`explore.rs:4006-4021`) builds the bundle from `acc.file_specs` directly and writes **nothing** when `acc.file_specs` is empty (no marker, no warning). Its `--spec-out` branch (`explore.rs:4040-4074`) is the one that falls back to `build_no_target_spec_bundle` when `acc.file_specs` is empty. The criteria below pin both sinks so the fix does not regress them and so the empty `-o *.json` case stops being silent.
- Repro (audit verifier, release binary built from the audit HEAD, fresh dir): `shatter explore 01-arithmetic.ts:classifyNumber --clean -o b.json` → stderr `[batch 1/1] classifyNumber: 30 iters, 4 paths, 3/3 branches (ok)`, then `[warn] JSON output for explore writes spec bundle; use --spec-out…` and `[info] Wrote no-target spec marker (reason=unclassified) to b.json`, exit 0. `b.json` is the marker above. `--spec -o x.json` gives the same result. Transcripts: `audits/2026-09-22/cli-ux-transcripts/explore-bundle{2,3,4}.json`, `explore-o3.err`.
- The same marker was written for a Rust target whose build timed out (`rust-spec.json`, finding artifacts-02), so a failed run and a successful run are indistinguishable in the file.
- Existing test `shatter-cli/tests/explore_no_target_spec.rs` covers only the true no-target case.

## Acceptance criteria

- [ ] Live path: `explore <target> -o out.json` writes a bundle that contains every explored function with its equivalence classes, whether or not `--spec`/`--spec-out` is given. For `01-arithmetic.ts:classifyNumber` this is one function with 4 classes. The warn line goes away or states exactly what was written. `--spec-out` output is unchanged for this case.
- [ ] Live path: the no-target marker is written to `-o *.json` only when analysis found no target (the str-jeen.67 case). This issue does not change what is written when targets were attempted and all failed; that is explore-spec-bundle-failed-functions.
- [ ] `--from-artifacts` (`finalize_explore`): for the same artifacts, `-o out.json` and `--spec-out spec.json` each contain the same functions as the live path would write. When there are no specs, `-o out.json` is no longer silently skipped: it writes the same marker (or failure bundle, once explore-spec-bundle-failed-functions lands) as the `--spec-out` sink, and logs what it wrote.
- [ ] CLI test in `shatter-cli/tests/` (extend `explore_no_target_spec.rs` or add one) covers, for each sink separately (`-o x.json` alone, `--spec-out y.json` alone) and for both the live path and `--from-artifacts`: success, and a true no-target file. At close, show the live `-o x.json` success case failing on current `main` and passing after the fix (test name plus before/after output in the close note).
- [ ] SPEC §2.1's `-o` row says what a `.json` destination contains, which is the spec bundle schema (§5).
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Build the per-file bundle whenever any JSON sink is requested: `-o *.json`, `--spec-out` or `--spec-json`. Share one "collect bundles" step between the live path and `finalize_explore`. Decide "no target" from the per-function summaries (`report_summaries`), not from whether a bundle is empty. Keep the `.first()` behaviour here. Changing it to all files is multi-file-spec-bundle-first-only.

## Out of scope

- Multi-file/glob output keeping only the first file's bundle (multi-file-spec-bundle-first-only).
- Representing attempted-but-failed functions in the bundle, and any spec schema version bump (explore-spec-bundle-failed-functions).
- `-o out.json` suppressing the markdown on stdout, and `--format`/stdout purity (str-qwua7.11, explore-format-flag-ignored).
- Formal JSON Schemas for artifacts (artifact-json-schemas).

## Priority

P1: the documented JSON output of the primary command is wrong on success.

## Type

bug

## Dependencies

- Blocked by: none.
- Blocks: multi-file-spec-bundle-first-only (same writer), explore-spec-bundle-failed-functions (same writer).
- Related: str-ni32 (closed; added the fallback), str-jeen.67 (closed; no-target marker), str-jeen.21 (closed; no-target reason schema), str-zt4v (closed; `-o` multi-format).

## References

Audit 2026-09-22 findings cli-ux-01 and artifacts-02 (both verified P1). Source draft: `drafts/shatter-code/26-explore-json-output-bundles.md` (single-file half; split per report §14 item 8). The failed-function half was split out to explore-spec-bundle-failed-functions after the 2026-09-23 cross-check.

---

<!-- file: 03-multi-file-spec-bundle-first-only.md -->

---
slug: multi-file-spec-bundle-first-only
kind: new
title: "Multi-file and glob explore write only the first file's spec bundle to -o *.json / --spec-out; a glob writes a no_targets marker"
priority: P1
type: bug
labels: [cli, explore, artifacts, spec, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [explore-o-json-empty-bundle]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Multi-file and glob explore write only the first file's spec bundle to -o *.json / --spec-out; a glob writes a no_targets marker

## Problem

`shatter explore` accepts several files or a glob. Its JSON outputs (`-o *.json`, `--spec-out`) still write one `FileSpecBundle`, the first file's, and drop the rest. A glob over a directory writes only a `no_targets` marker for the first file, while the markdown report for the same run lists every function. spec-diff, which decision D2 makes the regression tool, consumes `--spec-out` and silently sees a fraction of the explored code.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-cli/src/commands/explore.rs:6648` (`-o <path>.json` branch) and `:6703` (`--spec-out` branch) both write `file_spec_bundles.first()`. The comment at :6704 says "Single-target is the primary Make use case; write the first bundle."
- `explore.rs:4041-4057` (`finalize_explore`, `--from-artifacts`) builds one bundle from all artifacts' specs but labels it with `artifacts.first()`'s file. Functions from different files are merged under one `file`.
- Repro (audit verifier): `shatter explore 01-arithmetic.ts 02-strings.ts -o out.json --spec-out spec.json` → stderr `Wrote spec bundle (2 function(s)) to spec.json`. `classifyString` (in 02-strings.ts) appears 0 times in either file, although artifacts were written for all 4 functions.
- Repro: `shatter explore '*.ts' -o ts-all-default.json` over the 26 TS examples → the file is `{"file":"01-arithmetic.ts","functions":[],"status":"no_targets","no_target_reason":"unclassified"}`, while the markdown for the same run has 52 functions (`audits/2026-09-22/goals-runs/ts-all-default.{json,md}`). Part of this is the empty-bundle bug (explore-o-json-empty-bundle). Once that is fixed, this path would still write only 01-arithmetic.ts.
- spec-diff's loader (`SpecInput`, `shatter-cli/src/commands/diff.rs:43-70`) accepts either one `FileSpecBundle` (detected by a top-level `functions` array) or one bare `FunctionSpec` object. It does not accept a list.
- spec-diff pairs functions by bare name: `diff_spec_collections` (`diff.rs:223-262`) builds `HashMap<&str, &FunctionSpec>` keyed on `function_name` only. Once a bundle can hold several files, two same-named functions in different files (for example `classify` in `a.ts` and in `b.ts`, the collision behind behavior-map-cache-keys) would overwrite each other in the map and be compared against the wrong counterpart.
- `compare` (`shatter-cli/src/commands/compare.rs:19-22`) reads only a bare `FunctionSpec` per side, so it does not read `--spec-out` bundles at all today. Bundle support for `compare` is owned by spec-json-shapes-compare (shatter-reports-and-specs bucket), not this issue.

## Acceptance criteria

- [ ] A multi-file or glob explore writes every explored file's functions to `-o *.json` and `--spec-out`, in one of two documented shapes: a multi-file envelope (a `files: [FileSpecBundle]` array, with the spec schema version bumped), or one bundle per source file plus a manifest. Record the choice and the reason in the issue before implementing, and post the chosen shape on spec-json-shapes-compare so its shared reader accepts it.
- [ ] Single-file runs keep writing the current single `FileSpecBundle` shape, so existing Make recipes keep working.
- [ ] `finalize_explore` (`--from-artifacts`) produces the same shape and never labels one file's functions with another file's path.
- [ ] Incremental re-explore (`merge_file_spec_bundles`) merges per file.
- [ ] spec-diff matches functions by (source file, function name) whenever either input carries file identity (a bundle or the envelope), using the bundle's `file` normalized to a project-relative path. Bare-`FunctionSpec` inputs keep name-only matching. Added/removed lists name the file.
- [ ] spec-diff tests (unit or CLI):
  - two multi-file inputs with a change in the second file only report that change and nothing else;
  - **same-name collision:** both inputs contain `classify` in `a.ts` and `classify` in `b.ts`; only `b.ts:classify` changes; the diff reports exactly one change, attributed to `b.ts`, and `a.ts:classify` is unchanged. This test must fail against a name-keyed implementation;
  - a single-file bundle diffed against a single-file bundle behaves exactly as today.
- [ ] CLI test: a two-file explore with `-o out.json` and `--spec-out spec.json` contains both files' functions (`classifyNumber`, `compareMagnitudes`, `classifyString` and the fourth). A glob over a fixture dir contains all files. At close, show the test failing on current `main` and passing after the fix.
- [ ] SPEC §2.1 and §5 describe the multi-file shape and spec-diff's matching rule.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

After explore-o-json-empty-bundle has unified bundle collection, replace the two `.first()` writes with a writer that takes all bundles. The envelope is the simpler change for consumers, because one file in gives one file out. Teach spec-diff's `SpecInput` loader a third variant for the envelope, flatten every input to `(file, FunctionSpec)` pairs, and key `diff_spec_collections` on the pair. If spec-json-shapes-compare's shared core reader lands first, extend that reader instead of `SpecInput`.

## Out of scope

- The single-file empty bundle (explore-o-json-empty-bundle).
- `compare` reading bundles or envelopes, and pairing functions across two multi-file inputs (spec-json-shapes-compare owns the shared spec reader and `compare --function`).
- The SPEC §5 producer/consumer table and generated JSON Schemas (spec-s5-contract-table-and-samples, artifact-json-schemas). Those issues should describe the shape chosen here.

## Priority

P1: the spec outputs that spec-diff (the regression tool, D2) consumes silently omit most of a multi-file run.

## Type

bug

## Dependencies

- Blocked by: explore-o-json-empty-bundle (shares the bundle writer).
- Related: spec-json-shapes-compare (shatter-reports-and-specs; shared spec reader and `compare`, must accept the shape chosen here), explore-spec-bundle-failed-functions, artifact-json-schemas, spec-s5-contract-table-and-samples, behavior-map-cache-keys (same bare-name collision class), str-zt4v (closed).

## References

Audit 2026-09-22 finding goals-04 (verified P1). Source draft: `drafts/shatter-code/26-explore-json-output-bundles.md` (multi-file half; split per report §14 item 8).

---

<!-- file: 04-mixed-language-scan-deletes-artifacts.md -->

---
slug: mixed-language-scan-deletes-artifacts
kind: new
title: "Mixed-language scan: each per-language sub-scan deletes the previous one's function artifacts and overwrites summary.json; the `--resume auto` checkpoint is written to a different directory"
priority: P1
type: bug
labels: [scan, artifacts, resume, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Mixed-language scan: each per-language sub-scan deletes the previous one's function artifacts and overwrites summary.json; the `--resume auto` checkpoint is written to a different directory

## Problem

For a directory that contains more than one language, `shatter scan` runs one sub-scan per language (str-14en). All the sub-scans write into the same `scan-results/<id>/` directory. Each sub-scan deletes `functions/` and restarts the artifact index at `00001`, so only the last language's per-function artifacts and `summary.json` survive. The report on stdout still counts every language. Resume and `--from-artifacts` read the surviving artifacts, so they disagree with the report.

Separately, when `--resume auto` creates or discovers a checkpoint, it uses `project_root/shatter-artifacts/scan-results/<first 16 hex of id>/checkpoint.json`. Every other scan output goes to `resolve_artifact_root()/scan-results/<full 64-hex id>/`, which honours `SHATTER_ARTIFACT_DIR`. One scan's state is therefore split across two directories, and the auto checkpoint ignores the configured artifact root. (A scan without `--resume` writes no checkpoint, and `--resume PATH` deliberately uses the path the user gave; neither is in question.)

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-core/src/scan_orchestrator.rs:594-610` `prepare_fresh_scan_artifact_root` runs `remove_dir_all(scan_root/functions)`.
- `scan_orchestrator.rs:3889` computes `scan_id` and `:3912` calls `prepare_fresh_scan_artifact_root` inside `parallel_scan_with_progress` (:3806). `shatter-cli/src/commands/scan.rs:1270` (str-14en) groups analyses by language and runs the parallel scan once per language, so the cleanup runs once per language.
- `scan_orchestrator.rs:557-565` `scan_root` = `resolve_artifact_root` + `scan-results/<full id>`. `scan_artifact_path` (:586) numbers artifacts from a per-sub-scan counter.
- `shatter-core/src/checkpoint.rs:197-204` `ScanCheckpoint::default_path` hard-codes `project_root.unwrap_or(".")/shatter-artifacts/scan-results/<scan_id[..16]>/checkpoint.json`, and `auto_discover` (:191-194) looks only there. Both are reached only from the `ResumeDirective::Auto` arm in `shatter-cli/src/commands/scan.rs:1192-1214`. With no `--resume`, `resolved_resume_path` is `None` and no checkpoint is written; `--resume off` disables it; `--resume PATH` uses `PATH` as given (:1215-1224). The test `default_path_structure` (`checkpoint.rs:503`) pins the auto layout.
- `scan_orchestrator.rs:3911-3913`: `prepare_fresh_scan_artifact_root` is skipped whenever `resume_path` is set, so under `--resume` the per-language passes do not delete each other's `functions/`, but they still restart the artifact index and each overwrites `summary.json`.
- Repro (`scan mix`: 3 TS + 2 Go files): `scan-mix.err` shows all 12 artifact writes going to the same `scan-results/896e5a62…/functions/` dir. Go wrote `00001..00004_*.go__*.json`, then TS wrote `00001..00008`. Afterwards only the 8 TS artifacts exist. `summary.json` says `total_functions: 8, failed: 0`, and the report says 12 discovered, 4 failed (`audits/2026-09-22/artifact-samples/scan-summary.json` vs `scan-mix.json`, `scan-mix.err`).

## Acceptance criteria

- [ ] One scan invocation has one artifact namespace, with and without `--resume`. Stale-artifact cleanup (fresh runs only) and scan-id computation run once, before the per-language loop. Artifact indexes are global across languages, and there is one `summary.json` and one `manifest.json` covering every language.
- [ ] Under `--resume auto`, the checkpoint is created and discovered under `scan_root()` (the same full id and configured artifact root, honouring `SHATTER_ARTIFACT_DIR`).
- [ ] Unchanged behaviour, pinned by tests: a scan without `--resume` writes no checkpoint; `--resume off` writes none; `--resume PATH` reads and writes exactly `PATH`, not a file under `scan_root()`.
- [ ] Legacy auto checkpoints: either `--resume auto` also looks at the legacy 16-hex location once, or the changelog says that checkpoints from older versions are not resumed. Pick one and test it.
- [ ] Integration test in `shatter-cli/tests/` on a TS + Go fixture dir, run with and without `SHATTER_ARTIFACT_DIR`:
  - fresh scan (no `--resume`): every function in the report has an artifact file and a summary entry; summary counts equal the report's counts; manifest, run-status, summary and `functions/` share one directory; no checkpoint file exists;
  - `--resume auto`, run twice: the checkpoint is in that same directory; the second run resumes functions from both languages; and after it the summary still covers both languages with counts equal to the report's.

  At close, show the fresh-scan case failing on current `main` and passing after the fix (test name plus before/after output in the close note).
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Hoist `compute_scan_id` over the full target set and the `prepare_fresh_scan_artifact_root` call out of the per-language pass in `scan.rs`. Pass an artifact-index offset (or a shared counter) and a shared summary accumulator into each pass. Replace `ScanCheckpoint::default_path`/`auto_discover` with helpers that take the resolved `scan_root`, so the auto checkpoint goes through the same storage resolution as the other artifacts. Leave the `ResumeDirective::Path` arm alone. The progress totals were already made global for mixed scans in str-4oa1. Reuse that plumbing.

## Out of scope

- SPEC §6.1/§6.2 (progress-event fields, artifact layout, v2 id derivation). That is spec-s6-layout-and-checkpoint, which is blocked by this issue and documents the layout this issue produces.
- Artifact file naming (scan-artifact-filenames-abs-path).
- Explore resume keying (explore-resume-options-key).

## Priority

P1: a mixed-language scan silently loses most of its per-function artifacts, and resume/summary disagree with the report.

## Type

bug

## Dependencies

- Blocked by: none.
- Blocks: spec-s6-layout-and-checkpoint.
- Related: str-14en (closed; introduced per-language passes), str-9f6f (closed; stale-artifact cleanup), str-4oa1 (closed; mixed-scan progress totals), str-8q1b4 (closed; resume report parity).

## References

Audit 2026-09-22 findings artifacts-04 (verified P1) and docs-07 (code half: the checkpoint location). Source draft: `drafts/shatter-code/34-mixed-language-scan-clobbers-artifacts.md`.

---

<!-- file: 05-revalidate-return-values.md -->

---
slug: revalidate-return-values
kind: new
title: "`shatter revalidate` ignores return values: changed outputs on replayed inputs are reported as confirmed and exit 0"
priority: P1
type: bug
labels: [regression, revalidate, behavior-map, cli, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# `shatter revalidate` ignores return values: changed outputs on replayed inputs are reported as confirmed and exit 0

## Problem

SPEC §2.7 (SPEC.md:459-465) says `shatter revalidate <SOURCE>` replays each cached input "and compare[s] observed against cached behavior. Exit `0` = no regressions, `1` = issues found". The verdict does not use the return value or the thrown error value. It uses only whether the branch path matched and the error *severity*. When the code has changed, a path mismatch is classified `ExpectedDrift`, and that verdict counts as confirmed for both the summary and the exit code. A function whose output changed on the same input therefore passes revalidation.

`spec-diff` on the same change does report `[CHANGED] zero -> nil`, so Shatter's two regression paths disagree. Revalidation was built and closed in str-kab3 (verdict loop) and str-3lob (CLI and CI integration) under epic str-76z6, and neither checked outputs.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-core/src/revalidation.rs:87-120` `classify_verdict(code_changed, path_matches, expected_severity, observed_severity)`. No return value or error value is passed in. Same severity with a path match gives `Confirmed`. Same severity with a path mismatch and `code_changed` gives `ExpectedDrift`.
- `shatter-cli/src/commands/revalidate.rs:143-147`: `has_issues` is set only for verdicts other than `Confirmed` and `ExpectedDrift`, so drift exits 0.
- `revalidate.rs:180-186`: the "N/M behaviors confirmed" count includes `ExpectedDrift`.
- The nondeterminism mask already exists for paths (`branch_paths_match`, `revalidation.rs:127-154`), which masks on `field_path == "branch"` or a `branch.` prefix. It is not applied to outputs because outputs are not compared.
- The mask vocabulary for outputs is fixed by the producer, `detect_within_run_nondeterminism` (`shatter-core/src/nondeterminism.rs:486-600`): `return` (the whole primitive return value), `return.<path>` (a nested field of the return value), `thrown_error` (`FIELD_PATH_THROWN_ERROR`, :490; set when the error's `error_type` or `message` varied) and `<outcome>` (`FIELD_PATH_OUTCOME`, :487; set when one execution returned and another threw). Error comparison there uses only `error_type` and `message`, never stack traces or locations.
- Repro 1 (audit verifier): explore `01-arithmetic.ts`, change `return "zero"` to `return "nil"`, run `shatter revalidate 01-arithmetic.ts`. Every behavior prints `[ok] ... (confirmed)`, the summary is `6/6 behaviors confirmed.`, and the exit code is 0. `--output-format json` shows verdict `confirmed` for input `[0]`, whose return changed from `zero` to `nil`.
- Repro 2 (`audits/2026-09-22/goals-runs/regress2/`): the same change plus swapped even/odd branches printed `[drift] classifyNumber (expected drift)` twice, then `4/4 behaviors confirmed.`, and exit 0.
- Unit tests exercise only the verdict matrix on synthetic severity/path inputs.

## Acceptance criteria

- [ ] The return value and the thrown error are part of the verdict. On a replayed input, an output change is a regression (a new verdict such as `OutputChanged`, counted as an issue for the exit code and not as confirmed) whether or not the fingerprint changed.
- [ ] Output comparison uses the existing mask vocabulary exactly as `nondeterminism.rs` writes it: a `return` field masks the whole return value; `return.<path>` masks that nested path (compared with the same path syntax `structural_similarity` produces); `thrown_error` masks the error's type and message; `<outcome>` masks a return-vs-throw flip. No new prefix (such as `error`) is introduced. Errors are compared by `error_type` and `message` only; stack traces, file paths and line numbers are never compared.
- [ ] `ExpectedDrift` is reported separately from confirmed: the summary line reads like `N confirmed, D expected drift, R regressed of M`, and JSON output carries the counts separately. Whether `ExpectedDrift` alone fails the exit code is **not** changed by this issue (it stays exit 0 when outputs match); once outputs are compared, a drifted path with a changed output is already caught as `OutputChanged`. Any later change to the drift exit policy needs a maintainer decision.
- [ ] Unit tests:
  - a changed return value with a matching path returns the regression verdict;
  - the same with a `return` mask (primitive) and with a `return.<field>` mask on the differing field returns confirmed; a `return.<other_field>` mask does not hide a change in a different field;
  - a changed error message with a `thrown_error` mask returns confirmed; without it, a regression;
  - two errors with equal `error_type`/`message` but different stack text are equal;
  - a behavior map persisted by current code (with its real `nondeterministic_fields`) round-trips through the comparison, so a mask written by the producer is honoured by the consumer.
- [ ] E2E known-answer test: explore a TS fixture, mutate one return value, run `shatter revalidate`. It exits 1 and names the function and the input. At close, show the test failing on current `main` and passing after the fix (test name plus before/after output in the close note).
- [ ] SPEC §2.7 states what is compared (path, severity, return value, error type and message), how nondeterministic masks apply, how drift is reported, and which verdicts fail the exit code.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Carry the expected `return_value`/`thrown_error` from each `Behavior` into `revalidate_behaviors` and compare them with the observed execute response after masking. Reuse the `NondeterministicField` paths exactly as `detect_within_run_nondeterminism` emits them (`return`, `return.<path>`, `thrown_error`, `<outcome>`); put the output-vs-mask comparison next to that producer in `nondeterminism.rs` so the two cannot drift apart. Keep `classify_verdict` a pure function by adding an `output_matches: bool` parameter, which keeps the existing matrix tests easy to extend.

## Out of scope

- Behavior maps written with empty `behaviors` for some functions (a symptom of the path under-count; see float-probe-paths-uncounted). Revalidate can only check the behaviors a map contains. The E2E fixture here must use a function whose map is populated.
- Behavior-map cache keying across files (behavior-map-cache-keys). That issue also adds the rule that revalidate refuses a map from a different source file.
- spec-diff false negatives (str-qwua7.38).

## Priority

P1: the command documented to catch regressions reports a changed output as confirmed and exits 0.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-kab3 and str-3lob (closed; built revalidation without output comparison; reopen-note revalidate-reopen-note points here), str-76z6 (closed epic), float-probe-paths-uncounted, behavior-map-cache-keys.

## References

Audit 2026-09-22 finding goals-02 (verified P1). Source draft: `drafts/shatter-code/39-revalidate-ignores-return-values.md`.

---

<!-- file: 06-revalidate-reopen-note.md -->

---
slug: revalidate-reopen-note
kind: reopen-note
title: "Reopen-note on closed str-kab3 (and str-3lob): revalidate ignores return values; see revalidate-return-values"
priority: P1
type: note
labels: [audit-2026-09-22]
parent_epic: ""
blocked_by: [revalidate-return-values]
existing_id: str-kab3
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Reopen-note on closed str-kab3 (and str-3lob): revalidate ignores return values; see revalidate-return-values

## Tracker action

- Add the comment below to **str-kab3** (`bd comments add str-kab3 …`), and the same comment to **str-3lob**.
- Do not reopen either issue. The fix is tracked in the new issue.
- Filing order: file revalidate-return-values first, then replace `<revalidate-return-values id>` with its real id. (`blocked_by` in the front matter records only this ordering.)

## Comment text

> Audit 2026-09-22 (finding goals-02, verified P1): the revalidation shipped here does not detect output regressions.
>
> - `shatter-core/src/revalidation.rs:87-120` `classify_verdict` takes only `code_changed`, `path_matches` and error severity. The return value and the thrown error value are never compared.
> - A path mismatch under a code change maps to `ExpectedDrift`. `shatter-cli/src/commands/revalidate.rs:143-147` and `:180-186` treat `ExpectedDrift` as confirmed for both the exit code and the "N/M behaviors confirmed" line.
> - Repro: explore `01-arithmetic.ts`, change `return "zero"` to `return "nil"`, run `shatter revalidate 01-arithmetic.ts`. The output is `6/6 behaviors confirmed.` and the exit code is 0. `--output-format json` reports verdict `confirmed` for input `[0]`. `spec-diff` on the same change reports `[CHANGED] zero -> nil`.
>
> This contradicts SPEC §2.7 ("compare observed against cached behavior. Exit 0 = no regressions"). The unit tests cover only the verdict matrix on synthetic path/severity inputs.
>
> Not reopening: the fix is tracked in **<revalidate-return-values id>** ("`shatter revalidate` ignores return values: changed outputs on replayed inputs are reported as confirmed and exit 0"). That issue adds return-value and error comparison under the existing nondeterminism mask (`return`, `return.<path>`, `thrown_error`, `<outcome>`), reports expected drift separately from confirmed behaviors, and adds an E2E known-answer test.

---

<!-- file: 07-retire-snapshot-diff.md -->

---
slug: retire-snapshot-diff
kind: new
title: "Retire the snapshot `shatter diff` command and the unused Snapshot module; make spec-diff the documented regression tool"
priority: P1
type: task
labels: [diff, spec-diff, cli, docs, regression, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Retire the snapshot `shatter diff` command and the unused Snapshot module; make spec-diff the documented regression tool

## Problem

README, QUICKSTART and SPEC present `shatter diff <SNAPSHOT> <CURRENT>` as the behaviour-regression workflow. No command writes a snapshot. `Snapshot::from_behavior_map(s)` and `Snapshot::write_to_file` have no callers outside `snapshot.rs`, and `shatter diff` rejects every JSON file that Shatter does emit. str-6k6.1 ("JSON snapshot export and comparison") was closed after shipping only the reader.

**Maintainer decision D2 (2026-09-23):** retire the snapshot-diff command and the unused Snapshot writer/reader path. `shatter spec-diff` is *the* regression tool. No snapshot producer is to be added. This issue carries out that decision and leaves nothing open.

**The `diff` name.** Removing the command frees the `diff` subcommand name. Epic str-81xiw (diff-scoped exploration) currently plans `diff-explore` because `diff` was taken. Whether diff-scoped exploration now takes the `diff` name is for str-81xiw to decide. This issue does not decide it, does not reserve the name and does not alias `diff` to anything.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-core/src/snapshot.rs` (1,042 lines): `Snapshot::from_behavior_map` (:70), `from_behavior_maps` (:79), `write_to_file` (:101), `read_from_file` (:107), `diff` (:329) and `SnapshotDiff`. It is exported by `shatter-core/src/lib.rs:78` `pub mod snapshot;`. Its only consumer outside the module is `shatter-cli/src/commands/diff.rs:3` (`read_from_file` and `snapshot::diff`). No code writes a snapshot. The only non-module hit for `from_behavior_map` is the unrelated `mock_gen::build_mock_from_behavior_map`.
- `shatter-cli/src/commands/diff.rs:11-168` `run_diff` is the snapshot command. **The same file also holds `run_spec_diff` (:170) and `diff_spec_collections` (:225), which must stay.**
- CLI wiring: `shatter-cli/src/args.rs:1432` (`Diff {` variant) with parser tests at :3316-3350, and dispatch in `shatter-cli/src/main.rs:981-997` (exit 1 on regressions).
- Behaviour (audit verifier, release binary, fresh dir; each exits 2): `shatter diff` on an explore artifact gives `missing field created_at`, on `.shatter-cache/behavior-maps/*.json` gives `missing field version`, and on a `--spec-out` bundle gives `missing field function_id`.
- Docs that present snapshot diff:
  - SPEC.md:13, :27 and :48 (overview mentions of "regression snapshots" and "snapshot diffing").
  - SPEC.md:84 (command table row `diff`).
  - SPEC.md:395-405 (§2.6 heading "Spec and snapshot comparison" and the `shatter diff` subsection).
  - SPEC.md:669-671 ("Behavior maps are serialized as snapshots for `diff`").
  - SPEC.md:864-885 (§5.5 Behavior Snapshot).
  - README.md:338 ("`shatter diff` and `shatter spec-diff`: compare current behavior against a saved baseline").
  - QUICKSTART.md:160-164 (§5 follow-on `shatter diff snapshots/shipping.json current/shipping.json`, with no way to produce either file).
  - PLAN.md:838-841 and :924.
- Doc tooling: `scripts/docs-smoke.py:24-28`, `:528` and `:590-600` (`validate_snapshot_against_struct` validates SPEC §5.5 by running `shatter diff <f> <f>`), and `scripts/test_docs_smoke.py:33`, `:43`, `:51` (the `("diff",)` entries) and `:398` (comment naming `snapshot::Snapshot`).
- CONTRIBUTING.md:130 describes docs-smoke as validating against `snapshot::Snapshot`.
- `docs/perf/inventories/rust-shatter-core.txt:2704-2722` lists the `snapshot::tests::*` tests. It is a point-in-time inventory "captured at the same commit" (`docs/perf/gate-budgets.md:82-84`), so it is historical and is not edited here.
- Unrelated names that a loose search would hit and that must stay: `SourceFileSnapshot::path` (`shatter-cli/src/commands/run.rs:1318`) and `SourceFileSnapshot::line_count` (`shatter-core/src/run_manifest.rs:126`).
- spec-diff already detects the return-value change that snapshot diff was meant to catch (verified on the goals-02 repro: `[CHANGED] zero -> nil`).

## Acceptance criteria

- [ ] The `diff` subcommand is removed from `args.rs` (variant and tests) and `main.rs` (dispatch). `shatter diff a b` fails with clap's unknown-subcommand error, and `shatter --help` does not list it. No deprecation alias or shim is added, so the name is free for str-81xiw.
- [ ] `shatter-core/src/snapshot.rs` and `pub mod snapshot` are deleted. `run_diff` and the snapshot import are removed from `shatter-cli/src/commands/diff.rs`. `run_spec_diff` and its tests are unchanged. Moving them to `spec_diff.rs` is optional.
- [ ] SPEC: §2.6 covers only `spec-diff` and `compare` and names spec-diff as the regression tool. §5.5 is removed and later sections are renumbered, or §5.5 is replaced by a pointer to the spec bundle section. The overview lines (:13, :27, :48), the command table (:84) and the behavior-map purpose list (:669-671) no longer mention snapshots or `diff`. A §8 changelog row records the removal and names spec-diff as the replacement.
- [ ] README (:338) and QUICKSTART §5 show the spec-diff workflow instead: `explore --spec-out old.json`, change the code, `explore --spec-out new.json`, `shatter spec-diff old.json new.json` (exit 1 on regression). PLAN.md references are updated or marked retired.
- [ ] `scripts/docs-smoke.py` no longer validates snapshots (code and docstrings), `scripts/test_docs_smoke.py` drops the `diff` entries and the `snapshot::Snapshot` comment, and CONTRIBUTING.md:130 no longer names `snapshot::Snapshot`. `task docs-smoke` passes (forced, not served from the checksum cache: run with the cache cleared or `--force` on the leaf, and record the output).
- [ ] This repo-wide search prints nothing except SPEC §8 changelog rows and PLAN.md lines inside text explicitly marked retired, and the close note pastes its output:

  ```
  rg -n -e 'shatter diff([^-\w]|$)' -e '\bsnapshot::(Snapshot|diff)\b' \
        -e '\bSnapshot::(read_from_file|from_behavior_maps?|write_to_file)\b' -e '\bSnapshotDiff\b' \
        --glob '!audits/**' --glob '!.beads/**' --glob '!docs/perf/inventories/**'
  ```

  The pattern deliberately does not match `SourceFileSnapshot::…`, `shatter diff-explore`, or `spec-diff`. On `56c86168` it reports hits in README.md, SPEC.md, QUICKSTART.md, PLAN.md, CONTRIBUTING.md, `scripts/docs-smoke.py`, `scripts/test_docs_smoke.py` and `shatter-cli/src/commands/diff.rs` (plus `shatter-core/src/snapshot.rs` itself), which is the expected edit list. `docs/perf/inventories/**` is excluded as a historical record.
- [ ] E2E or CLI test (`shatter-cli/tests/`) for the documented replacement workflow: explore a TS fixture with `--spec-out`, mutate a return value, re-explore, run `shatter spec-diff`. It exits 1 and names the change. If an equivalent test already exists, cite it in the close note instead.
- [ ] `task affected` passes, and its `Gates selected` output is recorded. `task parity` and `task conformance` pass, in case any capability or help listing names `diff`.

## Suggested approach

Delete in this order: CLI variant and dispatch, `run_diff`, `snapshot.rs`, docs-smoke hooks, then docs. Build after each step so the compiler finds any remaining users. Check `shatter-cli/src/commands/telemetry.rs` and help-text snapshot tests for a hard-coded `diff` subcommand name. Coordinate the plugin side with shatter-agents withdraw-shatter-diff-skill, which corrects the plugin's `shatter diff --staged` docs to what exists today. Both can land independently.

## Out of scope

- Adding a snapshot producer (`--snapshot-out`). D2 rejected this option.
- Deciding whether diff-scoped exploration (str-81xiw) is named `diff` or `diff-explore`. See the note diff-name-freed-note on str-81xiw.
- spec-diff false negatives (str-qwua7.38) and spec JSON shape work (spec-json-shapes-compare). Those improve the remaining tool.
- The SPEC §5 producer/consumer table (spec-s5-contract-table-and-samples), which must not describe snapshot diff.

## Priority

P1: the documented regression workflow cannot be used, and D2 fixes it by removal.

## Type

task (removal plus doc correction)

## Dependencies

- Blocked by: none.
- Related: str-6k6.1 (closed; shipped only the reader. See snapshot-diff-reopen-note), str-81xiw (open epic. See diff-name-freed-note), str-qwua7.9.2 (closed; fixed the §5.5 sample that is now removed), withdraw-shatter-diff-skill (shatter-agents), spec-s5-contract-table-and-samples, str-qwua7.38.

## References

Audit 2026-09-22 findings goals-01 (P1), docs-02 (P1), artifacts-12. Maintainer decision D2 (2026-09-23). Source draft: `drafts/shatter-code/38-snapshot-producer-for-diff.md`, rewritten: option (a) is deleted and option (b) is decided.

---

<!-- file: 08-snapshot-diff-reopen-note.md -->

---
slug: snapshot-diff-reopen-note
kind: reopen-note
title: "Reopen-note on closed str-6k6.1: only the snapshot reader shipped; snapshot `shatter diff` is being retired (D2); see retire-snapshot-diff"
priority: P1
type: note
labels: [audit-2026-09-22]
parent_epic: ""
blocked_by: [retire-snapshot-diff]
existing_id: str-6k6.1
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Reopen-note on closed str-6k6.1: only the snapshot reader shipped; snapshot `shatter diff` is being retired (D2); see retire-snapshot-diff

## Tracker action

- Add the comment below to **str-6k6.1** (`bd comments add str-6k6.1 …`).
- Do not reopen it. Under D2 the missing producer will not be built.
- Filing order: file retire-snapshot-diff first, then replace `<retire-snapshot-diff id>` with its real id.

## Comment text

> Audit 2026-09-22 (findings goals-01, artifacts-12, docs-02): this issue ("JSON snapshot export and comparison") closed with reason "Closed" after shipping only the comparison half.
>
> - The reader and differ exist: `Snapshot::read_from_file` and `snapshot::diff`, used by `shatter diff` (`shatter-cli/src/commands/diff.rs`).
> - The export half never shipped. `Snapshot::from_behavior_map(s)` and `write_to_file` (`shatter-core/src/snapshot.rs:70-110`) have no callers outside the module, and no command or flag writes a snapshot.
> - `shatter diff` exits 2 on every JSON Shatter emits: `missing field created_at` (explore artifact), `missing field version` (behavior-map cache) and `missing field function_id` (`--spec-out` bundle). README, QUICKSTART §5 and SPEC §2.6/§5.5 still document the workflow.
>
> Maintainer decision D2 (2026-09-23): the snapshot `shatter diff` command and the unused Snapshot module are being retired, and `shatter spec-diff` is the regression tool. No snapshot producer will be added. The removal and the doc changes are tracked in **<retire-snapshot-diff id>** ("Retire the snapshot `shatter diff` command and the unused Snapshot module; make spec-diff the documented regression tool"). Not reopening this issue.

---

<!-- file: 09-diff-name-freed-note.md -->

---
slug: diff-name-freed-note
kind: note-to-existing
title: "Note on open epic str-81xiw: snapshot `shatter diff` is being retired, so the `diff` subcommand name becomes free (informational)"
priority: P2
type: note
labels: [audit-2026-09-22]
parent_epic: ""
blocked_by: [retire-snapshot-diff]
existing_id: str-81xiw
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on open epic str-81xiw: snapshot `shatter diff` is being retired, so the `diff` subcommand name becomes free (informational)

## Tracker action

- Add the comment below to **str-81xiw** (`bd comments add str-81xiw …`).
- Change nothing else: not the priority, the scope, the children (str-81xiw.2/.3/.4) or the planned command name.
- Filing order: file retire-snapshot-diff first, then replace `<retire-snapshot-diff id>` with its real id.

## Comment text

> Informational, from audit 2026-09-22 and maintainer decision D2 (2026-09-23).
>
> This epic's design notes say `shatter-cli/src/args.rs` "already defines `shatter diff <snapshot> <current>` for snapshot comparison. Do not repurpose that command incompatibly", and they use `shatter diff-explore` as the v1 name. That constraint is going away. Under D2 the snapshot `shatter diff` command and its Snapshot module are being removed, and `shatter spec-diff` becomes the regression tool, in **<retire-snapshot-diff id>**. That issue adds no alias or shim, so once it lands the `diff` subcommand name is unused.
>
> Whether diff-scoped exploration takes the `diff` name or keeps `diff-explore` is for this epic to decide. D2 and the retirement issue deliberately do not decide it. Nothing in this epic's scope changes. If the epic does choose `diff`, note that the shatter-agents plugin's `shatter-diff` skill currently documents a nonexistent `shatter diff --staged` (being withdrawn in shatter-agents withdraw-shatter-diff-skill). Any future plugin guidance should follow whatever name this epic picks.

---

<!-- file: 10-go-scan-coverage-clamp.md -->

---
slug: go-scan-coverage-clamp
kind: new
title: "Line coverage clamps the denominator up to the covered count and the concolic scan path bypasses the instrumentable count; Go scan reported 15/15 lines for a 67-line function with 7/18 branches"
priority: P1
type: bug
labels: [coverage, go, scan, parity, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Line coverage clamps the denominator up to the covered count and the concolic scan path bypasses the instrumentable count; Go scan reported 15/15 lines for a 67-line function with 7/18 branches

## Problem

Downstream coverage goals (for example the zolem and pickpackit ≥90% goals) are measured on `lines_covered / total_lines`. For Go under `scan`, that metric is inflated. Functions whose loop bodies and error returns never ran report 100% line coverage. The frontend sends an undersized instrumentable-line denominator, and `reconcile_line_coverage` hides it by raising the denominator to whatever was covered (`.max(covered)`, added by str-uabz). A metric that cannot show a gap makes the coverage goals unfalsifiable.

This issue covers the Go inflation, the silent clamp and a cross-language known-answer coverage test. The Rust frontend's missing `instrumentable_line_count` (the deflation side) is rust-instrumentable-line-count.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-core/src/observe.rs:128-147` `reconcile_line_coverage`: `denom = instrumentable_line_count.unwrap_or(span).max(covered)`. Unit tests `reconcile_bumps_denominator_when_observed_exceeds_instrumentable` (:869) and `reconcile_invariant_holds_for_zolem_style_overcount` (:881) pin the clamp as intended behaviour.
- Callers: `reconcile_observation_coverage` (:153) from `shatter-core/src/explorer.rs:1969`, `:2640` and `shatter-core/src/pipeline_orchestrator.rs:553`. `explorer.rs:1686-1690` and `:2612-2613` derive `total_lines` from `instrumentable_line_count`, falling back to the span.
- Parallel-path divergence: the concolic scan path overwrites the denominator with the raw span, `result.total_lines = analysis.end_line.saturating_sub(analysis.start_line) + 1` (`shatter-core/src/scan_orchestrator.rs:3129`). The random and concolic scan paths therefore compute coverage differently.
- Go instrumentable count comes from `shatter-go/protocol/handler.go:698-714` (`MaterializeInstrumentedDirectory` → `resp.InstrumentableLineCount`, str-szcn3). The E2E `e2e_go_instrumentable_line_count_matches_probed_lines` (`shatter-core/tests/e2e_concolic_go.rs:354`) covers `explore`, not `scan`.
- Observed (zolem `internal/fixture` default scan, `audits/2026-09-22/goals-runs/zolem-fixture-default.json`):
  - `(*Loader).Load` (loader.go:87-153, 67 lines): branches 7/18, `lines_covered 15 / total_lines 15`. Executed lines were 88, 89, 93-95, 99-101, 104, 108-110, 138, 145 and 152, so the loop body (111-136) and every error return never ran.
  - `(*fixturesYAMLSelector).Select`: 2/10 branches, 5/5 lines.
  - `(*SequenceCounters).Step`: 2/6 branches, 9/9 lines.
  - `(*wasmSelector).Select`: `lines_covered 3 / total_lines 0`. The clamp would have produced 3/3, so at least one scan path writes `total_lines` without going through `reconcile_*`.
- The verifier did not re-run the zolem scan and relied on the reviewer's artifact. Reproduce first (see acceptance).

## Acceptance criteria

- [ ] Root cause of the undersized Go denominator under `scan` is found and stated in the issue before the fix. Likely places: the instrumentable count may be computed for the wrong function or file in the scan-path Instrument request, it may be cached across functions, or the default and concolic scan paths may take different denominators. Include a reproduction on a checked-in Go fixture shaped like `(*Loader).Load` (a method with a loop and early error returns).
- [ ] The denominator is correct for Go under both `explore` and `scan`, and under both explorer modes. `scan_orchestrator.rs:3129` no longer bypasses the instrumentable count.
- [ ] `reconcile_line_coverage` no longer silently raises the denominator. When `covered > instrumentable`, it logs a `warn` naming the function and both numbers, and falls back to the span. When `total_lines == 0` with `lines_covered > 0`, it is treated the same way. The two unit tests that pin the clamp are rewritten to assert the new behaviour.
- [ ] Cross-language known-answer coverage test (new, in `shatter-core/tests/` or the E2E suites): the same small function in TS and Go, one fully covered and one half covered, run through both `explore` and `scan`. It asserts 100% lines for the fully covered function and <100% for the half-covered one. A Rust leg is added by rust-instrumentable-line-count. If that issue lands first, it creates this test with TS + Rust legs, and this issue adds Go. At close, show the Go `scan` leg failing on current `main` and passing after the fix. The test must live where an existing gate runs it (one of the three E2E suites run by `task e2e`, or a suite `task check` runs); a new standalone test file must be wired into a task in the same change.
- [ ] A conformance case asserts that `instrumentable_line_count` is present on Instrument responses for every frontend that `protocol/parity-matrix.yaml` marks supported.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.
- [ ] `task --force e2e-go` and `task --force e2e-ts` pass. These tasks build the frontends, set `SHATTER_EXAMPLES_DIR`/`SHATTER_GO_FRONTEND_BIN` and pass `--include-ignored`; a bare `cargo test --test e2e_concolic_go` skips the ignored tests, including `e2e_go_instrumentable_line_count_matches_probed_lines` (`#[ignore]` at `e2e_concolic_go.rs:353`), and does not count. The close note shows `test e2e_go_instrumentable_line_count_matches_probed_lines ... ok` and the new cross-language test's `... ok` line(s) from the run. Any new E2E test that is `#[ignore]`d must still run under these tasks.

## Suggested approach

Start by diffing the Instrument request/response for `(*Loader).Load` between `explore` and `scan` (log both at debug). Then trace which `total_lines` value each scan path writes into the summary. Fix the Go side, remove the clamp in the same change, and remove the span override at :3129 so every path goes through `reconcile_observation_coverage`.

## Out of scope

- Rust frontend `instrumentable_line_count` (rust-instrumentable-line-count).
- Adapter-owned executions that return empty coverage (str-j49xg).
- The branch metric counting sites instead of arms (branch-metric-counts-sites).

## Priority

P1: the coverage metric that downstream goals are measured on overstates Go coverage by a wide margin, and the clamp hides the error.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: rust-instrumentable-line-count (shares the cross-language test), str-szcn3 (closed; Go instrumentable count), str-uabz (closed; added the clamp), str-hbky (closed; span denominator), str-j49xg (open epic), branch-metric-counts-sites.

## References

Audit 2026-09-22 finding goals-06 (verified P1), Go half. Source draft: `drafts/shatter-code/78-line-coverage-metric-consistency.md` (split per report §14 item 11).

---

<!-- file: 11-rust-instrumentable-line-count.md -->

---
slug: rust-instrumentable-line-count
kind: new
title: "Rust frontend never reports instrumentable_line_count: fully covered functions show ~54% line coverage (Go was fixed in str-szcn3)"
priority: P1
type: bug
labels: [coverage, rust-frontend, parity, protocol, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust frontend never reports instrumentable_line_count: fully covered functions show ~54% line coverage (Go was fixed in str-szcn3)

## Problem

The core computes line coverage as lines executed / `instrumentable_line_count`. When a frontend omits that field, the core falls back to the function's source span, which counts blank lines, comments, braces and signature lines. TypeScript and Go send the count (Go since str-szcn3). shatter-rust never does. Every Rust function therefore under-reports line coverage. `classify_number` has all four outcomes and all branches covered, yet it reports 54%. Downstream Rust coverage goals are measured on this number.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-rust/src/protocol.rs:734` declares `instrumentable_line_count: Option<u32>`. Every response constructor sets it to `None`: :814, :1233, :1268, :1303, :1338, :1555 and :1590.
- The Rust instrumentor already inserts one probe per real source line: `shatter-rust/src/instrument.rs:230` `line_hit_stmt(line)` → `shatter_rust_runtime::line_hit(#line)`, used at :260, :289 and :317. The count is available but is not collected.
- The numerator is wider than the `line_hit` lines. The runtime reports `lines_executed` as the union of `line_hit` lines and the `line` of every recorded branch decision: `merge_lines_executed` (`shatter-rust-runtime/src/lib.rs:291-309`, used at :333) adds each `BranchDecision.line > 0`. Branch decisions come from `branch_hit` probes whose `line` can be a line with no `line_hit` probe, notably match-arm pattern lines (`instrument.rs:420-421`, `line_of(&arm.pat)`) and `if let` condition lines (`instrument.rs:335-348`). A denominator built from `line_hit` lines alone can therefore be smaller than the numerator, which is exactly the overcount that the core's `.max(covered)` clamp hides (go-scan-coverage-clamp).
- `instrumentable_line_count` is an Instrument-response field in the core protocol (`shatter-core/src/protocol.rs:579-589`, `Response::Instrument`). shatter-rust's response is one flat struct (`shatter-rust/src/protocol.rs:725-735`), which is why every constructor, including non-Instrument ones, spells the field out as `None`.
- `protocol/parity-matrix.yaml:854-874` (`instrumentable_line_count`) says TS tracks `instrumentableLines: Set<number>`, Go threads `instrumentableLines map[int]struct{}` (str-szcn3), and "Rust does not populate this field yet", with `rust: not_supported`.
- Core fallback: `shatter-core/src/observe.rs:137-145` (span when `None`) and `shatter-core/src/explorer.rs:1686-1687`.
- Observed: `shatter explore 01_arithmetic.rs:classify_number` (the audit's standalone fixture, `audits/2026-09-22/goals-runs/standalone/rust/01_arithmetic.rs`, function at lines 6-18) gives `4 path(s) · 54% coverage (7/13 lines)` with 3/3 branches (`audits/2026-09-22/goals-runs/rust-walk.md:9`). The verifier's re-run timed out under load average 92, so reproduce on an idle machine.
- No open tracker issue covers the Rust gap. str-j49xg is about adapter-owned executions returning empty coverage, which is a different problem.

## Acceptance criteria

- [ ] shatter-rust computes `instrumentable_line_count` as the number of distinct real source lines (line > 0) that **any** probe for the instrumented function can report as executed: every `line_hit` line and every `branch_hit` line (if/else, `if let`, match-arm pattern lines, loop heads). The invariant is "the set of lines that can appear in `lines_executed` is a subset of the counted set", so a fully covered function reports exactly 100% and never more. Only the Instrument response populates the field; no field is added to Execute responses, and the other response constructors keep `None`.
- [ ] `protocol/parity-matrix.yaml` marks `instrumentable_line_count` as `rust: supported` with a note, `shatter-rust/CLAUDE.md` is updated, and `task parity` and `task conformance` pass.
- [ ] A Rust E2E test, modelled on `e2e_go_instrumentable_line_count_matches_probed_lines` (`shatter-core/tests/e2e_concolic_go.rs:354`), is added to `shatter-core/tests/e2e_concolic_rust.rs`. A fully covered function reports 100% lines, and the reported count equals the number of probed lines. A second case uses a function with a `match` whose arm patterns sit on their own lines (a multi-line match, arms not on the same line as their bodies) and asserts, over all explored inputs, that every line in `lines_executed` is counted and full coverage is exactly 100%. At close, show both failing on current `main` and passing after the fix.
- [ ] Run with `task --force e2e-rust` (it builds the frontend, sets `SHATTER_EXAMPLES_DIR` and passes `--include-ignored`; a bare `cargo test --test e2e_concolic_rust` skips the `#[ignore]`d tests and does not count). The close note shows the `... ok` lines for the new tests from that run.
- [ ] The Rust leg of the cross-language coverage test from go-scan-coverage-clamp is added. If this issue lands first, create that test with TS + Rust legs.
- [ ] Unit tests in `shatter-rust/src/instrument.rs` for the count: a function with blank lines, comments and a multi-line expression; and a multi-line `match` whose arm-pattern lines have no `line_hit` probe (the count includes them). A property test (proptest) over generated small functions asserts that the set of probe lines emitted into the instrumented source equals the counted set.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Have the instrumentor visitor collect one `BTreeSet<u32>` of the `line` values passed to both `line_hit_stmt` and every `branch_hit`/`branch_hit_stmt` constructor, excluding zero lines. Return the set's length with the instrumented source, and thread it into the response constructors in `protocol.rs`. Follow the Go change in str-szcn3 as the template. Read `shatter-rust/CLAUDE.md` first for the per-crate parity and invocation-model rules.

## Out of scope

- The Go scan inflation and the core clamp (go-scan-coverage-clamp).
- Cross-crate and opaque-type analysis gaps in shatter-rust.

## Priority

P1: covered by verified P1 finding goals-06, and every Rust coverage number is wrong. prior-20 alone rated it P2.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: go-scan-coverage-clamp (shared cross-language test), str-szcn3 (closed; the Go fix to mirror), str-hbky (closed), str-j49xg (open; different cause).

## References

Audit 2026-09-22 findings goals-06 (verified P1, Rust half) and prior-20 (verified P2). Source draft: `drafts/shatter-code/78-line-coverage-metric-consistency.md` (split per report §14 item 11).

---

<!-- file: 12-behavior-map-cache-keys.md -->

---
slug: behavior-map-cache-keys
kind: new
title: "Behavior-map cache is stored under the bare function name but looked up by qualified id: unchanged re-scans never hit, and same-named functions in different files overwrite each other"
priority: P2
type: bug
labels: [cache, behavior-map, scan, revalidate, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Behavior-map cache is stored under the bare function name but looked up by qualified id: unchanged re-scans never hit, and same-named functions in different files overwrite each other

## Problem

Scan looks up the behavior-map cache by the qualified function id (file path + name, since str-fuhw). It stores maps with `cache.store(&behavior_map)`, which keys on `BehaviorMap.function_id` (the bare name) and records no fingerprint. The two keys never meet, so the incremental "skip unchanged functions" feature never triggers: an unchanged re-scan re-explores everything. Because the stored key is the bare name, `a.ts:classify` and `b.ts:classify` share one `classify.json`. The last writer wins, and `revalidate` or mocking can load another file's map.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- Lookup by qualified id: `shatter-core/src/scan_orchestrator.rs:4141-4142` (`cache.is_fresh(func_name, dfp)` / `cache.load(func_name)`). The same pattern appears at :1600-1601 (the non-progress scan path) and in the callee prefetch at :4280 (str-fuhw comment at :4270-4275).
- Store by bare name without fingerprint: `cache.store(&func_result.behavior_map)` at `scan_orchestrator.rs:3360` and `cache.store(&result.behavior_map)` at `:4931` (also `:1840`).
- `shatter-core/src/cache.rs:123` `store(&self, map)` keys on `map.function_id`. `store_with_fingerprint` (:134) exists, and its doc comment says scan should use it. `path_for(function_id)` (:291) maps the id directly to `<cache>/behavior-maps/<id>.json`.
- Repro (`audits/2026-09-22/cli-ux-transcripts/scan-cache-1.*`, `scan-cache-2.*`): two scans of an unchanged directory both report `0 expected skipped`. A March sample report showed `36 skipped (fingerprint match)`, so this used to work.
- Repro (`audits/2026-09-22/goals-runs/collide/`): exploring `a.ts:classify` and `b.ts:classify` together produces one `.shatter-cache/behavior-maps/classify.json` with `function_id: "classify"`. The `.inputs.json` sidecars are file-scoped (`a.ts/classify.inputs.json`, `b.ts/classify.inputs.json`). Also, `01-arithmetic.ts`, `arithmetic-v1.ts` and `arithmetic-v2.ts` all write the same `classifyNumber.json`.

## Acceptance criteria

- [ ] Store and load use the same key: the qualified id made project-relative (relative source path + function name), never an absolute path. Store records the deep fingerprint (`store_with_fingerprint`) on every scan and explore path. Grep every `cache.store(` call site.
- [ ] Old cache entries (bare-name files without fingerprints) are ignored and never served as a hit. They may be deleted lazily. There is no silent migration that could pair a map with the wrong file.
- [ ] CLI test: scan a fixture dir twice with no changes. The second run reports `expected_skipped == n` (all functions). At close, show the test failing on current `main` and passing after the fix.
- [ ] Test: two same-named functions in different files keep separate maps with their own return values.
- [ ] Stored maps record their source file (add the field if `BehaviorMap` lacks it). `revalidate` refuses, with exit 2 and a clear message, any map whose recorded source file differs from its `<SOURCE>` argument. `BehaviorMap` (`shatter-core/src/behavior.rs:214`) has no schema version today, so the field is added as `#[serde(default)] Option<String>`; a map without it is a legacy map and is handled by the legacy rule above (never served as a hit). BehaviorMap is core-internal cache state, not a frontend protocol type, so no parity-matrix change applies; say so in the close note.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Make `BehaviorMapCache` take an explicit key (`store(key, map, fingerprint)`) instead of reading `map.function_id`. Derive the key in one helper shared by scan, explore and revalidate. Encode the key into a bounded, filesystem-safe file name (relative path segments + short hash). The same naming rule is needed in scan-artifact-filenames-abs-path, so share the helper if both are in flight.

## Out of scope

- Scan's `--seed` not being part of cache freshness (str-9m9o3).
- Factoring the restore-or-skip logic into a helper (str-4ajhz). It can follow this fix.
- Explore resume keying (explore-resume-options-key).

## Priority

P2: incremental scan is silently disabled, and cross-file map collisions can mislead mocking and revalidate.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-fuhw (closed; qualified lookup ids), str-bo4z.11 (closed; explore writes fingerprinted maps), str-9m9o3 (open), str-4ajhz (open), scan-artifact-filenames-abs-path, revalidate-return-values.

## References

Audit 2026-09-22 findings cli-ux-12 (partially verified P2) and goals-14 (verified P2). Source draft: `drafts/shatter-code/32-behavior-map-cache-keys.md`.

---

<!-- file: 13-scan-artifact-filenames-abs-path.md -->

---
slug: scan-artifact-filenames-abs-path
kind: new
title: "Scan artifact filenames embed the mangled absolute source path; deep checkouts hit ENAMETOOLONG and the scan still exits 0"
priority: P2
type: bug
labels: [scan, artifacts, resume, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Scan artifact filenames embed the mangled absolute source path; deep checkouts hit ENAMETOOLONG and the scan still exits 0

## Problem

Each per-function scan artifact is named `{index:05}_{sanitized qualified name}.json`. The qualified name contains the absolute path of the source file. In a deep checkout (CI workspaces, worktrees under long home paths, temp dirs) the file name exceeds the 255-byte filename limit and the write fails. The failure is only `log::warn!`ed. The scan exits 0 and reports success, but the artifacts that `--resume` and `--from-artifacts` depend on are missing. The names also change when the same project is checked out at a different path, so artifacts are not portable between machines.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-core/src/scan_orchestrator.rs:586-592` `scan_artifact_path` formats `"{:05}_{}.json"` with `sanitize_artifact_component(function_name)`, where `function_name` is the qualified (absolute-path) id.
- `scan_orchestrator.rs:612-640` `write_scan_artifact_json` returns after `log::warn!` on every failure (create dir, serialize, temp write, rename), and nothing is counted as an error.
- Example name from a real run (`audits/2026-09-22/cli-ux-transcripts/ts-scan.err`): `00001_tmp_claude-1000_-home-ketan-project-shatter_<uuid>_scratchpad_proj_01-arithmetic.ts__classifyNumber.json`.
- Repro (`audits/2026-09-22/cli-ux-transcripts/scan-deep.{err,out}`): a source path about 230 characters deep gives `[warn] failed to write scan artifact temp file for …::f: File name too long (os error 36)`, and the scan exits 0.

## Acceptance criteria

- [ ] Artifact names are built from the project-relative source path, with directory structure preserved as subdirectories or as bounded segments, plus the function name and a short stable hash. Every file-name component is at most 255 bytes, whatever the checkout depth. The same project at two different absolute paths produces identical artifact names.
- [ ] A failure to write an artifact counts as a scan error. It appears in the error count and the report, and the exit code follows SPEC §2.11.
- [ ] Test: scan a fixture whose absolute path is more than 255 characters (create it under a temp dir). All artifacts are written, and the scan's error count is 0. A second test makes the artifact dir unwritable and asserts a non-zero error count and exit code. At close, show the long-path test failing on current `main` and passing after the fix.
- [ ] `--resume` and `--from-artifacts` read the new names. Old-name artifacts are either still readable for one release or rejected with a message telling the user to re-scan (state which in SPEC §8).
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Compute a relative id once, the same key that behavior-map-cache-keys needs, and derive the file name from it: `functions/<rel dir>/<index>_<fn>_<hash8>.json`, truncating long segments. Change `write_scan_artifact_json` to return `Result` and propagate it into the per-function outcome.

## Out of scope

- Mixed-language sub-scans deleting each other's artifacts (mixed-language-scan-deletes-artifacts).
- Explore artifact naming (`persist_root/<file>/<line>_<fn>`), which already uses the file path as a directory component. Check it for the same length issue, but fix it here only if it is trivially the same helper.

## Priority

P2: silent data loss in deep checkouts, and the exit status hides it.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: mixed-language-scan-deletes-artifacts (same artifact dir; do not conflict), behavior-map-cache-keys (shared relative-key helper), str-8q1b4 (closed; resume parity).

## References

Audit 2026-09-22 finding cli-ux-07 (verified P2). Source draft: `drafts/shatter-code/29-scan-artifact-filenames-abs-path.md`.

---

<!-- file: 14-qwua7-39-json-stdout-first-run.md -->

---
slug: qwua7-39-json-stdout-first-run
kind: note-to-existing
title: "Note on str-qwua7.39 (raise P2 -> P1): first-run `scan --format json` stdout is not JSON; widen to every JSON stdout command; implicit init can print an empty path"
priority: P1
type: note
labels: [audit-2026-09-22]
parent_epic: ""
blocked_by: []
existing_id: str-qwua7.39
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.39 (raise P2 -> P1): first-run `scan --format json` stdout is not JSON; widen to every JSON stdout command; implicit init can print an empty path

## Tracker action

- Add the comment below to **str-qwua7.39** (`bd comments add str-qwua7.39 …`).
- Raise its priority from P2 to **P1** (`bd update str-qwua7.39 --priority 1`). Reason: cli-ux-04 was verified at P1, and report §14 item 22 calls for it.
- Do not create a new issue. This note also covers audit finding docs-15, which duplicates the open str-qwua7.58/.39. No separate note goes on str-qwua7.58.

## Comment text

> Audit 2026-09-22 (findings cli-ux-04, verified P1; frontend-go-12, init-path part; docs-15) adds evidence and widens scope. Raising to P1.
>
> **1. This breaks JSON contracts, not only the look of explore.** In a fresh directory, `shatter scan . --format json` writes the following to **stdout** before the `{`:
>
> ```
>   Created  .shatter/
>   Created  .shatter/config.yaml  (detected language: unknown)
>   Created  .gitignore …
> Initialized Shatter project at …
> ```
>
> `json.load` then fails with `Expecting value: line 1 column 3` (`audits/2026-09-22/cli-ux-transcripts/scan-json.out`, on branch `audit-2026-09-22`). `shatter explore c.ts:classifyNumber > report.md` has the same problem: the report file starts with the init lines (docs-15).
>
> **2. Code facts (re-checked on `56c86168`).** `shatter-cli/src/commands/init.rs:82`, `:91` and `:143` use `println!`. Implicit init goes through `maybe_implicit_init` (`shatter-cli/src/main.rs:53`, which itself prints `No .shatter/ found — initializing project` to stderr) into `run_implicit_init` (`init.rs:42`). Today it is called only from explore (`main.rs:318`) and scan (`main.rs:648`), so fixing `run_init_impl` covers both. An earlier audit draft said list-targets and spec-diff also trigger it. That is not true at this commit. They still need fresh-directory JSON contract tests (below) so that a future implicit-init call site cannot regress them.
>
> **3. Language detection says `unknown`** for pure-Go and pure-Rust directories as well as bare TS files (`cli-ux-transcripts/go-explore.out`, `rust-explore.out`). `detect_language` (`init.rs:150`) looks only for `package.json`/`go.mod`/`Cargo.toml` in the resolved directory. Detect from the target file extension(s) first.
>
> **4. The printed path can be empty.** `init.rs:143` prints `resolved_dir.display()` from the directory passed in, unchanged. For a bare filename target the parent is `Some("")`, so implicit init has printed `Initialized Shatter project at ` with nothing after it (frontend-go-12). Print the canonicalized absolute path.
>
> **Extra acceptance criteria for this issue:**
>
> - [ ] Implicit-init status lines go to stderr at info level. (Explicit `shatter init` may keep stdout. Decide and document in SPEC §2.8, as the issue already says.)
> - [ ] The language is detected from the target files, with directory markers as the tiebreak. A pure-Go or pure-Rust target never prints `unknown`.
> - [ ] The printed project path is absolute and never empty.
> - [ ] `shatter-cli/tests/json_stdout_contract.rs` gains fresh-directory (not yet initialized) cases, each asserting that the whole of stdout parses as JSON: `scan --format json` (the implicit-init case), and as guards for the other JSON-on-stdout commands, `list-targets --format json`, `spec-diff --json`, `compare --json` and `revalidate --output-format json`. `specify --json` (needs an observation file from `shatter observe`, a pipeline-stage command) and `discover-deps --json` (Linux-only strace diagnostic) are intentionally not covered here; say so in the test file. At close, show the `scan --format json` case failing on current `main` and passing after the fix. The guard cases already pass.
> - [ ] A fresh-directory `explore <file>:<fn>` (markdown on stdout) case asserts that stdout contains none of the init status lines (`Created`, `Initialized Shatter project`). This covers docs-15 and does not depend on str-qwua7.11.
> - [ ] Not in this issue: a whole-stdout-is-JSON case for explore `--spec-json`. That belongs to str-qwua7.11 (open, P1), which makes `--spec-json` stdout-exclusive; its own acceptance should add the fresh-directory case. Snapshot `shatter diff --json` is being removed (retire-snapshot-diff, D2), so there is no case for it.
> - [ ] `task affected` passes, and its `Gates selected` output is recorded.
>
> Related: str-qwua7.58 (keep implicit init; document it; `--no-init`), str-qwua7.11 (`--spec-json` stdout exclusivity; both must hold for stdout to be clean).

---

<!-- file: 15-explore-spec-bundle-failed-functions.md -->

---
slug: explore-spec-bundle-failed-functions
kind: new
title: "Spec bundles cannot say a function was attempted and failed: a failed explore writes the same no_targets/unclassified marker as an empty file"
priority: P2
type: bug
labels: [explore, artifacts, spec, schema, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [explore-o-json-empty-bundle]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Spec bundles cannot say a function was attempted and failed: a failed explore writes the same no_targets/unclassified marker as an empty file

## Problem

When `shatter explore` attempts a function and exploration fails (analysis or preflight failure, harness build failure or build timeout, harness crash, exploration timeout), the JSON spec outputs (`-o *.json`, `--spec-out`) write the str-ni32/str-jeen.67 no-target marker:

```json
{"version":1,"file":"…","functions":[],"status":"no_targets","no_target_reason":"unclassified"}
```

That marker means "this file has nothing to explore". Batch tooling and spec-diff (the regression tool, decision D2) therefore cannot tell "nothing to explore" from "exploration broke". The process exit code does distinguish them (`decide_explore_exit_status`, str-960w/str-ni32), but the file on disk does not, and the file is what CI keeps.

The spec bundle schema has no way to express a failure. `FileSpecBundleStatus` (`shatter-core/src/spec.rs:227-232`) has only `Ok` and `NoTargets`, and `FunctionSpec` (`spec.rs:304-328`) has no execution-status field. This issue defines that representation, versions it and updates consumers.

This was split out of explore-o-json-empty-bundle, which fixes the success case without a schema change.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-core/src/spec.rs:227-232` `FileSpecBundleStatus { Ok, NoTargets }`. `spec.rs:251` `SPEC_SCHEMA_VERSION = 1`, with a bump policy (`spec.rs:234-250`) that requires a bump for any added field.
- `shatter-core/src/spec.rs:304-328` `FunctionSpec`: `function_name`, `location`, `classes`, `iterations`, `lines_covered`, `total_lines`, `invariants`, `fingerprint`, `nondeterministic_fields`. No status.
- `shatter-cli/src/commands/explore.rs:744-780` `decide_explore_exit_status` already classifies per-target outcomes from `ExploreSummary` buckets: `completed`, `build_failed`, `runtime_failed`, `timed_out`, and a `parser_failure…` status for analyze/preflight failure.
- `explore.rs:6643-6682` (live `-o *.json`) and `explore.rs:4040-4074` (`finalize_explore` `--spec-out`) fall back to `build_no_target_spec_bundle` whenever there are no specs, whatever the reason.
- Observed: a Rust target whose harness build timed out wrote the marker (`audits/2026-09-22/artifact-samples/rust-spec.json`, audit finding artifacts-02), identical to a true no-target file.

## Acceptance criteria

- [ ] Before implementing, the issue records the chosen representation. The default proposal, which the implementer may change only with a written reason in the issue:
  - `FileSpecBundle` gains `failed_functions: Vec<FailedFunction>` (serialized only when non-empty), where `FailedFunction { function_name, location, failure_class, message }`.
  - `failure_class` is a closed snake_case enum: `analyze_failed`, `build_failed`, `build_timed_out`, `harness_crashed`, `exploration_timed_out`. These are *exploration* failures. A target function that throws or panics on some inputs is **not** a failure: those remain ordinary `SpecClass` entries in `functions`.
  - `FileSpecBundleStatus` gains `Failed`, used when at least one function was attempted and none produced a spec. A bundle with some successes and some failures has status `Ok` (or absent) and a non-empty `failed_functions`.
  - `SPEC_SCHEMA_VERSION` is bumped, with the rationale comment the bump policy requires.
- [ ] `no_targets` is written only when analysis succeeded and found no target. Every attempted-but-failed function appears in `failed_functions` with its class, on both the live path and `--from-artifacts`, for both `-o *.json` and `--spec-out`.
- [ ] Exit codes are unchanged (`decide_explore_exit_status` stays the source of truth), and a unit test pins that the bundle status and the exit decision agree for each summary shape (all ok, mixed, all failed, no targets, analyze failed).
- [ ] spec-diff: a function that is a spec in the old bundle and a `failed_functions` entry in the new one is reported as `failed` (not `removed`) and counts as a regression for the exit code. A function failed on both sides is reported, not diffed. Reading a v1 bundle still works. Unit tests cover both.
- [ ] CLI test in `shatter-cli/tests/`: an attempted-but-failed function (use a fixture that deterministically fails, for example a Go file whose package does not build) writes `status: failed` with a `failed_functions` entry of the right class, and a true no-target file still writes `no_targets`. At close, show the failed case writing `no_targets/unclassified` on current `main` and the new shape after the fix.
- [ ] SPEC §5 documents `failed_functions`, the failure classes, the new status value and the version bump. SPEC §8 changelog has a row.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Build `failed_functions` from the same per-function summaries `decide_explore_exit_status` reads, in the shared bundle-collection step that explore-o-json-empty-bundle introduces. Map summary buckets to failure classes in one function with a unit test per bucket.

## Out of scope

- The success case writing an empty bundle (explore-o-json-empty-bundle).
- Multi-file bundles (multi-file-spec-bundle-first-only). If that lands first, `failed_functions` lives on each per-file bundle.
- Generated JSON Schemas (artifact-json-schemas) and the SPEC §5 producer/consumer table (spec-s5-contract-table-and-samples), which should describe the shape chosen here.

## Priority

P2: once explore-o-json-empty-bundle lands, a successful run is distinguishable and the exit code already flags failures; the remaining defect is that the persisted artifact mislabels failures as "nothing to explore".

## Type

bug

## Dependencies

- Blocked by: explore-o-json-empty-bundle (shared bundle writer).
- Related: multi-file-spec-bundle-first-only, str-ni32 (closed), str-jeen.67 (closed), str-jeen.21 (closed), str-960w (closed), artifact-json-schemas, spec-s5-contract-table-and-samples.

## References

Audit 2026-09-22 finding artifacts-02 (failure half). Split from explore-o-json-empty-bundle after the 2026-09-23 Codex cross-check (finding "02: failed-function output requires an undefined schema change").
