# BUNDLE: shatter-agent-guidance-and-repo-hygiene

- **Audit:** Shatter audit 2026-09-22 (final issue drafts; nothing is filed)
- **Bucket:** shatter-agent-guidance-and-repo-hygiene
- **Repo:** shatter
- **Tracker:** bd in /home/ketan/project/shatter (prefix str)
- **Parent epic (new issues):** "Epic: Audit 2026-09-22 findings"
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
| 03 | qwua7-51-identity-root-cause | note-to-existing | str-qwua7.51 | P2 | Note on str-qwua7.51: 'Owner: Test' came from the leaked repo-local fixture identity (removed 2026-09-23), not a missing SessionStart identity |
| 04 | fixture-corruption-incident-reverify | new | new | P2 | Record the 2026-09-07 fixture-corruption incident, review its recovery and contaminated branches, and re-verify str-qwua7.14 against an origin/main SHA |
| 05 | agent-config-gitignore | new | new | P2 | Make intended .claude/ and .codex/ agent config trackable: repo .gitignore negations plus a check-ignore meta test |
| 06 | repo-skills-rot | new | new | P2 | Repair rotted repo skills: check-go/rust/ts and bugfix bare commands, superseded protocol-sync, audit skill paths/steps and memory-contradiction check, plus a skill-command lint |
| 07 | env-doctor-decisions | new | new | P2 | Apply the 2026-09-06 storystore/bugshot decisions to .agent-mode.local and remove the doctor-flagged orphan worktree dirs |
| 08 | qwua7-23-agents-md-rtk-and-landing | note-to-existing | str-qwua7.23 | P2 | Note on str-qwua7.23: etiquette rules inside the rtk-managed block, rtk 'always safe' text, landing prose contradicting land.py, merged remote branches (bd sync -> D4 Dolt remote) |
| 09 | completion-checklist-spec-docs | new | new | P2 | Completion checklist and /pre-completion must require SPEC/QUICKSTART/changelog updates for CLI-visible changes |
| 10 | planning-rules-location-and-open-decisions | new | new | P2 | Planning rules in CLAUDE.md: plan/spec location + Status banner, and check open tracker decisions before planning |
| 11 | 35vtk-9-swarm-config | note-to-existing | str-35vtk.9 | P3 | Note on str-35vtk.9: bento swarm no longer reads .claude/swarm-config.md; batch-landing claim in CLAUDE.md is unbacked |

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
>   removal is now tracked by the new audit issues `env-doctor-decisions`
>   (five dirs) and `agent-config-gitignore` (str-umw3). Drop them from this
>   issue's acceptance.
> - A second instance of the same damage class was found: the fixture identity
>   `[user] name = Test, email = test@example.com` had leaked into the primary's
>   repo-local `.git/config` (str-jttrf leak). The maintainer removed it
>   2026-09-23. The `.mailmap` and fixture-side config snapshot are tracked in
>   the new audit issue `mailmap-and-fixture-config-snapshot`.
>
> **Re-scoped acceptance for this issue (the check only):**
> - A repo-state check (a new entry in `scripts/drift-patrol.py` `CHECKS`,
>   `:757`, as this issue already chose; optionally surfaced by
>   `scripts/setup-hooks.sh --check`) FAILs when any of these holds in the
>   checkout it runs in:
>   1. a repo-local `user.name` or `user.email` override exists
>      (`git config --local --get user.email` / `user.name` non-empty);
>   2. the effective `user.email` (any scope) matches `*@example.com` (also
>      `*.invalid` / `example.org`, if cheap);
>   3. `core.bare=true`;
>   4. a repo-local `core.hooksPath` override exists.
> - It SKIPs (does not FAIL) in CI or when not in a git work tree, consistent
>   with this issue's existing SKIP rule for machine-specific roots.
> - Unit tests in `scripts/test_drift_patrol.py` build a temporary repo per
>   condition and assert FAIL, plus one clean repo asserting PASS. Include a
>   failing-then-passing run in the close reason.
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
title: "Note on str-qwua7.51: 'Owner: Test' came from the leaked repo-local fixture identity (removed 2026-09-23), not a missing SessionStart identity"
priority: P2
type: task
labels: [agents, beads, git]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.51
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.51: real root cause of "Owner: Test"

Target: **str-qwua7.51** (open, P2, "Configure bd identity in the SessionStart
hook and require a close reason at landing"). Action: `bd comments add str-qwua7.51`
with the text below. The owner/maintainer then re-scopes the issue as the
comment proposes. Do not close it: the close-reason half is still valid.

## Comment text

> Audit 2026-09-22 root-cause correction (maintainer decision D5, 2026-09-23;
> evidence `audits/2026-09-22/findings.json` agent-repo-01, prior-04).
>
> The "Test" / "Test User" owners and assignees are **not** caused by a
> missing SessionStart bd identity. With no `BD_ACTOR` and no configured
> actor, bd falls back to git `user.name`; the match between the bd owners
> and the git authors below is consistent with that. The primary checkout's repo-local `.git/config` carried a leaked
> test-fixture identity (`[user] name = Test, email = test@example.com`),
> which the str-jttrf/str-y0rcz GIT_DIR fixture leak wrote there. It overrode
> the global identity for every git commit and every bd write made from the
> primary: all issues created since 2026-09-05 have `created_by: Test`. The
> "Test User" variant comes from fixtures that set `user.name "Test User"`
> (`scripts/test_walkthrough_examples_checkout.py:63`,
> `shatter-cli/tests/implicit_init_gitignore_test.rs:52`).
>
> The maintainer removed the leaked `[user]` section on 2026-09-23. Now
> `git -C /home/ketan/project/shatter config --show-origin user.name` resolves
> to `~/.gitconfig` (Ketan Gangatirkar). Follow-ups: `.mailmap` and a fixture
> config snapshot in the new audit issue `mailmap-and-fixture-config-snapshot`;
> the recurrence check on str-qwua7.1.
>
> **Proposed re-scope of this issue:**
> - Drop the `BD_ACTOR`-from-SessionStart requirement unless a fresh claim
>   still shows a wrong owner. First step: in a fresh session, run
>   `bd update <scratch-id> --claim` (or create and delete a scratch issue),
>   then `bd show` it. If the owner is the real name, record that and drop the
>   identity half.
> - Keep the close-reason half (every landing close carries a SHA or a
>   duplicate/won't-do reason; drift-patrol warns on reasonless closes).
> - Update the body's bd facts: the installed bd is now **1.1.0**, not
>   v0.63.3. Re-check the `bd close` reason flag against `bd close --help` on 1.1.0.
> - Existing issues keep `created_by: Test` (historical). Do not bulk-edit them.

---

<!-- file: 04-fixture-corruption-incident-reverify.md -->

---
slug: fixture-corruption-incident-reverify
kind: new
title: "Record the 2026-09-07 fixture-corruption incident, review its recovery and contaminated branches, and re-verify str-qwua7.14 against an origin/main SHA"
priority: P2
type: task
labels: [agents, git, governance, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Record the 2026-09-07 fixture-corruption incident, review its recovery and contaminated branches, and re-verify str-qwua7.14 against an origin/main SHA

## Problem

On 2026-09-07 the GIT_DIR fixture leak (fixed later by str-jttrf / str-y0rcz)
created a stray commit `e50fc399` ("init", author `Test <test@example.com>`).
The commit deletes 12,090 lines: it removes `shatter-vs/`, restores
`.claude/agents` and rewrites `.beads/issues.jsonl`. The recovery was done
ad hoc and never tracked:

- Two local `recovery/*` branches from 2026-09-12 have never been reviewed.
- Four remote `str-qwua7.*` feature branches contain the stray commit.
  str-qwua7.4's close reason says the duplicates were "both deleted as
  superseded", but they still exist on origin.
- **str-qwua7.14 (P1 bug) was closed as "Not reproducible against current main
  (e50fc399)".** `e50fc399` is not on main, so the diagnosis ran against a
  corrupted tree and the closure is unsound.

Nothing requires a diagnosis or close reason to cite a commit that is actually
on origin/main.

## Evidence

Re-verified 2026-09-23 in `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`
(origin/main = 70465921; remote-tracking refs as of the last fetch):

- `git merge-base --is-ancestor e50fc399 origin/main` -> exit 1 (NOT on main).
- `git show -s --format='%h %ad %an %s' --date=short e50fc399` ->
  `e50fc399 2026-09-07 Test init`. `git show --shortstat --format= e50fc399` ->
  `82 files changed, 620 insertions(+), 12090 deletions(-)`.
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
   leaked repo-local identity handled in `mailmap-and-fixture-config-snapshot`).
2. Each recovery branch is reviewed with
   `git diff origin/main...<branch> --stat` plus a content check. The review
   lists any content not already on origin/main; wanted content is filed or
   landed. Branches are deleted **only after explicit operator confirmation**,
   recorded in the issue.
3. For each of the four contaminated remote branches, the issue records the
   origin/main SHA where that issue's real work landed (or states it did not).
   Remote deletion (`git push origin --delete <branch>`) happens **only after
   explicit operator confirmation**. Afterwards,
   `git ls-remote origin 'refs/heads/str-qwua7*'` no longer lists them.
4. str-qwua7.14 is reopened and re-diagnosed on a build from an origin/main
   SHA (`git merge-base --is-ancestor <sha> origin/main` exits 0). It is then
   closed or kept open based on that result, with a reason that cites the SHA
   and the command output.
5. AGENTS.md gains one rule: diagnoses and close reasons that name a commit
   must name one that is an ancestor of origin/main. Check it with
   `git merge-base --is-ancestor <sha> origin/main` before citing it.

## Suggested approach

- Do the review in a scratch linked worktree, never in the primary checkout.
- For str-qwua7.14, rebuild the CLI and the Rust frontend at the chosen SHA
  first (a stale binary produced a false audit finding before; see prior-09),
  then re-run the walkthrough Rust step it names.

## Out of scope

- Enforcing the ancestor-SHA rule in bento land-work (bento tracker).
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
  The primary checkout's `.codex/` holds untracked symlinks (`skills`,
  `swarm-config.md`) and a `worktrees/` dir.
- An orphan checkout `/home/ketan/project/shatter/.claude/worktrees/str-umw3/`
  (9.0 MB; issue str-umw3 closed 2026-04-11) still exists. It is not
  registered in `git worktree list`, and greps still match it.
- `task meta` (`Taskfile.yml:396`) is the home for repo meta tests; its
  `sources:` list must include any new test file, or the checksum cache will
  skip it.
- Audit source: agent-repo-10 (`audits/2026-09-22/findings.json`,
  `audits/2026-09-22/areas/agent-repo.md`).

## Acceptance criteria

1. Repo `.gitignore` un-ignores `/.claude/` (`!/.claude/`) and re-ignores
   `/.claude/settings.local.json` and `/.claude/worktrees/`. It replaces the
   blanket `.codex/` ignore with rules that track `/.codex/AGENTS.md` (and any
   other codex files the project intends to track) and keep machine-local
   codex state ignored.
2. A meta test (for example `scripts/test_agent_config_trackable.py`) is wired
   into `task meta` `cmds:` and `sources:`. It asserts that
   `git check-ignore -q --no-index` **fails** (not ignored) for
   `.claude/skills/x/SKILL.md` and `.codex/AGENTS.md`, and **succeeds**
   (ignored) for `.claude/settings.local.json` and `.claude/worktrees/x`. It
   runs with the maintainer's real global excludes, and also with
   `core.excludesFile` pointing at a temp file containing `.claude/` and
   `.codex/`, so it holds on CI where the global file is absent.
3. Failing-then-passing proof in the close reason: the test run before the
   `.gitignore` change (fails) and after (passes).
4. `git status` in a worktree shows a newly created `.claude/skills/probe/SKILL.md`
   as untracked (recorded, then the probe is deleted).
5. The `.claude/worktrees/str-umw3/` orphan is removed **only after explicit
   operator confirmation**, with its size recorded. (This item moves here from
   str-qwua7.1.)
6. `task affected` passes, with `Gates selected` recorded.

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
- Changing the dotfiles global gitignore (see the alternative above; no
  dotfiles issue is filed from this audit).

## Dependencies

None.

---

<!-- file: 06-repo-skills-rot.md -->

---
slug: repo-skills-rot
kind: new
title: "Repair rotted repo skills: check-go/rust/ts and bugfix bare commands, superseded protocol-sync, audit skill paths/steps and memory-contradiction check, plus a skill-command lint"
priority: P2
type: task
labels: [agents, skills, docs, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [beads-retire-jsonl-import-dolt-remote]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Repair rotted repo skills: check-go/rust/ts and bugfix bare commands, superseded protocol-sync, audit skill paths/steps and memory-contradiction check, plus a skill-command lint

## Problem

Several repo skills under `.claude/skills/` tell agents to do things the
project forbids, or things that no longer work, and nothing lints skill content
against the Taskfile:

- `check-go`, `check-rust` and `check-ts` run bare `go test ./...`,
  `cargo test` and `npm test`. CLAUDE.md and `check-all` require the `task`
  facade (bare commands skip the heavyweight-slot wrapper, gate caching and
  parallelism budgets). Nothing references these three skills.
- `protocol-sync` hand-compares three protocol files. It ignores
  `protocol/registry.yaml`, the generated bindings and shatter-rust, all of
  which `task parity` and `scripts/protocol-codegen.py --check` already check
  mechanically.
- The `audit` skill uses bare commands in Phase 1. It names a root
  `GLOSSARY.md` that does not exist, samples only the last 20 commits in
  Phase 7, and in its post-audit step defers beads changes to `bd sync`, a
  command that no longer exists in bd 1.1.0. Under maintainer decision D4
  (2026-09-23), the JSONL import and `bd sync` are retired and tracker sync
  moves to a Dolt remote. Phase 7 also points at the wrong memory path and
  never checks memory against repo facts. Stale project memory that told
  agents to bypass hooks with `--no-verify` / `core.hooksPath=/dev/null` went
  undetected through the 2026-09-04 audit.
- The `bugfix` skill uses bare test commands.

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
- `.claude/skills/audit/SKILL.md`: `:17-21` bare `cargo test`, `npm test`,
  `go test ./...`; `:72` lists `GLOSSARY.md` (only `docs/GLOSSARY.md` exists);
  `:151` "Memory files in `.claude/projects/*/memory/`" (the real location is
  `~/.claude/projects/-home-ketan-project-shatter/memory/`); `:153`
  "Recent git log (last 20 commits)"; `:385` "Do NOT commit beads issue
  changes — those are handled by `bd sync`".
- `.claude/skills/bugfix/SKILL.md:29-31`, `:60-62`, `:67-70`: bare
  `cargo test`, `npm test`, `go test`, `cd shatter-rust && cargo test`.
- Task equivalents exist: namespaces `core`, `cli`, `ts`, `go`, `rust-fe`
  (`Taskfile.yml:12-30`), each with a `test` target, plus root `parity`
  (`:245`) and `conformance` (`:225`).
- `bd sync --help` on bd 1.1.0 -> `Error: unknown command "sync"`.
- Audit sources: agent-repo-14, plus the audit-skill part of sessions-03 /
  agent-repo-08 (drafts `shatter-agent/14`, `shatter-agent/07`).

## Acceptance criteria

1. `check-go`, `check-rust`, `check-ts` and `protocol-sync` are deleted, or
   rewritten as thin wrappers over `task go:test`, `task core:test` /
   `task cli:test`, `task ts:test` and `task parity`. The choice is recorded
   in the close reason.
2. `audit` and `bugfix` skills use the task facade (`task <ns>:test`) for
   suite runs. For the single-test red/green loop in `bugfix`, no crate
   Taskfile accepts pass-through args today (`CLI_ARGS` appears in none of
   them). Either add `{{.CLI_ARGS}}` to the `test` tasks, or keep the
   targeted bare command with a one-line note saying why it is allowed
   there. The lint in item 6 must accept whichever form is chosen.
3. Audit skill fixes: `:72` -> `docs/GLOSSARY.md`; Phase 7 covers commits
   since the previous audit's SHA (fallback: last ~150); `:151` names the
   real memory path.
4. The audit skill's Phase 7 gains a **memory-contradiction step**: grep the
   project memory dir for hook-bypass advice
   (`grep -rnE -- '--no-verify|hooksPath' <memory dir>`, where an explanatory
   "do not" mention is allowed) and for claims that contradict AGENTS.md or
   current repo state (for example `core.bare`, the installed `bd version`,
   commands AGENTS.md no longer names). The findings are listed in the report.
5. The audit skill's `:385` `bd sync` reference is replaced by the D4
   procedure that `beads-retire-jsonl-import-dolt-remote` records in AGENTS.md.
   That means: do not hand-commit `.beads/` files; tracker state lives in the
   local Dolt DB and reaches other machines via the Dolt remote
   (`bd dolt push` / `bd dolt pull`). No `bd sync` and no JSONL-export commit
   remain. The rest of the post-audit landing restructure belongs to
   `publish-audit-reports`. Whichever of the two lands second rebases onto
   the other.
6. A meta test (folded into str-u394l.4 if that lands first) is wired into
   `task meta` `cmds:` and `sources:`. It fails when a `.claude/skills/**/SKILL.md`
   names a bare `cargo test` / `go test` / `npm test` / `jest` invocation
   where an equivalent task exists, or names `task <x>` for a target that
   is not defined. Resolve targets by parsing the Taskfile YAML (root plus
   `includes:`), **not** `task --list-all --json`, which writes checksums.
   It also fails on `bd <subcommand>` names that `bd <subcommand> --help`
   rejects (skip if `bd` is absent).
7. Failing-then-passing proof for the lint: run it on the current tree (fails,
   listing the skills above), then after the fixes (passes). Record both in
   the close reason. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Do the deletions first (smallest diff), then the audit and bugfix edits, then
the lint, so the lint lands green. Check for skill symlinks under `.codex/skills`
in the primary checkout (it links to `../.claude/skills/`), so deletions also
disappear there.

## Out of scope

- The audit skill's post-audit landing flow (report via launch-work/land-work
  before filing): `publish-audit-reports`.
- AGENTS.md and `.beads/PRIME.md` `bd sync` removal: `beads-jsonl-consumers-drop-bd-sync`.
- Editing the memory files themselves: corrected by the maintainer on 2026-09-23.
- The frontend-parity skill (separate audit issue).

## Dependencies

- Blocked by `beads-retire-jsonl-import-dolt-remote` (bucket
  shatter-tracker-and-beads), for acceptance item 5 only: the skill must cite
  the sync procedure that issue records. Items 1-4 and 6 can start at once.
- Related: str-u394l.4 (agent-rules drift lint), str-qwua7.22 (audit Phase-10
  rewrite), str-qwua7.26.

---

<!-- file: 07-env-doctor-decisions.md -->

---
slug: env-doctor-decisions
kind: new
title: "Apply the 2026-09-06 storystore/bugshot decisions to .agent-mode.local and remove the doctor-flagged orphan worktree dirs"
priority: P2
type: chore
labels: [agents, stories, tooling, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Apply the 2026-09-06 storystore/bugshot decisions to .agent-mode.local and remove the doctor-flagged orphan worktree dirs

## Problem

The bento agent-env-doctor (a SessionStart hook) prints the same warnings every
session, and every session ignores them:

- "storystore dormant — decision pending"
- "bugshot dormant — decision pending"
- five orphan worktree directories

The maintainer decided both plugin questions on 2026-09-06:

- adopt storystore (str-qwua7.52);
- silence bugshot until bugshot's CLI capture (bgs-3tq) lands. str-qwua7.53
  says: "Until then set agent_env_doctor_skip_plugin=bugshot".

Neither decision was ever written into `.agent-mode.local`, the file that
encodes them. Warnings that repeat unchanged teach agents to skip
SessionStart output, which also hides new, real warnings.

## Evidence

Re-verified 2026-09-23:

- `cat /home/ketan/project/shatter/.agent-mode.local` ->
  `dangerous`, `agent_env_doctor_seen=bugshot,storystore`,
  `agent_env_doctor_superpowers_pointer_seen=true`. There is no
  `agent_env_doctor_skip_plugin` or `agent_env_doctor_remind_after` key.
- The doctor recognises both keys:
  `/home/ketan/project/bento/plugins/claude/bento/hooks/scripts/agent-env-doctor.py:69,72`
  (`RECOGNIZED_AGENT_MODE_KEYS`). `remind_after` takes
  `<plugin>:<YYYY-MM-DD>[,...]` (`:997-999`), and the pending text is built at `:537`.
- Orphan dirs, none of them a git repo any more:
  `~/.local/share/worktrees/shatter/str-6q1i` (109 MB),
  `str-hszo-tmpfix` (573 MB), `str-k6e61-scm-followups` (16 KB),
  `str-mambd-enum-variant-gen` (16 KB), `str-yhsp-concolic-run` (16 KB).
- `/home/ketan/project/shatter/docs/stories` does not exist. str-qwua7.52 and
  str-qwua7.53 are open and unclaimed (created 2026-09-07).
- Storystore today cannot see shatter's CLI: its inventory finds 0 clap
  surfaces (storystore extractor gap), and the installed plugin cache is stale
  (audit area `plugins-guidance.md`).
- Linked worktrees get their own `.agent-mode.local`, so they show the full
  nudges even after the primary is fixed. That is a bento bug, tracked in the
  bento audit bucket.
- Audit sources: agent-repo-15, plugins-08 (`audits/2026-09-22/findings.json`).

## Acceptance criteria

1. `agent_env_doctor_skip_plugin=bugshot` is present in
   `/home/ketan/project/shatter/.agent-mode.local`. Proof: run the doctor
   with a SessionStart payload from the primary checkout; its output no longer
   mentions bugshot (paste the output in the close reason).
2. Storystore: pick one and record it in this issue:
   (a) run `storystore:stories-init` under str-qwua7.52 once the storystore
   clap extractor and stale-cache problems are fixed; or
   (b) set `agent_env_doctor_remind_after=storystore:<YYYY-MM-DD>` with the
   reason "blocked on storystore clap extractor and stale plugin cache",
   linking the storystore issues in the comment.
   With (b), the doctor output no longer shows storystore as pending before
   that date (paste the output).
3. The five orphan dirs are removed **only after explicit operator
   confirmation**, with sizes recorded in the issue. Afterwards
   `ls ~/.local/share/worktrees/shatter/` no longer lists them, and the doctor
   stops reporting them.
4. A comment on str-qwua7.53 records the cross-repo dependency on bgs-3tq.
   (The bgs-3tq priority raise is filed in the bugshot bucket, not here.)

## Suggested approach

`.agent-mode.local` is untracked, per-checkout state, so the edit is an
operator/agent action recorded in the close reason, not a commit. Test the
doctor with the same invocation its SessionStart hook uses (see bento
`hooks.json`).

## Out of scope

- The bento doctor's per-worktree state location and escalation behaviour
  (bento tracker).
- Storystore extractor or plugin-cache fixes (storystore tracker).
- Raising bgs-3tq's priority (bugshot bucket).
- The `.claude/worktrees/str-umw3` orphan (`agent-config-gitignore`).

## Dependencies

None. Cross-repo relations (bd cannot express them): bgs-3tq (bugshot) and the
storystore clap-extractor issue.

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
> -> 35 merged branches, of 66 remote branches, per the local fetch state.
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
title: "Completion checklist and /pre-completion must require SPEC/QUICKSTART/changelog updates for CLI-visible changes"
priority: P2
type: task
labels: [agents, docs, skills, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Completion checklist and /pre-completion must require SPEC/QUICKSTART/changelog updates for CLI-visible changes

## Problem

CLI-visible changes land without SPEC, QUICKSTART or changelog updates because
no agent checklist asks for them. SPEC.md itself asks for a changelog row per
CLI-visible change, but that rule lives inside SPEC, which the landing flow
never reads. This audit found the results:

- changelog rows claiming section updates that were never made;
- missing rows for the 2026-09-14 and 2026-09-19 CLI changes;
- `--failure-threshold` still documented after it was removed;
- a stale "Last updated" header.

## Evidence

Re-verified 2026-09-23 in `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`:

- `grep -nE 'SPEC|changelog|QUICKSTART|stories' CLAUDE.md .claude/skills/pre-completion/SKILL.md`
  -> no matches.
- The Completion Checklist is `CLAUDE.md:43-53` (items 1-7, all test or
  parity gates). The only doc rule is `CLAUDE.md:80`: "Update README.md when
  build/run/config procedures change."
- `SPEC.md:7` holds the rule: "Any CLI-visible change (new command,
  new/renamed/removed flag, changed default, changed output shape) should add
  a row to the changelog". The changelog is `SPEC.md:1173` (`## 8. Changelog`),
  and CLI commands are `## 2.` at `:68`.
- `SPEC.md:3` "Last updated: 2026-09-09", but
  `git log --format='%h %ad' --date=short -- SPEC.md` shows edits on
  2026-09-14 (21981b1d) and 2026-09-19 (2de05fd9).
- `storystore:stories-impact-check` (a hard-trigger skill) is not referenced
  in CLAUDE.md, AGENTS.md or `.claude/`.
- The flag source of truth is `shatter-cli/src/args.rs`.
- Audit source: docs-10 (`audits/2026-09-22/findings.json`,
  `audits/2026-09-22/areas/docs.md`).

## Acceptance criteria

1. The CLAUDE.md Completion Checklist gains item 8: "**CLI-visible change**
   (`shatter-cli/src/args.rs`, output format, exit codes) → matching SPEC §2
   section + §8 changelog row + `Last updated` bump; QUICKSTART/README if
   first-run behaviour is affected; run `storystore:stories-impact-check` once
   `docs/stories/` exists."
2. `/pre-completion` (`.claude/skills/pre-completion/SKILL.md`) adds a
   diff-based check. If the branch diff against `origin/main` touches
   `shatter-cli/src/args.rs` or the report/output renderers (list the paths in
   the skill) and does not touch `SPEC.md`, the check reports FAIL, unless the
   completion message carries an explicit waiver line with a reason.
3. Demonstration in the close reason: on a scratch branch, a trivial
   `args.rs` help-text change without a SPEC edit makes the check FAIL; adding
   a SPEC changelog row (or a waiver) makes it PASS.
4. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Keep the check in the skill's existing summary table (one row, "Spec docs"),
so teammates' completion messages carry it automatically. Put the exact
path list in one place, the skill, and have CLAUDE.md point to it.

## Out of scope

- The mechanical CLI-surface drift gate that compares SPEC flag tables with
  clap (str-wurp).
- Fixing the existing SPEC drift (other audit docs issues).

## Dependencies

None. Related: str-wurp, str-u394l.4, str-qwua7.2.

---

<!-- file: 10-planning-rules-location-and-open-decisions.md -->

---
slug: planning-rules-location-and-open-decisions
kind: new
title: "Planning rules in CLAUDE.md: plan/spec location + Status banner, and check open tracker decisions before planning"
priority: P2
type: task
labels: [agents, docs, governance, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Planning rules in CLAUDE.md: plan/spec location + Status banner, and check open tracker decisions before planning

## Problem

Two planning failures trace back to rules the repo never states:

1. **Plan location.** The superpowers `writing-plans` skill saves plans to
   `docs/superpowers/plans/` "unless user preferences for plan location
   override this default". Shatter states no preference, so the duplicate
   plan tree keeps growing. On 2026-09-21 a 1,694-line plan landed there with
   no Status banner. Its epic has since closed, it still has 43 unticked
   boxes, and nothing links to it.
2. **Open decisions.** That plan told the implementer to "add shatter-llm
   under shatter-core [dev-dependencies]". This directly contradicts the open,
   decided issue str-qwua7.43, which removes the core→shatter-llm dev-dependency
   cycle. No planning step checks open tracker decisions that touch the files
   or dependency edges a plan changes.

## Evidence

Re-verified 2026-09-23 in `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`:

- `docs/superpowers/plans/2026-09-21-jev-frontier-ranking-benchmark.md`:
  1,694 lines; `grep -c -- '- \[ \]'` -> 43; no `Status:` line; `:1037`
  "Modify: `shatter-core/Cargo.toml` — add `shatter-llm = { path = "../shatter-llm" }` under `[dev-dependencies]`".
  Its issue str-hjrnp.4 is closed.
- `shatter-core/Cargo.toml:44` `shatter-llm = { path = "../shatter-llm" }`
  (dev-dep). Two test files use it: `shatter-core/tests/bench_frontier_ranking.rs`
  and `shatter-core/tests/e2e_llm_oracle.rs`.
- `grep -nE 'superpowers|docs/plans|docs/specs' CLAUDE.md AGENTS.md` -> no
  matches.
- Both `docs/plans/` and `docs/specs/` exist, alongside `docs/superpowers/`.
- str-qwua7.44 (open) will merge `docs/superpowers/{plans,specs}` into
  `docs/{plans,specs}` and add Status banners plus an orphan check. It does not
  add the CLAUDE.md rule that stops the default path from recreating the tree.
- Audit sources: docs-12, frontend-rust-09 (process part)
  (`audits/2026-09-22/findings.json`, `audits/2026-09-22/areas/docs.md`).

## Acceptance criteria

1. CLAUDE.md states: plans go in `docs/plans/`, design specs in `docs/specs/`,
   and each starts with
   `Status: draft | approved | implemented (str-x) | superseded-by <path>`.
   It says explicitly that this overrides the superpowers default location.
2. CLAUDE.md (or AGENTS.md) gains a planning rule: before writing a plan, run
   `bd search` for each file, crate and dependency edge the plan modifies,
   and list any open issue or recorded decision it contradicts in the plan
   header (or "none found", with the searches run).
3. `docs/superpowers/plans/2026-09-21-jev-frontier-ranking-benchmark.md`
   gets `Status: implemented (str-hjrnp)`. If str-qwua7.44 has landed first,
   the banner goes on the file at its moved path.
4. A comment is appended to str-qwua7.43: add
   `shatter-core/tests/bench_frontier_ranking.rs` (introduced by the
   2026-09-21 plan) to the move-out-of-core scope, next to `e2e_llm_oracle.rs`.
5. Proof in the close reason: `grep -n 'docs/plans' CLAUDE.md` shows the rule,
   and the str-qwua7.43 comment id is cited.

## Suggested approach

Keep both rules to two or three lines each in CLAUDE.md's Code Quality or
Agent Workflow section. str-qwua7.23 is cutting AGENTS.md, so do not grow it.

## Out of scope

- Moving the existing `docs/superpowers/` tree and adding the orphan check
  (str-qwua7.44).
- Removing the core→shatter-llm dev-dependency (str-qwua7.43).
- A dotfiles-level default plan location for all repos.

## Dependencies

None. Related: str-qwua7.44, str-qwua7.43.

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
> **Proposed scope update:** replace the "`.claude/swarm-config.md`
> quality-gates rewrite" with: write `.claude/swarm-config.json` with a
> `landing` block (`gate_scope: task affected`, `full_gate: task check`, and
> `mode` / `max_batch_size` matching this issue's protocol); delete the `.md`
> (or keep it only for Epic-mode prose that bento does not read); verify
> with `swarm-discover.py`'s output that the landing block is accepted with no
> warnings. Re-check whether the manual batch-land skill this issue plans is
> still needed, given bento swarm's built-in batch mode, and whether step (4)'s
> "push once with --no-verify" survives. Audit decision D4 and the 08-24 note
> already rule out blanket hook bypass. Until then, soften `CLAUDE.md:57` to
> say batch landing is planned (str-35vtk.9), not current.
