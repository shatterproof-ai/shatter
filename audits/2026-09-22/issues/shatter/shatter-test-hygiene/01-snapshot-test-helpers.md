---
slug: snapshot-test-helpers
kind: new
title: "Core snapshot tests pass on missing file"
priority: P2
type: task
labels: [testing, report, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Core snapshot tests pass on missing file

## Problem

The shatter-core snapshot tests pass when a snapshot file is missing: the helper writes the current output and returns. Deleting a snapshot, or renaming a test so it looks for a new path, turns the check into a no-op that still reports success. Four test files each define their own `assert_snapshot`. Two of them collapse all whitespace before comparing, which hides layout regressions in the HTML reports and in `outcome.md`, where line structure is the output.

`CLAUDE.md:13` says "Regression snapshots are checked into the repo and verified in CI". That cannot hold while a missing snapshot passes.

This issue only fixes the existing shatter-core snapshot helpers. New CLI output snapshots are a separate issue (`cli-output-snapshots`), so this correctness fix does not wait on that work.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23); the files are identical at audit HEAD 56c86168.

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
- Whitespace collapsing: `html_snapshots.rs:36` `normalize_ws` is used at :72-73. `outcome_md_snapshots.rs:24` copies it ("same helper as html_snapshots.rs") and uses it at :54-55, so each markdown snapshot is compared with every newline collapsed to one space. `run_markdown_ordering_snapshots.rs` and `source_set_summary_snapshots.rs` compare byte-exact.
- There are 4 separate `fn assert_snapshot` definitions. `shatter-core/tests/snapshots/` holds exactly 8 files: `explore_fn.html`, `explore_page.html`, `scan_report.html`, three `outcome_md_*.md`, `run_markdown_ordering.md` and `source_set_summary.md`.
- For contrast, shatter-cli already has a correct exact-match convention: `shatter-cli/tests/hide_exec_flags_help.rs:47-68` compares full `spec-diff --help` and `doctor --help` output against checked-in fixtures in `shatter-cli/tests/fixtures/help/` with `assert_eq!`, fails if a fixture is missing, and documents regeneration in the file header (lines 5-7).
- `insta` is not a dependency of any workspace crate (`grep -n insta Cargo.toml */Cargo.toml` returns nothing).
- Audit finding tests-ci-08 (verified).

## Acceptance criteria

- [ ] One shared helper (either `insta`, or a small helper in a shared test-support module following the `hide_exec_flags_help.rs` pattern) replaces all four `assert_snapshot` functions, and the four local copies are deleted. `grep -rn "fn assert_snapshot" shatter-core/tests` returns at most the one shared definition.
- [ ] A missing snapshot fails the test in every mode that CI and `task` gates use. Writing or updating snapshots is only possible through an explicit opt-in (e.g. `INSTA_UPDATE=always`/`cargo insta review`, or an `UPDATE_SNAPSHOTS=1` env var), never implicitly.
- [ ] Proof at close (red, then green): delete `shatter-core/tests/snapshots/outcome_md_*.md` (one file), run `CI=1 cargo test -p shatter-core --test outcome_md_snapshots` and paste the failing output. Also run it without `CI` set and paste that output, which must also fail. Restore the file and paste the passing run.
- [ ] Markdown snapshots (`outcome_md_*`, `run_markdown_ordering`, `source_set_summary`) are compared byte-exact, with no whitespace normalization. Proof: change one newline in a checked-in `outcome_md_*.md` fixture, show the test fails, then revert.
- [ ] HTML comparison does not erase meaningful whitespace. Either compare HTML byte-exact (preferred if the renderer output is deterministic), or use a normalizer that only touches whitespace proven insignificant. If a normalizer is kept, it has unit tests showing that:
  - `<span>Hello</span> <span>world</span>` and `<span>Hello</span><span>world</span>` normalize to different strings;
  - whitespace and newlines inside `<pre>`, `<code>` and `<textarea>` are preserved exactly;
  - text-node whitespace between words is preserved.
- [ ] Regenerated snapshot files are reviewed in the diff: for the byte-exact markdown and HTML files, the close comment states whether content changed apart from whitespace, and any non-whitespace change is explained.
- [ ] `.claude/skills/rust-conventions` gains a one-paragraph snapshot convention: use the shared helper, never self-create, regenerate only via the explicit opt-in.
- [ ] `task affected` passes, and its `Gates selected` output is recorded in the close comment.

## Suggested approach

Byte-exact comparison is the simplest rule and removes the need for a normalizer. The HTML renderers are Askama templates, so their output should already be deterministic; try byte-exact first and add normalization only for a demonstrated source of noise. If `insta` is adopted, add it as a dev-dependency of shatter-core only; shatter-cli can adopt it in `cli-output-snapshots`.

## Out of scope

- New CLI output snapshots (`cli-output-snapshots`).
- The retired `shatter diff` / Snapshot writer path (maintainer decision D2, handled in retire-snapshot-diff). This issue is only about test snapshots.
- Rewriting report renderers, or unrelated refactors in the touched test files.
- The CI-hollowness problem itself (shatter-gates-integrity bucket).

## Priority / type / labels

P2 · task · testing, report, audit · Size S

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Blocks: `cli-output-snapshots` (reuses the shared helper convention).
