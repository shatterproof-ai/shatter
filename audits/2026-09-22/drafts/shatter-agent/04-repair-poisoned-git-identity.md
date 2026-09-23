# Remove leaked fixture identity (Test <test@example.com>) from primary .git/config and add a repo-state check

- Priority: P1
- Type: bug
- Labels: agents,git,tooling,drift
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (partially covered by str-qwua7.1, str-qwua7.51; append note to str-qwua7.51)
- Source findings: agent-repo-01, prior-04
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
The primary checkout's repo-local `.git/config` contains a `[user]` section
with `name = Test`, `email = test@example.com` — the identity that shell/py
test fixtures write with `git config user.name "Test"`. It was left behind by
the GIT_DIR leak that str-jttrf/str-y0rcz fixed; the leak was fixed but the
damage was never repaired. Every commit pushed to GitHub since about
2026-06-23 is authored "Test"/"Test User", and every bd write records
`created_by: Test`. str-qwua7.51 misdiagnoses the bd "Test" owner as a missing
SessionStart identity.

## Current Code Facts
- `git -C /home/ketan/project/shatter config --show-origin user.email` ->
  `file:.git/config test@example.com` (overrides the global real identity).
- `git log -300 --format='%an <%ae>' | sort | uniq -c` -> ~116
  `Test <test@example.com>`, ~184 `Test User <test@example.com>`.
- All issues created since 2026-09-05 have `created_by: Test`.
- Fixtures that write this identity: `scripts/test_target_dir_report_json.sh:27-28`,
  `scripts/test_cleanup_merged_remote_branches.sh:47-48,137`,
  `scripts/test_git_sandbox_test_lib.sh:32,68`,
  `scripts/test_git_sandbox_test_lib.py:94,135`,
  `scripts/test_walkthrough_examples_checkout.py:57-63`,
  `shatter-cli/tests/implicit_init_gitignore_test.rs:51-52`.
- `scripts/test_git_fixture_isolation.py` exists (str-jttrf) but does not
  snapshot the real repo's `.git/config`.
- `core.bare` is now `false` (already repaired).

## Acceptance Criteria
- The `[user]` section is removed from the primary checkout's `.git/config`
  (operator action; record the before/after `git config --show-origin` output).
- New drift-patrol check (or `scripts/setup-hooks.sh --check` item) FAILs when
  the repo-local `user.email` matches `*@example.com` or differs from the
  global identity, when `core.bare=true`, or when `core.hooksPath` is set in
  repo-local config. Unit-tested.
- `test_git_fixture_isolation.py` snapshots `$(git rev-parse --git-common-dir)/config`
  before and after each fixture entrypoint and fails if it changed.
- A decision is recorded (in this issue) on history: leave it, or add a
  `.mailmap` mapping `test@example.com` to the real author. If `.mailmap` is
  chosen, `git log --use-mailmap` shows the real author for recent commits.
- str-qwua7.51 gets a note with the real root cause (see Suggested Approach).

## Suggested Approach
Note to append to str-qwua7.51: "Root cause of `Owner: Test` is the leaked
repo-local `[user]` in `.git/config` (fixture identity from the str-jttrf GIT_DIR
leak), not a missing SessionStart identity. Tracked by <this issue>."

## Out of Scope
Rewriting published history.
