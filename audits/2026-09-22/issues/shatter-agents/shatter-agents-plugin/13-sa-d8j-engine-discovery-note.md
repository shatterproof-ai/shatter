---
slug: sa-d8j-engine-discovery-note
kind: note-to-existing
title: "Note on sa-d8j: `shatter list-targets` is not a drop-in replacement for run_targets.py discovery; fix the prune locally"
priority: P2
type: note
labels: [audit-2026-09-22]
parent_epic: ""
blocked_by: []
existing_id: sa-d8j
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# Note on sa-d8j

Target: **sa-d8j** ("Generated harnesses inflate target discovery"), OPEN, P2. Post the comment below. It adds evidence against one tempting fix; it does not change the issue's scope or priority.

## Comment text

Audit 2026-09-22 (finding plugins-10) proposed fixing this by having `run_targets.py` take its roots from `shatter list-targets --format json` instead of walking the tree. The cross-check showed that does not work today, so keep the fix local (prune `.shatter/` at every depth in `should_exclude_dir`, as this issue already requests). Evidence, re-verified on 2026-09-23 against shatter-agents `119b807` and a shatter CLI built from shatter `70465921`:

- `shatter list-targets` "lists source files that would be selected for a scan". Its JSON (`kind: target_manifest`) has one `project_root` and `selected[]` / `excluded[]` / `unsupported[]` / `candidate_outside_policy[]` arrays of individual **source files** (`path`, `language`, `frontend`). It has no notion of package roots (Cargo.toml / go.mod / package.json directories), which run-shatter needs to find wrappers.
- Against the existing fixture `tests/fixtures/run-targets/mixed-repo` (manifests only, no sources), `list-targets` returns `selected: []`, while `tests/test_run_targets.py:39` expects three roots (`go-service`, `rust-lib`, `ts-app`).
- `list-targets` does not exclude generated harnesses either. In a temp tree with `src/lib.rs` and `.shatter/cache/harness/src/lib.rs`, `selected[].path` is `['.shatter/cache/harness/src/lib.rs', 'src/lib.rs']`. (The engine's native glob walker excludes `.shatter` — `shatter-cli/src/args.rs` `GLOB_WALK_EXCLUDE_DIRS` — but list-targets' discovery does not.) That engine-side gap belongs in the shatter tracker, not here.

Close proof for this issue stays as its own description says: the minimal-fixture test (root `Cargo.toml` plus `.shatter/cache/harness/rust/bin-only/<id>/Cargo.toml`, and the nested `api/` variant) fails on current code and passes after the prune. Delegating discovery to the engine can be reconsidered only if shatter adds a package-root listing that excludes managed state; that would be a new issue.

sa-c2q (Make wrappers invisible) is unaffected: wrapper detection stays in `run_targets.py` either way.
