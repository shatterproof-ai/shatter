# Restore rustfmt cleanliness once and gate `cargo fmt --check` (tree drifted again after str-fr1v)

- Priority: P2
- Type: task
- Labels: rust,quality-gates,agents
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (str-fr1v closed-but-unfixed)
- Source findings: sessions-11
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
No gate runs `cargo fmt --check`, so the tree drifted after str-fr1v closed.
On 2026-09-21 a crate-wide `cargo fmt -p shatter-core` churned 58 files
(+3068/-789), followed by a mass `git checkout --` revert and a script to
re-apply the agent's own edits. The lesson was recorded only as a memory
("never run crate-wide cargo fmt").

## Current Code Facts
- grep of Taskfile.yml, taskfiles/ and `.github/` finds no `fmt --check`.
- Memory `project_shatter_tree_not_rustfmt_clean.md` (dated 2026-09-21).
- str-fr1v ("Fix rustfmt 1.93 drift", acceptance `cargo fmt --all -- --check`
  must pass) is closed.

## Acceptance Criteria
- One dedicated commit formats the workspace (and standalone crates:
  shatter-rust, shatter-rust-runtime, shatter-llm) with the pinned toolchain.
- `cargo fmt --all -- --check` (and per standalone crate) wired into
  `check-static`; CI fails on drift.
- The memory is deleted and the rust-conventions skill says "run `cargo fmt`"
  normally.

## Out of Scope
Pinning a rust-toolchain.toml (separate finding) unless required for stable fmt.
