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
- Doc tooling: `scripts/docs-smoke.py:27` and `:590-600` (`validate_snapshot_against_struct` validates SPEC §5.5 by running `shatter diff <f> <f>`), and `scripts/test_docs_smoke.py:33`, `:43`, `:51` (the `("diff",)` entries).
- spec-diff already detects the return-value change that snapshot diff was meant to catch (verified on the goals-02 repro: `[CHANGED] zero -> nil`).

## Acceptance criteria

- [ ] The `diff` subcommand is removed from `args.rs` (variant and tests) and `main.rs` (dispatch). `shatter diff a b` fails with clap's unknown-subcommand error, and `shatter --help` does not list it. No deprecation alias or shim is added, so the name is free for str-81xiw.
- [ ] `shatter-core/src/snapshot.rs` and `pub mod snapshot` are deleted. `run_diff` and the snapshot import are removed from `shatter-cli/src/commands/diff.rs`. `run_spec_diff` and its tests are unchanged. Moving them to `spec_diff.rs` is optional.
- [ ] SPEC: §2.6 covers only `spec-diff` and `compare` and names spec-diff as the regression tool. §5.5 is removed and later sections are renumbered, or §5.5 is replaced by a pointer to the spec bundle section. The overview lines (:13, :27, :48), the command table (:84) and the behavior-map purpose list (:669-671) no longer mention snapshots or `diff`. A §8 changelog row records the removal and names spec-diff as the replacement.
- [ ] README (:338) and QUICKSTART §5 show the spec-diff workflow instead: `explore --spec-out old.json`, change the code, `explore --spec-out new.json`, `shatter spec-diff old.json new.json` (exit 1 on regression). PLAN.md references are updated or marked retired.
- [ ] `scripts/docs-smoke.py` no longer validates snapshots, and `scripts/test_docs_smoke.py` drops the `diff` entries. `task docs-smoke` passes (forced, not served from the checksum cache: run with the cache cleared or `--force` on the leaf, and record the output).
- [ ] A repo-wide search, `rg -n "shatter diff\b|Snapshot::|snapshot::" --glob '!audits/**' --glob '!.beads/**'`, finds no remaining references outside the changelog. Paste the output in the close note.
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
