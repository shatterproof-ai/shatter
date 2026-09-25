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
