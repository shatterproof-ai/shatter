# Cross-check review

- **Reviewer:** claude (DEGRADED same-runtime fallback)
- **Artifact type:** issue
- **Mode:** degraded

> **DEGRADED REVIEW.** The counterpart runtime was unavailable, so this review came from an independent agent of the *same* runtime. It shares the original author's model and blind spots; weight it accordingly.

## Findings

# Cross-check review (DEGRADED: same-runtime fallback) -- bundle bento-landing

**DEGRADED.** The Codex counterpart run failed identity validation (exit 4). This is a same-runtime (Claude) independent read-only review using review-issue.md. Claims were checked against bento origin/main 5f8b850. The bundle cites b1bb787, but `git diff b1bb787 origin/main` is empty for land-work, swarm, wire-land-verifier, closure and hooks, so the line citations still apply. Referenced bento issues were checked with `bd show`, and open bento issues were searched for duplicates.

## Findings

### BLOCKER -- 07 landing-deletes-remote-branches duplicates open bento-73de
bento-73de ("land-work cleanup: delete the landed feature branch on the remote, not only locally", open P2, filed 2026-09-23) already specifies a remote-delete helper with an ancestry check, an opt-out, primary-branch refusal and tests. It also shows that the `git ls-remote --heads origin <feature>` confirmation proposed in draft 07 can match tail refs such as `team/<branch>`. Do not file 07 as a new issue. At most, post its remaining deltas as a note on bento-73de: `--superseded` same-issue deletion, the preview-absence check, the closure report-only listing, and the shatter evidence.

### MAJOR -- 02 land-py-verifier-log-kept overlaps open bento-x4bm, and the two log locations conflict
bento-x4bm (open P2) names the same defect ("The raw verifier log is written inside the preview, which cleanup deletes"; "land.py never passes --log"). It also fixes it: land.py passes `--log <landing dir>/verifier.log`, with the acceptance criterion "verifier.log still exists after the preview is removed". Draft 02 proposes a different location (`$XDG_STATE_HOME/bento/land-work/<repo>/<branch>-<ts>.log`) and does not mention x4bm. Filed as written, the two issues would conflict. Reconcile first: either narrow 02 to what x4bm lacks (the failure-JSON `output_path`/tail and success-path retention) and block it on x4bm, or add a note to x4bm.

### MAJOR -- 02: the `verifier_log_tail` acceptance criterion assumes behaviour run-verifier does not have
`land-work-run-verifier.py:136-138` sets `verifier_log_tail` only when `status in ("killed", "timeout")`. For an ordinary `failed` verifier it is never emitted. So the proposed test (a stub that fails and prints a marker, with an assertion that `verifier_log_tail` contains it) cannot pass by "reusing run-verifier's existing value", as the criterion and approach say. Either widen run-verifier to emit the tail on `failed` (and put it in scope), or have land.py read the tail from the persisted log. The evidence line "already computes `diagnostics["verifier_log_tail"]`" should state this condition.

### MAJOR -- 03 verifier-log-reopen-note should point to bento-x4bm, not only a new issue
The comment on bento-rdtn.4 says the fix is "Tracked in `<id of land-py-verifier-log-kept>`". bento-x4bm already tracks the persistence fix, so the note should cite x4bm, or both issues once 02 is reconciled. "`verifier_log_tail` is also dropped" is also only true for killed/timeout runs; see the previous finding.

### MAJOR -- 09 land-work-skill-restructure contradicts bento-49pg's recorded owner decision
bento-49pg (open P2) records owner decision Option C (2026-09-22): untrack the JSONL export, with "one rule in land-work and beads-issue-flow". Draft 09's criterion says "The Tracker Handoff states no JSONL policy of its own", which is the opposite, and the draft never mentions 49pg. Align the criterion with 49pg (land-work keeps one short rule that defers to it) and add 49pg under Related or Blocked by.

### MINOR -- 09: the lint criterion extends beyond land-work
The proposed lint fails on an unescaped `$(` in any skill's `SKILL.md` or `references/*.md`. `$(` also appears in launch-work/SKILL.md, closure/SKILL.md and land-work/references/batch-landing.md. The Command Rule is land-work-specific, so either scope the lint to land-work or state that the other skills are fixed too.

### MINOR -- 05 land-py-merge-abort-ownership: the HEAD@{1} reasoning is partly wrong
In the `:177-185` fast-forward path, the ff creates a reflog entry and the merge creates another, so `HEAD@{1}` is exactly the leased pre-merge SHA. The ff does not make the reset wrong; only a concurrent HEAD move does. The third test ("forced tree mismatch after a fast-forward restores exactly the recorded SHA") therefore passes against current code and is not a regression test. Reword it to inject a concurrent HEAD move. The core `MERGE_HEAD` ownership claim (`:144-148`, called from `:349-350`, `:367-368`, `:377-378`, with `primary_root` set at `:278`) is confirmed.

### MINOR -- 06 land-py-invocation-progress-log: citation and count inconsistencies
The exit code is set at `land.py:383` (`return 0 if result["ok"] else 1`), not `:386-393`. The text says 14 runs but cites 36 occurrences of the command string. Say that the 36 includes non-invocation mentions, or reconcile the two numbers.

### MINOR -- 04 land-work-post-push-workflow-health: missing related issue
bento-2jo (open P2, "warn when pushing main will trigger publishing automation") is the pre-push side of the same GitHub-workflow awareness. Link it. The code claims were confirmed: no `gh run` anywhere in land-work (verify-landing uses `gh` only for `issue view`), no `gh` or workflow check in either agent-env-doctor.py, and `run()` ends at verify_landing.

### MINOR -- 08 verifier-contract-migration: "land.py currently ignores run-verifier warnings"
run-verifier emits no `warnings` key at all today, so land.py has nothing to ignore. State that both sides are new. The other citations (`:520-545` guard, `land.py:116-121`, wire-land-verifier emitting `executed` around `:905-917`, the wire-land-verifier directory having no references/) are confirmed.

### MINOR -- 10 eth-swarm-lead-lands-from-teammate: "bento-qiw settles who lands" overstates it
bento-qiw is open ("clarify landing ownership ..."), so nothing is settled yet. The swarm SKILL.md `:337-342` quote is confirmed.

### MINOR -- 14 merge-message-and-stale-branch-nudge: link bento-i76i
The bare-SHA merge refusal belongs to the same git guard that bento-i76i (open P2) is extending for commit, cherry-pick, pull, am and revert on the primary branch. Link it, or fold the refusal into it, so the two changes do not collide.

### MINOR -- Bundle header SHA is stale
bento origin/main is now 5f8b850. There are no land-work changes since b1bb787, but the filer should restate the SHA.

## Confirmed without findings
- 01 e583-preview-owner-lock: `leftover_preview_worktrees()` at `:62-82` has no owner, pid or age check; the refusal at `:252-275` says "remove them first"; `cleanup_preview()` at `:163-173` runs `worktree remove --force`; `land.py:291` passes only `--base-ref`. bento-e583 is open P1.
- 11 dyp7-admission-control: run-heavy exists only at `catalog/skills/launch-work/scripts/run-heavy` and is referenced only in the launch-work and swarm docs. bento-dyp7 is open P1.
- 12 rebase-before-land-configurable: `land.py:274` passes `--require-up-to-date` unconditionally, and SKILL.md `:239` is step 5.
- 13 merge-push-observability: `:309-313`, `:166-214` and `:216-269` match; push stderr is surfaced only on failure.
- Every referenced issue id exists with the stated status: e583, rdtn.3/.4/.6/.9/.14, 7n7, gd2, 1qry (in_progress), eth, qiw, dyp7, by8, 7kw.

## Verdict
**Not ready to file as-is.** 10 of the 14 drafts are accurate and well-formed, with checkable, failing-first acceptance criteria. Top fixes:
1. Drop 07 as a new issue and turn its deltas into a note on bento-73de.
2. Reconcile 02 and 03 with bento-x4bm (one log location, and block on or fold into x4bm), and fix the `verifier_log_tail` criterion, which only works for killed/timeout today.
3. Align 09's Tracker Handoff criterion with bento-49pg's Option C decision and link it.
