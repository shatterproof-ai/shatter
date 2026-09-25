---
slug: list-targets-selects-shatter-cache
kind: new
title: "list-targets (TargetManifest) selects Shatter's own generated harness sources under .shatter/cache/ because its default excludes omit .shatter, .git and build"
priority: P2
type: bug
labels: [cli, discovery, scan, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# list-targets selects Shatter's own generated harness sources under .shatter/cache/

## Problem

`shatter list-targets` builds its file list with `TargetManifest::build`
(`shatter-cli/src/commands/list_targets.rs:31` → `shatter-core/src/target_manifest.rs:137`). Its
`DEFAULT_EXCLUDES` (`shatter-core/src/target_manifest.rs:28-37`) covers `node_modules`, `vendor`,
`dist`, `target`, tests and `.d.ts`, but not `.shatter`, `.git` or `build`. After any Rust
exploration, the project contains generated harness sources under `.shatter/cache/harness/` (for
example `.shatter/cache/harness/src/lib.rs`), and `list-targets` selects them as targets.

The native glob walker used for positional wildcard targets already excludes these directories
(`GLOB_WALK_EXCLUDE_DIRS`, `shatter-cli/src/args.rs:2023-2030`: `.git`, `node_modules`, `target`,
`.shatter`, `dist`, `build`). The two discovery paths therefore disagree. Anything that consumes the
manifest, including the shatter plugin's `run-shatter` target discovery, can end up exploring
Shatter's own harness instead of the user's code.

The walker does honour the project's `.gitignore` and `.shatterignore` (`target_manifest.rs:155-156`), so a project that already ignores `.shatter/` hides the bug. It shows in projects and fixtures without that ignore line, for example before `shatter init` or in any repo whose `.gitignore` predates it. Discovery must not depend on the user's ignore file for Shatter's own output.

Found during the 2026-09-22 audit's shatter-agents plugin revision
(`audits/2026-09-22/issues/shatter-agents/shatter-agents-plugin/REVISION.md`, "Engine-side gaps"):
on a project with an existing `.shatter/cache/harness/src/lib.rs`, `list-targets` listed it.

## Acceptance criteria

- [ ] Reproduce on main first: a test fixture directory containing `src/lib.rs` plus
  `.shatter/cache/harness/src/lib.rs`, `.git/hooks/x.rs` and `build/gen.rs`. `list-targets` on it
  selects only `src/lib.rs`. The test fails on main (record the failing assertion), and it lives in
  `shatter-core` (`TargetManifest::build`) so every manifest consumer is covered, not only the CLI.
- [ ] Both discovery paths use one shared exclusion list (one constant, referenced by
  `TargetManifest` and the glob walker), so they cannot drift again. A unit test asserts that the
  glob walker and `TargetManifest` exclude the same directory names.
- [ ] Excluded generated paths are silent, like the existing defaults: they do not appear in the
  manifest's `excluded` list.
- [ ] `shatter scan <dir>` on the same fixture explores no file under `.shatter/`.
- [ ] `task affected` passes, with `Gates selected` recorded.

## Out of scope

- Changing user-configurable include/exclude semantics.
- The shatter plugin's own local pruning in `run_targets.py` (tracked in shatter-agents).

## Related

str-qwua7.56 (closed; lifecycle-export exclusion in target discovery). The shatter-agents drafts
that note this engine gap.
