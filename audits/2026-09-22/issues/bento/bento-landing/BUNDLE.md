# Bundle: bento-landing

- **Bucket:** bento-landing
- **Repo:** bento (bd in /home/ketan/project/bento, prefix bento)
- **Parent epic:** Epic: Audit 2026-09-22 findings (bento)
- **Theme:** land.py and land-work: preview ownership, verifier logs, post-push workflow results, merge-state ownership, progress, branch cleanup, skill restructure.
- **Status:** drafts only. Nothing is filed (D6).
- **Code re-verified against:** bento origin/main b1bb787. `git diff 1c0c1e6 b1bb787` is empty for catalog/skills/land-work, swarm and wire-land-verifier, so the old drafts' line numbers mostly held. Corrections: create-preview refusal 266-276 -> 252-275; merge_push region 190-262 -> 166-~271 (recorded at 309-313); SKILL.md is 6,342 words, not 6,376, and swarm is 4,178, not 4,272; wire-land-verifier has no reference doc, so its SKILL.md is the target. Shatter facts were checked in the audit worktree at 56c86168: 66 remote branches with 38 merged, the 4 contaminated str-qwua7.* branches still present, 6 "Merge commit" merges, and live `gh run list` workflow counts. The /tmp preview a5l9ycto is no longer registered.

## Maintainer decisions (2026-09-23)

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix. Fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64), do not drop them. Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot-diff command and the unused Snapshot writer. spec-diff is the regression tool. Update SPEC, README and QUICKSTART. The `diff` name becomes free; str-81xiw decides whether to take it. Correct the shatter-agents `shatter diff --staged` docs.
- **D3 Concolic positioning:** measure first. P1 controlled default-vs-concolic benchmark; P1 fix concolic early termination; a follow-up decision issue, blocked by both, re-decides the positioning. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import and sync the tracker through a Dolt remote. The first step checks whether the stale-JSONL import has been clobbering newer DB state. AGENTS.md drops `bd sync`. str-qwua7.28 is superseded. bento beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT or hook-bypass guidance.
- **D5 Git identity:** the leaked [user] section is already removed. Add a .mailmap (test@example.com "Test"/"Test User" -> Ketan Gangatirkar <33678+ketang@users.noreply.github.com>), a git-state check (identity override / example.com / core.bare / hooksPath), and a fixture .git/config snapshot test.
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. No agent files anything.

How they apply here: D1 appears only in `land-work-post-push-workflow-health`'s out-of-scope list (the release legs are fixed in shatter, not dropped). D4 shapes `land-work-skill-restructure`'s Tracker Handoff criterion, which defers to beads-issue-flow and `beads-dolt-remote-guidance`. No draft in this bucket suggests hook bypass or a hook-timeout env var. D2, D3 and D5 do not touch this bucket.

## Dependency edges (by slug)

- `land-py-invocation-progress-log` blocks `land-work-skill-restructure` and `merge-push-observability`.
- The reopen note `verifier-log-reopen-note` references the id assigned to `land-py-verifier-log-kept` (the filer substitutes it).

## Contents

| NN | Slug | Kind | Target | P | Title |
|---|---|---|---|---|---|
| 01 | e583-preview-owner-lock | note-to-existing | bento-e583 | P1 | Note on bento-e583: confirmed mechanism (the leftover-preview refusal has no owner check); add an owner lock, --force-foreign, and a two-process test |
| 02 | land-py-verifier-log-kept | new | - | P1 | land.py reports verifier.log as output_path after deleting it along with the preview worktree |
| 03 | verifier-log-reopen-note | reopen-note | bento-rdtn.4 | P1 | Comment on closed bento-rdtn.4: on the land.py path the persisted verifier log is deleted before it is reported |
| 04 | land-work-post-push-workflow-health | new | - | P1 | land-work: report GitHub workflow conclusions for the landed SHA; doctor flags workflows that stay red on the primary branch |
| 05 | land-py-merge-abort-ownership | new | - | P2 | land.py aborts or hard-resets merge state in the shared primary checkout even when it did not start the merge |
| 06 | land-py-invocation-progress-log | new | - | P2 | land.py: document the canonical long-running invocation; write a progress log and a heartbeat |
| 07 | landing-deletes-remote-branches | new | - | P2 | land-work: delete the landed remote feature branch and superseded same-issue branches as a verified step |
| 08 | verifier-contract-migration | new | - | P2 | Verifier contract drift: warn when verifier payloads lack `executed`, flag outdated manifests, require gate output pass-through |
| 09 | land-work-skill-restructure | new | - | P2 | land-work SKILL.md: restructure around land.py, move the manual and batch flows to references, remove $(...) that contradicts its own Command Rule |
| 10 | eth-swarm-lead-lands-from-teammate | note-to-existing | bento-eth | P2 | Note on bento-eth: the swarm lead should land from a lead-owned scratch worktree, not the teammate's; add land.py --branch |
| 11 | dyp7-admission-control | note-to-existing | bento-dyp7 | P2 | Note on bento-dyp7: route git-hook gates and land.py's verify step through the admission governor; print a 'machine busy' status line |
| 12 | rebase-before-land-configurable | new | - | P3 | land.py: make rebase-before-land (--require-up-to-date) a per-repo policy option; the preview already verifies the exact merge |
| 13 | merge-push-observability | new | - | P3 | land.py merge_push: split into timed sub-steps and capture pre-push hook output |
| 14 | merge-message-and-stale-branch-nudge | new | - | P3 | land-work: merge message names branch and issue; refuse bare-SHA merges into the primary branch; SessionStart nudge for aging pushed branches; one session per branch |

---

<!-- file: 01-e583-preview-owner-lock.md -->

---
slug: e583-preview-owner-lock
kind: note-to-existing
title: "Note on bento-e583: confirmed mechanism (the leftover-preview refusal has no owner check); add an owner lock, --force-foreign, and a two-process test"
priority: P1
type: note
labels: [audit, land-work, concurrency]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-e583
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-e583: confirmed mechanism, owner lock, --force-foreign, two-process test

Target: **bento-e583** (open, P1, "land-work-preview-* worktree destroyed mid-verification by concurrent session"). Post as a comment. Do not file a new issue. Leave the priority at P1.

Comment text:

> **Audit 2026-09-22 (shatter), finding bento-01: mechanism confirmed**
>
> The stale-preview refusal added by bento-rdtn.3 has no owner check. Its "remove them first" hint tells a lander to delete another lander's live preview. Shatter sessions followed that hint and destroyed each other's in-flight landings.
>
> **Code (re-verified 2026-09-23 at bento origin/main b1bb787; land-work scripts unchanged since 1c0c1e6), `catalog/skills/land-work/scripts/`:**
> - `land-work-create-preview.py:62-81`: `leftover_preview_worktrees()` returns every registered worktree whose name starts with `land-work-preview-`. It checks no pid, lock, session or age.
> - `land-work-create-preview.py:252-275`: when any leftovers exist, it fails with `leftover land-work-preview-* worktree(s) exist: ...; remove them first (land-work-create-preview.py --cleanup --preview-dir <p>) or pass --allow-existing`.
> - `land-work-create-preview.py:163-173`: `cleanup_preview()` runs `git worktree remove --force` on any path it is given, with no owner check.
> - `land.py:291`: `create_preview` is called with only `--base-ref`. It never passes `--allow-existing`, so every concurrent landing in the same repo trips the refusal.
>
> **Transcript evidence (shatter, 2026-09-20/21):**
> - 23:49: session 87606e10 got `create_preview: failed` with "leftover ... /tmp/land-work-preview-mz7t9g27; remove them first". `git status` inside that preview showed a merge in progress, meaning it was live. The session ran `--cleanup` on it. The owning session 9f13ca23 then reported `verify: failed (238.362s)` and `cleanup: failed`.
> - 23:54-23:55: the same thing in the other direction (previews d1bfx2yj and g663hhnu, this time removed with `rm -rf`).
> - 00:00: a session hit `FileNotFoundError` on its own preview 2vu7job9.
>
> **Proposed additions to acceptance criteria:**
> 1. create-preview writes an owner record (`.land-work/owner.json`: pid, hostname, session id when known, start time, branch) into each scratch preview. land.py holds an `fcntl.flock` on it for the whole run.
> 2. `leftover_preview_worktrees()` skips a preview whose lock is held or whose recorded pid is alive on this host. When only such previews exist, it proceeds, or, if a repo-wide landing lease is wanted, fails with "another landing is in progress (pid N, branch B); wait". Neither message says "remove".
> 3. Only previews that are unlocked, have a dead pid, and are older than a grace period count as leftovers.
> 4. `--cleanup` refuses a preview it does not own (lock held by another process, or recorded pid alive and not the caller's) unless `--force-foreign` is passed. The refusal names the owner.
> 5. Regression test with two real processes: process A creates a preview and holds it (for example through the existing `BENTO_LAND_TEST_DELAY_*` style seam), and process B runs create-preview and `--cleanup` against it. A's preview must still exist and A must finish. Commit the test failing against the current code first, then passing.
>
> **Proof at close:** the two-process test's name and a failing-then-passing run (`python3 -m unittest tests.land_work.<module>`), not "merged".
>
> Related: bento-rdtn.3 (closed; introduced the refusal), bento-7n7 and bento-gd2 (closed; preview leaks), `stale-previews-leak-and-scoping` (same epic; leak and scoping side), `dyp7-admission-control` (host load).

---

<!-- file: 02-land-py-verifier-log-kept.md -->

---
slug: land-py-verifier-log-kept
kind: new
title: "land.py reports verifier.log as output_path after deleting it along with the preview worktree"
priority: P1
type: bug
labels: [audit, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land.py reports verifier.log as output_path after deleting it along with the preview worktree

## Problem

When the verify step fails under `land.py`, the final JSON's `output_path` points to `<preview>/.land-work/verifier.log`. By the time that JSON is printed, land.py has already removed the preview worktree, so the file does not exist. The agent cannot see why the verifier failed. In shatter the agent had to rebuild a preview by hand and rerun the gates, which took several more minutes each time.

bento-rdtn.4 (closed) was meant to persist raw verifier output. bento-rdtn.14 (closed) added land.py, whose failure path cleans up the preview. Each was tested on its own, and no test covers the two together. On the land.py path, rdtn.4's goal is not met.

## Evidence

Re-verified 2026-09-23 at bento origin/main b1bb787. The land-work scripts are unchanged since the audit's 1c0c1e6. Paths are relative to `catalog/skills/land-work/scripts/`.

- `land-work-run-verifier.py:351`: `log_path = Path(args.log).resolve() if args.log else candidate / ".land-work" / "verifier.log"`. The default path is inside the candidate, which is the preview.
- `land-work-run-verifier.py:138`: already computes `diagnostics["verifier_log_tail"]` (last 20 lines).
- `land.py:296-305`: builds `verifier_args` without `--log`.
- `land.py:126`: `raise StepFailure(step, message, output_path=payload.get("verifier_log"))`. `verifier_log_tail` is dropped.
- `land.py:365-375`: the `except StepFailure` handler calls `driver.cleanup_preview()` (`git worktree remove --force` on the preview) and then emits `"output_path": exc.output_path`.
- Shatter session 9f13ca23 had three verify failures: 2026-09-19 15:38, 2026-09-20 23:49 and 2026-09-20 23:55. Each final JSON reported `output_path=/tmp/land-work-preview-XXXX/.land-work/verifier.log` after `cleanup: passed`, and the file no longer existed. Previews involved include g663hhnu, jus6t3ug and mz7t9g27.

## Acceptance criteria

- [ ] After a failed verify step, `output_path` in land.py's final JSON names a file that exists after land.py exits. The file is outside any preview or scratch worktree.
- [ ] The failure JSON includes `verifier_log_tail` (last N lines, reusing run-verifier's existing value).
- [ ] Successful runs keep their log too, at the same location, and the path is included in the success JSON. Retention is bounded, for example the last 20 logs per repo, and the bound is documented.
- [ ] Regression test in `tests/land_work/test_land_driver.py`: a verifier stub that fails and prints a marker line. The test asserts `os.path.exists(result["output_path"])`, that the log contains the marker, and that `verifier_log_tail` contains it. The test is committed failing against the current code first, then passing.
- [ ] Proof at close: the test name and the failing-then-passing `python3 -m unittest tests.land_work.test_land_driver` output. "Merged" is not sufficient.

## Suggested approach

- land.py passes `--log <state_dir>/<repo-name>/<branch>-<UTC timestamp>.log` to run-verifier. `<state_dir>` is `$XDG_STATE_HOME/bento/land-work`, falling back to `~/.local/state/bento/land-work`. Use the same directory as the progress log from `land-py-invocation-progress-log`, so one landing leaves its files side by side.
- Add an optional `tail` argument to `StepFailure` and fill it from `payload.get("verifier_log_tail")`.
- Keep cleanup-before-emit unchanged. The log now lives outside the preview, so the order no longer matters.

## Out of scope

- Shatter's own verifier discarding gate output with `>/dev/null 2>&1`. That is shatter str-qwua7.55. The bento-side contract is `verifier-contract-migration`.
- Preview ownership and locking (bento-e583).

## Dependencies

- Blocked by: none.
- Related: bento-rdtn.4 and bento-rdtn.14 (closed; see `verifier-log-reopen-note`), `land-py-invocation-progress-log` (shares the state directory), `verifier-contract-migration`.

Priority: P1 · Type: bug · Labels: audit, land-work · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/02, bento-02

---

<!-- file: 03-verifier-log-reopen-note.md -->

---
slug: verifier-log-reopen-note
kind: reopen-note
title: "Comment on closed bento-rdtn.4: on the land.py path the persisted verifier log is deleted before it is reported"
priority: P1
type: note
labels: [audit, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-rdtn.4
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Comment on closed bento-rdtn.4

Target: **bento-rdtn.4** (closed, "land-work-run-verifier: persist raw verifier output and report 'killed' distinctly from 'failed'"). Post as a comment only. Do not reopen it; the fix is tracked in the new issue.

Comment text:

> Audit 2026-09-22 (shatter) follow-up, finding bento-02. This issue's goal is not met when landing goes through `land.py` (bento-rdtn.14). run-verifier persists the raw log by default at `<candidate>/.land-work/verifier.log` (`land-work-run-verifier.py:351`), which is inside the preview worktree. land.py does not pass `--log` (`land.py:296-305`). On failure it raises `StepFailure(..., output_path=payload.get("verifier_log"))` (`land.py:126`), then calls `cleanup_preview()`, which runs `git worktree remove --force`, before emitting `output_path` (`land.py:365-375`). The reported log therefore never exists. `verifier_log_tail` is also dropped.
>
> Observed in shatter session 9f13ca23 on all three verify failures (2026-09-19 15:38, 2026-09-20 23:49, 2026-09-20 23:55): `output_path=/tmp/land-work-preview-XXXX/.land-work/verifier.log` was reported after `cleanup: passed`, and the file was gone. The agent rebuilt a preview by hand to find the failure.
>
> Tracked in `<id of land-py-verifier-log-kept>`, which moves the log out of the preview, propagates the tail, and requires a failing-then-passing test that checks the file exists. For future closes: the test for a persistence fix should run through the real driver (land.py), not only through the component script.

(Filer: replace `<id of land-py-verifier-log-kept>` with the id assigned to that slug.)

---

<!-- file: 04-land-work-post-push-workflow-health.md -->

---
slug: land-work-post-push-workflow-health
kind: new
title: "land-work: report GitHub workflow conclusions for the landed SHA; doctor flags workflows that stay red on the primary branch"
priority: P1
type: feature
labels: [audit, land-work, hygiene, ci]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land-work: report GitHub workflow conclusions for the landed SHA; doctor flags workflows that stay red on the primary branch

## Problem

Nothing in the bento agent workflow looks at GitHub Actions results after a landing. Agents close issues citing a local verifier pass, while workflows on the primary branch can stay red for weeks without anyone noticing. In shatter, every workflow except `ci.yml` is persistently red. Fixing those workflows is shatter work. This issue adds the missing feedback loop on the bento side, so that any consumer repo, not only shatter, sees workflow conclusions at landing time and at session start.

This is the bento counterpart of shatter's `workflow-health-patrol` (shatter epic, bucket shatter-ci-workflows). That issue adds a repo-side drift-patrol check. This one makes land-work and the doctor report the signal to every agent.

## Evidence

Re-verified 2026-09-23: `gh run list --workflow <w> -L 50 --json conclusion` in the shatter audit worktree (56c86168):

| Workflow | Last 50 runs |
|---|---|
| drift-patrol.yml | 7 failure, 2 success (last success 2026-08-07) |
| perf-ci.yml | 13 failure, 0 success |
| devcontainer.yml | 19 failure, 2 success (both 2026-02) |
| docker-publish.yml | 7 failure, 0 success |
| release.yml | 50 failure, 0 success (0 successes ever; 169 failures and 98 cancellations in the last 300) |
| ci.yml | 41 success, 7 failure, 2 cancelled |

Bento code, at origin/main b1bb787 (land-work scripts unchanged since the audit's 1c0c1e6):
- `catalog/skills/land-work/scripts/land.py:273-341`: `run()` ends after `verify_landing`. There is no `gh` call anywhere in land-work (`grep -rn "gh run" catalog/skills/land-work` finds nothing).
- `land-work-verify-landing.py` checks that the expected tree landed on the ref, not CI.
- `catalog/hooks/bento/{claude,codex}/scripts/agent-env-doctor.py` has no workflow-health check (no `gh` invocation).
- bento-1qry (in_progress) binds local gate evidence and adds a stop when the primary branch is red locally. It does not consult GitHub workflow conclusions.

## Acceptance criteria

- [ ] After a successful push, land.py (or verify-landing) runs `gh run list --commit <landed-sha> --json name,conclusion,status,url`, polling until all runs complete or a timeout expires. The timeout is configurable in verifier.json (`post_push_workflows: {enabled, timeout_s}`, default enabled with about 600 s; 0 disables it).
- [ ] land.py prints one line per workflow and adds `workflows: [{name, conclusion, url}]` plus `workflows_status: complete|timed_out|skipped` to the final JSON. A `failure` conclusion is reported as a warning. The landing is not rolled back and the exit code is unchanged.
- [ ] The step is skipped cleanly, with `workflows_skipped_reason` in the JSON, when `gh` is missing or unauthenticated or the remote is not GitHub.
- [ ] The SessionStart doctor lists each workflow on the primary branch, scheduled workflows included, whose last 3 or more completed runs all failed. It prints one collapsed line per workflow with the latest run URL, and caches the result so SessionStart makes at most one `gh` call per repo per hour.
- [ ] Tests use a stubbed `gh` on PATH and cover: all green, one failure (warning, exit 0), timeout, `gh` missing, non-GitHub remote, and the doctor's 3-consecutive-failures rule.
- [ ] Proof at close: the test names and passing output, plus one real land.py run against a GitHub repo whose final JSON contains a populated `workflows` array (paste it into the close reason). "Merged" is not sufficient.

## Suggested approach

- Reuse the landed SHA that land.py already has (`merge_sha`). Run the poll after `verify_landing`, so a slow CI never delays the landing verdict itself.
- Build the doctor check on `gh run list --branch <primary> --workflow <file> -L 3`, with the list of workflows taken from `gh workflow list`. Include scheduled workflows: they are the ones nobody watches.
- Keep the output collapsed. One line per red workflow is the ceiling.

## Out of scope

- Fixing shatter's workflows: drift-patrol `go-version-file` (`drift-patrol-workflow-go-mod`), the release.yml Windows Z3 and aarch64 openssl legs (`release-windows-z3-build` and `release-aarch64-openssl-cross`, which per maintainer decision D1 are fixed, not dropped), perf-ci, devcontainer and docker. Those are shatter issues.
- Making CI a hard landing gate.
- Filing tracker issues automatically for red workflows. That belongs to the shatter-side `workflow-health-patrol`.

## Dependencies

- Blocked by: none.
- Related: bento-1qry (in_progress; local gate evidence), bento-rdtn.14 (closed; land.py), shatter `workflow-health-patrol` (repo-side counterpart), `merge-push-observability`.

Priority: P1 · Type: feature · Labels: audit, land-work, hygiene, ci · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/04, tests-ci-03 (verifier kept P1)

---

<!-- file: 05-land-py-merge-abort-ownership.md -->

---
slug: land-py-merge-abort-ownership
kind: new
title: "land.py aborts or hard-resets merge state in the shared primary checkout even when it did not start the merge"
priority: P2
type: bug
labels: [audit, land-work, safety, concurrency]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land.py aborts or hard-resets merge state in the shared primary checkout even when it did not start the merge

## Problem

On any failure or signal, land.py runs `git merge --abort` in the primary checkout whenever `.git/MERGE_HEAD` exists. This happens even for failures at `prepare`, `create_preview` or `verify`, before land.py has touched the primary. Separately, the tree-mismatch recovery path runs `git reset --hard HEAD@{1}`, which resets to the wrong commit if anything else moved HEAD in the meantime. In repos where several sessions share one primary checkout, as in shatter, either action can destroy another session's in-progress merge or commits.

This was found by reading the code. No collision has been observed yet. It is the same class of failure as bento-e583 (acting on shared state without an ownership check).

## Evidence

Re-verified 2026-09-23 at bento origin/main b1bb787 (unchanged since 1c0c1e6). File: `catalog/skills/land-work/scripts/land.py`.

- `:144-148`: `abort_primary_merge_if_in_progress()` runs `git merge --abort` whenever `self.primary_root/.git/MERGE_HEAD` exists. It does not check who created it.
- It is called from the signal handler (`:348-350`), the `except StepFailure` handler (`:365-368`) and the `except BaseException` handler (`:376-378`). `self.primary_root` is set right after `prepare` (`:278`), so a failure in `create_preview` or `verify` still reaches the abort.
- `:196-199`: after land.py's own failed `git merge`, the abort is correct.
- `:200-204`: on tree mismatch, `subprocess.run(["git", "reset", "--hard", "HEAD@{1}"], cwd=primary_root, ...)`. `HEAD@{1}` is the previous reflog entry. It is only the pre-merge SHA if nothing else moved HEAD, and in the `:177-185` path land.py has itself just fast-forwarded, which adds its own reflog entry.

## Acceptance criteria

- [ ] land.py records `merge_started = True` and `pre_merge_sha = rev_parse("HEAD", primary_root)` immediately before its own `git merge` in `_merge_in_primary`. `abort_primary_merge_if_in_progress()` does nothing unless `merge_started` is true.
- [ ] The tree-mismatch path resets to the recorded `pre_merge_sha` and never to `HEAD@{1}`. Before resetting, it checks that HEAD is still land.py's own merge commit. If it is not, it refuses to reset and reports the situation.
- [ ] Test: a primary checkout with a pre-existing `MERGE_HEAD`, plus a land.py failure injected at `verify`, leaves `MERGE_HEAD` and the index untouched.
- [ ] Test: a SIGINT during land.py's own merge (existing `BENTO_LAND_TEST_DELAY_MERGE` seam) still aborts, which preserves current behaviour.
- [ ] Test: a forced tree mismatch after a fast-forward restores exactly the recorded SHA.
- [ ] Proof at close: the three test names, with the first test shown failing against the current code and then passing (`python3 -m unittest tests.land_work.test_land_driver`). "Merged" is not sufficient.

## Suggested approach

Store the two fields on `Driver`. Set them inside `_merge_in_primary` just before the `git merge --no-ff` call at `:193`. Clear `merge_started` once the push succeeds.

## Out of scope

- Locking between sessions more generally: preview ownership is bento-e583, host admission is bento-dyp7.
- The `_push_from_preview` route, which never merges in the primary.

## Dependencies

- Blocked by: none.
- Related: bento-e583, bento-dyp7, bento-rdtn.14 (closed).

Priority: P2 · Type: bug · Labels: audit, land-work, safety, concurrency · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/06, bento-05

---

<!-- file: 06-land-py-invocation-progress-log.md -->

---
slug: land-py-invocation-progress-log
kind: new
title: "land.py: document the canonical long-running invocation; write a progress log and a heartbeat"
priority: P2
type: feature
labels: [audit, land-work, skills]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land.py: document the canonical long-running invocation; write a progress log and a heartbeat

## Problem

A land.py run takes 10-25 minutes in shatter: create_preview about 300 s, verify 266-578 s, merge_push 259-1108 s. The skill shows only `land-work/scripts/land.py --runtime <runtime>`. Agents run it however they like, and the Bash tool's 2-minute foreground limit forces them to improvise. In shatter session 9f13ca23 it was run 14 times as `land.py --runtime claude 2>&1 | tail -100`. Four of those runs failed (`prepare: failed`, `create_preview: failed`, `verify: failed`) but came back `[exited with code 0]`, because the pipe returned tail's exit status. land.py itself exits 1 on failure. While it runs, land.py writes nothing to disk and prints nothing between step lines, so an agent cannot tell a slow step from a hung one.

## Evidence

Re-verified 2026-09-23 at bento origin/main b1bb787 (unchanged since 1c0c1e6).

- `catalog/skills/land-work/SKILL.md:258-263`: the only invocation shown is `land-work/scripts/land.py --runtime <runtime>`. There is no guidance on backgrounding, output capture or exit codes.
- `catalog/skills/land-work/scripts/land.py:93-103`: `_record()` prints one line per step to stderr after the step finishes. There is no progress file and no heartbeat.
- `land.py:386-393`: the exit code is 1 on failure. Piping through `tail` hides it.
- Transcript 9f13ca23 contains 36 occurrences of `land.py --runtime claude 2>&1 | tail -100`.
- Shatter memory `feedback_never_pipe_git_commit_through_tail.md` records the same lesson, which was never pushed upstream into bento.

## Acceptance criteria

- [ ] SKILL.md has a short "Running land.py" block that says to:
  - run it in the background (the runtime's background mechanism, for example `run_in_background`, or `nohup`), with stdout (the final JSON) going to a file and stderr going to a log;
  - read the exit status directly and never pipe land.py through `tail`/`head`;
  - wait for the completion notification instead of polling in a loop;
  - read the final JSON's `failed_step`, `error` and `output_path`.
- [ ] land.py creates a progress log at `<state_dir>/<repo-name>/<branch>-<UTC timestamp>.progress.log`, where `<state_dir>` is `$XDG_STATE_HOME/bento/land-work`, falling back to `~/.local/state/bento/land-work`. It prints `progress log: <path>` as the first stderr line, and every step line also goes to the log.
- [ ] During any step, land.py writes a heartbeat line (`… <step> still running (<elapsed>s)`) to stderr and to the log every 60 s. The interval can be overridden by an environment variable (for example `BENTO_LAND_HEARTBEAT_S`) for tests.
- [ ] The final JSON includes `progress_log`.
- [ ] Tests assert: the first stderr line names an existing file; with a 1 s heartbeat and a verifier stub that sleeps 3 s, at least two heartbeat lines appear; the exit code is 1 on an injected failure.
- [ ] Proof at close: test names plus passing output. "Merged" is not sufficient.

## Suggested approach

- Run each child script with `subprocess.Popen` and a timer thread, or a `wait(timeout=interval)` loop, that emits the heartbeat. Keep capturing child stdout for the JSON payload as now.
- Share the state directory and naming with `land-py-verifier-log-kept`, so one landing's verifier log and progress log sit side by side.

## Out of scope

- Splitting merge_push into timed sub-steps (`merge-push-observability`).
- The broader SKILL.md rewrite (`land-work-skill-restructure`, which reuses this block).

## Dependencies

- Blocked by: none.
- Blocks: `land-work-skill-restructure`, `merge-push-observability`.
- Related: bento-rdtn.14 (closed; per-step lines), shatter str-qwua7.26 (open; 2-minute foreground rule in shatter guidance).

Priority: P2 · Type: feature · Labels: audit, land-work, skills · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/07, bento-06

---

<!-- file: 07-landing-deletes-remote-branches.md -->

---
slug: landing-deletes-remote-branches
kind: new
title: "land-work: delete the landed remote feature branch and superseded same-issue branches as a verified step"
priority: P2
type: feature
labels: [audit, land-work, cleanup]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land-work: delete the landed remote feature branch and superseded same-issue branches as a verified step

## Problem

Landing cleans up only the local branch and its worktree. Consumer repos therefore pile up merged remote branches, and branches that a re-landing superseded stay on the remote. Shatter's AGENTS.md calls remote deletion "mandatory" and keeps a hand-run `scripts/cleanup-merged-remote-branches.sh` to make up for it, but land.py never deletes a remote branch and never checks for one. Close reasons still claim the deletions happened.

## Evidence

Re-verified 2026-09-23 in the shatter audit worktree (56c86168) and bento origin/main b1bb787:

- `git branch -r | wc -l` gives 66, and `git branch -r --merged origin/main | wc -l` gives 38. Examples: `origin/str-hjrnp.1` through `.4`, `origin/str-2tyfk-lint-errcheck`.
- `git ls-remote origin 'refs/heads/str-qwua7*'` still lists `str-qwua7.4-testplan-http-body-fix`, `str-qwua7.7-protocol-registry-validate`, `str-qwua7.16-restore-bd-dolt` and `str-qwua7.17-stale-claims-cleanup`. They are unmerged, each about 102 commits ahead of main, and each contains a stray fixture commit, e50fc399 "init". The close reason for str-qwua7.4 says its duplicate branches were deleted; one of them is still there.
- At audit time a `/tmp/land-work-preview-a5l9ycto` worktree (detached at 16794cef) was still registered after its landing. It has since been removed (`git worktree list` now shows no preview), but nothing in land.py checks for its own preview after cleanup.
- `catalog/skills/land-work/SKILL.md:516-533` (step 10) deletes only the local branch and worktree.
- `catalog/skills/land-work/scripts/land.py`: the only pushes are to the primary ref (`:211`, `:232-233`). There is no `git push origin --delete`.

## Acceptance criteria

- [ ] After `verify_landing` succeeds, land.py deletes `origin/<feature>` when it is an ancestor of the landed SHA. Repos can opt out with verifier.json `delete_remote_branch: false`.
- [ ] land.py then confirms the deletion with `git ls-remote --heads origin <feature>` and reports `remote_branch_deleted: true|false|skipped` (plus a reason) in the final JSON. A failed deletion is a warning, not a failed landing.
- [ ] Optional `--superseded <branch>...`: land.py deletes those remote branches too, but only after checking that each one's issue id (parsed with the same issue-id rule closure uses) matches the landed branch's issue id. It confirms each deletion with `ls-remote` and reports it.
- [ ] land.py removes its own scratch preview in a `finally` block and confirms that `git worktree list --porcelain` no longer contains it. The result is reported in the JSON.
- [ ] Closure gains a report-only section listing remote branches already merged into the primary branch, with the `git push origin --delete` command for each. It does not delete anything automatically.
- [ ] Tests with a bare-remote fixture cover: merged branch deleted; opt-out honoured; unmerged feature ref not deleted; `--superseded` with a mismatched issue id refused; preview absent after both success and failure.
- [ ] Proof at close: test names plus passing output, and one real landing's JSON showing `remote_branch_deleted: true`. "Merged" is not sufficient.

## Suggested approach

Add a `delete_remote` step after `verify_landing`, recorded with `_record()` like the other steps. Use `git merge-base --is-ancestor origin/<feature> <merge_sha>` as the safety check.

## Out of scope

- One-off cleanup of shatter's existing 38 merged and 4 contaminated remote branches. That is a shatter task.
- The general preview-leak and scoping work (`stale-previews-leak-and-scoping`, bento-e583).

## Dependencies

- Blocked by: none.
- Related: bento-rdtn.14 (closed), bento-gd2 and bento-7n7 (closed; preview leaks), bento-rdtn.9 (closed; closure tracker_mismatch), `close-reason-evidence` (bucket bento-guards-doctor-tracker), shatter str-qwua7.19.

Priority: P2 · Type: feature · Labels: audit, land-work, cleanup · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/11, bento-15, prior-06

---

<!-- file: 08-verifier-contract-migration.md -->

---
slug: verifier-contract-migration
kind: new
title: "Verifier contract drift: warn when verifier payloads lack `executed`, flag outdated manifests, require gate output pass-through"
priority: P2
type: feature
labels: [audit, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Verifier contract drift: warn when verifier payloads lack `executed`, flag outdated manifests, require gate output pass-through

## Problem

bento-rdtn.6 added a per-check `executed` flag and a guard that fails when every check was served from cache. By the maintainer's decision on that issue, payloads without the field are accepted silently. So verifiers wired before the contract changed get no protection and no signal that they are out of date. Existing consumer repos are never prompted to regenerate their wrappers.

Shatter's hand-written verifier shows the consequence. It emits only `{name, status}` and runs every gate with `>/dev/null 2>&1`, so even the rdtn.4 persisted log is empty. Shatter's `task` gates also report cached "is up to date" tasks as passes. A cached no-op landing gate therefore looks exactly like a real run.

## Evidence

Re-verified 2026-09-23 at bento origin/main b1bb787 (unchanged since 1c0c1e6) and the shatter audit worktree 56c86168:

- `catalog/skills/land-work/scripts/land-work-run-verifier.py:520-545`: the all-cached guard trips only when `all(c["executed"] is False ...)`. The comment says a payload with no `executed` field "stays fully accepted".
- `catalog/skills/land-work/scripts/land.py:116-121`: prints `[cached]`/`[executed]` only when the flags exist.
- `catalog/skills/wire-land-verifier/scripts/wire-land-verifier.py:853-917`: generated wrappers do emit `executed`. Nothing detects or prompts repos whose wrappers predate this.
- `catalog/skills/wire-land-verifier/` has only `SKILL.md`, `metadata.json` and `scripts/`, with no reference doc. The output pass-through requirement belongs in SKILL.md.
- Shatter `scripts/land_work_verifier.sh` (last changed d01a22db, 2026-08-05): `run_check` runs `"$@" >/dev/null 2>&1` (line 17) and emits `{"name":...,"status":...}` only.
- None of the 11 land.py verify lines observed in shatter transcripts shows `[cached]` or `[executed]`.

## Acceptance criteria

- [ ] When any selected check lacks `executed`, run-verifier adds the warning `checks without execution evidence: <names>` to its payload. land.py prints it under the verify step line (land.py currently ignores run-verifier warnings) and includes it in the final JSON.
- [ ] verifier.json gains `contract_version`, and wire-land-verifier writes the current version. The SessionStart doctor flags a manifest with a missing or older `contract_version` with one line: "land-work verifier predates contract vN; regenerate with wire-land-verifier".
- [ ] wire-land-verifier's SKILL.md and generated wrapper template state that gate stdout and stderr must pass through to the verifier's output (captured into the verifier log), never to `/dev/null`. A test asserts that the generated wrapper contains no `/dev/null` redirection of a gate command.
- [ ] Tests cover: payload without `executed` produces the warning, and land.py shows it; payload with `executed` produces no warning; doctor output for missing, old and current `contract_version`.
- [ ] Proof at close: test names plus passing output. "Merged" is not sufficient.

## Suggested approach

Keep the rdtn.6 decision: a missing field still does not fail the verify step. Add the warning and the doctor nudge as a deprecation path. A later issue can make the field mandatory once consumers have regenerated.

## Out of scope

- Rewriting shatter's verifier: shatter str-qwua7.55 and str-qwua7.2.
- Making a missing `executed` a hard failure.

## Dependencies

- Blocked by: none.
- Related: bento-rdtn.6 and bento-rdtn.4 (closed), `land-py-verifier-log-kept`, shatter str-qwua7.55, str-qwua7.2.

Priority: P2 · Type: feature · Labels: audit, land-work · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/14, bento-13, agent-repo-11 (context)

---

<!-- file: 09-land-work-skill-restructure.md -->

---
slug: land-work-skill-restructure
kind: new
title: "land-work SKILL.md: restructure around land.py, move the manual and batch flows to references, remove $(...) that contradicts its own Command Rule"
priority: P2
type: task
labels: [audit, land-work, skills]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [land-py-invocation-progress-log]
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land-work SKILL.md: restructure around land.py, move the manual and batch flows to references, remove $(...) that contradicts its own Command Rule

## Problem

`catalog/skills/land-work/SKILL.md` is the largest bento skill, and all of it loads on every landing: the manual 10-step flow, batch mode and the manifest rules, even though land.py now drives the serial path. The skill also contradicts itself:

- Its Command Rule forbids `$(...)` in commands, but its own command blocks use it.
- Its Tracker Handoff prescribes a `.beads/issues.jsonl` policy ("may be intentionally untracked ... do not re-add or commit it") instead of deferring to the tracker skill. Consumer repos document different policies. Shatter's AGENTS.md currently requires committing the file, though maintainer decision D4 retires shatter's JSONL import in favour of a Dolt remote.

In shatter, bento:land-work was invoked for only about 9 of 35 first-parent merges to main since 2026-09-05. Several other landings were hand-scripted, including 4 raw `git push --no-verify ... :refs/heads/main` by one session.

bento-by8 (open, P3) trims about 1,500 words of repeated text across several skills. This issue goes further for land-work: a structural rewrite that makes land.py the serial path.

## Evidence

Re-verified 2026-09-23 at bento origin/main b1bb787 (unchanged since 1c0c1e6):

- `wc -w catalog/skills/land-work/SKILL.md` gives 6,342 words (45,480 bytes). `swarm/SKILL.md` is 4,178 words, the next largest.
- Command Rule, `SKILL.md:101-106`: "... with `&&`, pipes, `$(...)`, or inline interpreters". Violations in commands the skill tells agents to run: `:143` (`--head-sha $(git rev-parse HEAD)`), `:181-182` (`BASE_SHA=$(git merge-base ...)`, `HEAD_SHA=$(git rev-parse HEAD)`), `:449` (`--merge-sha $(git rev-parse <primary-branch>)`). `:670` is deliberately escaped (`\$(...)`) and explained, so it is not a violation.
- The land.py invocation sits in step 7a (`:255-263`), behind the manual compare-and-set flow. Batch Landing runs `:545-722`.
- Tracker Handoff, `:783-789`.

## Acceptance criteria

- [ ] The serial path in SKILL.md is: run land.py (using the "Running land.py" block from `land-py-invocation-progress-log`), read the final JSON, and fix the step it names. The manual compare-and-set flow, batch mode and the manifest rules move to `catalog/skills/land-work/references/`, and SKILL.md links to each by name.
- [ ] `wc -w catalog/skills/land-work/SKILL.md` is at most 2,500.
- [ ] No unescaped `$(` appears in any fenced command block of SKILL.md or of its references. The computations move into scripts, or land.py derives the values itself.
- [ ] The Tracker Handoff states no JSONL policy of its own. It defers to `beads-issue-flow` or `github-issue-flow` and to the repo's own documented policy. (`beads-dolt-remote-guidance` in this epic gives beads-issue-flow the D4 guidance: the JSONL is an export, and sync goes through a Dolt remote.)
- [ ] A skill lint test (in the existing skill test suite) fails when any `SKILL.md` or `references/*.md` fenced `bash`/`sh` block contains an unescaped `$(`, and when land-work's SKILL.md exceeds the word budget. It is committed failing against the current SKILL.md, then passing.
- [ ] Proof at close: the `wc -w` output, plus the lint test failing and then passing. "Merged" is not sufficient.

## Suggested approach

1. Land `land-py-invocation-progress-log` first so the invocation block exists.
2. Move sections out verbatim into references, then trim what remains in SKILL.md.
3. Add `land.py --print-shas` or similar only if the references still need computed SHAs. Otherwise let land.py compute them.
4. Coordinate with bento-by8 so the two do not trim the same paragraphs twice. Link the new issue as related to bento-by8.

## Out of scope

- Changes to land.py's behaviour, apart from any small helper needed to remove `$(...)`.
- The beads-issue-flow content itself (`beads-dolt-remote-guidance`).

## Dependencies

- Blocked by: `land-py-invocation-progress-log`.
- Related: bento-by8 (open; this extends it), bento-7kw, `beads-dolt-remote-guidance` (bucket bento-guards-doctor-tracker).

Priority: P2 · Type: task · Labels: audit, land-work, skills · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/16, bento-14 · Decision: D4 (Tracker Handoff wording only)

---

<!-- file: 10-eth-swarm-lead-lands-from-teammate.md -->

---
slug: eth-swarm-lead-lands-from-teammate
kind: note-to-existing
title: "Note on bento-eth: the swarm lead should land from a lead-owned scratch worktree, not the teammate's; add land.py --branch"
priority: P2
type: note
labels: [audit, swarm, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-eth
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-eth: land from a lead-owned scratch worktree

Target: **bento-eth** (open, P3, "Bake recurring teammate-prompt lessons into the swarm template; make lead-lands the default ..."). Post as a comment. Do not file a new issue. The audit rates this addendum P2. The maintainer can decide whether to raise bento-eth.

Comment text:

> **Addendum from the shatter audit 2026-09-22 (finding bento-12)**
>
> bento-eth makes lead-lands the default, and bento-qiw settles who lands. Neither says *where* the lead lands from. Today the swarm skill has the lead land from inside the teammate's worktree:
>
> - `catalog/skills/swarm/SKILL.md:337-342` (Serial-Mode Landing step 3, re-verified 2026-09-23 at bento origin/main b1bb787): "Invoke `bento:land-work` from within the teammate's worktree". land.py derives the branch from its cwd and needs the branch up to date (`--require-up-to-date`, `land.py:274`), so the lead ends up running checkout and rebase inside the teammate's tree.
> - A shatter lesson from 2026-09-08 (memory `feedback_lead_landing_prep_separate_worktree`) records that this "silently moved the worktree's HEAD out from under" a teammate who was still making review fixes.
>
> **Proposed additions to acceptance criteria:**
> 1. The lead lands from a lead-owned scratch worktree created at the teammate's branch tip, or from the teammate's worktree only after confirming that the teammate session has exited.
> 2. `land.py --branch <name>` lands a named branch from any worktree without checking it out in the teammate's tree. The preview is built from the ref, and local cleanup skips any worktree it does not own.
> 3. Swarm SKILL.md step 3 is updated to match, and the teammate prompt template says "the lead will not touch your worktree while you are live".
> 4. Test: `land.py --branch` run from a different worktree leaves the teammate worktree's HEAD, index and working tree unchanged.
>
> Proof at close: the test above passing, and the SKILL.md diff. Related: bento-qiw, `rebase-before-land-configurable` (drops the rebase need when a repo opts out of it).

---

<!-- file: 11-dyp7-admission-control.md -->

---
slug: dyp7-admission-control
kind: note-to-existing
title: "Note on bento-dyp7: route git-hook gates and land.py's verify step through the admission governor; print a 'machine busy' status line"
priority: P2
type: note
labels: [audit, land-work, concurrency, performance]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-dyp7
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-dyp7: hooks and land.py through the governor

Target: **bento-dyp7** (open, P1, "Host-wide weighted admission governor for heavy agent jobs"). Post as a comment. Do not file a new issue. Preview ownership locking is a separate matter, covered by bento-e583 (`e583-preview-owner-lock`).

Comment text:

> **Addendum from the shatter audit 2026-09-22 (finding sessions-07)**
>
> Observed on the shared 32-core host while sessions from several projects ran at once (shatter, kapow, pickpackit, Codex):
> - load average 141-174, and swap nearly full on 2026-09-19 (agents' own AskUserQuestion texts: "System load just hit 141 (32 cores) ...", "System memory is critically tight (swap nearly full)");
> - in 15 sessions, 49 `ps`, 23 `uptime`, 16 `pgrep` and 10 `free` calls spent diagnosing load by hand.
>
> The heaviest gates run where the governor cannot see them. Shatter's git hooks (pre-commit `cargo test`, pre-push `task affected` / `task check` for main) and land.py's verifier do not go through `run-heavy`. Re-verified 2026-09-23: `grep -rn run-heavy` over bento's `catalog/skills/land-work/scripts/` and over shatter's `scripts/precommit-rust.sh` finds nothing. `run-heavy` exists only at `catalog/skills/launch-work/scripts/run-heavy`, and nothing calls it automatically.
>
> **Proposed additions to scope:**
> 1. The governor documents a hook entry point (`run-heavy -- <cmd>`) that consumer repos' git hooks can call, with a copy-paste example for pre-commit and pre-push.
> 2. land.py's verify step (and its merge_push step, when the repo declares `push_hook_runs_gates`; see `merge-push-observability`) acquires a governor slot. When the host is overloaded it waits instead of failing or competing.
> 3. While waiting, land.py prints one line, `machine busy: load X/<cores>, mem Y% — waiting for slot (Ns)`, and repeats it through the heartbeat from `land-py-invocation-progress-log`. Agents then stop diagnosing load by hand.
> 4. The doctor mentions the hook entry point once when a repo's hooks run known-heavy commands without it.
>
> Acceptance proof for these additions: a test in which a saturated governor makes land.py's verify wait (visible as the status line) and then proceed, plus the documented hook example. The audit rates this addendum P2 within dyp7's P1 scope.

---

<!-- file: 12-rebase-before-land-configurable.md -->

---
slug: rebase-before-land-configurable
kind: new
title: "land.py: make rebase-before-land (--require-up-to-date) a per-repo policy option; the preview already verifies the exact merge"
priority: P3
type: feature
labels: [audit, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land.py: make rebase-before-land (--require-up-to-date) a per-repo policy option

## Problem

land.py always calls prepare with `--require-up-to-date`. A branch that is behind the primary branch by even one commit fails `prepare`. The agent must then rebase and force-push. In shatter, each of those steps re-runs the slow git hooks (post-checkout beads import, pre-push `task affected`), and the Stop-hook "unpushed" counts balloon (see `check-unpushed-overcount-and-blocks`).

Rebase-first is documented, deliberate policy: SKILL.md step 5 says "Rebase onto the preferred primary-branch base", which keeps history linear. It is not a leftover to remove. But it is not needed for verification. create-preview already merges the feature onto the leased primary SHA, and the verifier checks that exact tree. Repos that do not need linear history should be able to turn it off.

## Evidence

Re-verified 2026-09-23 at bento origin/main b1bb787 (unchanged since 1c0c1e6):

- `catalog/skills/land-work/scripts/land.py:274`: `prepare = self._run_script("prepare", PREPARE, ["--require-up-to-date"])`. The flag is unconditional.
- `land.py:291`: the preview is created with `--base-ref <leased_sha>`, so verification runs on the merge with current main.
- `catalog/skills/land-work/SKILL.md:239`: step 5, "Rebase onto the preferred primary-branch base reported by the helper".
- Shatter prepare failures: "current branch is behind the primary branch by 2 commit(s)" (session 87606e10, 2026-09-20 04:53); "by 4 commit(s)" (9f13ca23, 2026-09-21 15:19); "no commits ahead; behind by 1" (2026-09-20 23:23). Each forced a rebase and force-push cycle.

## Acceptance criteria

- [ ] verifier.json gains `require_up_to_date: true|false`, defaulting to `true`, so current behaviour is unchanged for every repo that does not set it. land.py passes `--require-up-to-date` only when the value is true.
- [ ] With `false`, a branch that is behind but merges cleanly lands through the preview merge. A merge conflict in the preview still fails `create_preview`, with an error that says a rebase is needed.
- [ ] "No commits ahead" still fails under both settings.
- [ ] SKILL.md step 5 and the land.py docs describe the option and name linear history as the reason for the default.
- [ ] Tests cover: default (behind fails), `false` + behind + clean merge (lands), `false` + conflict (fails with "rebase needed").
- [ ] Proof at close: test names plus passing output. "Merged" is not sufficient.

## Out of scope

- Changing the default policy.
- Swarm-config landing blocks, unless verifier.json turns out to be the wrong home for the option; decide that during implementation and record the reason.

## Dependencies

- Blocked by: none.
- Related: `check-unpushed-overcount-and-blocks` (bucket bento-guards-doctor-tracker), `eth-swarm-lead-lands-from-teammate`.

Priority: P3 · Type: feature · Labels: audit, land-work · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/19, bento-07 (verifier lowered it from P2 to P3 because the rebase is documented policy)

---

<!-- file: 13-merge-push-observability.md -->

---
slug: merge-push-observability
kind: new
title: "land.py merge_push: split into timed sub-steps and capture pre-push hook output"
priority: P3
type: feature
labels: [audit, land-work, observability]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [land-py-invocation-progress-log]
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land.py merge_push: split into timed sub-steps and capture pre-push hook output

## Problem

`merge_push` is one opaque step. In shatter it took 258.6, 259.8, 298.7, 309.2, 498.6, 594.4, 614.9, 678.8 and 1107.8 s. Most of that time is the consumer repo's pre-push hook: for `refs/heads/main`, shatter's pre-push runs the full gate (`task check`), straight after the verifier ran its own gates. land.py captures the git output but never shows the hook output, and records only the total time. Agents cannot tell whether the step is making progress or stuck, and they cannot see that the time is going into the repo's own gates.

## Evidence

Re-verified 2026-09-23 at bento origin/main b1bb787 (unchanged since 1c0c1e6). File: `catalog/skills/land-work/scripts/land.py`.

- `:309-313`: the whole `_merge_and_push()` call is timed as one `_record("merge_push", ...)`.
- `_merge_in_primary` (`:166-214`) runs fast-forward sync, `git merge --no-ff`, a tree check and `git push`. `_push_from_preview` (`:216-~271`) runs `git commit`, `git push`, then `git fetch` and `git merge --ff-only` to sync the primary. Every call goes through `git(...)` with captured output. Push stderr, which carries the hook output, appears only inside a `StepFailure` message on failure and is never shown on success.
- Shatter `.git/hooks/pre-push` sets `SHATTER_GATE_RANK=2` for `refs/heads/main` (lines 70-71; audit verifier note on bento-18), which selects the full gate.

## Acceptance criteria

- [ ] The step line reports sub-durations, for example `merge_push: passed (620s; merge 3s, push 610s, sync 7s)`, and the final JSON has `merge_push_substeps: [{name, seconds, status}]`. The sub-steps cover whichever route ran (merge-in-primary or push-from-preview).
- [ ] Push stdout and stderr, including hook output, stream to the landing progress log from `land-py-invocation-progress-log` as they arrive, not only after the push ends. On failure, the last N lines appear in the JSON as `push_output_tail`.
- [ ] Optional verifier.json flag `push_hook_runs_gates: true`. When it is set, land.py prints `pre-push hook is running repo gates (this can take minutes)` before pushing, so the time is explained up front.
- [ ] Tests: a fake remote with a pre-push hook that prints a marker and sleeps. Assert that the sub-durations are present and add up to roughly the total, that the marker appears in the progress log, and that on a failing hook it appears in `push_output_tail`.
- [ ] Proof at close: test names plus passing output. "Merged" is not sufficient.

## Out of scope

- Whether consumer repos should keep double gating (verifier plus pre-push). That is the consumer's decision (shatter str-qwua7.55).
- Admission control for the push step (bento-dyp7; see `dyp7-admission-control`).

## Dependencies

- Blocked by: `land-py-invocation-progress-log` (the progress log the hook output streams into).
- Related: bento-rdtn.14 (closed; one line per step), `git-hook-latency-visibility` (bucket bento-guards-doctor-tracker; hook timing in other scripts).

Priority: P3 · Type: feature · Labels: audit, land-work, observability · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/20, bento-18

---

<!-- file: 14-merge-message-and-stale-branch-nudge.md -->

---
slug: merge-message-and-stale-branch-nudge
kind: new
title: "land-work: merge message names branch and issue; refuse bare-SHA merges into the primary branch; SessionStart nudge for aging pushed branches; one session per branch"
priority: P3
type: feature
labels: [audit, land-work, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land-work: merge message names branch and issue; refuse bare-SHA merges; nudge for aging branches; one session per branch

## Problem

Consumer history fills with landing noise that neither land.py nor the doctor prevents:

- Merges titled `Merge commit '<sha>' into HEAD`, which name neither a branch nor an issue. These come from manual merges of a bare SHA.
- Branches that sit pushed for weeks and then need re-landing (`-landing2` branches, or diffs "ported/rewritten against current main").
- Two concurrent sessions making opposite decisions on the same branch, which in shatter led to a revert.

Since land.py took over (2026-09-19), its merges are uniform (`Merge branch '<branch>'`), but they still omit the issue id. Manual merges are unconstrained. Stale branches are reported only when closure is run on demand.

## Evidence

Re-verified 2026-09-23 in the shatter audit worktree (56c86168) and bento origin/main b1bb787:

- `git log origin/main --merges --since=2026-09-05 --format='%h %s' | grep -c "Merge commit '"` gives 6: af6839f0, a19a8aec, 2aecd43a, c1364378, af3ae54a, 6c8bc87f (2026-09-07/08).
- `16794cef Merge branch 'str-qwua7.4-landing2'` is a re-landing. Branches authored 2026-08-27 to 08-31 landed on 09-21/22.
- `catalog/skills/land-work/scripts/land.py:194` and `:221`: the message is `f"Merge branch '{feature_branch}'"` and does not include the issue id.
- bento-rdtn.9 (closed): closure's `tracker_mismatch` report (`catalog/skills/closure/scripts/*.py`, `annotate_branches_with_tracker_mismatch`) runs only when closure is invoked.

## Acceptance criteria

- [ ] land.py's merge message is `Merge branch '<branch>' (<issue-id>)` when the branch name contains an issue id (parsed with closure's existing rule), and `Merge branch '<branch>'` otherwise. Both routes use it (`:194`, `:221`), and a test asserts both.
- [ ] land-work guidance says never to `git merge <bare-sha>` into the primary branch. Where feasible, the bento git guard refuses `git merge <40-hex or short sha>` while the primary branch is checked out, and a guard test covers it. If the guard cannot do this reliably, record why in the issue and keep only the guidance.
- [ ] The SessionStart doctor prints at most one collapsed line when remote branches older than 7 days carry an issue id whose issue is not `in_progress`, for example `3 pushed branches >7d with idle issues; see closure report`. It reuses closure's tracker_mismatch logic and is cached so the tracker is not queried on every session.
- [ ] The swarm and land-work skills state: "One session owns a branch at a time; hand off explicitly (message + tracker note) before another session touches it."
- [ ] Tests cover the merge message (with and without an issue id), the doctor line (fixture with an old branch and an idle issue), and the guard refusal if implemented.
- [ ] Proof at close: test names plus passing output. "Merged" is not sufficient.

## Out of scope

- Rewriting existing history.
- Automatic branch deletion (`landing-deletes-remote-branches`).

## Dependencies

- Blocked by: none.
- Related: bento-rdtn.9, bento-rdtn.14 (closed), `landing-deletes-remote-branches`, `claim-branch-reconciliation` (bucket bento-guards-doctor-tracker).

Priority: P3 · Type: feature · Labels: audit, land-work, hygiene · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/21, agent-repo-17
