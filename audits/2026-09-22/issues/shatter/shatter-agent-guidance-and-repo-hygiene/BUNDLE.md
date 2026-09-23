# BUNDLE: shatter-agent-guidance-and-repo-hygiene

- **Audit:** Shatter audit 2026-09-22 (final issue drafts; nothing is filed)
- **Bucket:** shatter-agent-guidance-and-repo-hygiene
- **Repo:** shatter
- **Tracker:** bd in /home/ketan/project/shatter (prefix str)
- **Parent epic (new issues):** "Epic: Audit 2026-09-22 findings"
- **Revision:** revised 2026-09-23 after the Codex cross-check (`../../crosscheck/shatter-agent-guidance-and-repo-hygiene.codex.md`); see `REVISION.md`.
- **Theme:** Repo-level agent guidance and git hygiene: identity (D5), git-state checks, fixture incident, skills, env-doctor decisions, AGENTS.md rtk/landing prose, completion and planning rules.

## Maintainer decisions (2026-09-23), which override the report and old drafts

- **D1 Releases:** KEEP Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix. Fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64), do not drop them. Release work closes only with a green release-run URL.
- **D2 shatter diff:** RETIRE the snapshot-diff command and the unused Snapshot writer path; spec-diff is THE regression tool. Update SPEC/README/QUICKSTART. The `diff` name becomes free; str-81xiw decides its later use. Correct the shatter-agents plugin's `shatter diff --staged` docs.
- **D3 Concolic positioning:** MEASURE FIRST. P1 default-vs-concolic benchmark; P1 fix concolic early termination; a follow-up decision issue (blocked by both) re-decides "concolic-first" positioning. No doc softening now.
- **D4 Beads hook stall:** bd's post-checkout hook spends about 6 min importing `.beads/issues.jsonl`. RETIRE the JSONL import; move tracker sync to a Dolt remote; first verify whether the stale import clobbered newer DB state; AGENTS.md drops `bd sync`; str-qwua7.28 superseded; bento beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT fix and no hook-bypass guidance.
- **D5 Git identity:** the leaked `[user]` section in the shared `.git/config` was ALREADY REMOVED on 2026-09-23. Draft a `.mailmap` (test@example.com "Test"/"Test User" -> Ketan Gangatirkar <33678+ketang@users.noreply.github.com>, no history rewrite), a git-state check (folded into str-qwua7.1), and a `test_git_fixture_isolation.py` `.git/config` snapshot.
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. No agent files anything.

## Contents

| NN | Slug | Kind | Target | P | Title |
|---|---|---|---|---|---|
| 01 | mailmap-and-fixture-config-snapshot | new | new | P1 | Map the leaked fixture identity (test@example.com) to the real author via .mailmap, and make test_git_fixture_isolation.py guard the real checkout's .git/config |
| 02 | qwua7-1-git-state-check | note-to-existing | str-qwua7.1 | P1 | Note on str-qwua7.1: core.bare is repaired; re-scope to the git-state check (local identity override, *@example.com, core.bare, local hooksPath) |
| 03 | qwua7-51-identity-root-cause | note-to-existing | str-qwua7.51 | P2 | Note on str-qwua7.51: 'Owner: Test' most likely came from the leaked repo-local fixture identity (removed 2026-09-23); bd's actor override is BEADS_ACTOR, not BD_ACTOR; close-reason SHAs must be labelled |
| 04 | fixture-corruption-incident-reverify | new | new | P2 | Record the 2026-09-07 fixture-corruption incident and review its two recovery branches and four contaminated remote branches |
| 05 | agent-config-gitignore | new | new | P2 | Make intended .claude/ and .codex/ agent config trackable: repo .gitignore negations plus a check-ignore meta test |
| 06 | repo-skills-rot | new | new | P2 | Repair rotted repo skills: delete check-go/rust/ts and protocol-sync, move audit and bugfix suite runs onto governed gates, fix audit skill paths/steps and add its memory-contradiction check |
| 07 | env-doctor-decisions | note-to-existing | str-qwua7.53 | P2 | Note on str-qwua7.53: its interim step (agent_env_doctor_skip_plugin=bugshot) was never applied; do it now, independent of bgs-3tq |
| 08 | qwua7-23-agents-md-rtk-and-landing | note-to-existing | str-qwua7.23 | P2 | Note on str-qwua7.23: etiquette rules inside the rtk-managed block, rtk 'always safe' text, landing prose contradicting land.py, merged remote branches (bd sync -> D4 Dolt remote) |
| 09 | completion-checklist-spec-docs | new | new | P2 | Require SPEC section + changelog + Last-updated updates for CLI-visible changes (checked semantically by /pre-completion), and run stories-impact-check before behavioural edits |
| 10 | planning-rules-location-and-open-decisions | new | new | P2 | Planning rules in CLAUDE.md: plans/specs go in docs/plans and docs/specs (overriding the superpowers default), and check open tracker decisions before planning |
| 11 | 35vtk-9-swarm-config | note-to-existing | str-35vtk.9 | P3 | Note on str-35vtk.9: bento swarm no longer reads .claude/swarm-config.md; batch-landing claim in CLAUDE.md is unbacked |
| 12 | qwua7-14-reverify-on-main | reopen-note | str-qwua7.14 | P1 | Reopen str-qwua7.14: its 'not reproducible' closure cited e50fc399, a stray fixture commit that is not on origin/main; re-verify on an origin/main build |
| 13 | orphan-worktree-dirs-cleanup | new | new | P3 | Review the six orphan worktree directories (five doctor-flagged under ~/.local/share/worktrees/shatter plus .claude/worktrees/str-umw3) and remove or retain each with operator approval |
| 14 | u394l-4-skill-command-lint | note-to-existing | str-u394l.4 | P2 | Note on str-u394l.4: skill-command lint requirements from the 2026-09-22 audit (subcommand resolution, not --help exit codes; governed suite runs; Taskfile parsing without task --list-all) |
| 15 | qwua7-52-storystore-interim-nudge | note-to-existing | str-qwua7.52 | P2 | Note on str-qwua7.52: storystore adoption is blocked on the storystore clap extractor and stale plugin cache; set a dated remind_after meanwhile |

---

<!-- file: 01-mailmap-and-fixture-config-snapshot.md -->

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
it had already done, so nearly every commit made in the primary checkout from
about 2026-06-23 was authored `Test` or `Test User <test@example.com>`, and
all were pushed to GitHub. The maintainer removed the leaked `[user]` section
on 2026-09-23 (decision D5). **That step is done and is not part of this
issue.**

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
   in 2026-06..09. A fixture that bypasses the sanitizer could rewrite the
   real config and this test would still pass.
3. **Unregistered fixtures are invisible.** The test only runs what is listed
   in `ENTRYPOINTS`. A fixture creator that nobody registered is never
   executed, so no before/after snapshot, of the sentinel or of the real
   config, can detect its leak. Several identity-writing fixtures are already
   unregistered (see Evidence). Guarding the real config therefore needs a
   **registration-completeness** check as well, or the protection claim must
   be narrowed to registered entrypoints.

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
  -> 177 `Test User`, 123 `Test`, 0 real. Since 2026-06-24 origin/main has
  about 6 real-identity commits against several hundred fixture-identity ones.
  The first leaked commit is `131ebe06` (2026-06-23, `Test User`).
- No `.mailmap` exists at the repo root.
- Fixtures that write this identity (any one of them hits the real repo if
  GIT_DIR leaks in):
  `scripts/test_target_dir_report_json.sh:27-28`,
  `scripts/test_cleanup_merged_remote_branches.sh:47-48,136-137`,
  `scripts/test_git_sandbox_test_lib.sh:31-32,67-68`,
  `scripts/test_git_sandbox_test_lib.py:93-94,134-135`,
  `scripts/test_walkthrough_examples_checkout.py:57,63` (`Test User`),
  `shatter-cli/tests/implicit_init_gitignore_test.rs:51-52` (`Test User`),
  `shatter-cli/src/commands/init.rs:379-380` (unit test, `Test User`),
  `shatter-cli/src/generated_paths.rs:750-752` (unit test, `Test`),
  `shatter-core/src/scm.rs:709` (`t@example.com`).
- `scripts/test_git_fixture_isolation.py:19-31` (`ENTRYPOINTS`) lists only the
  shell and Python fixtures. None of the four Rust fixture sites above is
  registered, and nothing checks that a fixture creator is registered.
  `:50-111` snapshots only the temporary `caller` repo, never `ROOT`'s git
  common dir.
- `git grep -l -E 'user\.(email|name)'` over `scripts/`, `*/tests/`, `*/src/`
  also matches non-fixtures (for example `shatter-core/tests/e2e_concolic.rs:952`,
  a JS `props.user.name` string), so a completeness check needs an explicit
  allowlist, not a bare grep.
- The shared `$(git rev-parse --git-common-dir)/config` is legitimately
  rewritten by concurrent sessions (`branch.<name>.*` from `push -u`, remotes,
  worktree config). A whole-file digest would fail spuriously during swarms.
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
3. **Guarded key set, not a whole-file hash.** The test reads the real
   config (`git config --file <common-dir>/config --list`, common dir resolved
   with the test's `clean_env` so a leaked `GIT_DIR` cannot redirect it)
   before and after each `ENTRYPOINTS` command and around the whole run. It
   compares only the damage-class keys: `user.*`, `core.bare`,
   `core.hooksPath`, `core.worktree`, plus any newly added key whose value
   contains `example.com`, `example.invalid` or `example.org`. On a change it
   fails, naming the entrypoint and the key diff. It only reads the real
   config and never writes it. The guarded key list is a named constant in
   the test file.
4. **No false positive under concurrency.** A unit test adds a
   `branch.<x>.remote` entry to a temp repo's config between the before and
   after reads and asserts the guard passes. A second unit test adds
   `user.email=leak@example.com` and asserts it fails.
5. **The guard is a function that takes the repo root as a parameter**
   (required form, so the proof exercises the same code path as the real
   run). Failing-then-passing proof in the close reason: call it against a
   temporary clone with a throwaway entrypoint that runs
   `git -C "$root" config user.email leak@example.com`; record the failure,
   remove the entrypoint, record the pass. Never run the leaking probe
   against the real primary checkout.
6. **Registration completeness.** A second test enumerates tracked files
   under `scripts/`, `*/tests/` and `*/src/` that set a git identity
   (`git config user.email|user.name`, `-c user.email=`, or equivalent) and
   fails unless each is either reachable from an `ENTRYPOINTS` command or
   listed in a `NOT_EXECUTED = {path: reason}` allowlist in the test file.
   Close-time proof: the test fails on the current tree, listing at least the
   four Rust sites above, and passes after they are registered or allowlisted.
7. The four Rust fixture sites are either registered (a focused
   `cargo test -p <crate> <name>` entrypoint each) or allowlisted with a
   reason stating why they cannot reach a real repo. The choice is recorded
   in the comment block above `ENTRYPOINTS`. If any are allowlisted, the
   test file's docstring states that the real-config guard covers registered
   entrypoints only.
8. `task meta` passes, and `task affected` passes with its `Gates selected`
   output recorded.

## Suggested approach

- Add `guarded_config(root) -> dict[str, list[str]]` next to `snapshot()`,
  and call it around the existing loop body so both contamination modes are
  covered.
- For the completeness test, prefer `git grep -n -E` over tracked files with
  a pattern that targets git-config calls, and keep the allowlist small and
  explained.

## Out of scope

- Removing the leaked `[user]` section: already done 2026-09-23 (D5).
- Rewriting published history or force-pushing.
- The drift-patrol / `setup-hooks.sh --check` git-state check (str-qwua7.1).
- bd `created_by: Test` on existing issues (historical; see the str-qwua7.51 note).
- GitHub's web UI does not apply `.mailmap` to its contribution graph; that is
  accepted.

## Dependencies

None.

---

<!-- file: 02-qwua7-1-git-state-check.md -->

---
slug: qwua7-1-git-state-check
kind: note-to-existing
title: "Note on str-qwua7.1: core.bare is repaired; re-scope to the git-state check (local identity override, *@example.com, core.bare, local hooksPath)"
priority: P1
type: chore
labels: [agents, git, tooling, drift]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.1
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.1: re-scope to the git-state check

Target: **str-qwua7.1** (open, P1, "Repair primary checkout (core.bare=true)
and add a git-state hygiene check"). Action: `bd comments add str-qwua7.1`
with the text below. Do not close it and do not change its priority.

## Comment text

> Audit 2026-09-22 update (maintainer decision D5, 2026-09-23). Evidence:
> `audits/2026-09-22/findings.json` agent-repo-01, prior-04, prior-09.
>
> **Repair half is done or moving elsewhere:**
> - `core.bare` is already `false` in the primary checkout
>   (`git -C /home/ketan/project/shatter config --get core.bare` -> `false`).
> - Unregistered /tmp land-work previews: none registered at the time of the
>   audit (`git worktree list` shows none).
> - The five dead dirs under `~/.local/share/worktrees/shatter/` and the
>   `.claude/worktrees/str-umw3/` orphan still exist. Their operator-confirmed
>   removal (or documented retention) is now tracked by
>   <orphan-worktree-dirs-cleanup>. Drop them from this issue's acceptance.
> - A second instance of the same damage class was found: the fixture identity
>   `[user] name = Test, email = test@example.com` had leaked into the primary's
>   repo-local `.git/config` (str-jttrf leak). The maintainer removed it
>   2026-09-23. The `.mailmap` and fixture-side config guard are tracked in
>   <mailmap-and-fixture-config-snapshot>.
>
> **Re-scoped acceptance for this issue (the check only):**
> - A repo-state check (a new entry in `scripts/drift-patrol.py` `CHECKS`,
>   `:757`, as this issue already chose; optionally surfaced by
>   `scripts/setup-hooks.sh --check`) inspects **the checkout drift-patrol is
>   invoked from** (its repo root, not an arbitrary cwd such as a fixture
>   repo) and FAILs when any of these holds:
>   1. a repo-local `user.name` or `user.email` override exists
>      (`git config --local --get user.email` / `user.name` non-empty);
>   2. the effective `user.email` (any scope) matches `*@example.com` (also
>      `*.invalid` / `example.org`, if cheap);
>   3. `core.bare=true`;
>   4. a repo-local `core.hooksPath` override exists.
> - **Discovery precedence (this order, tested):** (a) locate the repository
>   with `git rev-parse --git-dir` / `--git-common-dir`, which succeed even
>   when `core.bare=true` makes `--is-inside-work-tree` return false; (b) read
>   the common dir's `config` directly (`git config --file <common>/config`)
>   and evaluate conditions 1-4; (c) only if no git directory can be
>   discovered at all, or the run is in CI, report SKIP. A checkout whose
>   config says `core.bare=true` must never be reported as "not a work tree ->
>   SKIP".
> - Unit tests in `scripts/test_drift_patrol.py` build a temporary repo per
>   condition and assert FAIL, plus one clean repo asserting PASS, plus one
>   directory with no repository asserting SKIP. One test sets
>   `core.bare=true` on a non-bare checkout and asserts **FAIL, not SKIP**.
>   Include a failing-then-passing run in the close reason.
> - `python3 scripts/drift-patrol.py` shows the check PASS on the primary
>   checkout. Record the output in the close reason.
> - The prunable-worktree / stale-preview / non-repo-dir detections from the
>   original body may stay as extra conditions. Keep them only if they are
>   unit-tested the same way.
>
> This unblocks str-qwua7.18 and str-qwua7.19, which are `blocked_by` .1 only
> because of the repair premise. Consider removing those edges once this
> re-scope is accepted.

---

<!-- file: 03-qwua7-51-identity-root-cause.md -->

---
slug: qwua7-51-identity-root-cause
kind: note-to-existing
title: "Note on str-qwua7.51: 'Owner: Test' most likely came from the leaked repo-local fixture identity (removed 2026-09-23); bd's actor override is BEADS_ACTOR, not BD_ACTOR; close-reason SHAs must be labelled"
priority: P2
type: task
labels: [agents, beads, git]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.51
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.51: probable root cause of "Owner: Test", actor variable, close-reason SHAs

Target: **str-qwua7.51** (open, P2, "Configure bd identity in the SessionStart
hook and require a close reason at landing"). Action: `bd comments add str-qwua7.51`
with the text below. The owner/maintainer then re-scopes the issue as the
comment proposes. Do not close it: the close-reason half is still valid.

## Comment text

> Audit 2026-09-22 root-cause correction (maintainer decision D5, 2026-09-23;
> evidence `audits/2026-09-22/findings.json` agent-repo-01, prior-04,
> agent-repo-16).
>
> **1. The environment variable in this issue is wrong.** This issue's body
> plans to export `BD_ACTOR` from a SessionStart hook. Installed bd 1.1.0
> does not read `BD_ACTOR`: `bd --help` documents
> `--actor string  Actor name for audit trail (default: $BEADS_ACTOR, git user.name, $USER)`.
> A hook that exports `BD_ACTOR` would change nothing.
>
> **2. Probable root cause (to be confirmed by the probe below).** The
> primary checkout's repo-local `.git/config` carried a leaked test-fixture
> identity (`[user] name = Test, email = test@example.com`), written there by
> the str-jttrf/str-y0rcz GIT_DIR fixture leak. With no `--actor` and no
> `BEADS_ACTOR`, bd's documented actor fallback is git `user.name`, so the
> leak would explain "Test" as the actor for every bd write from the primary;
> all issues created since 2026-09-05 show `Owner: Test`. The "Test User"
> variant matches fixtures that set `user.name "Test User"`
> (`scripts/test_walkthrough_examples_checkout.py:63`,
> `shatter-cli/tests/implicit_init_gitignore_test.rs:52`). This is inferred
> from the documented fallback and the matching names; it was not traced
> through bd's source.
>
> The maintainer removed the leaked `[user]` section on 2026-09-23. Now
> `git -C /home/ketan/project/shatter config --show-origin user.name` resolves
> to `~/.gitconfig` (Ketan Gangatirkar). Follow-ups: `.mailmap` and a fixture
> config guard in <mailmap-and-fixture-config-snapshot>; the recurrence check
> on str-qwua7.1.
>
> **3. Proposed re-scope of the identity half.** bd records three distinct
> identities; check each separately before dropping anything:
> - **actor** (audit trail / event author): `--actor` > `$BEADS_ACTOR` > git
>   `user.name` > `$USER`, per `bd --help`;
> - **assignee**: set by `bd update <id> --claim` ("sets assignee to you");
> - **owner / created_by**: set at create time; its source is not
>   documented in `bd create --help`.
>
> Probe, in a fresh session from the primary checkout with `BEADS_ACTOR`
> unset: create a scratch issue, claim it, close it, then
> `bd show <scratch-id> --json` and record owner, created_by, assignee and
> the event actor, plus `echo "${BEADS_ACTOR-unset}"` and
> `git config --show-origin user.name`. Delete the scratch issue afterwards.
> If all four show the real name, record that and drop the SessionStart
> identity requirement. If any is wrong, the fix sets **`BEADS_ACTOR`** (not
> `BD_ACTOR`) or the documented config key, and the probe is re-run to show
> the corrected value.
>
> **4. Keep the close-reason half, with one precision.** Every landing close
> carries a SHA or a duplicate/won't-do reason, and drift-patrol warns on
> reasonless closes. Add: a close reason (or diagnosis) that cites a commit
> must say what that commit is. A claim that work **is landed** or that
> behaviour was checked **on main** must cite a SHA for which
> `git merge-base --is-ancestor <sha> origin/main` exits 0. Any other SHA
> (an unmerged reproduction, a feature-branch fix, a bisect point) is allowed
> but must be labelled as such, for example "tested on feature branch
> `<branch>` at `<sha>` (not on main)". Motivating case: str-qwua7.14 was
> closed "Not reproducible against current main (e50fc399)", but `e50fc399`
> is a stray fixture commit that is not an ancestor of origin/main (see
> <fixture-corruption-incident-reverify> and <qwua7-14-reverify-on-main>).
>
> **5. Housekeeping.** Update the body's bd facts: the installed bd is now
> **1.1.0**, not v0.63.3. Re-check the `bd close` reason flag against
> `bd close --help` on 1.1.0. Existing issues keep `created_by: Test`
> (historical). Do not bulk-edit them.

---

<!-- file: 04-fixture-corruption-incident-reverify.md -->

---
slug: fixture-corruption-incident-reverify
kind: new
title: "Record the 2026-09-07 fixture-corruption incident and review its two recovery branches and four contaminated remote branches"
priority: P2
type: task
labels: [agents, git, governance, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Record the 2026-09-07 fixture-corruption incident and review its two recovery branches and four contaminated remote branches

## Problem

On 2026-09-07 the GIT_DIR fixture leak (fixed later by str-jttrf / str-y0rcz)
created a stray commit `e50fc399` ("init", author `Test <test@example.com>`).
The commit deletes 12,090 lines: among other things it removes `shatter-vs/`,
deletes all three `.claude/agents/*/AGENT.md` files and rewrites
`.beads/issues.jsonl`. The recovery was done ad hoc and never tracked:

- Two local `recovery/*` branches from 2026-09-12 have never been reviewed.
- Four remote `str-qwua7.*` feature branches contain the stray commit.
  str-qwua7.4's close reason says the duplicates were "both deleted as
  superseded", but they still exist on origin.
- str-qwua7.14 (P1 bug) was closed as "Not reproducible against current main
  (e50fc399)". `e50fc399` is not on main, so that diagnosis may have run
  against a corrupted tree. Its re-verification is split out as
  `qwua7-14-reverify-on-main`; the close-reason SHA-labelling rule is added
  to str-qwua7.51 (note `qwua7-51-identity-root-cause`).

This issue is the incident record plus a review of the six branches. It is
complete when every branch has a recorded disposition. **Deleting a branch is
optional and needs explicit operator approval; a declined deletion is a valid
outcome, not a blocker.**

## Evidence

Re-verified 2026-09-23 in `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`
(origin/main = 70465921; remote-tracking refs as of the last fetch):

- `git merge-base --is-ancestor e50fc399 origin/main` -> exit 1 (NOT on main).
- `git show -s --format='%h %ad %an %s' --date=short e50fc399` ->
  `e50fc399 2026-09-07 Test init`. `git show --shortstat --format= e50fc399` ->
  `82 files changed, 620 insertions(+), 12090 deletions(-)`.
- `git show --name-status --format= e50fc399 -- .claude/agents` ->
  `D .claude/agents/go-dev/AGENT.md`, `D .claude/agents/rust-dev/AGENT.md`,
  `D .claude/agents/ts-dev/AGENT.md` (deletions, not restorations).
- `git branch --list 'recovery/*' -v` ->
  `recovery/shatter-index-20260912-11_4ty6m a6f4cbc0 recovery: preserve captured Shatter index`,
  `recovery/shatter-main-20260912-11_4ty6m e50fc399 init`.
- `git branch -r --contains e50fc399` ->
  `origin/str-qwua7.16-restore-bd-dolt`, `origin/str-qwua7.17-stale-claims-cleanup`,
  `origin/str-qwua7.4-testplan-http-body-fix`, `origin/str-qwua7.7-protocol-registry-validate`.
- `bd show str-qwua7.14` -> CLOSED, close reason begins
  "Not reproducible against current main (e50fc399)".
- `bd search recovery` finds no tracking issue.
- Audit sources: agent-repo-16, prior-06 (`audits/2026-09-22/findings.json`,
  `audits/2026-09-22/areas/agent-repo.md`, on branch `audit-2026-09-22`).

## Acceptance criteria

1. This issue (a comment or a linked `docs/` note) records the incident: a
   timeline (2026-09-07 stray commit, 2026-09-12 recovery branches, str-jttrf /
   str-y0rcz fixes), the cause (fixture `git` calls under a leaked `GIT_DIR`),
   and the blast radius (the 4 remote branches, the 2 recovery branches, the
   leaked repo-local identity handled in `mailmap-and-fixture-config-snapshot`,
   and str-qwua7.14 handled in `qwua7-14-reverify-on-main`).
2. Each of the six branches gets a disposition table row, recorded in this
   issue, with: the command output of `git diff origin/main...<branch> --stat`;
   a list of content not already on origin/main (or "none"); for the four
   `str-qwua7.*` branches, the origin/main SHA where that issue's real work
   landed (verified with `git merge-base --is-ancestor <sha> origin/main`) or
   "did not land"; and a disposition: `salvage` (wanted content filed as a new
   issue or landed, with its id/SHA), `delete` or `retain`.
3. For each `delete` disposition, the operator's explicit approval is quoted
   in the issue before deletion. After deletion,
   `git ls-remote origin 'refs/heads/str-qwua7*'` (remote) or
   `git branch --list 'recovery/*'` (local) no longer lists that branch.
4. For each `retain` disposition (including a declined deletion), the issue
   records the reason and marks the branch "known-contaminated: contains
   e50fc399, do not merge". The issue can close with retained branches.
5. The close reason links the incident record and the disposition table.

## Suggested approach

- Do the review in a scratch linked worktree, never in the primary checkout.
- A content check beyond `--stat`: `git log --oneline origin/main..<branch>`
  and `git diff origin/main...<branch> -- ':!.beads'` for anything that is not
  part of the stray deletion.

## Out of scope

- Re-verifying str-qwua7.14 (`qwua7-14-reverify-on-main`).
- The close-reason SHA rule (str-qwua7.51, via `qwua7-51-identity-root-cause`).
- Enforcing an ancestor-SHA rule in bento land-work (bento tracker).
- The general merged-branch sweep (`scripts/cleanup-merged-remote-branches.sh`;
  see the str-qwua7.23 note).
- Fixture-leak prevention (`mailmap-and-fixture-config-snapshot`, str-qwua7.1).

## Dependencies

None.

---

<!-- file: 05-agent-config-gitignore.md -->

---
slug: agent-config-gitignore
kind: new
title: "Make intended .claude/ and .codex/ agent config trackable: repo .gitignore negations plus a check-ignore meta test"
priority: P2
type: task
labels: [agents, git, skills, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Make intended .claude/ and .codex/ agent config trackable: repo .gitignore negations plus a check-ignore meta test

## Problem

New agent config files are silently left untracked:

- The maintainer's global gitignore (`~/.config/git/ignore:35-36`) ignores
  `.claude/` and `.codex/`.
- The repo's own `.gitignore:134` also ignores `.codex/`.

So a new skill under `.claude/skills/`, or the `.codex/AGENTS.md` decided in
str-qwua7.54, would not show up in `git status`. The 17 tracked files under
`.claude/` exist only because someone force-added them. Nothing tests that
agent config the project means to track is actually trackable.

## Evidence

Re-verified 2026-09-23 in `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`:

- `git check-ignore -v --no-index .claude/skills/newskill/SKILL.md .codex/AGENTS.md .claude/settings.local.json` ->
  `/home/ketan/.config/git/ignore:35:.claude/  .claude/skills/newskill/SKILL.md`,
  `.gitignore:134:.codex/  .codex/AGENTS.md`,
  `/home/ketan/.config/git/ignore:35:.claude/  .claude/settings.local.json`.
- Repo `.gitignore:130` `.claude/settings.local.json`, `:134` `.codex/`,
  `:165` `.claude/worktrees/`. There is no `!` negation.
- `git ls-files .claude | wc -l` -> 17. `git ls-files .codex | wc -l` -> 0.
  The primary checkout's `.codex/` holds untracked symlinks (`skills` ->
  `../.claude/skills`, `swarm-config.md`) and a `worktrees/` dir. Un-ignoring
  `.codex/` wholesale would surface these as untracked.
- `task meta` (`Taskfile.yml:396`) is the home for repo meta tests; its
  `sources:` list must include any new test file, or the checksum cache will
  skip it.
- Audit source: agent-repo-10 (`audits/2026-09-22/findings.json`,
  `audits/2026-09-22/areas/agent-repo.md`).

## Acceptance criteria

1. Repo `.gitignore` un-ignores `/.claude/` (`!/.claude/`) and re-ignores
   `/.claude/settings.local.json` and `/.claude/worktrees/`. It replaces the
   blanket `.codex/` ignore with exactly: ignore `/.codex/*`, un-ignore
   `!/.codex/AGENTS.md`. Everything else under `.codex/` (the `skills` and
   `swarm-config.md` symlinks, `worktrees/`, session state) stays ignored.
   Tracking any further `.codex/` file needs its own `!` line and test case.
2. A meta test (for example `scripts/test_agent_config_trackable.py`) is wired
   into `task meta` `cmds:` and `sources:`. It asserts that
   `git check-ignore -q --no-index` **fails** (not ignored) for
   `.claude/skills/x/SKILL.md` and `.codex/AGENTS.md`, and **succeeds**
   (ignored) for `.claude/settings.local.json`, `.claude/worktrees/x`,
   `.codex/skills`, `.codex/swarm-config.md` and `.codex/worktrees/x`. It
   runs with the maintainer's real global excludes, and also with
   `core.excludesFile` pointing at a temp file containing `.claude/` and
   `.codex/`, so it holds on CI where the global file is absent.
3. Failing-then-passing proof in the close reason: the test run before the
   `.gitignore` change (fails) and after (passes).
4. `git status --porcelain` in a scratch worktree shows a newly created
   `.claude/skills/probe/SKILL.md` as `??` and shows nothing under `.codex/`
   in the primary checkout (recorded, then the probe is deleted).
5. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

A negation in the repo `.gitignore` overrides the global excludes file,
because repo `.gitignore` has higher precedence than `core.excludesFile`. Keep
the negations near the existing `.claude/` lines (130/165) so the intent is
visible.

**Possible alternative (not filed):** narrow the global ignore in dotfiles
(`~/.config/git/ignore:35-36`) to `.claude/settings.local.json`,
`.claude/worktrees/` and codex session state. That would fix every repo at
once. It is a dotfiles-side change, so it is mentioned here only as an option.
The repo-level negation plus test is still needed, so shatter does not depend
on each developer's global config.

## Out of scope

- Writing `.codex/AGENTS.md` itself (str-qwua7.54).
- Removing the `.claude/worktrees/str-umw3/` orphan (moved to
  `orphan-worktree-dirs-cleanup`).
- Changing the dotfiles global gitignore (see the alternative above; no
  dotfiles issue is filed from this audit).

## Dependencies

None.

---

<!-- file: 06-repo-skills-rot.md -->

---
slug: repo-skills-rot
kind: new
title: "Repair rotted repo skills: delete check-go/rust/ts and protocol-sync, move audit and bugfix suite runs onto governed gates, fix audit skill paths/steps and add its memory-contradiction check"
priority: P2
type: task
labels: [agents, skills, docs, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [beads-retire-jsonl-import-dolt-remote]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Repair rotted repo skills: delete check-go/rust/ts and protocol-sync, move audit and bugfix suite runs onto governed gates, fix audit skill paths/steps and add its memory-contradiction check

## Problem

Several repo skills under `.claude/skills/` tell agents to do things the
project forbids, or things that no longer work:

- `check-go`, `check-rust` and `check-ts` run bare `go test ./...`,
  `cargo test` and `npm test`. Nothing references these three skills.
- `protocol-sync` hand-compares three protocol files. It ignores
  `protocol/registry.yaml`, the generated bindings and shatter-rust, all of
  which `task parity` and `scripts/protocol-codegen.py --check` already check
  mechanically.
- The `audit` skill runs bare suites in Phase 1, names a root `GLOSSARY.md`
  that does not exist, uses the invalid `bd epic list`, samples only the last
  20 commits in Phase 7, and in its post-audit step defers beads changes to
  `bd sync`, a command that no longer exists in bd 1.1.0. Under maintainer
  decision D4 (2026-09-23) the JSONL import and `bd sync` are retired and
  tracker sync moves to a Dolt remote. Phase 7 also points at the wrong
  memory path and never checks memory against repo facts. Stale project
  memory that told agents to bypass hooks with `--no-verify` /
  `core.hooksPath=/dev/null` went undetected through the 2026-09-04 audit.
- The `bugfix` skill runs bare module suites for its regression step.

**Why the fix is not simply "use `task <ns>:test`".** The project's
machine-wide heavyweight-slot governance (str-35vtk.5) lives in
`scripts/gate-wrapper.sh`, and only some tasks call it. `task core:test`
(`shatter-core/Taskfile.yml:14-30`) runs `cargo nextest` / `cargo test`
directly, so invoking it alone skips the slot semaphore, nice/ionice and the
timing CSV exactly as a bare `cargo test` does. Suite runs in skills must use
a **governed** invocation.

The skill-content lint that would catch this class of rot is owned by
str-u394l.4; this audit's requirements for it are in the note
`u394l-4-skill-command-lint`, not here.

## Evidence

Re-verified 2026-09-23 in `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`:

- `.claude/skills/check-go/SKILL.md:10` "Run `go test ./...` in `shatter-go/`";
  `check-rust/SKILL.md:10` "Run `cargo test` in the workspace root";
  `check-ts/SKILL.md:10` "Run `npm test` in `shatter-ts/`". A repo-wide grep
  for `check-go|check-rust|check-ts|protocol-sync` outside the skills
  themselves and `audits/` finds no references.
- `.claude/skills/protocol-sync/SKILL.md:10-14` reads only
  `shatter-core/src/protocol.rs`, `shatter-ts/src/protocol.ts`,
  `shatter-go/protocol/types.go` and optional `protocol/schemas/`.
- `.claude/skills/audit/SKILL.md`: `:17-22` bare `cargo test`, `cargo clippy`,
  `npm test`, `go test ./...`; `:72` lists `GLOSSARY.md` (only
  `docs/GLOSSARY.md` exists); `:115` `bd epic list` (bd 1.1.0 `bd epic` has
  only `close-eligible` and `status`); `:151` "Memory files in
  `.claude/projects/*/memory/`" (the real location is
  `~/.claude/projects/-home-ketan-project-shatter/memory/`); `:153`
  "Recent git log (last 20 commits)"; `:385` "Do NOT commit beads issue
  changes — those are handled by `bd sync`".
- `.claude/skills/bugfix/SKILL.md:26-31` and `:60-62` (single-test red/green
  commands), `:65-70` (module suites: `cargo test`, `cd shatter-ts && npm test`,
  `cd shatter-go && go test ./...`, `cd shatter-rust && cargo test`).
- Governed entry points: `task affected` (`Taskfile.yml:511-514`,
  `bash scripts/gate-wrapper.sh affected task affected-governed`),
  `task check`, `task parity`, `task conformance`, `task e2e*`. Namespace
  test tasks (`core:test`, `go:test`, `ts:test`, ...) are **not** wrapped.
  `gate-wrapper.sh` appends a row per governed run to
  `~/.cache/shatter/gate-times.csv` (`timestamp,worktree,label,...`).
- `bd sync --help` on bd 1.1.0 -> `Error: unknown command "sync"`.
- Audit sources: agent-repo-14, plus the audit-skill part of sessions-03 /
  agent-repo-08 (drafts `shatter-agent/14`, `shatter-agent/07`).

## Acceptance criteria

1. `check-go`, `check-rust`, `check-ts` and `protocol-sync` are deleted
   (preferred: nothing references them). If any is kept instead, it invokes
   only a governed gate (`task parity`, `task affected`, or
   `bash scripts/gate-wrapper.sh <label> task <ns>:test`) and the reason is
   in the close reason.
2. Suite-level runs in the `audit` and `bugfix` skills use a governed
   invocation: `task affected` for "run the affected suites", `task check`
   for the full gate, or `bash scripts/gate-wrapper.sh <label> task <ns>:test`
   for one namespace. No skill prescribes an unwrapped `task <ns>:test` or a
   bare suite command for a suite-level run.
3. The single-test red/green commands in `bugfix` may stay targeted
   (`cargo test -p <crate> <name>`, `go test -run <Name> ./<pkg>`,
   `npx jest -t <pattern>`), each preceded by a one-line allowlist comment
   saying why a focused single test is exempt from gate governance. Use the
   comment form that str-u394l.4's lint accepts (see
   `u394l-4-skill-command-lint`).
4. Audit skill fixes: `:72` -> `docs/GLOSSARY.md`; `:115` -> `bd epic status`;
   Phase 7 covers commits since the previous audit's SHA (fallback: last
   ~150); `:151` names the real memory path.
5. The audit skill's Phase 7 gains a **memory-contradiction step**: grep the
   project memory dir for hook-bypass advice
   (`grep -rnE -- '--no-verify|hooksPath' <memory dir>`, where an explanatory
   "do not" mention is allowed) and for claims that contradict AGENTS.md or
   current repo state (for example `core.bare`, the installed `bd version`,
   commands AGENTS.md no longer names). The findings are listed in the report.
6. The audit skill's `:385` `bd sync` reference is replaced by the D4
   procedure that `beads-retire-jsonl-import-dolt-remote` records in AGENTS.md:
   do not hand-commit `.beads/` files; tracker state lives in the local Dolt
   DB and reaches other machines via the Dolt remote. No `bd sync`, no
   JSONL-export commit, no hook-timeout variable and no hook-bypass
   instruction remains. The rest of the post-audit landing restructure
   belongs to `publish-audit-reports`; whichever of the two lands second
   rebases onto the other.
7. **Close-time proof (all recorded in the close reason):**
   - `grep -rnE 'cargo test|go test|npm test|bd sync|bd epic list|task [a-z-]+:test' .claude/skills/`
     before the change (24 matches on 2026-09-23) and after. After the
     change no match remains in the deleted skills, in audit Phase 1
     (`:17-22`), at audit `:115`/`:385`, or in bugfix's module-suite step;
     every remaining match is listed in the close reason with its class:
     allowlisted single test (item 3, with its comment), prose mention (for
     example audit `:172`, optimize-tokens `:74`), or a command CLAUDE.md
     itself prescribes (frontend-parity `:86`, the E2E suites);
   - one run of each governed invocation the skills now name, with the
     matching `gate-times.csv` row (label and exit code) showing it went
     through `gate-wrapper.sh`;
   - `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Do the deletions first (smallest diff), then the audit and bugfix edits.
`.codex/skills` in the primary checkout is a symlink to `../.claude/skills/`,
so deletions also disappear there.

## Out of scope

- The skill-command lint itself (str-u394l.4; requirements in
  `u394l-4-skill-command-lint`).
- Making every namespace test task governed (a Taskfile change; if wanted,
  file separately).
- The audit skill's post-audit landing flow (report via launch-work/land-work
  before filing): `publish-audit-reports`.
- AGENTS.md and `.beads/PRIME.md` `bd sync` removal: `beads-jsonl-consumers-drop-bd-sync`.
- Editing the memory files themselves: corrected by the maintainer on 2026-09-23.
- The frontend-parity skill (separate audit issue).

## Dependencies

- Blocked by `beads-retire-jsonl-import-dolt-remote` (bucket
  shatter-tracker-and-beads), for acceptance item 6 only: the skill must cite
  the sync procedure that issue records. Items 1-5 can start at once.
- Related: str-u394l.4 (agent-rules drift lint), str-qwua7.22 (audit Phase-10
  rewrite), str-qwua7.26.

---

<!-- file: 07-env-doctor-decisions.md -->

---
slug: env-doctor-decisions
kind: note-to-existing
title: "Note on str-qwua7.53: its interim step (agent_env_doctor_skip_plugin=bugshot) was never applied; do it now, independent of bgs-3tq"
priority: P2
type: chore
labels: [agents, tooling]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.53
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.53: apply the bugshot interim step now

Target: **str-qwua7.53** (open, P2, "Wire bugshot for walkthrough output once
bugshot supports CLI capture (bgs-3tq)"). Action: `bd comments add str-qwua7.53`
with the text below. Do not change its priority.

This draft was a new issue in the first revision. The cross-check found that
str-qwua7.53 and str-qwua7.52 already own these decisions and their
"no SessionStart nudge" acceptance, so it is now a note here plus a sibling
note on str-qwua7.52 (`qwua7-52-storystore-interim-nudge`). The orphan
worktree directories that the old draft also covered moved to
`orphan-worktree-dirs-cleanup`.

## Comment text

> Audit 2026-09-22 (findings agent-repo-15, plugins-08;
> `audits/2026-09-22/findings.json`).
>
> This issue's decision says "Until then set
> `agent_env_doctor_skip_plugin=bugshot` in `.agent-mode.local` so the nudge
> stops". That interim step was never applied, so every SessionStart still
> prints "bugshot dormant — decision pending", and agents learn to skip
> SessionStart output (hiding new warnings).
>
> Evidence, re-verified 2026-09-23:
> - `cat /home/ketan/project/shatter/.agent-mode.local` -> `dangerous`,
>   `agent_env_doctor_seen=bugshot,storystore`,
>   `agent_env_doctor_superpowers_pointer_seen=true`. No
>   `agent_env_doctor_skip_plugin` key.
> - The doctor recognises the key:
>   `/home/ketan/project/bento/plugins/claude/bento/hooks/scripts/agent-env-doctor.py:69,72`
>   (`RECOGNIZED_AGENT_MODE_KEYS`).
> - Cross-repo dependency: the permanent wiring waits on bugshot **bgs-3tq**
>   (CLI capture template). Its priority raise is proposed separately in
>   the bugshot tracker.
>
> **Proposed checkpoint inside this issue (do it now; it does not wait on
> bgs-3tq, and this issue stays open for the real wiring):**
> 1. Add `agent_env_doctor_skip_plugin=bugshot` to
>    `/home/ketan/project/shatter/.agent-mode.local` (untracked per-checkout
>    state; record the action, there is no commit).
> 2. Proof: run the doctor with the same invocation its SessionStart hook
>    uses (bento `hooks.json`) from the primary checkout, before and after,
>    and paste both outputs: bugshot appears before and not after.
> 3. Known limit: linked worktrees get their own `.agent-mode.local`, so
>    they still show the nudge. That is a bento-side bug tracked in the bento
>    audit bucket; note it, do not work around it here.

---

<!-- file: 08-qwua7-23-agents-md-rtk-and-landing.md -->

---
slug: qwua7-23-agents-md-rtk-and-landing
kind: note-to-existing
title: "Note on str-qwua7.23: etiquette rules inside the rtk-managed block, rtk 'always safe' text, landing prose contradicting land.py, merged remote branches (bd sync -> D4 Dolt remote)"
priority: P2
type: task
labels: [agents, docs]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.23
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.23: rtk block, landing prose, merged branches

Target: **str-qwua7.23** (open, P2, "Cut AGENTS.md by ~40%, delegate
procedures to bento skills, pin a byte-budget test"). Action: post ONE comment
with `bd comments add str-qwua7.23` using the text below. This combines the
old drafts `shatter-agent/17` and `shatter-docs-ui/30`.

## Comment text

> Audit 2026-09-22 re-confirms this issue's scope and adds four items.
> Line numbers were re-verified 2026-09-23 at origin/main 70465921.
> Evidence: `audits/2026-09-22/findings.json` plugins-12, agent-repo-09,
> agent-repo-12; `audits/2026-09-22/areas/{plugins-guidance,agent-repo}.md`.
>
> **1. Project rules still live inside the rtk-managed block.**
> `AGENTS.md:528` opens `<!-- headroom:rtk-instructions -->`. `:535`
> `## Shared-Machine Resource Etiquette (str-35vtk.5)` (heavyweight slots,
> cargo/go parallelism budgets, gate timing) sits inside it, and `:594` closes
> it. The dotfiles guidance (`codex/AGENTS.md:10-12`) says rtk's tooling
> manages these markers per repo, so an rtk re-sync would probably delete the
> etiquette rules. (That overwrite was inferred from the marker semantics and
> not executed.) Acceptance addition: the etiquette section sits outside the
> markers, next to the gate docs.
>
> **2. The rtk block text contradicts the tool-precedence rules.** `:531`
> "When running shell commands, **always prefix with `rtk`**" and `:533` "it
> is always safe to use". Project memory records rtk corrupting file
> redirects and serving stale git ref data. The global rule says dedicated
> Read/Grep tools come before any shell command. `Key Commands` (`:560`)
> advertises `rtk cargo test` (a bare test command) and `Rules` (`:590`)
> shows `rtk git add .`. The dotfiles claim that "the RTK precedence rule
> lives inside the markers" is false for this repo: the only "precedence" in
> AGENTS.md is `:73`, about `CARGO_TARGET_DIR`. Acceptance addition: after
> moving the etiquette section out, the block body (or a project section just
> above it) states the precedence: dedicated tools first; task facade over
> bare test commands; rtk optional and never for `find` with predicates,
> file redirects or landing-gating git ref reads. Coordinate the block-template
> wording with the dotfiles/rtk template owner (dotfiles #3 and #10 are closed
> but the text persists here, in bugshot `AGENTS.md:89` and in shatter-agents
> `AGENTS.md:36`).
>
> **3. Landing prose contradicts the shipped bento `land.py` driver.**
> - `AGENTS.md:104-133` "Landing the Plane" prescribes
>   `git push --force-with-lease` (`:118`), `git checkout main` (`:119`) and
>   `git merge --no-ff <issue-branch>` (`:121`).
> - `:125` ends with "`bd sync` once here so the landing yields a single sync
>   commit".
> - `:255-285` "Git Workflow" repeats the manual merge and cleanup.
> - `:340-369` "Beads Sync Cadence" is built around `bd sync`.
> - `:370-375` allows a "transient" `core.hooksPath` bypass.
> - `:513` says "**Never** run `git worktree remove`".
>
> bento land-work (`land.py`: lease, preview worktree, verifier, teardown) is
> the actual path. `bd sync` does not exist in bd 1.1.0.
>
> Under maintainer decision **D4 (2026-09-23)**, the beads JSONL import and
> `bd sync` are retired and cross-machine tracker sync moves to a Dolt remote.
> Replacement landing text should therefore say:
> - `bd` state changes (claim/update/close) write the local Dolt DB;
> - landing does **not** commit `.beads/*.jsonl`;
> - tracker state is shared with `bd dolt push` / `bd dolt pull` against the
>   Dolt remote configured by the new audit issue
>   `beads-retire-jsonl-import-dolt-remote`, following the procedure that
>   issue records once in AGENTS.md.
>
> Do not add a `bd sync` step, a JSONL-export commit, a hook-timeout env var,
> or any hook-bypass exception. Drop the `:370-375` "transiently bypass"
> clause. The line-level `bd sync` removal across AGENTS.md, `.beads/PRIME.md`
> and skills is owned by `beads-jsonl-consumers-drop-bd-sync`. This issue owns
> replacing the landing sections with a pointer to `bento:land-work` plus the
> shatter-specific inputs (verifier manifest, `task affected` /
> `task check` gates, the Dolt-remote sync step).
>
> **4. Merged remote branches pile up.** `git for-each-ref --merged origin/main refs/remotes/origin`
> -> 37 merged refs of 64 remote-tracking refs on 2026-09-23 (both counts
> include `origin/HEAD`; they drift daily, so re-run the command before
> acting).
> AGENTS.md calls cleanup "mandatory", but nothing runs it. Four unmerged
> branches (`origin/str-qwua7.4-testplan-http-body-fix`,
> `.7-protocol-registry-validate`, `.16-restore-bd-dolt`,
> `.17-stale-claims-cleanup`) contain the stray fixture commit `e50fc399`;
> their review and deletion are tracked by the new audit issue
> `fixture-corruption-incident-reverify`. Acceptance addition: run
> `scripts/cleanup-merged-remote-branches.sh` (dry-run, operator review, then
> `--execute`) once after the landing prose is replaced. Its in-progress
> protection currently reads the stale `.beads/issues.jsonl`
> (`AGENTS.md:274-276`), so run it only after
> `beads-jsonl-consumers-drop-bd-sync` has moved it off the JSONL, or review
> the list by hand against `bd list --status in_progress`.
>
> Size: AGENTS.md is still 30,731 bytes (+ CLAUDE.md 8,617 = 39,348), so this
> issue's 12 KB / 20 KB budget is unchanged.

---

<!-- file: 09-completion-checklist-spec-docs.md -->

---
slug: completion-checklist-spec-docs
kind: new
title: "Require SPEC section + changelog + Last-updated updates for CLI-visible changes (checked semantically by /pre-completion), and run stories-impact-check before behavioural edits"
priority: P2
type: task
labels: [agents, docs, skills, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Require SPEC section + changelog + Last-updated updates for CLI-visible changes (checked semantically by /pre-completion), and run stories-impact-check before behavioural edits

## Problem

CLI-visible changes land without SPEC, QUICKSTART or changelog updates because
no agent checklist asks for them. SPEC.md itself asks for a changelog row per
CLI-visible change, but that rule lives inside SPEC, which the landing flow
never reads. This audit found the results:

- changelog rows claiming section updates that were never made;
- missing rows for the 2026-09-14 and 2026-09-19 CLI changes;
- `--failure-threshold` still documented after it was removed;
- a stale "Last updated" header.

A check that only asks "did `SPEC.md` change?" would not have caught the
first item (a row was added, the section was not), and a check that only
watches `args.rs` misses exit-code and output-shape changes made in
`main.rs`, `helpers.rs`, the renderers or `commands/`.

Separately, `storystore:stories-impact-check` is a hard-trigger skill that
must run **before** behavioural edits to user-facing surfaces. Listing it
only in a completion checklist would surface protected intent after the
change is already written.

## Evidence

Re-verified 2026-09-23 in `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`:

- `grep -nE 'SPEC|changelog|QUICKSTART|stories' CLAUDE.md .claude/skills/pre-completion/SKILL.md`
  -> no matches.
- The Completion Checklist is `CLAUDE.md:43-53` (items 1-7, all test or
  parity gates). The only doc rule is `CLAUDE.md:80`: "Update README.md when
  build/run/config procedures change."
- `SPEC.md:7` holds the rule: "Any CLI-visible change (new command,
  new/renamed/removed flag, changed default, changed output shape) should add
  a row to the changelog". The changelog is `SPEC.md:1173` (`## 8. Changelog`,
  a `| Date | Change | Section |` table, newest row first), and CLI commands
  are `## 2.` at `:68` with `### 2.N` subsections.
- `SPEC.md:3` "Last updated: 2026-09-09", but
  `git log --format='%h %ad' --date=short -- SPEC.md` shows edits on
  2026-09-14 (21981b1d) and 2026-09-19 (2de05fd9).
- CLI-visible code lives in more than `args.rs`: exit codes are set in
  `shatter-cli/src/main.rs`, `shatter-cli/src/helpers.rs` and
  `shatter-cli/src/commands/build_frontend.rs` (`process::exit` /
  `ExitCode`); output is rendered in `shatter-cli/src/render.rs`,
  `shatter-cli/src/commands/*.rs`, `shatter-core/src/report.rs` and
  `shatter-core/src/reporter.rs`.
- `storystore:stories-impact-check` is not referenced in CLAUDE.md, AGENTS.md
  or `.claude/`. Its skill description makes it a hard trigger "before any
  behavioral change to user-facing surfaces", and it keys on
  `docs/stories/INDEX.md`, which does not exist yet (adoption is
  str-qwua7.52).
- `/pre-completion` already emits a summary table
  (`.claude/skills/pre-completion/SKILL.md:110-129`).
- Audit source: docs-10 (`audits/2026-09-22/findings.json`,
  `audits/2026-09-22/areas/docs.md`).

## Acceptance criteria

1. **Pre-edit rule.** CLAUDE.md's Agent Workflow section states: before
   editing a user-facing CLI surface (the path list in item 3), if
   `docs/stories/INDEX.md` exists, run `storystore:stories-impact-check` for
   the planned change and resolve any locked/accepted-story conflict before
   editing. The completion step (item 4) verifies it happened; it does not
   replace it.
2. **Completion checklist item 8** in CLAUDE.md: "CLI-visible change → the
   matching SPEC §2 subsection, a §8 changelog row naming that subsection, a
   `Last updated` bump, QUICKSTART/README if first-run behaviour changes.
   Checked by `/pre-completion` (Spec docs row)." The path list lives only in
   the skill; CLAUDE.md points to it.
3. **Trigger paths** are listed once in `.claude/skills/pre-completion/SKILL.md`
   and include at least: `shatter-cli/src/args.rs`, `shatter-cli/src/main.rs`,
   `shatter-cli/src/helpers.rs`, `shatter-cli/src/render.rs`,
   `shatter-cli/src/commands/**`, `shatter-core/src/report.rs`,
   `shatter-core/src/reporter.rs`. Changes limited to `#[cfg(test)]` modules
   or test files do not trigger.
4. **Semantic Spec-docs check** in `/pre-completion` (a script, for example
   `scripts/spec-docs-check.py`, invoked by the skill and adding one "Spec
   docs" row to its table). When the branch diff against `origin/main`
   touches a trigger path, it reports PASS only if the `SPEC.md` diff:
   (a) adds at least one row to the `## 8. Changelog` table dated on or after
   the branch's first commit; (b) changes the `Last updated:` line to that
   row's date or later; and (c) has at least one hunk inside each `§2.N`
   subsection that the new row's Section column names. It also reports the
   stories-impact-check result recorded for the branch (item 1) or `N/A`
   when `docs/stories/INDEX.md` does not exist. Otherwise it reports FAIL,
   naming which of (a)-(c) is missing.
5. **Waiver** is machine-readable: a `Spec-Waiver: <reason>` trailer in any
   commit on the branch turns FAIL into `WAIVED (<reason>)`. No free-text
   waiver in a completion message counts.
6. **Unit tests** for the script (wired into `task meta` `cmds:` and
   `sources:`) cover: trigger path touched with no SPEC change -> FAIL;
   changelog row added but no §2 hunk -> FAIL (c); row and §2 hunk but stale
   `Last updated` -> FAIL (b); all three -> PASS; `main.rs` exit-code change
   alone -> FAIL; test-only change -> not triggered; `Spec-Waiver` trailer ->
   WAIVED.
7. **Demonstration in the close reason** on a scratch branch: a help-text
   change in `args.rs` with no SPEC edit -> FAIL; adding only a changelog row
   -> still FAIL; adding the §2 edit and the `Last updated` bump -> PASS.
8. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Parse `git diff -U0 origin/main...HEAD -- SPEC.md` hunks against the section
line ranges of the base and head `SPEC.md`. Keep the check a small,
dependency-free Python script so `task meta` can test it.

## Out of scope

- The mechanical CLI-surface drift gate that compares SPEC flag tables with
  clap (str-wurp).
- Fixing the existing SPEC drift (other audit docs issues).
- Adopting storystore (str-qwua7.52).

## Dependencies

None. Related: str-wurp, str-u394l.4, str-qwua7.2, str-qwua7.52.

---

<!-- file: 10-planning-rules-location-and-open-decisions.md -->

---
slug: planning-rules-location-and-open-decisions
kind: new
title: "Planning rules in CLAUDE.md: plans/specs go in docs/plans and docs/specs (overriding the superpowers default), and check open tracker decisions before planning"
priority: P2
type: task
labels: [agents, docs, governance, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Planning rules in CLAUDE.md: plans/specs go in docs/plans and docs/specs (overriding the superpowers default), and check open tracker decisions before planning

## Problem

Two planning failures trace back to rules the repo never states:

1. **Plan location.** The superpowers `writing-plans` skill saves plans to
   `docs/superpowers/plans/` "unless user preferences for plan location
   override this default". Shatter states no preference, so the duplicate
   plan tree keeps growing. On 2026-09-21 a 1,694-line plan landed there with
   no Status banner. Its epic has since closed, it still has 43 unticked
   boxes, and nothing links to it. str-qwua7.44 will merge the existing trees
   and define the Status-banner convention, but it does not add the rule that
   stops the default path from recreating the tree.
2. **Open decisions.** The core -> shatter-llm dev-dependency already existed
   (added 2026-05-25 in 4db63be3 for `e2e_llm_oracle.rs`), and str-qwua7.43
   (open, decided) removes that edge. The 2026-09-21 plan nevertheless told
   the implementer to rely on it for a **second** consumer,
   `bench_frontier_ranking.rs` (plan `:1037`: "add `shatter-llm` ... under
   `[dev-dependencies]`"), deepening an edge a decided issue is removing. No
   planning step checks open tracker decisions that touch the files or
   dependency edges a plan changes.

## Ownership (one owner per deliverable)

- **This issue:** the two CLAUDE.md rules (location override; open-decision
  check).
- **str-qwua7.44:** the Status-banner convention and its value set, moving
  `docs/superpowers/{plans,specs}`, banners on existing files (including the
  2026-09-21 plan), and the orphan check. This issue references that
  convention and does not define its own. The companion comment below adds
  the 2026-09-21 plan to .44's banner list.
- **str-qwua7.43:** moving `bench_frontier_ranking.rs` out of core and
  dropping the edge; already covered by the audit note
  `qwua7-43-bench-dev-dep-cycle` (bucket shatter-concolic-and-engine-design).

## Evidence

Re-verified 2026-09-23 in `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`:

- `docs/superpowers/plans/2026-09-21-jev-frontier-ranking-benchmark.md`:
  1,694 lines; `grep -c -- '- \[ \]'` -> 43; no `Status:` line; `:1037`
  "Modify: `shatter-core/Cargo.toml` — add `shatter-llm = { path = "../shatter-llm" }` under `[dev-dependencies]`".
  Its issue str-hjrnp.4 is closed.
- `shatter-core/Cargo.toml:44` `shatter-llm = { path = "../shatter-llm" }`
  (dev-dep, first added in 4db63be3, 2026-05-25). Two test files use it:
  `shatter-core/tests/e2e_llm_oracle.rs` and
  `shatter-core/tests/bench_frontier_ranking.rs`.
- `grep -nE 'superpowers|docs/plans|docs/specs' CLAUDE.md AGENTS.md` -> no
  matches.
- Both `docs/plans/` and `docs/specs/` exist, alongside `docs/superpowers/`.
- `bd show str-qwua7.44` (open): acceptance requires "docs carry
  `Status: current | draft | approved | implemented (str-xxxx) | deferred |
  superseded-by <path>`; every existing orphan gets one" and merging
  `docs/superpowers/{plans,specs}` into `docs/{plans,specs}`. It has no
  CLAUDE.md location rule.
- Audit sources: docs-12, frontend-rust-09 (process part)
  (`audits/2026-09-22/findings.json`, `audits/2026-09-22/areas/docs.md`).

## Acceptance criteria

1. CLAUDE.md states, in two or three lines: new plans go in `docs/plans/`,
   design specs in `docs/specs/`; this overrides the superpowers default
   location; each new plan/spec starts with a `Status:` line using the
   convention defined by str-qwua7.44 (link the issue, or the doc that .44
   lands, rather than restating the value list).
2. CLAUDE.md (or AGENTS.md, if str-qwua7.23's byte budget allows) gains a
   planning rule: before writing a plan, run `bd search` for each file,
   crate and dependency edge the plan modifies, and list in the plan header
   any open issue or recorded decision it contradicts (or "none found",
   with the searches run).
3. **Rule-effect proof in the close reason:** in a scratch session, invoke
   the superpowers `writing-plans` skill for a throwaway plan and record the
   path it writes to (`docs/plans/...`, not `docs/superpowers/plans/...`) and
   that the plan header contains the open-decisions line. Delete the
   throwaway plan afterwards.
4. `grep -nE 'docs/plans|superpowers' CLAUDE.md` shows the rule (output in
   the close reason); `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Put both rules in CLAUDE.md's Agent Workflow section. str-qwua7.23 is cutting
AGENTS.md, so do not grow it.

## Out of scope

- The Status-banner convention, moving the existing `docs/superpowers/` tree,
  banners on existing plans, and the orphan check (str-qwua7.44).
- Removing the core->shatter-llm dev-dependency (str-qwua7.43, via
  `qwua7-43-bench-dev-dep-cycle`).
- A dotfiles-level default plan location for all repos.

## Dependencies

None. Related: str-qwua7.44, str-qwua7.43, str-qwua7.23.

## Comment for `str-qwua7.44`

> Audit 2026-09-22 (finding docs-12): please include
> `docs/superpowers/plans/2026-09-21-jev-frontier-ranking-benchmark.md`
> (1,694 lines, 43 unticked boxes, no `Status:` line; its issue str-hjrnp.4
> is closed) in this issue's banner pass, as
> `Status: implemented (str-hjrnp)`, at its moved path. The CLAUDE.md rule
> that stops the superpowers default from recreating `docs/superpowers/` is
> filed separately as <planning-rules-location-and-open-decisions>; it
> references this issue's banner convention instead of defining one.

---

<!-- file: 11-35vtk-9-swarm-config.md -->

---
slug: 35vtk-9-swarm-config
kind: note-to-existing
title: "Note on str-35vtk.9: bento swarm no longer reads .claude/swarm-config.md; batch-landing claim in CLAUDE.md is unbacked"
priority: P3
type: task
labels: [agents, swarm]
parent_epic: "(existing issue; parent str-35vtk)"
blocked_by: []
existing_id: str-35vtk.9
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-35vtk.9: swarm config format changed

Target: **str-35vtk.9** (open, P1, "WS-F: Batch-landing protocol for swarm
leads (lands with WS-E)"). Action: `bd comments add str-35vtk.9` with the text
below. This note's own priority is P3, so do not change .9's priority. There
is no old draft for this note; it comes from finding agent-repo-19 (report §15.1).

## Comment text

> Audit 2026-09-22 (finding agent-repo-19, `audits/2026-09-22/findings.json`):
> this issue's premise is stale and its scope needs updating.
>
> - The body says "bento:swarm reads swarm-config.md" and plans a
>   `.claude/swarm-config.md` quality-gates rewrite. The installed bento
>   swarm no longer reads it:
>   `bento/plugins/claude/bento/skills/swarm/scripts/swarm-discover.py:18-22`
>   reads only `swarm-config.json` (repo root), `.claude/swarm-config.json` or
>   `.codex/swarm-config.json` (bento-96ua.2, closed). It validates a
>   `landing` block (`mode`, `gate_scope`, `full_gate`, `max_batch_size`,
>   `linger_minutes`, `batch_boundary_paths`; `:88-183`). An invalid or
>   absent block degrades to serial landing. The schema is in bento
>   `skills/swarm/references/landing-config.md`.
> - Shatter has only `.claude/swarm-config.md` (plus an untracked
>   `.codex/swarm-config.md` symlink in the primary). No `swarm-config.json`
>   exists, so swarms land serially and the .md gates (`/check-all`,
>   `/pre-completion`, `/walkthrough-review`) are at best read as prose.
>   `/check-all` also has `disable-model-invocation: true`
>   (`.claude/skills/check-all/SKILL.md:5`), so a lead cannot invoke it
>   through the Skill tool. Reference `task check` directly.
> - `CLAUDE.md:57` states "The lead runs one full `task check` at batch
>   landing", but nothing configured implements batch landing today.
>
> **Proposed scope update:**
> - Write the config at the repo root as `swarm-config.json`, so both
>   runtimes read it: `swarm-discover.py:18-22,257-270` checks the
>   runtime-specific file (`.claude/` or `.codex/swarm-config.json`) and then
>   the root file, and Codex never falls back to `.claude/swarm-config.json`.
>   Delete `.claude/swarm-config.md` and the primary's untracked
>   `.codex/swarm-config.md` symlink (or keep the `.md` only for Epic-mode
>   prose that bento does not read).
> - `landing` block: `full_gate: task check`, and `mode` / `max_batch_size`
>   matching this issue's protocol.
> - **`gate_scope` needs an adapter; `task affected` does not fit the
>   contract.** bento's `landing-config.md` defines `gate_scope` as a
>   "command that emits scoped gate commands for a diff", and teammates run
>   the emitted commands. `task affected` (`Taskfile.yml:511-514`)
>   *executes* the selected gates and prints status and logs; it emits no
>   commands. Add a small executable (for example `scripts/gate-scope.sh`)
>   that calls `python3 scripts/affected-gates.py --base <base> --head <head>`
>   and prints one runnable command per selected gate (`task <gate>`,
>   nothing for `(none)`), with no other stdout.
> - **Behavioural validation, not just warning-free discovery.** A
>   `gate_scope` string whose first word resolves on `PATH` passes
>   `swarm-discover.py` validation whatever it prints. Acceptance: (1)
>   `swarm-discover.py --runtime claude` and `--runtime codex` both report
>   the same non-null `landing` block with `mode` as configured and no
>   warnings; (2) a test runs the adapter on a fixture diff touching one
>   crate and asserts stdout is exactly the expected `task <gate>` lines,
>   and on a docs-only diff asserts it prints only the docs gates (or
>   nothing); (3) every emitted line runs successfully when executed as a
>   command in a scratch worktree (record the output in the close reason).
> - Re-check whether the manual batch-land skill this issue plans is still
>   needed, given bento swarm's built-in batch mode, and whether step (4)'s
>   "push once with --no-verify" survives. Audit decision D4 and the 08-24
>   note already rule out hook bypass, so drop it.
> - Until batch landing is configured and validated, soften `CLAUDE.md:57`
>   to say batch landing is planned (str-35vtk.9), not current.

---

<!-- file: 12-qwua7-14-reverify-on-main.md -->

---
slug: qwua7-14-reverify-on-main
kind: reopen-note
title: "Reopen str-qwua7.14: its 'not reproducible' closure cited e50fc399, a stray fixture commit that is not on origin/main; re-verify on an origin/main build"
priority: P1
type: bug
labels: [agents, git, frontend-rust, audit-2026-09-22]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.14
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Reopen str-qwua7.14: re-verify on an origin/main build

Target: **str-qwua7.14** (CLOSED, P1 bug, "Rust frontend: walkthrough
examples 0% covered — analyzer/harness param-type disagreement
(hypothesis)"). Action: `bd reopen str-qwua7.14`, then
`bd comments add str-qwua7.14` with the text below. Keep its priority. Split
out of `fixture-corruption-incident-reverify` so the re-diagnosis has its own
owner and proof. Filer note: `file-all.sh` only posts the comment;
`bd reopen str-qwua7.14` is a manual step for the maintainer.

## Comment text

> Audit 2026-09-22 (findings agent-repo-16, prior-06, prior-09;
> `audits/2026-09-22/findings.json`). Reopened because the closure's
> reference point is not on main.
>
> - The close reason says "Not reproducible against current main
>   (e50fc399)" and "rebuilt shatter-cli/shatter-rust from HEAD".
>   `git merge-base --is-ancestor e50fc399 origin/main` exits **1**:
>   `e50fc399` ("init", author `Test <test@example.com>`, 2026-09-07,
>   82 files, -12,090 lines) is a stray commit made by the GIT_DIR fixture
>   leak (str-jttrf / str-y0rcz). The diagnosis therefore **may** have run on a
>   corrupted tree; the "not reproducible" result is unproven, not refuted.
>   Incident record: <fixture-corruption-incident-reverify>.
> - The close reason's other points (the cited walkthrough evidence was
>   mis-cited; `negotiate_language`'s 5% is a separate tractability gap) are
>   not disputed.
>
> **Acceptance for the re-verification:**
> 1. Choose a SHA `S` with `git merge-base --is-ancestor S origin/main` exit
>    0 (record the command and exit code). Work in a scratch linked
>    worktree, never the primary checkout.
> 2. Rebuild the CLI and the Rust frontend at `S` before running anything
>    (a stale binary produced a false audit finding before; prior-09), and
>    record `shatter --version` or the binary's build SHA.
> 3. Run the walkthrough's Rust step for `classify_number` and `safe_divide`
>    through the same harness mode the walkthrough uses (standalone-file vs
>    crate), and record the command, the coverage numbers and any
>    `deserialization failed` errors.
> 4. Decide from that output: if the 0%-coverage / deserialization error
>    reproduces, keep the issue open as a confirmed bug with the repro
>    command; if not, close it with a reason citing `S`, the commands and
>    their output. Either way the reason follows the SHA-labelling rule
>    proposed on str-qwua7.51 (<qwua7-51-identity-root-cause>).

---

<!-- file: 13-orphan-worktree-dirs-cleanup.md -->

---
slug: orphan-worktree-dirs-cleanup
kind: new
title: "Review the six orphan worktree directories (five doctor-flagged under ~/.local/share/worktrees/shatter plus .claude/worktrees/str-umw3) and remove or retain each with operator approval"
priority: P3
type: chore
labels: [agents, git, tooling, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Review the six orphan worktree directories and remove or retain each with operator approval

## Problem

Six directories look like worktrees but are no longer registered with git:

- five under `~/.local/share/worktrees/shatter/`, which the bento
  agent-env-doctor reports as orphan worktree dirs at every SessionStart;
- `/home/ketan/project/shatter/.claude/worktrees/str-umw3/` (issue str-umw3
  closed 2026-04-11), which recursive greps from the primary still match.

Removing them is destructive, so it needs explicit operator approval. This
cleanup was previously bundled into three other issues (str-qwua7.1's repair
half, and the audit drafts `env-doctor-decisions` and `agent-config-gitignore`),
where a declined deletion left those issues without a defined outcome. It now
lives here alone, and **retaining a directory is a valid, documented outcome.**

## Evidence

Re-verified 2026-09-23:

- `~/.local/share/worktrees/shatter/str-6q1i` (109 MB),
  `str-hszo-tmpfix` (573 MB), `str-k6e61-scm-followups` (16 KB),
  `str-mambd-enum-variant-gen` (16 KB), `str-yhsp-concolic-run` (16 KB). None
  is a git repo any more, and none appears in `git worktree list`.
- `/home/ketan/project/shatter/.claude/worktrees/str-umw3/` (9.0 MB), not in
  `git worktree list`.
- Audit sources: agent-repo-10, agent-repo-15, prior-04
  (`audits/2026-09-22/findings.json`).

## Acceptance criteria

1. For each of the six directories the issue records: size (`du -sh`),
   `git worktree list` showing it unregistered, whether it contains a `.git`
   file or dir, and whether it holds any file not present on origin/main that
   someone might want (a quick listing of top-level contents and any
   uncommitted-looking source files).
2. Each directory gets a disposition: `delete` or `retain`. For `delete`, the
   operator's explicit approval is quoted in the issue before removal; after
   removal `ls` no longer lists it. For `retain`, the reason is recorded.
3. The close reason includes the agent-env-doctor output from a SessionStart
   run in the primary checkout: deleted dirs are no longer reported; any
   retained dir that the doctor still reports is named with its retain
   reason. The issue can close with retained directories.

## Out of scope

- The bento doctor's detection logic or a way to silence a retained dir
  (bento tracker).
- The git-state check in str-qwua7.1.

## Dependencies

None. The re-scope comment on str-qwua7.1 (note `qwua7-1-git-state-check`)
points here.

---

<!-- file: 14-u394l-4-skill-command-lint.md -->

---
slug: u394l-4-skill-command-lint
kind: note-to-existing
title: "Note on str-u394l.4: skill-command lint requirements from the 2026-09-22 audit (subcommand resolution, not --help exit codes; governed suite runs; Taskfile parsing without task --list-all)"
priority: P2
type: task
labels: [agents, skills, lint]
parent_epic: "(existing issue; parent str-u394l)"
blocked_by: []
existing_id: str-u394l.4
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-u394l.4: skill-command lint requirements

Target: **str-u394l.4** (open, P2, "Agent rules drift lint"). Action:
`bd comments add str-u394l.4` with the text below. This transfers the lint
half of the earlier audit draft `repo-skills-rot` to its existing owner, so
there is one implementation; `repo-skills-rot` now only fixes the skills.

## Comment text

> Audit 2026-09-22 (finding agent-repo-14; `audits/2026-09-22/findings.json`).
> This issue stays the single owner of the agent-rules / skill-command lint.
> The audit adds these requirements and corrects two of the checks already
> listed in this issue's notes.
>
> **1. Resolve subcommands; do not trust `--help` exit codes.** Check (f)
> ("every `bd <verb> --flag` ... is accepted by `bd <verb> --help`") and this
> issue's own regression case would both miss `bd epic list`: on bd 1.1.0
> `bd epic list --help` **exits 0** and prints the parent `bd epic` help,
> whose "Available Commands" are only `close-eligible` and `status`.
> Validate a documented `bd a b c ...` by walking the command tree: at each
> level, the next word must appear in that level's "Available Commands"
> list (parse `bd <prefix> --help`), then check flags against the leaf's
> help. Same approach for any other CLI the lint covers. Regression tests:
> `bd epic list` FAILs, `bd epic status` and `bd update <id> --claim` PASS.
> Skip with a warning when `bd` is absent.
>
> **2. Cover skill bodies, and check governance, not just existence.**
> Scan `.claude/skills/**/SKILL.md` as well as CLAUDE.md/AGENTS.md. Fail on a
> bare suite command (`cargo test`, `go test ./...`, `npm test`, `jest`)
> **and** on an unwrapped namespace test task (`task core:test`,
> `task go:test`, ...): these tasks run cargo/go/npm directly and skip
> `scripts/gate-wrapper.sh`, the heavyweight-slot governance of
> str-35vtk.5. Accepted forms: governed gates (`task affected`, `task check`,
> `task parity`, `task conformance`, `task e2e*`, or
> `bash scripts/gate-wrapper.sh <label> task <ns>:test`), and targeted single
> tests preceded by a documented allowlist comment (define its exact form
> here; `repo-skills-rot` uses it in the bugfix skill). Derive the governed
> set from the Taskfiles (a task counts as governed if its `cmds` invoke
> `gate-wrapper.sh`), not from a hard-coded list.
>
> **3. Resolve `task <x>` names by parsing Taskfile YAML** (root plus
> `includes:` namespaces), **not** `task --list-all` / `--json`, which writes
> checksum state (see str-qwua7.3). This amends check (a) in the notes.
>
> **4. Close-time proof:** the lint run on the pre-fix tree (before
> <repo-skills-rot> lands) FAILs and lists at least audit `SKILL.md:115`
> (`bd epic list`), audit `:17-22` and bugfix `:65-70` (bare suites), and the
> check-go/check-rust/check-ts skills; after the skill fixes it PASSes. Record
> both outputs. The lint is wired into `task meta` `cmds:` and `sources:`
> and into drift-patrol, as this issue already requires.

---

<!-- file: 15-qwua7-52-storystore-interim-nudge.md -->

---
slug: qwua7-52-storystore-interim-nudge
kind: note-to-existing
title: "Note on str-qwua7.52: storystore adoption is blocked on the storystore clap extractor and stale plugin cache; set a dated remind_after meanwhile"
priority: P2
type: chore
labels: [agents, stories, tooling]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.52
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.52: interim storystore nudge

Target: **str-qwua7.52** (open, P2, "Adopt storystore: initialise
docs/stories, seed CLI stories, land str-u394l.3"). Action:
`bd comments add str-qwua7.52` with the text below. Do not change its
priority. Sibling of the str-qwua7.53 note (`env-doctor-decisions`); split
from the first-revision draft of that slug.

## Comment text

> Audit 2026-09-22 (findings agent-repo-15, plugins-08;
> `audits/2026-09-22/findings.json`, `audits/2026-09-22/areas/plugins-guidance.md`).
>
> - Still dormant: `/home/ketan/project/shatter/docs/stories` does not exist,
>   and every SessionStart prints "storystore dormant — decision pending".
> - Adoption as decided cannot produce useful stories yet: storystore's
>   inventory finds **0 clap surfaces** in shatter (storystore extractor gap),
>   and the installed storystore plugin cache is stale. Both are storystore
>   tracker items.
> - The bento doctor supports a dated reminder:
>   `agent_env_doctor_remind_after=<plugin>:<YYYY-MM-DD>[,...]`
>   (`/home/ketan/project/bento/plugins/claude/bento/hooks/scripts/agent-env-doctor.py:69,72`
>   recognised keys; value format at `:997-999`).
>
> **Proposed interim checkpoint (this issue stays open for adoption):**
> 1. Record in this issue which way adoption goes first: (a) run
>    `storystore:stories-init` now and accept observed-mode stories without
>    CLI surfaces, or (b) wait for the storystore extractor and cache fixes
>    (link those storystore issues here).
> 2. With (b), add `agent_env_doctor_remind_after=storystore:<date>` to
>    `/home/ketan/project/shatter/.agent-mode.local` (untracked; record the
>    action) with a date no more than 60 days out, and paste the doctor's
>    SessionStart output before (storystore pending) and after (not shown).
> 3. Adoption (this issue's own acceptance) is unchanged. Note for the
>    ordering: <completion-checklist-spec-docs> makes
>    `storystore:stories-impact-check` a pre-edit step once
>    `docs/stories/INDEX.md` exists, so adoption also switches that on.
