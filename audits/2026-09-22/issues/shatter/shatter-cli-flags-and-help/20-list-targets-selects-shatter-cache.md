---
slug: list-targets-selects-shatter-cache
kind: new
title: "list-targets (TargetManifest) selects Shatter's own generated harness sources under .shatter/ when the project's .gitignore does not exclude it"
priority: P2
type: bug
labels: [cli, discovery, scan, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# list-targets selects Shatter's own generated harness sources under .shatter/

## Problem

Shatter-managed output under `.shatter/` must never be selected as a scan target, whatever the
user's `.gitignore` says. Today it is selected unless the user's ignore file happens to exclude it.

`shatter list-targets` builds its file list with `TargetManifest::build`
(`shatter-cli/src/commands/list_targets.rs:31` → `shatter-core/src/target_manifest.rs:137`). Its
`DEFAULT_EXCLUDES` (`shatter-core/src/target_manifest.rs:28-37`) covers `node_modules`, `vendor`,
`dist`, `target`, `__tests__`, test files and `.d.ts`, but not `.shatter`. After any Rust
exploration, the project contains generated harness sources under `.shatter/cache/harness/` (for
example `.shatter/cache/harness/src/lib.rs`), and `list-targets` selects them.

The walker honours the project's `.gitignore` and `.shatterignore`
(`target_manifest.rs:155-156`), so a project whose `.gitignore` already lists `.shatter/` hides the
bug. It shows in projects without that line, for example before `shatter init` or in any repo whose
`.gitignore` predates it. Anything that consumes the manifest, including the shatter plugin's
`run-shatter` target discovery, can then explore Shatter's own harness instead of the user's code.

Precedent: the native glob walker used for positional wildcard targets already skips `.shatter`
(`GLOB_WALK_EXCLUDE_DIRS`, `shatter-cli/src/args.rs:2023-2030`). The two paths use different
matching (directory names in `shatter-cli` versus glob patterns in `shatter-core`), so this issue
does not require them to share one constant.

Found during the 2026-09-22 audit's shatter-agents plugin revision
(`audits/2026-09-22/issues/shatter-agents/shatter-agents-plugin/REVISION.md`, "Engine-side gaps").

## Evidence

Reproduced 2026-09-24 with the audit checkout's debug binary
(`target/debug/shatter`, built 2026-09-22) on a fresh fixture with no `.gitignore`, no
`.shatterignore` and no `.git`, containing only `src/lib.rs` and
`.shatter/cache/harness/src/lib.rs`:

```
$ shatter list-targets .
Target manifest — <fixture>
  config hash:      170de33e...
  source set hash:  07aae94e...

Selected (2):
  .shatter/cache/harness/src/lib.rs  [rust, 1 lines]
  src/lib.rs  [rust, 1 lines]
```

Exit status 0.

## Acceptance criteria

- [ ] Reproduce on main first with a `shatter-core` test on `TargetManifest::build`, so every
  manifest consumer is covered, not only the CLI. Fixture: `src/lib.rs` plus
  `.shatter/cache/harness/src/lib.rs`, with no `.gitignore` or `.shatterignore`. Assert that only
  `src/lib.rs` is selected. The test fails on main; the close comment records the failing assertion.
- [ ] `TargetManifest` excludes everything under `.shatter/` by default, regardless of the
  project's `.gitignore`/`.shatterignore`.
- [ ] The exclusion is silent, like the existing defaults: `.shatter/` paths do not appear in the
  manifest's `excluded` list.
- [ ] `shatter list-targets` on the fixture above lists only `src/lib.rs`.
- [ ] `task affected` passes, with `Gates selected` recorded.

## Optional consideration

The glob walker also skips `.git` and `build`, which `DEFAULT_EXCLUDES` does not. Whether the
manifest should skip them too is a separate decision (`build/` can hold user sources in some
projects). Not required here; if the implementer adds them, note it in the close comment and add
test cases for each.

## Out of scope

- Changing user-configurable include/exclude semantics.
- Unifying the glob walker's and the manifest's exclusion lists.
- `shatter scan`'s own file discovery. In the audit checkout, the only non-test caller of
  `TargetManifest::build` is `list_targets.rs` (`:31`, `:222`); scan does not go through it.
- The shatter plugin's own local pruning in `run_targets.py` (tracked in shatter-agents).

## Related

str-qwua7.56 (closed; lifecycle-export exclusion in target discovery). The shatter-agents drafts
that note this engine gap.
