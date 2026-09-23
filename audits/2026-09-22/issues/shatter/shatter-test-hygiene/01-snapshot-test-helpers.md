---
slug: snapshot-test-helpers
kind: new
title: "Snapshot tests self-create missing snapshots and pass; four copy-pasted helpers; whitespace-collapsed markdown; no CLI output snapshots"
priority: P2
type: task
labels: [testing, report, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Snapshot tests self-create missing snapshots and pass; four copy-pasted helpers; whitespace-collapsed markdown; no CLI output snapshots

## Problem

The shatter-core snapshot tests pass when a snapshot file is missing. If the file does not exist, the helper writes the current output and returns. Deleting a snapshot, or renaming a test so that it looks for a new path, turns the check into a no-op that still reports success. Four test files each define their own `assert_snapshot`. Two of them collapse all whitespace before comparing. That hides layout regressions in the HTML reports and also in `outcome.md`, where line structure is the output. No test pins the terminal output of `shatter explore`/`shatter scan` or the top-level `--help`, so regressions in human-facing CLI output are caught only by manual walkthrough review.

`CLAUDE.md:13` says "Regression snapshots are checked into the repo and verified in CI". That statement cannot hold while a missing snapshot passes.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23); the files are identical to audit HEAD 56c86168.

- `shatter-core/tests/outcome_md_snapshots.rs:45-51`, `run_markdown_ordering_snapshots.rs:38-44` and `source_set_summary_snapshots.rs:32-38` all contain:
  ```rust
  fn assert_snapshot(path: &Path, actual: &str) {
      if !path.exists() {
          std::fs::create_dir_all(path.parent().unwrap()) ...;
          std::fs::write(path, actual) ...;
          return;
      }
  ```
- `shatter-core/tests/html_snapshots.rs:63-64` has the same pattern. Its module doc (lines 8-11) documents "First-run behaviour: ... writes the current output to disk and passes".
- Whitespace collapsing: `html_snapshots.rs:36` `normalize_ws` is used at :72-73. `outcome_md_snapshots.rs:24` copies it ("same helper as html_snapshots.rs") and uses it at :54-55, so a markdown snapshot is compared with every newline collapsed to one space. `run_markdown_ordering_snapshots.rs` and `source_set_summary_snapshots.rs` compare byte-exact.
- There are 4 separate `fn assert_snapshot` definitions. `shatter-core/tests/snapshots/` holds exactly 8 files: `explore_fn.html`, `explore_page.html`, `scan_report.html`, three `outcome_md_*.md`, `run_markdown_ordering.md` and `source_set_summary.md`.
- `shatter-cli/tests/` has no snapshot files. `json_stdout_contract.rs` checks JSON structure only, and `hide_exec_flags_help.rs` asserts a few substrings of `--help`.
- `insta` is not a dependency of any workspace crate (`grep -n insta Cargo.toml */Cargo.toml` returns nothing).
- Audit finding tests-ci-08 (verified).

## Acceptance criteria

- [ ] `insta` (or an equivalent single shared helper) replaces all four `assert_snapshot` functions, and the local helpers are deleted.
- [ ] A missing snapshot fails the test when `CI` is set (insta's default `INSTA_UPDATE=no` behaviour under CI). Local update is an explicit opt-in (`cargo insta review` / `INSTA_UPDATE=always`).
- [ ] Proof at close: delete one snapshot file, run `CI=1 cargo test -p shatter-core --test outcome_md_snapshots`, and paste the failing output into the issue. Restore the file and paste the passing run.
- [ ] Markdown snapshots (`outcome_md_*`, `run_markdown_ordering`, `source_set_summary`) are compared byte-exact, with no whitespace collapsing.
- [ ] HTML snapshots keep only structural normalization: insignificant inter-tag whitespace/indentation. Whitespace inside `<pre>`/`<code>` and text nodes is not collapsed.
- [ ] New normalized CLI snapshots in `shatter-cli/tests/` cover `explore` and `scan` terminal output on the `01-arithmetic` TS and Go examples, plus top-level `shatter --help`. Absolute paths are made relative, and durations/timestamps/PIDs are redacted, so the snapshots are deterministic across machines. Run each twice to show it is stable.
- [ ] `.claude/skills/rust-conventions` gains a one-paragraph snapshot convention (use the shared helper; never self-create).
- [ ] `task affected` passes, and its `Gates selected` output is recorded in the close comment.

## Suggested approach

Add `insta` as a dev-dependency of shatter-core and shatter-cli. Convert the existing 8 snapshot files by running the tests once with `INSTA_UPDATE=always` and checking that the regenerated content matches the old files, apart from whitespace in outcome_md. For HTML, apply a small normalizer that strips whitespace between tags before handing the text to insta. For the CLI snapshots, use `insta` filters (regex redactions) for paths and timings. Depending on `pin-examples-repo` is not required, but CLI snapshots over `01-arithmetic` will be more stable once the examples SHA is pinned.

## Out of scope

- The retired `shatter diff` / Snapshot writer path (maintainer decision D2, handled in retire-snapshot-diff). This issue is only about test snapshots.
- Rewriting report renderers, or unrelated refactors in the touched test files.
- The CI-hollowness problem itself (shatter-gates-integrity bucket).

## Priority / type / labels

P2 · task · testing, report, audit · Size M

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related: `pin-examples-repo` (stabilizes the CLI snapshot inputs).
