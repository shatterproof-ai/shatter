---
slug: rustfmt-gate
kind: new
title: "Restore rustfmt cleanliness in one dedicated commit (107 files drifted after str-fr1v) and gate `cargo fmt --check` in check-static"
priority: P2
type: task
labels: [rust, quality-gates, agents, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Restore rustfmt cleanliness in one dedicated commit (107 files drifted after str-fr1v) and gate `cargo fmt --check` in check-static

## Problem

No gate runs `cargo fmt --check`, so the Rust tree drifted again after str-fr1v ("Fix rustfmt 1.93 drift") closed. Agents cannot safely run `cargo fmt` now. On 2026-09-21, one crate-wide `cargo fmt -p shatter-core` churned 58 files (+3068/-789). The agent then ran a mass `git checkout --` revert and wrote a script to re-apply its own edits. The lesson was recorded only in a private agent memory ("never run crate-wide cargo fmt"), which works around the drift instead of removing it.

The fix has two separate parts, and they must not be mixed with any other change:

1. one commit that only formats the workspace and the standalone crates;
2. a `cargo fmt --check` gate so the tree stays clean.

## Evidence

Re-verified 2026-09-23 in the worktree at `56c86168` with local `rustfmt 1.9.0-stable (ac68faa20c 2026-05-25)`:

- `cargo fmt --all -- --check` (workspace) lists 97 files: shatter-core 61, shatter-cli 25, shatter-llm 11 (for example `shatter-cli/build.rs:37`, `shatter-cli/src/args.rs:361`).
- `cd shatter-rust && cargo fmt --all -- --check` lists 9 files. `cd shatter-rust-runtime && cargo fmt --all -- --check` lists 1 file. These crates are excluded from the workspace (`Cargo.toml:3`), so the workspace command does not cover them.
- A grep of `Taskfile.yml`, `taskfiles/`, `shatter-*/Taskfile.yml` and `.github/` finds no `fmt --check` / `fmt -- --check`.
- There is no `rust-toolchain.toml`. CI uses `dtolnay/rust-toolchain@stable` (`ci.yml:38-41`), so the rustfmt version floats. That is how str-fr1v's "1.93 drift" happened.
- Session c1689435 (2026-09-21T22:53): `cargo fmt -p shatter-core` → "58 files changed, 3068 insertions(+), 789 deletions(-)", then `... | xargs git checkout --` and `scratchpad/apply_child_a.py` (sessions-11).
- str-fr1v is closed with reason "Closed". Its acceptance was `cargo fmt --all -- --check` must pass.
- `.claude/skills/rust-conventions/SKILL.md` has no formatting guidance.

## Acceptance criteria

- [ ] One dedicated commit, containing nothing but `cargo fmt` output, formats the workspace (`cargo fmt --all`), `shatter-rust` and `shatter-rust-runtime` with the same pinned rustfmt version the gate uses. The commit message states the rustfmt version. Land it quickly, since it touches many files and will conflict with in-flight branches. Announce it in the landing notes.
- [ ] Add that commit's SHA to a `.git-blame-ignore-revs` file.
- [ ] `check-static` runs `cargo fmt --all -- --check` plus the per-standalone-crate checks. CI runs them through `task check`.
- [ ] The rustfmt version is pinned in one place, used by both CI and the local gate. Options: a `rust-toolchain.toml` with `components = ["rustfmt", "clippy"]`, or a pinned toolchain for the fmt step only, with the local task checking `rustfmt --version`. This keeps a future stable release from turning the gate red on its own.
- [ ] Proof the gate executes: on a scratch branch, mis-indent one line, force the gate (`task check-static --force` or clear its checksum), show it failing, then revert. Paste both outputs in the close reason, along with a CI run URL in which the fmt step executed.
- [ ] The rust-conventions skill says to run `cargo fmt` normally. The maintainer is told the memory `project_shatter_tree_not_rustfmt_clean.md` can be deleted. Agents do not edit memory as part of this issue.

## Suggested approach

Pick and pin the toolchain first, then run the format commit from a clean `main` with no other agents mid-landing. Add the gate in the next commit, then land both together. Do not combine this with the go-lint work or any refactor.

## Out of scope

- Clippy lint changes.
- Go formatting (`go-lint-and-gofmt-gated`).
- Pinning the whole Rust toolchain for other reasons. Pin only what stable fmt needs, unless a rust-toolchain.toml is simply the easiest way to do that.

## Dependencies

- None blocking.
- Related: str-fr1v (closed; see `rustfmt-reopen-note`), `go-lint-and-gofmt-gated`.

Priority: P2 · Type: task · Labels: rust, quality-gates, agents, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-agent/26, sessions-11
