# Bundle: bento-landing

- **Bucket:** bento-landing
- **Repo:** bento (bd in /home/ketan/project/bento, prefix bento)
- **Parent epic:** Epic: Audit 2026-09-22 findings (bento)
- **Theme:** land.py and land-work: preview ownership, verifier logs, post-push workflow results, merge-state ownership, progress, remote branch cleanup, skill restructure, lead landing.
- **Status:** drafts only. Nothing is filed (D6). Revised 2026-09-23 after the Codex cross-check (`issues/crosscheck/bento-landing.codex.md`) and the same-runtime review; see `REVISION.md`.
- **Code re-verified against:** bento origin/main 0b8d488. `git diff b1bb787 0b8d488` touches only the check-unpushed hooks; catalog/skills/land-work, swarm, wire-land-verifier and closure are unchanged since 1c0c1e6, so line numbers hold. Shatter facts were checked in the audit worktree at 56c86168. Tracker state was checked with `bd show` on 2026-09-23 (bento-73de, bento-x4bm, bento-49pg, bento-eth, bento-qiw, bento-i76i, bento-2jo, bento-e583, bento-dyp7).

## Maintainer decisions (2026-09-23)

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix. Fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64), do not drop them. Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot-diff command and the unused Snapshot writer. spec-diff is the regression tool. Update SPEC, README and QUICKSTART. The `diff` name becomes free; str-81xiw decides whether to take it. Correct the shatter-agents `shatter diff --staged` docs.
- **D3 Concolic positioning:** measure first. P1 controlled default-vs-concolic benchmark; P1 fix concolic early termination; a follow-up decision issue, blocked by both, re-decides the positioning. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import and sync the tracker through a Dolt remote. The first step checks whether the stale-JSONL import has been clobbering newer DB state. AGENTS.md drops `bd sync`. str-qwua7.28 is superseded. bento beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT or hook-bypass guidance.
- **D5 Git identity:** the leaked [user] section is already removed. Add a .mailmap (test@example.com "Test"/"Test User" -> Ketan Gangatirkar <33678+ketang@users.noreply.github.com>), a git-state check (identity override / example.com / core.bare / hooksPath), and a fixture .git/config snapshot test.
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. No agent files anything.

How they apply here: D1 appears only in `land-work-post-push-workflow-health`'s out-of-scope list (the release legs are fixed in shatter, not dropped). D4 shapes `land-work-skill-restructure`'s Tracker Handoff criterion, which carries bento-49pg's single Option C rule (export untracked, never committed) and otherwise defers to beads-issue-flow and `beads-dolt-remote-guidance`. No draft in this bucket suggests hook bypass or a hook-timeout env var. D2, D3 and D5 do not touch this bucket.

## Dependency edges (by slug)

- `land-py-invocation-progress-log` blocks `land-work-skill-restructure` and `merge-push-observability`.
- `land-py-verifier-log-kept` blocks the reopen note `verifier-log-reopen-note` (ordering only: the comment cites its id). Its draft also carries a companion comment for bento-x4bm.
- `land-py-branch-flag` blocks the note `eth-swarm-lead-lands-from-teammate` (ordering only: the comment cites its id).
- External (manual, not dep edges): `land-work-skill-restructure` should land after bento-49pg; `land-py-verifier-log-kept` and bento-x4bm share the `<git-common-dir>/bento/landing/` location.

## Splits and conversions in this revision

- `landing-deletes-remote-branches` (07): converted from a new issue to a note on bento-73de (slug kept).
- `eth-swarm-lead-lands-from-teammate` (10): narrowed to swarm template text; the driver change split out as `land-py-branch-flag` (15).
- `merge-message-and-stale-branch-nudge` (14): narrowed to the merge message (slug kept); split out `stale-pushed-branch-doctor-nudge` (16) and `one-session-per-branch-guidance` (17); the bare-SHA merge guard was dropped (already covered by bento-rdtn.15).

## Contents

| NN | Slug | Kind | Target | P | Title |
|---|---|---|---|---|---|
| 01 | e583-preview-owner-lock | note-to-existing | bento-e583 | P1 | Note on bento-e583: confirmed mechanism (the leftover-preview refusal has no owner check); add an owner lock with a token handoff to the cleanup subprocess, --force-foreign, and a two-process test |
| 02 | land-py-verifier-log-kept | new | - | P1 | land.py reports verifier.log as output_path after deleting it with the preview; ordinary verifier failures carry no log tail |
| 03 | verifier-log-reopen-note | reopen-note | bento-rdtn.4 | P1 | Comment on closed bento-rdtn.4: on the land.py path the persisted verifier log is deleted before it is reported |
| 04 | land-work-post-push-workflow-health | new | - | P1 | land-work: report GitHub workflow conclusions for the landed SHA; doctor flags workflows that stay red on the primary branch |
| 05 | land-py-merge-abort-ownership | new | - | P2 | land.py aborts or hard-resets merge state in the shared primary checkout even when it did not start the merge |
| 06 | land-py-invocation-progress-log | new | - | P2 | land.py: document the canonical long-running invocation; write a progress log and a heartbeat |
| 07 | landing-deletes-remote-branches | note-to-existing | bento-73de | P2 | Note on bento-73de: shatter evidence; call the lease-protected delete helper from land.py; superseded same-issue branches are report-only; closure report of merged remote heads |
| 08 | verifier-contract-migration | new | - | P2 | Verifier contract drift: define execution evidence for non-task checks, warn when payloads carry none, flag outdated manifests, require gate output pass-through |
| 09 | land-work-skill-restructure | new | - | P2 | land-work SKILL.md: restructure around land.py, move the manual and batch flows to references, remove $(...) that contradicts its own Command Rule |
| 10 | eth-swarm-lead-lands-from-teammate | note-to-existing | bento-eth | P2 | Note on bento-eth: the swarm template should have the lead land from a lead-owned scratch worktree, not the teammate's (driver support tracked separately) |
| 11 | dyp7-admission-control | note-to-existing | bento-dyp7 | P2 | Note on bento-dyp7: route git-hook gates and land.py's verify step through the admission governor with reentrant lease handoff; print a 'machine busy' status line |
| 12 | rebase-before-land-configurable | new | - | P3 | land.py: make rebase-before-land (--require-up-to-date) a per-repo policy option; the preview already verifies the exact merge |
| 13 | merge-push-observability | new | - | P3 | land.py merge_push: split into timed sub-steps and capture pre-push hook output |
| 14 | merge-message-and-stale-branch-nudge | new | - | P3 | land.py: landing merge message should name the issue id as well as the branch |
| 15 | land-py-branch-flag | new | - | P2 | land.py --branch <name>: land a named branch from any worktree without checking it out or rebasing in the owner's worktree |
| 16 | stale-pushed-branch-doctor-nudge | new | - | P3 | SessionStart doctor: one collapsed, cached line for pushed branches older than 7 days whose issue is not in_progress |
| 17 | one-session-per-branch-guidance | new | - | P3 | land-work and swarm: state that one session owns a branch at a time and that handoff is explicit |

---

<!-- file: 01-e583-preview-owner-lock.md -->

---
slug: e583-preview-owner-lock
kind: note-to-existing
title: "Note on bento-e583: confirmed mechanism (the leftover-preview refusal has no owner check); add an owner lock with a token handoff to the cleanup subprocess, --force-foreign, and a two-process test"
priority: P1
type: note
labels: [audit, land-work, concurrency]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-e583
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-e583: confirmed mechanism, owner lock with token handoff, --force-foreign, two-process test

Target: **bento-e583** (open, P1, "land-work-preview-* worktree destroyed mid-verification by concurrent session"). Post as a comment. Do not file a new issue. Leave the priority at P1.

Comment text:

> **Audit 2026-09-22 (shatter), finding bento-01: mechanism confirmed**
>
> The stale-preview refusal added by bento-rdtn.3 has no owner check. Its "remove them first" hint tells a lander to delete another lander's live preview. Shatter sessions followed that hint and destroyed each other's in-flight landings.
>
> **Code (re-verified 2026-09-23 at bento origin/main 0b8d488; land-work scripts unchanged since 1c0c1e6), `catalog/skills/land-work/scripts/`:**
> - `land-work-create-preview.py:60-79`: `leftover_preview_worktrees()` returns every registered worktree whose name starts with `land-work-preview-`. It checks no pid, lock, session or age.
> - `land-work-create-preview.py:252-275`: when any leftovers exist, it fails with `leftover land-work-preview-* worktree(s) exist: ...; remove them first (land-work-create-preview.py --cleanup --preview-dir <p>) or pass --allow-existing`.
> - `land-work-create-preview.py:163-173`: `cleanup_preview()` runs `git worktree remove --force` on any path it is given, with no owner check.
> - `land.py:291`: `create_preview` is called with only `--base-ref`. It never passes `--allow-existing`, so every concurrent landing in the same repo trips the refusal.
> - `land.py:129-142`: land.py's own cleanup runs `land-work-create-preview.py --cleanup --preview-dir <p>` as a **separate subprocess**. Any ownership rule therefore has to let that child act for its parent; a plain "pid/lock belongs to someone else" test would refuse land.py's own cleanup.
>
> **Transcript evidence (shatter, 2026-09-20/21):**
> - 23:49: session 87606e10 got `create_preview: failed` with "leftover ... /tmp/land-work-preview-mz7t9g27; remove them first". `git status` inside that preview showed a merge in progress, meaning it was live. The session ran `--cleanup` on it. The owning session 9f13ca23 then reported `verify: failed (238.362s)` and `cleanup: failed`.
> - 23:54-23:55: the same thing in the other direction (previews d1bfx2yj and g663hhnu, this time removed with `rm -rf`).
> - 00:00: a session hit `FileNotFoundError` on its own preview 2vu7job9.
>
> **Proposed additions to acceptance criteria:**
> 1. **Owner record and lock.** create-preview writes `.land-work/owner.json` into each scratch preview: a random owner token, pid, hostname, session id when known, start time, branch. It prints the token in its JSON payload. The driver that created the preview (land.py, or an agent following the manual flow) holds an `fcntl.flock` on `.land-work/owner.lock` for as long as it uses the preview.
> 2. **Authenticated handoff to the cleanup subprocess.** `--cleanup` accepts `--owner-token <t>` (or reads it from `BENTO_LAND_OWNER_TOKEN`, which land.py sets for its children). A cleanup whose token matches `owner.json` is the owner acting through a child process and proceeds even though the parent holds the lock. The manual flow in SKILL.md passes the token printed by create-preview.
> 3. **Foreign cleanup refused.** `--cleanup` without a matching token refuses while the lock is held or the recorded pid is alive on this host, unless `--force-foreign` is passed. The refusal names the owner (pid, branch, start time).
> 4. **Leftover detection.** `leftover_preview_worktrees()` skips any preview whose lock is held or whose recorded pid is alive on this host. Only previews that are unlocked, have a dead (or absent) owner pid and are older than a grace period count as leftovers. When only live previews exist, create-preview proceeds; if a repo-wide landing lease is wanted instead, it fails with "another landing is in progress (pid N, branch B); wait". Neither message tells the lander to remove anything.
> 5. **Tests.**
>    - Two real processes: process A (land.py, paused through a `BENTO_LAND_TEST_DELAY_*` style seam while it holds its preview) and process B running create-preview and then `--cleanup` against A's preview without the token. A's preview still exists, B's cleanup exits non-zero naming A, and A finishes with `ok: true`.
>    - Owner cleanup: an unmodified land.py run (success and injected verify failure) still records `cleanup: passed` and leaves no registered preview, i.e. the token handoff lets land.py's own cleanup subprocess through while the lock is held.
>    - `--force-foreign` removes a live foreign preview.
>    - The two-process test is committed failing against the current code first, then passing.
>
> **Proof at close:** the three test names, and the two-process test's failing-then-passing run (`python3 -m unittest tests.land_work.test_land_driver tests.land_work.test_land_work_scripts`), pasted into the close reason. "Merged" is not sufficient.
>
> Related: bento-rdtn.3 (closed; introduced the refusal), bento-7n7 and bento-gd2 (closed; preview leaks), `stale-previews-leak-and-scoping` (same epic; leak and scoping side), `dyp7-admission-control` (host load).

---

<!-- file: 02-land-py-verifier-log-kept.md -->

---
slug: land-py-verifier-log-kept
kind: new
title: "land.py reports verifier.log as output_path after deleting it with the preview; ordinary verifier failures carry no log tail"
priority: P1
type: bug
labels: [audit, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land.py reports verifier.log as output_path after deleting it with the preview; ordinary verifier failures carry no log tail

## Problem

When the verify step fails under `land.py`, the final JSON's `output_path` points to `<preview>/.land-work/verifier.log`. By the time that JSON is printed, land.py has already removed the preview worktree, so the file does not exist. The agent cannot see why the verifier failed. In shatter the agent had to rebuild a preview by hand and rerun the gates, which took several more minutes each time.

A second gap makes this worse: run-verifier attaches `verifier_log_tail` only when the verifier was `killed` or timed out. An ordinary failure (the verifier ran and reported a failing check) carries no tail at all, so even the JSON has nothing to show.

bento-rdtn.4 (closed) was meant to persist raw verifier output. bento-rdtn.14 (closed) added land.py, whose failure path cleans up the preview. Each was tested on its own, and no test covers the two together. On the land.py path, rdtn.4's goal is not met.

**Relation to bento-x4bm (open P2).** bento-x4bm ("land.py: validate the closure note before merging, record the landing durably ...") also passes `--log <git-common-dir>/bento/landing/<issue-id>/verifier.log`, but only when land.py is run with `--issue`/`--closure-note`, and it is blocked by four other issues (bento-wzbt, bento-sy49, bento-79j2, bento-bo9c). Without those flags, behaviour is unchanged under x4bm. This issue makes the log survive on **every** land.py run, now, and uses the same `<git-common-dir>/bento/landing/` root so x4bm can adopt the location instead of introducing a second one. A companion comment on bento-x4bm (below) records the agreed location.

## Evidence

Re-verified 2026-09-23 at bento origin/main 0b8d488 (land-work scripts unchanged since 1c0c1e6). Paths are relative to `catalog/skills/land-work/scripts/`.

- `land-work-run-verifier.py:351`: `log_path = Path(args.log).resolve() if args.log else candidate / ".land-work" / "verifier.log"`. The default path is inside the candidate, which is the preview.
- `land-work-run-verifier.py:136-138` (`_fail()`): `verifier_log_tail` is set only when `status in ("killed", "timeout")`. A verifier that exits and reports `failed` gets no tail.
- `land.py:296-305`: builds `verifier_args` without `--log`.
- `land.py:126`: `raise StepFailure(step, message, output_path=payload.get("verifier_log"))`. Any `verifier_log_tail` in the payload is dropped.
- `land.py:366-375`: the `except StepFailure` handler calls `driver.cleanup_preview()` (which runs `--cleanup`, i.e. `git worktree remove --force`, on the preview) and then emits `"output_path": exc.output_path`.
- Shatter session 9f13ca23 had three verify failures: 2026-09-19 15:38, 2026-09-20 23:49 and 2026-09-20 23:55. Each final JSON reported `output_path=/tmp/land-work-preview-XXXX/.land-work/verifier.log` after `cleanup: passed`, and the file no longer existed. Previews involved include g663hhnu, jus6t3ug and mz7t9g27.

## Acceptance criteria

- [ ] land.py always passes `--log <landing-dir>/verifier.log` to run-verifier, where `<landing-dir>` is `<git rev-parse --git-common-dir>/bento/landing/<key>/` and `<key>` is the issue id when land.py knows it (x4bm's `--issue`), otherwise `<sanitized-branch>-<UTC timestamp>`. The directory survives preview removal and is never tracked.
- [ ] After a failed verify step, `output_path` in land.py's final JSON names that file, and the file exists after land.py exits.
- [ ] run-verifier's `_fail()` attaches `verifier_log_tail` (last 20 lines) for every failure in which the verifier command ran, including an ordinary `failed` result, not only `killed`/`timeout`. land.py copies it into the failure JSON as `verifier_log_tail`.
- [ ] Successful runs keep their log at the same location and the success JSON includes `verifier_log`. Retention is bounded (for example the newest 20 non-issue landing dirs per repo are kept) and the bound is documented in SKILL.md.
- [ ] Regression test in `tests/land_work/test_land_driver.py`: a verifier stub that prints a marker line to stderr and then emits **valid verifier JSON with a failing check** and exits normally (so the run is classified `failed`, not `killed` or `timeout`; the test asserts `verifier_status == "failed"` in the payload). The test asserts `os.path.exists(result["output_path"])`, that the file contains the marker, that `result["verifier_log_tail"]` contains the marker, and that no registered preview remains. It is committed failing against the current code first, then passing.
- [ ] Unit test in `tests/land_work/test_land_work_verifier.py`: an ordinary failing verifier produces `verifier_log_tail`.
- [ ] Proof at close: the test names and the failing-then-passing `python3 -m unittest tests.land_work.test_land_driver tests.land_work.test_land_work_verifier` output in the close reason. "Merged" is not sufficient.

## Suggested approach

- Compute `<landing-dir>` once in `Driver.run()` after `prepare` (the git common dir is stable across worktrees).
- Add an optional `tail` field to `StepFailure` and fill it from `payload.get("verifier_log_tail")`.
- In `_fail()`, change the tail condition to "the verifier command ran and `log_text` is not None".
- Keep cleanup-before-emit unchanged. The log now lives outside the preview, so the order no longer matters.

## Out of scope

- Shatter's own verifier discarding gate output with `>/dev/null 2>&1`. That is shatter str-qwua7.55. The bento-side contract is `verifier-contract-migration`.
- Preview ownership and locking (bento-e583).
- The closure-note record and tracker close (bento-x4bm).

## Dependencies

- Blocked by: none.
- Related: bento-x4bm (open; adopts this location for its durable record), bento-rdtn.4 and bento-rdtn.14 (closed; see `verifier-log-reopen-note`), `land-py-invocation-progress-log` (writes its progress log into the same landing dir), `verifier-contract-migration`.

Priority: P1 · Type: bug · Labels: audit, land-work · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/02, bento-02

## Comment for bento-x4bm

> Audit 2026-09-22 (shatter), cross-reference: `<id of land-py-verifier-log-kept>` makes land.py pass `--log <git-common-dir>/bento/landing/<key>/verifier.log` on every run (key = issue id when known, else branch plus timestamp), so the verifier log survives preview cleanup even without `--issue`. When this issue adds the durable landing record, please reuse that directory and file name rather than a second location; the "verifier.log still exists after the preview is removed" check here then holds for both paths.

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
blocked_by: [land-py-verifier-log-kept]
existing_id: bento-rdtn.4
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Comment on closed bento-rdtn.4

Target: **bento-rdtn.4** (closed, "land-work-run-verifier: persist raw verifier output and report 'killed' distinctly from 'failed'"). Post as a comment only. Do not reopen it; the fix is tracked in the new issue `land-py-verifier-log-kept`, and the durable-record variant in open bento-x4bm.

Comment text:

> Audit 2026-09-22 (shatter) follow-up, finding bento-02. This issue's goal is not met when landing goes through `land.py` (bento-rdtn.14). run-verifier persists the raw log by default at `<candidate>/.land-work/verifier.log` (`land-work-run-verifier.py:351`), which is inside the preview worktree. land.py does not pass `--log` (`land.py:296-305`). On failure it raises `StepFailure(..., output_path=payload.get("verifier_log"))` (`land.py:126`), then calls `cleanup_preview()`, which runs `git worktree remove --force`, before emitting `output_path` (`land.py:365-375`). The reported log therefore never exists. land.py also drops any `verifier_log_tail`, and run-verifier sets that tail only for `killed`/`timeout` runs (`land-work-run-verifier.py:136-138`), so an ordinary `failed` verifier leaves the agent with neither the log nor a tail.
>
> Observed in shatter session 9f13ca23 on all three verify failures (2026-09-19 15:38, 2026-09-20 23:49, 2026-09-20 23:55): `output_path=/tmp/land-work-preview-XXXX/.land-work/verifier.log` was reported after `cleanup: passed`, and the file was gone. The agent rebuilt a preview by hand to find the failure.
>
> Tracked in `<id of land-py-verifier-log-kept>`, which makes land.py always pass `--log <git-common-dir>/bento/landing/<key>/verifier.log`, emits the tail for ordinary failures too, and requires a failing-then-passing driver test (with a verifier stub that reports a normal `failed` result) that checks the file exists. bento-x4bm (open) uses the same location for its durable landing record when `--issue` is given. For future closes: the test for a persistence fix should run through the real driver (land.py), not only through the component script.

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

Bento code, at origin/main 0b8d488 (land-work scripts unchanged since the audit's 1c0c1e6):
- `catalog/skills/land-work/scripts/land.py:273-341`: `run()` ends after `verify_landing`. There is no `gh` call anywhere in land-work (`grep -rn "gh run" catalog/skills/land-work` finds nothing).
- `land-work-verify-landing.py` checks that the expected tree landed on the ref, not CI.
- `catalog/hooks/bento/{claude,codex}/scripts/agent-env-doctor.py` has no workflow-health check (no `gh` invocation).
- bento-1qry (in_progress) binds local gate evidence and adds a stop when the primary branch is red locally. It does not consult GitHub workflow conclusions.

## Acceptance criteria

Post-push reporting (land.py):

- [ ] After `verify_landing` succeeds, land.py polls `gh run list --commit <merge_sha> --json databaseId,name,workflowName,event,status,conclusion,url --limit 100`. Polling has two phases with separate budgets from verifier.json `post_push_workflows: {enabled, discovery_s, timeout_s}` (defaults: enabled, `discovery_s` about 90, `timeout_s` about 600; `enabled: false` disables it):
  - **Discovery.** GitHub creates runs asynchronously, so an empty first response does not mean "no workflows". land.py keeps polling until at least one run appears or `discovery_s` expires. If none ever appears it reports `workflows_status: no_runs` (distinct from `complete`).
  - **Settling.** Once runs exist, land.py keeps polling until every run it has seen is `completed` and no new run has appeared for one further poll interval, or `timeout_s` expires (`workflows_status: timed_out`, with the incomplete runs listed).
  - **Pagination.** If a response returns exactly `--limit` rows, land.py reports `workflows_truncated: true` rather than silently treating the first page as the full set.
- [ ] land.py prints one line per workflow run and adds `workflows: [{name, event, conclusion, url}]` and `workflows_status: complete|no_runs|timed_out|skipped` to the final JSON. A `failure` conclusion is a warning: the landing is not rolled back and the exit code is unchanged.
- [ ] The step is skipped cleanly, with `workflows_status: skipped` and `workflows_skipped_reason`, when `gh` is missing or unauthenticated or the remote is not GitHub.

Doctor (SessionStart):

- [ ] The doctor fetches the primary branch's recent runs in **one** call, `gh run list --branch <primary> --status completed --limit 200 --json workflowName,conclusion,createdAt,url`, groups them by workflow client-side, and lists each workflow (scheduled ones included) whose 3 most recent completed runs all have conclusion `failure`. In-progress and queued runs are excluded by `--status completed`; `cancelled` and `skipped` runs are ignored when counting, so they neither break nor extend a streak. Output is one collapsed line per red workflow with the latest failed run URL.
- [ ] The result is cached per repo for one hour, so SessionStart makes at most one `gh` call per repo per hour. A workflow with fewer than 3 completed runs in the window is not flagged.

Tests and proof:

- [ ] Tests use a stubbed `gh` on PATH (a script that replays a sequence of responses) and cover: all green; one failure (warning, exit 0); **initially empty then runs appear** (reported `complete`, not `no_runs`); never any runs (`no_runs` after `discovery_s`); a run that appears after the first ones completed (included); timeout; truncated page; `gh` missing; non-GitHub remote; the doctor's streak rule with interleaved `cancelled` runs; and the doctor cache (a second invocation within the hour makes no `gh` call).
- [ ] Proof at close: the test names and passing output, plus one real land.py run against a GitHub repo whose final JSON contains a populated `workflows` array and `workflows_status: complete` (paste it into the close reason). "Merged" is not sufficient.

## Suggested approach

- Reuse the landed SHA that land.py already has (`merge_sha`). Run the poll after `verify_landing`, so a slow CI never delays the landing verdict itself.
- Use a fixed poll interval (for example 15 s) and keep a set of seen `databaseId`s so newly appearing runs are detected.
- Keep the doctor output collapsed. One line per red workflow is the ceiling.

## Out of scope

- Fixing shatter's workflows: drift-patrol `go-version-file` (`drift-patrol-workflow-go-mod`), the release.yml Windows Z3 and aarch64 openssl legs (`release-windows-z3-build` and `release-aarch64-openssl-cross`, which per maintainer decision D1 are fixed, not dropped), perf-ci, devcontainer and docker. Those are shatter issues.
- Making CI a hard landing gate.
- Filing tracker issues automatically for red workflows. That belongs to the shatter-side `workflow-health-patrol`.

## Dependencies

- Blocked by: none.
- Related: bento-2jo (open; the pre-push side: warn when pushing main triggers publishing workflows), bento-1qry (in_progress; local gate evidence), bento-rdtn.14 (closed; land.py), shatter `workflow-health-patrol` (repo-side counterpart), `merge-push-observability`.

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

On any failure or signal, land.py runs `git merge --abort` in the primary checkout whenever `.git/MERGE_HEAD` exists. This happens even for failures at `prepare`, `create_preview` or `verify`, before land.py has touched the primary. Separately, the tree-mismatch recovery path runs `git reset --hard HEAD@{1}`, which resets to the wrong commit if another session moved HEAD between land.py's merge and the reset. In repos where several sessions share one primary checkout, as in shatter, either action can destroy another session's in-progress merge or commits.

This was found by reading the code. No collision has been observed yet. It is the same class of failure as bento-e583 (acting on shared state without an ownership check).

## Evidence

Re-verified 2026-09-23 at bento origin/main 0b8d488 (land-work unchanged since 1c0c1e6). File: `catalog/skills/land-work/scripts/land.py`.

- `:144-148`: `abort_primary_merge_if_in_progress()` runs `git merge --abort` whenever `self.primary_root/.git/MERGE_HEAD` exists. It does not check who created it.
- It is called from the signal handler (`:348-350`), the `except StepFailure` handler (`:366-368`) and the `except BaseException` handler (`:376-378`). `self.primary_root` is set right after `prepare` (`:278`), so a failure in `create_preview` or `verify` still reaches the abort.
- `land-work-prepare.py:117-121` refuses a dirty primary checkout, so a merge that was **already** in progress before land.py started normally stops the run at `prepare`, before `primary_root` is set. The realistic hazard is a merge that another session starts in the primary checkout **after** land.py's prepare, while land.py is in `create_preview`, `verify` or `lease_check` (several minutes in shatter).
- `:196-199`: after land.py's own failed `git merge`, the abort is correct.
- `:200-204`: on tree mismatch, `subprocess.run(["git", "reset", "--hard", "HEAD@{1}"], cwd=primary_root, ...)`. `HEAD@{1}` is the previous reflog entry. After land.py's own fast-forward (`:177-185`) and merge, `HEAD@{1}` is exactly the pre-merge SHA, so the fast-forward alone is not a defect. It is wrong only when another session moves HEAD in the primary checkout between the merge and the reset.
- `:187-192`: the existing test seam `BENTO_LAND_TEST_DELAY_MERGE` sleeps **before** `git merge` runs, so the existing SIGINT test never has a merge in progress when the signal arrives. It cannot show that a real in-progress merge is aborted.

## Acceptance criteria

Ownership rule. A flag set in land.py's own process ("I attempted a merge") is not proof of ownership: another session could start a merge after land.py's prepare, and land.py would then abort it. Ownership must be established under serialization:

- [ ] land.py serializes its primary-checkout mutation with other bento landers: it takes an exclusive `fcntl.flock` on `<git-common-dir>/bento/primary-checkout.lock` immediately before the fast-forward/merge in `_merge_in_primary` and holds it until the push has succeeded or the merge has been undone. (Other land-work helpers that mutate the primary checkout take the same lock; bento-e583's preview lock is separate.)
- [ ] Under that lock, land.py checks that `MERGE_HEAD` does not exist and records `pre_merge_sha = HEAD`. If `MERGE_HEAD` already exists it stops with "another merge is in progress in the primary checkout" and touches nothing.
- [ ] `abort_primary_merge_if_in_progress()` aborts only when land.py holds the lock, recorded that it started the merge, **and** `MERGE_HEAD` contains the SHA land.py merged (the feature head). Otherwise it leaves the merge state alone and reports it in the failure JSON (`primary_merge_left_untouched: true`).
- [ ] The tree-mismatch path resets to the recorded `pre_merge_sha`, never to `HEAD@{1}`, and only while holding the lock and after confirming that HEAD is still land.py's own merge commit (first parent `pre_merge_sha`, second parent the feature head). If that check fails it refuses to reset and reports why. The remaining window (a non-bento process mutating the primary checkout without taking the lock) is documented in the code comment as out of scope.

Tests (`tests/land_work/test_land_driver.py`):

- [ ] **Foreign merge after prepare.** Using a seam that runs after `prepare` (for example a new `BENTO_LAND_TEST_AFTER_PREPARE` hook command, or the verifier stub itself), another process starts `git merge --no-commit` of an unrelated branch in the primary checkout; land.py's verify then fails. After land.py exits, the foreign `MERGE_HEAD`, index and working tree are unchanged. This test is committed failing against the current code first (today land.py aborts the foreign merge), then passing.
- [ ] **Signal during land.py's own merge.** A seam that pauses **while** land.py's merge is in progress (for example a `pre-merge-commit` hook in the fixture repo that sleeps, or a new seam between `git merge --no-commit` and the commit). The test first asserts that `MERGE_HEAD` exists at the pause point, then sends SIGINT, then asserts the merge was aborted and HEAD equals `pre_merge_sha`. The old `BENTO_LAND_TEST_DELAY_MERGE` test stays but is not counted as proof of abort.
- [ ] **Concurrent HEAD move before reset.** Force a tree mismatch and, through a seam between the merge and the reset, add a commit on top of land.py's merge in the primary checkout from another process. land.py refuses to reset and reports it; the extra commit survives. (A plain fast-forward-then-mismatch case is not a regression test: current code already restores the right SHA there.)
- [ ] **Existing merge refused.** With the lock free but a foreign `MERGE_HEAD` present at merge time, land.py stops with the "another merge is in progress" error and leaves it untouched.
- [ ] Proof at close: the four test names, with the foreign-merge and concurrent-HEAD tests shown failing against the current code and then passing (`python3 -m unittest tests.land_work.test_land_driver`), in the close reason. "Merged" is not sufficient.

## Suggested approach

Store `merge_started`, `merged_head` and `pre_merge_sha` on `Driver` and hold the lock file descriptor there as well. Release the lock in the same `finally` path that runs cleanup.

## Out of scope

- Locking between sessions beyond the primary-checkout merge window: preview ownership is bento-e583, host admission is bento-dyp7.
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

A land.py run takes 10-25 minutes in shatter: create_preview about 300 s, verify 266-578 s, merge_push 259-1108 s. The skill shows only `land-work/scripts/land.py --runtime <runtime>`. Agents run it however they like, and the Bash tool's 2-minute foreground limit forces them to improvise. In shatter session 9f13ca23 it was run 20 times (tool invocations, counted 2026-09-23) as `land.py --runtime claude 2>&1 | tail -100`. Four of those runs failed (`prepare: failed`, `create_preview: failed`, `verify: failed`) but came back `[exited with code 0]`, because the pipe returned tail's exit status. land.py itself exits 1 on failure. While it runs, land.py writes nothing to disk and prints nothing between step lines, so an agent cannot tell a slow step from a hung one.

## Evidence

Re-verified 2026-09-23 at bento origin/main 0b8d488 (land-work unchanged since 1c0c1e6).

- `catalog/skills/land-work/SKILL.md:258-263`: the only invocation shown is `land-work/scripts/land.py --runtime <runtime>`. There is no guidance on backgrounding, output capture or exit codes.
- `catalog/skills/land-work/scripts/land.py:93-103`: `_record()` prints one line per step to stderr after the step finishes. There is no progress file and no heartbeat.
- `land.py:383`: `return 0 if result["ok"] else 1`, so the exit code is 1 on failure. Piping through `tail` hides it.
- Transcript `9f13ca23-bf49-4efb-abd0-ed3519e0dd38.jsonl` contains 20 Bash tool invocations of `land.py --runtime claude 2>&1 | tail -100` (40 raw string occurrences, because each command is echoed in its tool result). Counted 2026-09-23 by parsing assistant `tool_use` inputs.
- Shatter memory `feedback_never_pipe_git_commit_through_tail.md` records the same lesson, which was never pushed upstream into bento.

## Acceptance criteria

- [ ] SKILL.md has a short "Running land.py" block that says to:
  - run it in the background (the runtime's background mechanism, for example `run_in_background`, or `nohup`), with stdout (the final JSON) going to a file and stderr going to a log;
  - read the exit status directly and never pipe land.py through `tail`/`head`;
  - wait for the completion notification instead of polling in a loop;
  - read the final JSON's `failed_step`, `error` and `output_path`.
- [ ] land.py creates a progress log at `<landing-dir>/progress.log`, where `<landing-dir>` is the per-landing directory under `<git-common-dir>/bento/landing/` defined by `land-py-verifier-log-kept` (whichever of the two issues lands first creates the helper that computes it). It prints `progress log: <path>` as the first stderr line, and every step line also goes to the log.
- [ ] During any step, land.py writes a heartbeat line (`… <step> still running (<elapsed>s)`) to stderr and to the log every 60 s. The interval can be overridden by an environment variable (for example `BENTO_LAND_HEARTBEAT_S`) for tests.
- [ ] The final JSON includes `progress_log`.
- [ ] Tests assert: the first stderr line names an existing file that is still present after land.py exits; with a 1 s heartbeat and a verifier stub that sleeps 3 s, at least two heartbeat lines appear in both stderr and the log; the exit code is 1 on an injected failure; a SKILL.md lint check finds the "Running land.py" block and no `land.py ... | tail` or `| head` example anywhere in land-work's SKILL.md or references.
- [ ] Proof at close: test names plus passing output. "Merged" is not sufficient.

## Suggested approach

- Run each child script with `subprocess.Popen` and a timer thread, or a `wait(timeout=interval)` loop, that emits the heartbeat. Keep capturing child stdout for the JSON payload as now.
- Share the landing directory with `land-py-verifier-log-kept`, so one landing's verifier log and progress log sit side by side.

## Out of scope

- Splitting merge_push into timed sub-steps (`merge-push-observability`).
- The broader SKILL.md rewrite (`land-work-skill-restructure`, which reuses this block).

## Dependencies

- Blocked by: none.
- Blocks: `land-work-skill-restructure`, `merge-push-observability`.
- Related: `land-py-verifier-log-kept` (shares the landing dir), bento-rdtn.14 (closed; per-step lines), shatter str-qwua7.26 (open; 2-minute foreground rule in shatter guidance).

Priority: P2 · Type: feature · Labels: audit, land-work, skills · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/07, bento-06

---

<!-- file: 07-landing-deletes-remote-branches.md -->

---
slug: landing-deletes-remote-branches
kind: note-to-existing
title: "Note on bento-73de: shatter evidence; call the lease-protected delete helper from land.py; superseded same-issue branches are report-only; closure report of merged remote heads"
priority: P2
type: note
labels: [audit, land-work, cleanup]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-73de
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-73de: shatter evidence and remaining deltas

Target: **bento-73de** (open, P2, "land-work cleanup: delete the landed feature branch on the remote, not only locally"). Post as a comment. Do not file a new issue.

This draft was originally a new issue. bento-73de (filed 2026-09-23) already specifies the core fix, and specifies it more safely than the draft did: a helper `land-work-delete-remote-branch.py` that fetches the exact `refs/heads/<branch>` into a private ref, checks ancestry of that exact SHA, and deletes with `--force-with-lease=refs/heads/<branch>:<remote_sha>`, so a push that lands after the check is never deleted (its `test_delete_remote_branch_lease_rejected`). The draft's `git ls-remote --heads` confirmation and unconditional ancestry-then-delete are dropped in favour of 73de's design. Only the deltas below are posted.

Comment text:

> **Addendum from the shatter audit 2026-09-22 (findings bento/11, bento-15, prior-06)**
>
> **Evidence from shatter** (audit worktree 56c86168, 2026-09-23):
> - `git branch -r | wc -l` gives 66, and `git branch -r --merged origin/main | wc -l` gives 38 (for example `origin/str-hjrnp.1` through `.4`, `origin/str-2tyfk-lint-errcheck`).
> - `git ls-remote origin 'refs/heads/str-qwua7*'` still lists `str-qwua7.4-testplan-http-body-fix`, `str-qwua7.7-protocol-registry-validate`, `str-qwua7.16-restore-bd-dolt` and `str-qwua7.17-stale-claims-cleanup`: unmerged, each about 102 commits ahead of main, each carrying a stray fixture commit (e50fc399 "init"). The close reason of str-qwua7.4 says its duplicate branches were deleted; one is still there. Shatter's AGENTS.md calls remote deletion "mandatory" and keeps a hand-run `scripts/cleanup-merged-remote-branches.sh` to compensate.
>
> **Proposed deltas to this issue's scope** (each optional; the maintainer decides which to take here and which to split):
> 1. **land.py calls the helper.** 73de says "if land.py later gains teardown, it calls the same helper". Since land.py is the default serial path, add a `delete_remote` step after `verify_landing` that invokes `land-work-delete-remote-branch.py --branch <feature>` and copies its JSON into the final result as `remote_branch: {deleted, reason, remote_sha}`. A non-`deleted` outcome is a warning, not a failed landing. Test: a land.py run against the bare-origin fixture ends with `remote_branch.reason == "deleted"`, and with the opt-out key set ends with `"opted-out"`.
> 2. **Superseded same-issue branches are reported, never auto-deleted.** A matching issue id proves neither that a branch's commits are safe to discard nor that no other session is using it (the shatter str-qwua7.* branches above have about 102 unique commits each). So land.py only lists other remote branches that carry the landed branch's issue id, with `git cherry` unique-commit counts and whether any local worktree has them checked out, under `superseded_candidates` in the final JSON. Deletion stays an explicit operator action through the same helper, which already refuses anything that is not an ancestor of the primary branch.
> 3. **Closure report of merged remote heads.** 73de lists "closure remote reporting" as a possible follow-up. Shatter shows it is needed for the backlog that already exists: a report-only closure section listing remote heads that are ancestors of the primary branch, each with the helper command to delete it. No automatic deletion.
>
> Related: bento-rdtn.14 (closed; land.py), bento-rdtn.9 (closed; closure tracker_mismatch), `close-reason-evidence` (audit bucket bento-guards-doctor-tracker), shatter str-qwua7.19.

---

<!-- file: 08-verifier-contract-migration.md -->

---
slug: verifier-contract-migration
kind: new
title: "Verifier contract drift: define execution evidence for non-task checks, warn when payloads carry none, flag outdated manifests, require gate output pass-through"
priority: P2
type: feature
labels: [audit, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Verifier contract drift: define execution evidence for non-task checks, warn when payloads carry none, flag outdated manifests, require gate output pass-through

## Problem

bento-rdtn.6 added a per-check `executed` flag and a guard that fails when every check was served from cache. By the maintainer's decision on that issue, payloads without the field are accepted silently. So verifiers wired before the contract changed get no protection and no signal that they are out of date. Existing consumer repos are never prompted to regenerate their wrappers.

Shatter's hand-written verifier shows the consequence. It emits only `{name, status}` and runs every gate with `>/dev/null 2>&1`, so even the rdtn.4 persisted log is empty. Shatter's `task` gates also report cached "is up to date" tasks as passes. A cached no-op landing gate therefore looks exactly like a real run.

## Evidence

Re-verified 2026-09-23 at bento origin/main 0b8d488 (land-work and wire-land-verifier unchanged since 1c0c1e6) and the shatter audit worktree 56c86168:

- `catalog/skills/land-work/scripts/land-work-run-verifier.py:520-545`: the all-cached guard trips only when `all(c["executed"] is False ...)`. The comment says a payload with no `executed` field "stays fully accepted".
- `catalog/skills/land-work/scripts/land.py:116-121`: prints `[cached]`/`[executed]` only when the flags exist.
- `catalog/skills/wire-land-verifier/scripts/wire-land-verifier.py:893-917`: the current wrapper template emits `executed` **only for go-task commands** (it parses task's "is up to date" message). For every other command it sets `executed = None` and omits the field (`:908-910`, `:916-917`), deliberately, because it cannot tell whether that command served a cache. So a freshly regenerated wrapper for a make/npm/cargo repo also lacks `executed`, and "regenerate" alone is not a migration endpoint. Nothing detects or prompts repos whose wrappers predate the contract.
- `catalog/skills/wire-land-verifier/` has only `SKILL.md`, `metadata.json` and `scripts/`, with no reference doc. The output pass-through requirement belongs in SKILL.md.
- Shatter `scripts/land_work_verifier.sh` (last changed d01a22db, 2026-08-05): `run_check` runs `"$@" >/dev/null 2>&1` (line 17) and emits `{"name":...,"status":...}` only.
- None of the 11 land.py verify lines observed in shatter transcripts shows `[cached]` or `[executed]`.
- run-verifier's payload has no `warnings` key today, and land.py prints only step-level `warning` entries produced by its own merge step (`land.py:102-103`). Both sides of the warning path below are new.

## Acceptance criteria

Execution-evidence semantics. Each selected check reports one of three states: `executed: true` (the gate ran), `executed: false` (served from cache), or `cache_status: "unknown"` with no `executed` field (the wrapper ran the command but cannot tell whether the command's own tooling skipped work). A check with neither field is "no execution evidence" and is what the warning targets.

- [ ] The wire-land-verifier template emits `cache_status: "unknown"` for every non-go-task check (where it now omits `executed`). A test generates a wrapper for a fixture repo with a non-task gate (for example `make check`), runs it, and asserts that every check carries either `executed` or `cache_status`, i.e. a freshly generated wrapper produces **no** warning. This is the migration endpoint.
- [ ] run-verifier adds a new top-level `warnings` list to its payload. When a selected check has neither `executed` nor `cache_status`, it adds `checks without execution evidence: <names>`. land.py prints each run-verifier warning under the verify step line and includes them in the final JSON (`verify_warnings`).
- [ ] The all-cached guard (`land-work-run-verifier.py:520-545`) is unchanged: `cache_status: "unknown"` counts as neither cached nor executed, and a missing field still does not fail the step.
- [ ] verifier.json gains `contract_version`, and wire-land-verifier writes the current version. The SessionStart doctor flags a manifest with a missing or older `contract_version` with one line: "land-work verifier predates contract vN; regenerate with wire-land-verifier".
- [ ] wire-land-verifier's SKILL.md and generated wrapper template state that gate stdout and stderr must pass through to the verifier's output (captured into the verifier log), never to `/dev/null`. A test asserts that the generated wrapper contains no `/dev/null` redirection of a gate command.
- [ ] Tests cover: a hand-written payload with only `{name, status}` produces the warning and land.py shows it; payloads with `executed` or with `cache_status: "unknown"` produce no warning; the regenerated-wrapper endpoint test above; doctor output for missing, old and current `contract_version`.
- [ ] Proof at close: test names plus passing output in the close reason. "Merged" is not sufficient.

## Suggested approach

Keep the rdtn.6 decision: a missing field still does not fail the verify step. Add the warning and the doctor nudge as a deprecation path. A later issue can make execution evidence (`executed` or `cache_status`) mandatory once consumers have regenerated.

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
- Its Tracker Handoff states a `.beads/issues.jsonl` policy ("may be intentionally untracked ... do not re-add or commit it") while beads-issue-flow's session-completion text allows committing the export. bento-49pg (open P2, owner decision Option C, 2026-09-22) resolves this: the export is untracked, and land-work and beads-issue-flow each carry **one** matching rule. Shatter's AGENTS.md currently requires committing the file; maintainer decision D4 retires shatter's JSONL import in favour of a Dolt remote, which agrees with Option C.

In shatter, bento:land-work was invoked for only about 9 of 35 first-parent merges to main since 2026-09-05. Several other landings were hand-scripted, including 4 raw `git push --no-verify ... :refs/heads/main` by one session.

bento-by8 (open, P3) trims about 1,500 words of repeated text across several skills. This issue goes further for land-work: a structural rewrite that makes land.py the serial path.

## Evidence

Re-verified 2026-09-23 at bento origin/main 0b8d488 (land-work unchanged since 1c0c1e6):

- `wc -w catalog/skills/land-work/SKILL.md` gives 6,342 words (45,480 bytes). `swarm/SKILL.md` is 4,178 words, the next largest.
- Command Rule, `SKILL.md:101-106`: "... with `&&`, pipes, `$(...)`, or inline interpreters". Violations in commands the skill tells agents to run: `:143` (`--head-sha $(git rev-parse HEAD)`), `:181-182` (`BASE_SHA=$(git merge-base ...)`, `HEAD_SHA=$(git rev-parse HEAD)`), `:449` (`--merge-sha $(git rev-parse <primary-branch>)`). `:670` is deliberately escaped (`\$(...)`) and explained, so it is not a violation.
- The land.py invocation sits in step 7a (`:255-263`), behind the manual compare-and-set flow. The `## Batch Landing` section runs `:545-722` even though `references/batch-landing.md` already exists and SKILL.md already links it (`:85`, `:88`).
- Obligations of the current serial path that **land.py does not perform** (it runs prepare, fetch, create_preview, verify, lease_check, merge_push, cleanup and verify_landing only; `land.py:273-340`): `pre`/`post` lifecycle hook scripts and hook skills (steps 2a and 8a, `:129-162`, `:436-460`), gate-evidence discovery and the primary-branch baseline (step 2c, `:163-170`), the independent code review (step 4, `:171-235`), the tracker handoff and issue close, root hygiene (`land-work-root-hygiene.py`, `:80`), and feature-worktree/branch teardown (step 10, `:516-533`).
- Other skills also contain unescaped `$(` in command blocks (launch-work, closure, beads-issue-flow, github-issue-flow, issue-readiness-check). They have their own Command Rules or none; this issue does not touch them.
- Tracker Handoff, `:783-789`.

## Acceptance criteria

- [ ] The serial path in SKILL.md is an ordered checklist with land.py at its centre: (1) `pre` hook scripts and skills, (2) gate-evidence discovery and primary-branch baseline, (3) independent code review, (4) run land.py (using the "Running land.py" block from `land-py-invocation-progress-log`), read the final JSON and fix the step it names, (5) `post` hook scripts and skills, (6) tracker handoff and close, (7) root hygiene, (8) feature worktree and local branch teardown. Every obligation listed under Evidence as "not performed by land.py" appears in that checklist in SKILL.md itself (not only in a reference), each with its command or skill name. A review table in the close note maps each current step (1-10) to where it now lives.
- [ ] The manual compare-and-set flow and the manifest rules move to `catalog/skills/land-work/references/` (for example `references/manual-landing.md`), and the `## Batch Landing` section is folded into the existing `references/batch-landing.md`. SKILL.md links each by file name.
- [ ] `wc -w catalog/skills/land-work/SKILL.md` is at most 2,500.
- [ ] No unescaped `$(` appears in any fenced command block of land-work's SKILL.md or of `catalog/skills/land-work/references/*.md`. The computations move into scripts, or land.py derives the values itself.
- [ ] The Tracker Handoff carries exactly the single rule bento-49pg defines for land-work (the Beads JSONL export is untracked local state and is never committed during landing) and otherwise defers to `beads-issue-flow` or `github-issue-flow`. If bento-49pg has not landed yet, this issue leaves the Tracker Handoff wording to 49pg and only moves it.
- [ ] A lint test in the existing skill test suite, **scoped to `catalog/skills/land-work/`**, fails when SKILL.md or a `references/*.md` fenced `bash`/`sh` block contains an unescaped `$(`, when SKILL.md exceeds the word budget, or when any of the checklist items above is missing (by a fixed list of required step names). It is committed failing against the current SKILL.md, then passing.
- [ ] Proof at close: the `wc -w` output, the step-mapping table, and the lint test failing and then passing, in the close reason. "Merged" is not sufficient.

## Suggested approach

1. Land `land-py-invocation-progress-log` first so the invocation block exists.
2. Move sections out verbatim into references, then trim what remains in SKILL.md.
3. Add `land.py --print-shas` or similar only if the references still need computed SHAs. Otherwise let land.py compute them.
4. Coordinate with bento-by8 so the two do not trim the same paragraphs twice. Link the new issue as related to bento-by8.

## Out of scope

- Changes to land.py's behaviour, apart from any small helper needed to remove `$(...)`.
- `$(` in other skills' command blocks (launch-work, closure, beads-issue-flow, github-issue-flow, issue-readiness-check). A repo-wide lint would need its own issue with an inventory.
- The JSONL policy decision itself (bento-49pg).
- The beads-issue-flow content itself (`beads-dolt-remote-guidance`).

## Dependencies

- Blocked by: `land-py-invocation-progress-log`.
- Related: bento-49pg (open; owns the Tracker Handoff rule, land it first if possible), bento-by8 (open; this extends it), bento-7kw, `beads-dolt-remote-guidance` (bucket bento-guards-doctor-tracker).

Priority: P2 · Type: task · Labels: audit, land-work, skills · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/16, bento-14 · Decision: D4 (Tracker Handoff wording only; consistent with bento-49pg Option C)

---

<!-- file: 10-eth-swarm-lead-lands-from-teammate.md -->

---
slug: eth-swarm-lead-lands-from-teammate
kind: note-to-existing
title: "Note on bento-eth: the swarm template should have the lead land from a lead-owned scratch worktree, not the teammate's (driver support tracked separately)"
priority: P2
type: note
labels: [audit, swarm, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [land-py-branch-flag]
existing_id: bento-eth
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-eth: land from a lead-owned scratch worktree

Target: **bento-eth** (open, P3, "Bake recurring teammate-prompt lessons into the swarm template; make lead-lands the default ..."). Post as a comment. Do not file a new issue. bento-eth's scope explicitly excludes "land-work's own mechanics", so the driver change this needs (`land.py --branch`) is a separate new issue, `land-py-branch-flag`, and this comment covers only swarm template text. bento-eth is sequenced after bento-qiw (open), which rewrites the Phase 2/4 landing text; the maintainer can decide whether this text belongs in qiw's rewrite instead. The audit rates this addendum P2; the maintainer can decide whether to raise bento-eth.

Comment text:

> **Addendum from the shatter audit 2026-09-22 (finding bento-12)**
>
> The operator decision recorded on bento-qiw and bento-eth (2026-06-12) makes lead-lands the default. Neither issue says *where* the lead lands from. Today the swarm skill has the lead land from inside the teammate's worktree:
>
> - `catalog/skills/swarm/SKILL.md:337-342` (Serial-Mode Landing step 3, re-verified 2026-09-23 at bento origin/main 0b8d488): "Invoke `bento:land-work` from within the teammate's worktree". land.py derives the branch from its cwd and needs the branch up to date (`--require-up-to-date`, `land.py:274`), so the lead ends up running checkout and rebase inside the teammate's tree.
> - A shatter lesson from 2026-09-08 (memory `feedback_lead_landing_prep_separate_worktree`) records that this "silently moved the worktree's HEAD out from under" a teammate who was still making review fixes.
>
> **Proposed additions to this issue's template text** (swarm SKILL.md only):
> 1. Serial-Mode Landing step 3: the lead lands from a lead-owned scratch worktree using `land.py --branch <teammate-branch>` (added by `<id of land-py-branch-flag>`), or from the teammate's worktree only after confirming that the teammate session has exited.
> 2. The teammate prompt template says "the lead will not check out, rebase or reset in your worktree while you are live".
>
> Acceptance for these additions: `grep -n "from within the teammate's worktree" catalog/skills/swarm/SKILL.md` finds nothing; step 3 names `land.py --branch` and the exited-session exception; the teammate prompt template contains the sentence above. Proof at close: the SKILL.md diff. Related: bento-qiw, `<id of land-py-branch-flag>`, `rebase-before-land-configurable` (drops the rebase need when a repo opts out of it).

---

<!-- file: 11-dyp7-admission-control.md -->

---
slug: dyp7-admission-control
kind: note-to-existing
title: "Note on bento-dyp7: route git-hook gates and land.py's verify step through the admission governor with reentrant lease handoff; print a 'machine busy' status line"
priority: P2
type: note
labels: [audit, land-work, concurrency, performance]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-dyp7
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-dyp7: hooks and land.py through the governor

Target: **bento-dyp7** (open P1 epic, "Host-wide weighted admission governor for heavy agent jobs"). Post as a comment. Do not file a new issue. Preview ownership locking is a separate matter, covered by bento-e583 (`e583-preview-owner-lock`).

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
> 2a. **Reentrancy, to avoid a nested-acquisition deadlock.** If land.py holds a slot around `merge_push` and the repo's pre-push hook then calls `run-heavy` (item 1), the hook would wait for a slot its own parent is holding; with the governor saturated, neither can proceed. The governor must therefore support reservation handoff: a holder exports a lease token (for example `BENTO_HEAVY_LEASE=<id>`) to its children, and `run-heavy` invoked with a live token that the governor recognises runs inside the parent's reservation instead of acquiring a new slot. Tokens are only honoured while the issuing lease is live, so a leaked environment variable cannot bypass admission.
> 3. While waiting, land.py prints one line, `machine busy: load X/<cores>, mem Y% — waiting for slot (Ns)`, and repeats it through the heartbeat from `land-py-invocation-progress-log`. Agents then stop diagnosing load by hand.
> 4. The doctor mentions the hook entry point once when a repo's hooks run known-heavy commands without it.
>
> Acceptance proof for these additions: (a) a test in which a saturated governor makes land.py's verify wait (visible as the status line) and then proceed; (b) a nested test: governor at capacity except for the one slot land.py holds, land.py's merge_push runs a fixture pre-push hook that calls `run-heavy`, and the landing completes within a bounded time (no deadlock) with the hook's command running under land.py's lease; (c) a stale-token test: `run-heavy` with a token whose lease has ended acquires normally; (d) the documented hook example. The audit rates this addendum P2 within dyp7's P1 scope.

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

Re-verified 2026-09-23 at bento origin/main 0b8d488 (land-work unchanged since 1c0c1e6):

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

Re-verified 2026-09-23 at bento origin/main 0b8d488 (land-work unchanged since 1c0c1e6). File: `catalog/skills/land-work/scripts/land.py`.

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
title: "land.py: landing merge message should name the issue id as well as the branch"
priority: P3
type: feature
labels: [audit, land-work, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land.py: landing merge message should name the issue id as well as the branch

This draft originally bundled four deliverables (merge message, bare-SHA merge guard, stale-branch doctor nudge, one-session-per-branch rule). After the cross-check it covers only the merge message. The doctor nudge is `stale-pushed-branch-doctor-nudge` and the ownership rule is `one-session-per-branch-guidance`. The bare-SHA guard was dropped: see "Not included" below.

## Problem

land.py's merges are titled `Merge branch '<branch>'`. When the branch name carries an issue id this is readable, but the id is not a separate, parseable part of the subject, and branch names do not always start with it. History readers and tools (closure's tracker_mismatch, close-reason evidence) have to re-parse the branch name. A landing merge should state the issue it lands.

## Evidence

Re-verified 2026-09-23 at bento origin/main 0b8d488 (land-work and closure unchanged since 1c0c1e6):

- `catalog/skills/land-work/scripts/land.py:194` (merge-in-primary route) and `:221` (push-from-preview route): the message is `f"Merge branch '{feature_branch}'"` and does not include the issue id.
- `catalog/skills/closure/scripts/closure-scan.py:1436-1452`: closure already has an issue-id rule for branch names (`_BRANCH_ISSUE_ID_RE`, `resolve_branch_issue_id`), which this can reuse.
- In shatter, since land.py took over (2026-09-19), landing merges are uniform `Merge branch '<branch>'`, for example `16794cef Merge branch 'str-qwua7.4-landing2'`.

## Acceptance criteria

- [ ] land.py's merge message is `Merge branch '<branch>' (<issue-id>)` when the branch name yields an issue id under closure's rule, and `Merge branch '<branch>'` otherwise. Both routes (`:194`, `:221`) use one helper.
- [ ] The issue-id rule is shared (moved into a module both closure and land.py import, or imported from closure), not copied.
- [ ] Tests in `tests/land_work/test_land_driver.py` assert the subject for both routes, with and without an issue id in the branch name. They are committed failing first, then passing.
- [ ] Proof at close: test names plus the failing-then-passing output in the close reason. "Merged" is not sufficient.

## Not included (dropped after cross-check)

- **Refusing `git merge <bare-sha>` into the primary branch.** The six `Merge commit '<sha>' into HEAD` merges in shatter history (af6839f0, a19a8aec, 2aecd43a, c1364378, af3ae54a, 6c8bc87f) are all dated 2026-09-07/08. Since bento-rdtn.15 (closed; commit 5210e74, 2026-09-10), `require-worktree-git-guard.py` denies **every** `git merge` typed as a Bash command in the primary checkout, so this path is already closed, and land.py's own `git merge --ff-only <leased_sha>` (`land.py:179`) runs as a subprocess the guard never sees. A SHA-specific rule would add nothing; remaining guard gaps belong to bento-i76i and the audit's `git-guard-bypasses-and-false-positives`.

## Out of scope

- Rewriting existing history.
- Remote branch deletion (bento-73de; see `landing-deletes-remote-branches`).

## Dependencies

- Blocked by: none.
- Related: bento-rdtn.9 and bento-rdtn.14 (closed), `stale-pushed-branch-doctor-nudge`, `one-session-per-branch-guidance`, `close-reason-evidence` (bucket bento-guards-doctor-tracker).

Priority: P3 · Type: feature · Labels: audit, land-work, hygiene · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/21, agent-repo-17

---

<!-- file: 15-land-py-branch-flag.md -->

---
slug: land-py-branch-flag
kind: new
title: "land.py --branch <name>: land a named branch from any worktree without checking it out or rebasing in the owner's worktree"
priority: P2
type: feature
labels: [audit, land-work, swarm, concurrency]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land.py --branch <name>: land a named branch from any worktree without touching the owner's worktree

Split from the audit's bento-eth addendum (`eth-swarm-lead-lands-from-teammate`): bento-eth covers only swarm template text and excludes land-work mechanics, so the driver capability is its own issue.

## Problem

land.py can land only the branch checked out in its cwd. A swarm lead, who lands by default (operator decision on bento-qiw/bento-eth, 2026-06-12), therefore has to run land.py from inside the teammate's worktree, and when the branch is behind, rebase there. In shatter this moved a live teammate's HEAD out from under it (memory `feedback_lead_landing_prep_separate_worktree`, 2026-09-08). The lead needs to land a named branch from a worktree it owns, without changing anything in the teammate's worktree.

## Evidence

Re-verified 2026-09-23 at bento origin/main 0b8d488 (land-work unchanged since 1c0c1e6). Files under `catalog/skills/land-work/scripts/`.

- `land.py:74-78` (`parse_args`): only `--timeout` and `--runtime`.
- `land.py:274-277`: the branch comes from `land-work-prepare.py`'s view of the cwd (`prepare["branch"]`), and prepare runs with `--require-up-to-date`.
- `land-work-create-preview.py` already takes a `--feature-ref` argument (`:36`; `feature_ref = args.feature_ref or branch` at `:250`), so the preview can be built from a ref other than the cwd's branch.
- `land.py:193-195`: `_merge_in_primary` merges `feature_branch` by name in the primary checkout, which works for any local branch ref.
- `catalog/skills/swarm/SKILL.md:340-341`: "Invoke `bento:land-work` from within the teammate's worktree".

## Acceptance criteria

- [ ] `land.py --branch <name>` lands the named local branch from any worktree of the repo (including a lead-owned scratch worktree or a linked worktree on another branch). prepare, create-preview (via `--feature-ref`), verify and merge all use `<name>`; nothing is checked out, rebased or reset in the worktree that has `<name>` checked out.
- [ ] If `<name>` is behind the primary branch and `--require-up-to-date` applies, land.py fails at prepare with a message naming the branch and the owning worktree; it never rebases. (A repo that sets `require_up_to_date: false` via `rebase-before-land-configurable` lands through the preview merge instead.)
- [ ] Local cleanup after a `--branch` landing does not remove or modify any worktree land.py did not create; the final JSON reports `owner_worktree` and that it was left untouched.
- [ ] Without `--branch`, behaviour is unchanged.
- [ ] Tests in `tests/land_work/test_land_driver.py`: (a) with a "teammate" worktree on `feat` that has an uncommitted file and a staged change, `land.py --branch feat` run from a different worktree lands `feat`, and the teammate worktree's HEAD, index (`git diff --cached`) and working tree (`git status --porcelain`) are byte-identical before and after; (b) `--branch` on a behind branch fails at prepare and leaves the teammate worktree unchanged; (c) no-flag regression. Test (a) is committed failing (unknown flag) first, then passing.
- [ ] Proof at close: the test names and the failing-then-passing `python3 -m unittest tests.land_work.test_land_driver` output in the close reason. "Merged" is not sufficient.

## Out of scope

- Swarm SKILL.md text (bento-eth, with the audit's addendum).
- Preview ownership between concurrent landers (bento-e583).

## Dependencies

- Blocked by: none.
- Blocks: `eth-swarm-lead-lands-from-teammate` (the bento-eth comment cites this issue's id).
- Related: bento-qiw, bento-eth, `rebase-before-land-configurable`, `land-py-merge-abort-ownership`.

Priority: P2 · Type: feature · Labels: audit, land-work, swarm, concurrency · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento-12

---

<!-- file: 16-stale-pushed-branch-doctor-nudge.md -->

---
slug: stale-pushed-branch-doctor-nudge
kind: new
title: "SessionStart doctor: one collapsed, cached line for pushed branches older than 7 days whose issue is not in_progress"
priority: P3
type: feature
labels: [audit, closure, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# SessionStart doctor: one collapsed, cached line for aging pushed branches with idle issues

Split from the audit draft `merge-message-and-stale-branch-nudge`, which bundled this with unrelated deliverables.

## Problem

Branches sit pushed for weeks and then need re-landing: `-landing2` branches, or diffs "ported/rewritten against current main". bento-rdtn.9 (closed) added closure's `tracker_mismatch` report, which finds exactly these branches (branch carries an issue id whose issue is not in_progress), but it runs only when someone invokes closure. Nothing surfaces the signal at session start, when an agent could act on it.

## Evidence

Re-verified 2026-09-23 in the shatter audit worktree (56c86168) and bento origin/main 0b8d488:

- `16794cef Merge branch 'str-qwua7.4-landing2'` is a re-landing. Branches authored 2026-08-27 to 08-31 landed on 09-21/22.
- `catalog/skills/closure/scripts/closure-scan.py:1558` (`annotate_branches_with_tracker_mismatch`) and `:1539` (`suggest_tracker_mismatch_action`): the rdtn.9 logic, reachable only through closure.
- `catalog/hooks/bento/{claude,codex}/scripts/agent-env-doctor.py`: no branch-age or tracker-mismatch check.

## Acceptance criteria

- [ ] The SessionStart doctor (both runtimes) prints at most **one** line when remote branches whose last commit is older than 7 days carry an issue id whose issue is not `in_progress`, for example `3 pushed branches >7d with idle issues; run closure for details`. It reuses closure's tracker_mismatch logic by import, not by copy.
- [ ] The check is cached per repo (for example 6 hours) so SessionStart does not query the tracker on every session, and it has a hard time budget (for example 2 s) after which it is skipped silently. Branches whose issue lookup fails are not counted.
- [ ] Opt-out key in `.agent-mode.local` (added to `RECOGNIZED_AGENT_MODE_KEYS` in both doctors).
- [ ] Tests in `tests/test_agent_env_doctor.py` and `tests/test_agent_env_doctor_codex.py` with a fixture remote containing an old branch whose issue is closed (stubbed bd), a fresh branch, and an old branch whose issue is in_progress: exactly one line, counting 1. A second run within the cache window makes no tracker call. The opt-out suppresses the line.
- [ ] Proof at close: test names plus passing output in the close reason. "Merged" is not sufficient.

## Out of scope

- Deleting branches (bento-73de).
- The reverse direction, in_progress issues with no live branch (`claim-branch-reconciliation`, bucket bento-guards-doctor-tracker).

## Dependencies

- Blocked by: none.
- Related: bento-rdtn.9 (closed), `claim-branch-reconciliation`, `landing-deletes-remote-branches` (note on bento-73de).

Priority: P3 · Type: feature · Labels: audit, closure, hygiene · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/21, agent-repo-17

---

<!-- file: 17-one-session-per-branch-guidance.md -->

---
slug: one-session-per-branch-guidance
kind: new
title: "land-work and swarm: state that one session owns a branch at a time and that handoff is explicit"
priority: P3
type: task
labels: [audit, land-work, swarm, skills]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land-work and swarm: one session owns a branch at a time; handoff is explicit

Split from the audit draft `merge-message-and-stale-branch-nudge`, which bundled this with unrelated deliverables.

## Problem

Neither land-work nor swarm says which session owns a branch. In shatter two concurrent sessions worked the same branch and made opposite decisions on it, which ended in a revert (audit finding agent-repo-17). Swarm's lead-lands default (bento-qiw operator decision) makes the handoff between teammate and lead routine, so the ownership rule needs to be written down where agents read it.

## Evidence

Re-verified 2026-09-23 at bento origin/main 0b8d488:

- `grep -n -i "owns\|one session" catalog/skills/land-work/SKILL.md catalog/skills/swarm/SKILL.md` finds only ownership of landing and cleanup (land-work `:775` "The landing agent owns direct post-merge cleanup"; swarm `:310` "The lead owns the single serialized landing path"). Nothing says who may commit to or rebase a branch while it is in flight.
- `catalog/skills/swarm/SKILL.md:340-341` has the lead land "from within the teammate's worktree" (see `eth-swarm-lead-lands-from-teammate`).

## Acceptance criteria

- [ ] land-work SKILL.md and swarm SKILL.md each contain one short rule: "One session owns a branch at a time. Before another session touches it, the owner hands off explicitly: a message to the receiving session and a tracker note naming the new owner." Swarm's rule names the teammate-to-lead handoff at completion as the standard case.
- [ ] The swarm teammate prompt template includes the teammate side of the rule ("after you report completion, do not push further commits to the branch unless the lead hands it back").
- [ ] A skill lint check (in the existing skill test suite) asserts the rule text is present in both files, so a later trim cannot drop it silently.
- [ ] Proof at close: the SKILL.md diffs and the passing lint output in the close reason.

## Out of scope

- Mechanical enforcement (branch locks, claim checks). `claim-branch-reconciliation` (bucket bento-guards-doctor-tracker) covers claim/branch reconciliation.
- Where the lead lands from (`eth-swarm-lead-lands-from-teammate`, `land-py-branch-flag`).

## Dependencies

- Blocked by: none.
- Related: bento-qiw, bento-eth, `land-work-skill-restructure` (must keep the rule when trimming SKILL.md), `claim-branch-reconciliation`.

Priority: P3 · Type: task · Labels: audit, land-work, swarm, skills · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: agent-repo-17
