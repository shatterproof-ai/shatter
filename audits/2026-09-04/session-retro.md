# Shatter session retrospective (programmatic, 2026-09-03)

Source: 252 Claude Code JSONL transcripts ≥5 KB under
`~/.claude/projects/-home-ketan-project-shatter*` and the 60+ sibling
`-home-ketan--local-share-worktrees-shatter-*` dirs, parsed with
`scratchpad/retro/retro.py` → `sessions.json`, aggregated by `aggregate.py`
and `supp.py`. Transcripts were never read in full. Recommendation numbers
(#n) refer to `agents/agent-system.md`.

## 1. Metrics

### Population

| Metric | Value |
|---|---|
| Sessions parsed | 252 (110 primary-checkout dir, 137 worktree dirs, 5 other) |
| Date span | 2026-07-10 → 2026-09-03 (Jul 4, Aug 234, Sep 14) |
| Sessions with ≥1 human message | 144; 108 are autonomous (wakeup / teammate / notification only) |
| Human messages total | 409 (median 1 per human session); teammate msgs 175; task notifications 443 |
| Assistant turns | 24,529 (median 24) |
| Tool calls | 12,840 (median 13; 13 sessions >200; max 1,805) |
| Wall-clock | median 2.5 min; 13 sessions ran >24 h (autonomous loops / long landings) |
| Interrupts (`[Request interrupted by user`) | 15 sessions, 1 each — 9 of them are `code-review`/security-review sessions |
| Agent tool calls | 201 in 34 sessions (general-purpose 113, fork 25, claude 20, Explore 13) |
| Skill invocations | code-review 32, pre-completion 23, launch-work 19, land-work 18, formal-methods-policy 8, go/ts/rust-conventions 11, closure 2, swarm 2, cross-check 1, audit 1 |
| API error messages | 61 in 22 sessions; 20 Bash calls refused with "auto mode cannot determine safety (rate-limited)" |

### Tool distribution (all sessions)

| Tool | Calls | Tool | Calls |
|---|---|---|---|
| Bash | 8,523 (66%) | Skill | 135 |
| Read | 1,546 | Write | 108 |
| Edit | 660 | TaskOutput | 99 |
| Grep | 341 | ListAgents | 81 |
| ScheduleWakeup | 254 | Glob | 55 |
| SendMessage | 220 | AskUserQuestion | 38 |
| Agent | 201 | ReportFindings | 32 |
| ToolSearch | 167 | Workflow | 16 |
| Monitor | 156 | | |

### Bash usage and failure

| Metric | Value |
|---|---|
| Bash calls with recorded result | 8,501; errors 362 (4.3%) |
| Consecutive same-tool failure runs (≥2) | 28 (Bash 23, Agent 4, Grep 1); longest run 5 |
| Identical Bash command failing ≥2× in a session | 9 distinct commands; worst `gh pr checks 1` ×7 in one session |
| Edit/Write without prior Read of the path | 138 (Write 101, Edit 37) in 42 sessions — Write dominates, i.e. new files, mostly legitimate |
| Text-tool-in-Bash where Read/Grep/Glob fit (non-piped lead `grep/cat/sed/find/rg/ls`) | 886 calls in 104 sessions (grep 387, cat 211, sed 166, find 61); worst session 142 |
| `--no-verify` | 100 commands in 14 sessions (46 commit, 54 push); one recovery session used it 64× |
| `core.hooksPath=/dev/null` in a command | 23 |
| Foreground `sleep` blocked by harness | 40 of 91 `sleep` calls (45%) |
| "cd before git … untrusted hooks" approval prompt | 6; "multiple operations" approval | 11; blocked outside allowed dirs | 3 |

### Top 30 Bash command prefixes (count / failures / rate)

| Prefix | n | fail | rate | Prefix | n | fail | rate |
|---|---|---|---|---|---|---|---|
| grep | 1280 | 58 | 4.5% | true | 131 | 0 | 0% |
| git status | 430 | 22 | 5.1% | python3 (inline) | 112 | 2 | 1.8% |
| cat | 374 | 5 | 1.3% | rm | 109 | 8 | 7.3% |
| sed | 309 | 2 | 0.6% | for … | 107 | 10 | 9.3% |
| tail | 265 | 5 | 1.9% | git rev-parse | 105 | 0 | 0% |
| run-heavy (gate wrapper) | 263 | 0 | 0% | land-work-run-verifier.py | 102 | 3 | 2.9% |
| ls | 257 | 19 | 7.4% | sleep | 91 | 41 | 45.1% |
| git fetch | 222 | 5 | 2.3% | go test | 82 | 1 | 1.2% |
| echo | 221 | 9 | 4.1% | wc | 79 | 6 | 7.6% |
| git push | 208 | 7 | 3.4% | cargo test shatter-core | 76 | 1 | 1.3% |
| git diff | 200 | 3 | 1.5% | mkdir | 74 | 3 | 4.1% |
| land-work-create-preview.py | 175 | 3 | 1.7% | land-work-verify-lease.py | 66 | 11 | 16.7% |
| bd show | 136 | 2 | 1.5% | git show | 64 | 1 | 1.6% |
| git add | 134 | 2 | 1.5% | git rebase | 60 | 2 | 3.3% |
| find | 131 | 11 | 8.4% | bd create | 58 | 2 | 3.4% |

Highest failure-rate prefixes (n ≥ 8): `gh pr` 77% (13/10 — CI red on a PR, re-polled), `sleep` 45%, `land-work-prepare.py` 32% (18/57), `launch-work-verify.py` 27% (6/22), `land-work-verify-lease.py` 17%, `swarm-worktree-verify.py` 13%, `find` 8%, `ls` 7%.

### Command families

| Family | n | fail | rate | Family | n | fail | rate |
|---|---|---|---|---|---|---|---|
| other (text/shell utils) | 4,097 | 212 | 5.2% | task | 353 | 11 | 3.1% |
| git | 2,518 | 90 | 3.6% | go | 253 | 7 | 2.8% |
| cargo | 625 | 14 | 2.2% | node | 80 | 3 | 3.8% |
| python | 512 | 25 | 4.9% | gh | 64 | 13 | 20% |
| bd | 506 | 15 | 3.0% | git commit/push (hooks fire) | 436 | 15 | 3.4% |

Error-text classes across the 362 failures: plain exit 1 (217), "worktree/main/branch" wording (70 — mostly bento verify scripts refusing), file-not-found (31), timeout (26), cargo compile errors (20), rtk rewrite rejections (6), bd (4), killed (3).

## 2. User Effectiveness Report

Human-authored text is small: 409 messages across 144 sessions, median one message per session. 81 sessions open with the `code-review`/security-review prompt and 34 with the "independent, skeptical reviewer" cross-check prompt — i.e. the user mostly drives through skills and autonomous loops, not conversation. Genuine corrections are rare (8 short correction messages after excluding review boilerplate; 15 interrupts, 9 of them on review sessions). Convention-explaining messages ("always/never/from now on"): 2. Mid-task scope escalation ("also…", "and then…"): 9 messages in 7 sessions.

### Top 3 opportunities

1. **Steer autonomous sessions with a written brief, not "try again".** The three longest, most correction-heavy sessions (`c5f4fd7f` 1,805 tools / 15 corrections / 64 `--no-verify`; `d98d1b93` 504 / 16; `42058bc5` 351 / 14) are recovery or infra sessions where the user answered repeatedly with "try again" / short nudges while the agent thrashed on land-work and hook problems. Pattern: when a session exceeds ~150 tool calls, ask for a `bento:handoff` file and restart with the brief; the 108 autonomous sessions show the loop model works well when the initial prompt is complete.
2. **Interrupting review sessions to re-scope.** 9 of 15 interrupts land in `code-review`/security-review sessions and the follow-up is a re-issued prompt with a different target. Suggested pattern: pass the target and the effort level in the first `/code-review` invocation (it accepts a PR/branch/path and `--level`), and use `ReportFindings`-style structured output rather than re-running.
3. **Conventions are already in docs, but sessions rediscover them (see §4).** The user rarely re-explains conventions (2 messages), yet agents hit the same facts repeatedly. The lever is not more prose from the user but converting the rediscovered facts into enforced tooling (#2, #7, #10, #24, #25) and trimming AGENTS.md so the remaining rules are actually read (#5).

Only-one-sentence quotes to illustrate: "why would check-unpushed.py run just from ending a turn? that's not ending a session." (Stop-hook semantics rediscovered — #19); "don't call it 'make weekly' … this doesn't need to use make at all" (agent defaulted to a Make target after the Taskfile migration — memory `project_taskfile_migration` exists but was not consulted).

## 3. Claude Effectiveness Report

### Top 3 anti-patterns

1. **Shell text tools instead of Read/Grep/Glob/Edit — 886 calls in 104 sessions.** Bash is 66% of all tool calls; `grep` alone is the #1 prefix (1,280) with `cat` 374 / `sed` 309 / `find` 131. 58 `grep` and 11 `find` failures include 6 rtk rewrites of `find -not/-exec` into unsupported `rtk find`. AGENTS.md says "use Grep/Glob/Read" but the auto-mode harness prompt and the RTK block say the opposite (agent-system §2 item 1). **Fix:** resolve the contradiction in `codex/AGENTS.md` (#23), make the rtk hook skip `find`/pipelines/redirects (#24), and put the `/usr/bin/grep` note in global docs rather than memory (#25).
2. **Hook bypass as routine — 100 `--no-verify`, 23 `hooksPath=/dev/null`, in 14 sessions.** Cause chain visible in the data: pre-push `task affected` and beads hooks push `git commit`/`git push` past the 2-minute foreground timeout (2 commit + 7 push timeouts recorded), the agent reads memory `project_beads_git_hook_timeout` / `…corrupts_worktree` (kw hits: no_verify 25 in 11 sessions, hooksPath 4) and bypasses. Since the corrupting test is fixed, the advice is now pure harm. **Fix:** retract those memories, set `BEADS_HOOK_TIMEOUT` sanely and make pre-push gates run in the background or in CI only (#10); add a CLAUDE.md line "hooks may take >2 min — run `git push` with `run_in_background`, never `--no-verify`".
3. **Thrashing on landing mechanics.** The land-work pipeline is the most repeated manual procedure (49× the exact 3-gram `git fetch → create-preview → run-verifier`, 195 create-preview calls vs 90 cleanups in 17 sessions — consistent with the 5 orphaned `/tmp/land-work-preview-*` worktrees). `land-work-prepare.py` fails 32% and `verify-lease.py` 17% of the time (dirty tree, lease moved, or run from the primary checkout on `main` — 4 `launch-work-verify` failures show agents starting on `main` in the primary checkout). Since 2026-08-23, 8 commands in 6 sessions hit `fatal: this operation must be run in a work tree` — the bare-primary breakage from agent-system §1 dates from that recovery session (`78569314`, which also made 6 of the 12 edits into primary-checkout paths). **Fix:** bento #13 (doctor detects `core.bare`/prunable previews), #15 (always-clean previews, verifier logs), #16 (native diverged-primary handling), #17 (executed-vs-cached), plus a shatter memory line: "the primary checkout is bare — never run git there; use the preview worktree".

### Secondary patterns
- **Harness-blocked waits:** 40 `sleep N && tail …` calls blocked ("use Monitor"). Monitor is used 156×, so the fix is known; add to CLAUDE.md "never foreground-sleep; use Monitor/TaskOutput".
- **bd flag guessing:** 8 `unknown flag` errors (`--search`, `--reason`, `--issue-type`, `--note`, `--comment`, `--deps`) and 6 `bd show --json` parse errors (output is a list, not a dict). AGENTS.md's bd quick-ref should list `--json` shape and the create/close flags that exist (extend #5).
- **Bare `cargo test` (446) vs `task test*` (137) and `task check` (58)** — the task-graph rule ("do not run underlying commands bare") is ignored ~3:1, which also means Task's checksum cache is bypassed *and* never warmed, so `task check` later re-runs everything or nothing (#2).
- **`gh pr checks` polled 7× failing in one session** — CI red on a PR the repo policy says shouldn't exist ("do NOT create pull requests"); user corrected once ("no PR the docs are very clear").
- **13 sessions >200 tool calls, all ≥80% Bash, ≤1 Read in some** (`5f2377ef`: 320 Bash, 1 Read) — long autonomous loops driving CI/perf via shell. These are the sessions where Agent fan-out (34 sessions use it) would cut context.

### Proposed memory / CLAUDE.md / skill additions
- Memory (replace 6 workaround entries, #28): `primary checkout is bare since 2026-08-23 — do not git there`; `git hooks run task affected: push in background, never --no-verify`; `bd show --json returns a list`; `foreground sleep is blocked — use Monitor`.
- CLAUDE.md (project): one "Tool rules" block: Read/Grep/Glob for files; Bash for commands; `/usr/bin/grep` and `/usr/bin/git` inside scripts; no foreground `sleep`; hooks-aware push; never `cargo test` bare when a `task` exists (with the cache caveat, #2).
- Skills: a `land` wrapper skill/script that chains prepare → fetch → preview → verifier → lease → merge → push → verify-landing → cleanup with a trap (the 3-gram evidence above), replacing the ~10-step manual sequence; a `bd-create` helper that validates flags (#5); make `/pre-completion` print the cached/executed status of each gate (#2, #17).

## 4. Tooling Failure Evidence (cross-referenced)

| Evidence | Count | Supports |
|---|---|---|
| rtk rewrote `find -not/-exec` into unsupported `rtk find` | 6 failures, 6 sessions; rtk keyword in assistant text 8 hits / 7 sessions | #24 |
| `fatal: this operation must be run in a work tree` in primary checkout | 8 cmds / 6 sessions from 2026-08-23 | #1, #13 |
| land-work verify/prepare failures | prepare 18/57, verify-lease 11/66, launch-work-verify 6/22; `land_work_flake` keyword 43 hits in 16 sessions | #15, #16, #17 |
| create-preview vs cleanup imbalance | 195 vs 90 | #15 |
| `--no-verify` / hooksPath bypass | 100 / 23 commands, 14 sessions; commit/push timeouts 9 | #10, #28 |
| git hook / commit timeouts at 2 m foreground | 9; 26 timeouts overall incl. 6 cargo, 2 task | #10, #2 |
| bd flag errors / JSON shape errors | 8 / 6 | #5 (bd quick-ref) |
| Foreground `sleep` blocked | 40 | CLAUDE.md tool rules |
| Auto-mode "rate-limited, cannot determine safety" refusals | 20 Bash calls | global settings (#26/#27) |
| Bare `cargo test` outnumbering task facade 3:1 | 446 vs 137 | #2, #7 |
| `e2e_concolic` / `parity-matrix` re-derived in assistant text | 34 hits / 23 sessions; 28 / 16 | #6 (per-crate CLAUDE.md size → rules not retained) |
| Edits into primary-checkout paths | 12 in 4 sessions (6 in the recovery session that preceded the bare-repo breakage) | #1, #13 |
| Stop hook confusion ("why would check-unpushed run from ending a turn") | 1 user correction | #19 |

### Memory gaps (facts rediscovered ≥3 sessions without a memory or doc line)
- Foreground `sleep` is blocked (40 hits, no memory).
- `bd show --json` returns a list; `bd create` has no `--issue-type/--reason/--note` (14 hits, no memory).
- Primary checkout is bare / unusable for git (6 sessions since 08-23, no memory).
- rtk `find` limitation (6 sessions; memory covers redirects/refs, not `find`).
- `task` vs bare `cargo test` and the checksum cache (no memory, no doc; #30).
- Land-work must be run from the linked worktree, not the primary on `main` (4 launch-work-verify failures).
