---
slug: closure-orphan-worktree-dirs
kind: note-to-existing
title: "Note on bento-nljv: audit 2026-09-22 confirms the five shatter orphan dirs persist; add a build-output-only classification so non-empty orphans get a reviewable, per-path cleanup"
priority: P2
type: note
labels: [audit, closure, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-nljv
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-nljv

Target: bento-nljv (open, P3, "closure: report orphan directories under the dedicated worktree root, and safely remove the empty ones"). Action: add a comment. Do not file a new issue.

Why this is a note and not a new issue: the audit draft asked for a closure apply mode that deletes orphan worktree directories. bento-nljv already specifies the orphan report, the shared orphan rule with bento-8oj0 (which also removes the doctor's wrong "safe to remove" wording), and a safe empty-only removal. The draft's deletion rule ("not a registered worktree and no process cwd inside") was unsafe: it would delete a slash-branch parent holding a live worktree (bento-8oj0), and it treated an unregistered directory as disposable even though it may hold the only copy of uncommitted work. nljv's rule, "non-empty orphans are never deleted automatically", is the correct one and is kept.

Source finding: bento-10 (shatter audit 2026-09-22).

## Comment text

Audit 2026-09-22 (shatter; finding bento-10), re-checked 2026-09-23:

- The five shatter orphans this issue cites are still present and the doctor has flagged them at every session start since the 2026-09-04 audit: `str-6q1i` (109M, a partial source checkout: `.beads/`, `Cargo.toml`, `PROTOCOL.md`, ...), `str-hszo-tmpfix` (573M), and `str-k6e61-scm-followups`, `str-mambd-enum-variant-gen`, `str-yhsp-concolic-run` (16K each). Shatter's AGENTS.md forbids agents from deleting worktree dirs, so nobody acts on the warning; about 700 MB has sat there since June to July 2026.
- Of these, the empty-only apply mode here would remove none of the space: all five are non-empty.

Suggested addition, keeping this issue's "never delete non-empty automatically" rule:

1. Add `content_class` to each `orphan_worktree_dirs` entry: `empty`, `build-output-only` (every file lies under a top-level `target/`, `node_modules/`, `dist/`, `build/` or `.venv/`), or `other`. For `other`, include up to 10 sample relative paths outside those directories so a reviewer can see what would be lost.
2. Keep deletion of non-empty orphans behind explicit per-path confirmation. If a mode is added (for example `--apply remove-build-output-orphans --path <p>`), it accepts only paths listed in the latest scan with `content_class: build-output-only`, reuses this issue's no-follow, snapshot and `registered-now` checks, and removes only the build-output subtrees plus the then-empty directory. `other` orphans are report-only, with "inspect before deleting" wording.
3. Tests: a `build-output-only` orphan is removed only when its path is passed; an `other` orphan (for example one with a modified tracked-looking file such as `src/lib.rs`) is never removed by any mode; a slash-branch parent containing a registered worktree at any depth is never an orphan (the shared fixture table with bento-8oj0).
4. Close-time proof for that addition: the scan JSON from the maintainer's machine showing the five shatter entries classified (expected: `str-6q1i` is `other`, the rest `build-output-only` if they hold only `target/`).
