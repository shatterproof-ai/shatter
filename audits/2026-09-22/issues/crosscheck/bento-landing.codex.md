# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** 0b141df5878121dccf3411446a45bf17a3569a3c094d3a208898c9a94962da08
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket bento-landing (bento). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

**Not ready to file as-is.** The verifier-log deletion bug is supported by the code, but several proposed fixes have safety gaps or acceptance criteria that would give misleading proof. Live tracker status could not be verified: even `bd --readonly --sandbox` required a database lock unavailable in this read-only environment.

1. **BLOCKER — 07: Remote deletion can remove work pushed after the safety check.**  
   Checking ancestry against `origin/<feature>` and then deleting the remote branch leaves a race where another session pushes unmerged commits between those operations. Require deletion conditional on the exact inspected remote SHA; for `--superseded`, define how unique commits and active ownership are handled, since matching issue IDs proves neither is safe to discard.

2. **MAJOR — 01: The ownership rule rejects the driver’s own cleanup subprocess.**  
   `land.py` invokes `land-work-create-preview.py --cleanup` as a separate process, so the proposed owner PID and held lock belong to another process from the cleaner’s perspective. Specify an authenticated ownership handoff or inherited lock mechanism, and test successful owner cleanup alongside refusal of foreign cleanup.

3. **MAJOR — 05: “Attempted merge” is not proof of merge ownership.**  
   Another session can start a merge after prepare; this driver then sets `merge_started=True`, encounters that existing merge, and aborts it under the proposed rule. Require ownership established under serialization or an equivalent protected protocol, including protection against state changing between the HEAD check and reset.

4. **MAJOR — 05: The proposed regression setup does not exercise the claimed failures.**  
   Prepare rejects a dirty primary checkout before assigning `primary_root`, so a pre-existing conflicted merge normally prevents reaching the proposed verify failure; inject the foreign merge after prepare instead. Also, `BENTO_LAND_TEST_DELAY_MERGE` pauses **before** `git merge`, so its existing SIGINT test cannot prove that an actual in-progress merge is aborted.

5. **MAJOR — 02/03: Forwarding the existing tail does not cover ordinary verifier failures.**  
   [`_fail()`](/home/ketan/project/bento/catalog/skills/land-work/scripts/land-work-run-verifier.py:135) populates `verifier_log_tail` only for `killed` and `timeout`, not a valid verifier result reporting failure. Expand the fix to generate tails for ordinary failures, and make the regression stub emit valid failure JSON so it cannot pass by accidentally exercising the “killed” classification.

6. **MAJOR — 04: Workflow polling can announce completion before workflows appear.**  
   “Until all runs complete” leaves an empty initial response, delayed workflow creation, and the command’s default 20-run limit undefined. Specify discovery/grace behavior, pagination, and a distinct no-runs outcome, with tests for initially empty and subsequently appearing runs.

7. **MAJOR — 04: The doctor’s suggested implementation contradicts its call budget.**  
   Acceptance allows at most one `gh` call per repo per hour, but the suggested implementation uses `gh workflow list` plus one `gh run list` per workflow. Choose a batched retrieval contract or revise the budget, and explicitly filter completed runs before evaluating the three-failure rule.

8. **MAJOR — 07: Preview-cleanup verification is already implemented.**  
   [`Driver.run()`](/home/ketan/project/bento/catalog/skills/land-work/scripts/land.py:315) passes its scratch preview path to verify-landing, which checks registered worktrees; driver tests already assert preview removal after success and verifier failure. Replace “nothing checks” with the specific remaining failure-path gap, and preserve the deliberately persistent integration-worktree exception.

9. **MAJOR — 08: Newly generated wrappers also omit `executed`.**  
   The [current wrapper template](/home/ketan/project/bento/catalog/skills/wire-land-verifier/scripts/wire-land-verifier.py:893) emits `executed` only for go-task checks; other commands intentionally omit it. Regenerating therefore does not eliminate the proposed warning, so define execution-evidence semantics for current non-task wrappers and test that migration has a meaningful endpoint.

10. **MAJOR — 09: The shortened serial path could drop required work outside `land.py`.**  
    The driver does not implement independent review, lifecycle hook scripts/skills, primary baseline checks, tracker closure, root hygiene, or feature-worktree cleanup—all currently required by SKILL.md. Add an explicit preservation checklist and keep those obligations reachable in the default serial path.

11. **MAJOR — 09: The lint criterion silently expands into a repository-wide rewrite.**  
    Requiring every `SKILL.md` and reference command block to avoid `$(` reaches unrelated skills: launch-work and beads-issue-flow already contain matching commands. Scope the lint to land-work or create a separately scoped migration with an inventory of affected skills.

12. **MAJOR — 10: The addendum exceeds the existing issue’s stated scope.**  
    The available `bento-eth` export explicitly excludes land-work mechanics, whereas `land.py --branch` requires changes to branch discovery, preparation, preview creation, and ownership handling. Make that driver capability a separate prerequisite or explicitly revise the existing issue’s scope.

13. **MAJOR — 11: Nested governor acquisition can deadlock landing.**  
    The proposal reserves capacity around `merge_push` while also encouraging its pre-push hook to acquire capacity through `run-heavy`. Define reentrancy or reservation handoff, and test a saturated governor with a governed driver invoking a governed hook.

14. **MAJOR — 14: This bundles independent features and bans a legitimate driver operation.**  
    Merge-message formatting, a Git guard, stale-branch monitoring, and session ownership have independent implementations and done-conditions and should be split. The bare-SHA prohibition also needs an explicit synchronization exception: `_merge_in_primary()` currently uses `git merge --ff-only <leased_sha>`.

15. **MINOR — 05: Fast-forwarding alone does not make `HEAD@{1}` incorrect.**  
    After a fast-forward followed by a successful merge commit, `HEAD@{1}` points to the post-fast-forward, pre-merge commit—the same point the proposed `pre_merge_sha` records. Keep the concurrent-HEAD-movement concern, but remove the claim that the preceding fast-forward itself demonstrates the defect.

The top fixes are to define safe ownership and conditional-deletion protocols, correct the code claims and regression setups, and split unrelated scope while reconciling contradictory acceptance criteria.
