---
slug: mailmap-and-fixture-config-snapshot
kind: new
title: "Map the leaked fixture identity (test@example.com) to the real author via .mailmap, and make test_git_fixture_isolation.py guard the real checkout's .git/config"
priority: P1
type: bug
labels: [agents, git, tooling, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Map the leaked fixture identity (test@example.com) to the real author via .mailmap, and make test_git_fixture_isolation.py guard the real checkout's .git/config

## Problem

A test-fixture git identity leaked into the primary checkout's repo-local
`.git/config` as a `[user]` section (`name = Test`, `email = test@example.com`).
It was a side effect of the GIT_DIR fixture leak that str-jttrf and str-y0rcz
fixed on 2026-09-12. Those fixes stopped the leak but never repaired the damage
it had already done, so every commit made in the primary checkout from about
2026-06-23 was authored `Test` or `Test User <test@example.com>`, and all were
pushed to GitHub. The maintainer removed the leaked `[user]` section on
2026-09-23 (decision D5). **That step is done and is not part of this issue.**

Two things are still missing:

1. **History still shows the fixture identity.** D5 says not to rewrite
   history. A `.mailmap` should make `git log`, `git shortlog` and `git blame`
   show the real author.
2. **Nothing would catch a recurrence.** `scripts/test_git_fixture_isolation.py`
   (added by str-jttrf) builds a disposable *sentinel* caller repo, runs each
   fixture entrypoint with a contaminated `GIT_*` environment pointing at that
   sentinel, and checks that the sentinel is unchanged. It never checks the
   repository the test runs in: the real checkout's
   `$(git rev-parse --git-common-dir)/config`. That file is what was damaged
   in 2026-06..09. A fixture that bypasses the sanitizer, or a new fixture
   missing from `ENTRYPOINTS`, could rewrite the real config and this test
   would still pass.

The repo-state check (FAIL on local identity override, `*@example.com`,
`core.bare=true`, local `core.hooksPath`) is **not** in this issue. It goes to
str-qwua7.1 (see the note drafted as `qwua7-1-git-state-check`).

## Evidence

Re-verified 2026-09-23 in the audit worktree
(`/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`, origin/main = 70465921):

- Identity is clean now:
  `git -C /home/ketan/project/shatter config --show-origin --get-all user.email`
  -> `file:/home/ketan/.gitconfig 33678+ketang@users.noreply.github.com` (no
  `.git/config` entry). `git config --local --list` shows no `user.*`.
- History still carries the fixture identity:
  `git log --all --format='%an <%ae>%n%cn <%ce>' | grep example | sort | uniq -c`
  -> `285 Test <test@example.com>`, `961 Test User <test@example.com>` (author
  and committer lines). `git log -300 origin/main --format='%an <%ae>' | sort | uniq -c`
  -> 177 `Test User`, 123 `Test`, 0 real. The first leaked commit is
  `131ebe06` (2026-06-23, `Test User`).
- No `.mailmap` exists at the repo root.
- Fixtures that write this identity (any one of them hits the real repo if
  GIT_DIR leaks in):
  `scripts/test_target_dir_report_json.sh:27-28`,
  `scripts/test_cleanup_merged_remote_branches.sh:47-48,136-137`,
  `scripts/test_git_sandbox_test_lib.sh:31-32,67-68`,
  `scripts/test_git_sandbox_test_lib.py:93-94,134-135`,
  `scripts/test_walkthrough_examples_checkout.py:57,63` (`Test User`),
  `shatter-cli/tests/implicit_init_gitignore_test.rs:51-52` (`Test User`),
  `shatter-core/src/scm.rs:709` (`t@example.com`).
- `scripts/test_git_fixture_isolation.py:19-31` (`ENTRYPOINTS`) lists only the
  shell and Python fixtures. The Rust fixtures (`implicit_init_gitignore_test.rs`,
  the `scm.rs` unit tests) are not covered. `:48-105` snapshots only the
  temporary `caller` repo, never `ROOT`'s git common dir.
- The test runs under `task meta` (`Taskfile.yml:450`,
  `python3 -m unittest scripts.test_git_fixture_isolation`) and is already in
  `meta`'s `sources:` (`Taskfile.yml:418`).
- Audit sources: agent-repo-01, prior-04 (`audits/2026-09-22/findings.json`
  and `audits/2026-09-22/areas/agent-repo.md` on branch `audit-2026-09-22`).

## Acceptance criteria

1. A `.mailmap` at the repo root contains exactly these mappings (no history
   rewrite):
   ```
   Ketan Gangatirkar <33678+ketang@users.noreply.github.com> Test <test@example.com>
   Ketan Gangatirkar <33678+ketang@users.noreply.github.com> Test User <test@example.com>
   ```
2. Proof in the close reason:
   `git check-mailmap 'Test <test@example.com>' 'Test User <test@example.com>'`
   prints the real identity twice, and
   `git log --use-mailmap --format='%aN <%aE>' origin/main | grep -c example.com`
   prints `0`.
3. `scripts/test_git_fixture_isolation.py` hashes
   `$(git -C ROOT rev-parse --git-common-dir)/config` before and after each
   `ENTRYPOINTS` command, and before and after the whole run. It fails, naming
   the entrypoint and the diff, if the file changed. The test only reads the
   real config and never writes it.
4. Failing-then-passing proof in the close reason: on a scratch branch, add a
   throwaway entrypoint that runs `git -C "$ROOT" config user.email leak@example.com`
   (against a *copy* of the repo, reached by pointing `ROOT` at a temporary
   clone). Record the test failing, then remove it and record the pass. Do
   not run the leaking probe against the real primary checkout.
5. The Rust fixtures that set identities (`implicit_init_gitignore_test.rs`,
   `scm.rs` tests) are either added as entrypoints (a focused
   `cargo test -p <crate> <name>` invocation) or covered by the same
   before/after config hash in a wrapper. The choice is recorded in the test
   file's comment block above `ENTRYPOINTS`.
6. `task meta` passes, and `task affected` passes with its `Gates selected`
   output recorded.

## Suggested approach

- Add a small `real_config_digest()` helper next to `snapshot()`. Call it
  around the existing loop body so both contamination modes are covered. Use
  `git rev-parse --git-common-dir` with a clean env (the same `clean_env` the
  test builds), so a leaked `GIT_DIR` cannot redirect the probe.
- For the proof in item 4, make `ROOT` overridable through an env var used
  only by the test, or factor the check into a function that takes the root
  as a parameter and unit-test that function against a temporary clone.

## Out of scope

- Removing the leaked `[user]` section: already done 2026-09-23 (D5).
- Rewriting published history or force-pushing.
- The drift-patrol / `setup-hooks.sh --check` git-state check (str-qwua7.1).
- bd `created_by: Test` on existing issues (historical; see the str-qwua7.51 note).
- GitHub's web UI does not apply `.mailmap` to its contribution graph; that is
  accepted.

## Dependencies

None.
