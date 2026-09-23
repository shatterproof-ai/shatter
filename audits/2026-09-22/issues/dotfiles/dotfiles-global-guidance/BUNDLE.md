# Bundle: dotfiles-global-guidance (audit 2026-09-22)

- **Bucket:** dotfiles-global-guidance. The theme is global agent guidance and hooks: loading the required rules, waiting behaviour, framing of bypass options, plugin auto-update, rtk, the memory lifecycle, validators and escalation.
- **Repo / tracker:** dotfiles, via `gh -R ketang/dotfiles` (GitHub Issues; the repo has no `.beads`). GitHub has no priority field, so each body states its priority in the text. The only labels available are the GitHub defaults (bug, documentation, enhancement, ...).
- **Parent epic:** "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)". Each body starts with "Part of #<epic>", and the filer substitutes the epic's number.
- **Cross-references:** bodies refer to other drafts by slug. The filer resolves `blocked_by` slugs and posts a slug-to-issue map on the epic.
- **Status:** drafts only. Nothing has been filed. Under D6 the maintainer runs one filer script after reconciliation and the Codex cross-check.
- **Verified against:** dotfiles `main` @ `81f35e1`, the shatter `audit-2026-09-22` worktree @ `56c86168`, and live `~/.claude` state, all on 2026-09-23.

## Maintainer decisions (2026-09-23). These override the report and older drafts.

- **D1 Releases.** Keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix, and fix them: Z3 header and static linking on Windows, openssl-sys under cross for aarch64. Release work closes only with a URL to a green release run.
- **D2 shatter diff.** Retire snapshot `shatter diff` and the unused Snapshot writer path; spec-diff is the regression tool. Update SPEC, README and QUICKSTART to match. The `diff` name becomes free, and str-81xiw decides whether to use it. Correct the shatter-agents `shatter diff --staged` docs.
- **D3 Concolic positioning.** Measure first:
  - P1: a controlled default-vs-concolic benchmark, reported per release.
  - P1: fix concolic early termination.
  - A follow-up decision issue, blocked by both, decides the positioning. No doc softening now.
- **D4 Beads hook stall.** Retire the JSONL import and sync the tracker through a Dolt remote:
  - First, verify whether the stale JSONL import has been clobbering newer DB state.
  - AGENTS.md drops `bd sync`.
  - str-qwua7.28 is superseded.
  - Bento's beads-issue-flow gets matching guidance.
  - No BEADS_HOOK_TIMEOUT env var and no hook-bypass guidance anywhere.
- **D5 Git identity.** The leaked `[user]` section in the shared `.git/config` was already removed. Remaining work:
  - A `.mailmap` for test@example.com, with no history rewrite.
  - A git-state check for identity overrides, example.com emails, core.bare and hooksPath.
  - A before/after snapshot of `.git/config` around the fixture entrypoints.
- **D6 Filing.** The maintainer runs one filer script after reconciliation and the Codex cross-check. Agents file nothing.

Decisions that touch this bucket:
- **D4:** no draft proposes, lists or describes a hook bypass. `never-recommend-bypass` forbids offering bypass options at all; `blocked-escalation` treats a classifier denial as an unauthorized action, never a prompt to retry with a bypass. Both carry D4 in their bodies.
- **D2:** `first-party-plugin-autoupdate` notes that the refreshed plugin will temporarily ship the `shatter-diff` skill until shatter-agents `withdraw-shatter-diff-skill` lands.
- **D6:** every body says nothing from this audit is filed by agents. Drafts no longer tell implementers to file follow-up issues; they tell the maintainer in the closing comment.
- D1, D3 and D5 do not change any draft in this bucket.

## Changes from the Codex cross-check (2026-09-23)

See `REVISION.md` for the per-finding table. In short:
- `first-party-plugin-autoupdate` split: the staleness checker is now `first-party-plugin-staleness-check` (12).
- `global-guidance-actually-loads` split: Codex delivery is now `codex-render-composes-core-rules` (13), blocked by 01, with a composition-aware render/status/adopt contract and round-trip tests.
- `never-recommend-bypass` no longer allows listing bypasses as a secondary option (D4).
- `blocked-escalation`: a classifier denial means the action is unauthorized; switching paths is allowed only when the other path is already authorized.
- `tool-precedence-vs-harness-mode` reframed as landing #10's missed precedence sentence; no relaxation.
- Proofs no longer rely on `agents-sync.sh status`, and every Python test names its pytest command.
- `hooks-dotfiles-env-unset` test isolated with stubs and event payloads; `background-wait-rule-and-hook` guard catches the alternating sleep/ReadNotifications loop; `validators-fail-on-empty-extraction` no longer fails on valid non-matching allowlist entries; `memory-lifecycle-rule` corrects the false "already cleaned up" claim.

## Changes from the old drafts (other-first-party/01-11), found during re-verification

1. **Overlap with open dotfiles issues #18, #20, #21 and #23, all filed on 2026-09-23 from the zolem review.**
   - #20 owns the absolute-path fix, so it was removed from `global-guidance-actually-loads`.
   - #18 inlines a Definition of Done that includes the wiring rule, so it is not duplicated.
   - #23 caps `codex/AGENTS.md` at 800 words: Self-Improvement Loop ≤ 12 words, Tool-Specific Notes ≤ 150. So the rules now live in leaves plus a SessionStart-injected core file, not inline.
   - #21 covers the pending Codex render. (Revised after the Codex cross-check: `agents-sync.sh status` is no longer accepted as proof that a rule reached Codex.)
2. **Closed #4-#8 were closed as overscoped on 2026-09-07 and never built.** No `session-lint.py` or `memory-index-audit.py` exists. The acceptance criteria that said "extend the #4 / #5-#7 tooling" became small, standalone scripts.
3. **`hooks-dotfiles-env-unset`:** the PreToolUse `rtk_prefilter.py` hook (`settings.json:285`) also uses `$DOTFILES`. When the variable is unset, neither the #11 protection nor rtk runs.
4. **`first-party-plugin-autoupdate`:** the shatterproof remote is reachable (`ls-remote` returns 119b807), and marketplaces without autoUpdate refreshed today. So the missing autoUpdate is the likely cause. The setting's source is `claude/hosts/pontoon/settings.json:36-41`.
5. **`tool-precedence-vs-harness-mode`:** every rtk `find -not/-exec` rejection predates #11 (2026-08-27 to 09-07), so that acceptance criterion was dropped.
6. **`rtk-head-range-compound`:** re-reproduced on 2026-09-23.
7. **`memory-lifecycle-rule`:** only the `--no-verify` memory was corrected on 2026-09-23. The `core.bare` memory and the unindexed 07_10 file are still stale (corrected in the cross-check revision).
8. **Priorities follow the verifier:** background-wait P2, falsification-probe-first P3, blocked-escalation P3.

## Contents

| # | Slug | Title | P | Type | Blocked by |
|---|---|---|---|---|---|
| 01 | global-guidance-actually-loads | Global Required-Loads guidance almost never loads: inject a short core-rules set into every Claude Code session | P1 | bug | - |
| 02 | background-wait-rule-and-hook | Agents busy-wait on background work with alternating sleep-1 and ReadNotifications loops: add a waiting rule and a PreToolUse guard | P2 | enhancement | - |
| 03 | never-recommend-bypass | Agents offer hook or gate bypass as the (Recommended) option: forbid offering bypass options in failing-checks.md | P2 | enhancement | - |
| 04 | first-party-plugin-autoupdate | Installed shatter/refute plugins are 27 commits stale: enable autoUpdate for the shatterproof marketplace | P2 | bug | - |
| 05 | rtk-head-range-compound | rtk still replaces `head -N` output with a summary inside compound commands (follow-up to #11) | P2 | bug | - |
| 06 | memory-lifecycle-rule | Agent memory stands in for a tracker and goes stale: add an issue-first, retire-on-close memory rule | P2 | enhancement | - |
| 07 | validators-fail-on-empty-extraction | Fail-closed guidance does not cover vacuous validators: require failure on empty extraction and a canary test | P3 | enhancement | - |
| 08 | hooks-dotfiles-env-unset | Global hooks use $DOTFILES, which is unset in non-interactive sessions: 94 hook failures, skipped summaries, rtk prefilter not running | P3 | bug | - |
| 09 | falsification-probe-first | Planning guidance: experiment and benchmark plans must start with a falsification or upper-bound probe | P3 | enhancement | - |
| 10 | blocked-escalation | Sessions stall for hours on unseen questions and classifier denials: add escalation guidance | P3 | enhancement | - |
| 11 | tool-precedence-vs-harness-mode | Tool-Specific Notes never say the Read/Grep-first rule overrides the harness's bypass-mode Bash guidance (missed #10 acceptance check) | P3 | enhancement | - |
| 12 | first-party-plugin-staleness-check | Warn at SessionStart when an installed first-party plugin is behind its local source checkout | P2 | enhancement | - |
| 13 | codex-render-composes-core-rules | Deliver the core-rules file to Codex: make agents-sync.sh render, status and adopt composition-aware | P1 | bug | global-guidance-actually-loads |


---

<!-- FILE: 01-global-guidance-actually-loads.md -->

---
slug: global-guidance-actually-loads
kind: new
title: "Global Required-Loads guidance almost never loads: inject a short core-rules set into every Claude Code session"
priority: P1
type: bug
labels: [bug, documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Global Required-Loads guidance almost never loads: inject a short core-rules set into every Claude Code session

Part of #<epic>. Priority: P1. Type: bug.

The Codex half of this delivery is split out as `codex-render-composes-core-rules`, which is blocked by this issue because it needs the core file. Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic.

## Problem

The only auto-loaded global file is `codex/AGENTS.md`. `~/.claude/CLAUDE.md` imports it with `@../codex/AGENTS.md`, and Codex reads the rendered copy `~/.codex/AGENTS.md`. At line 16 it says "Follow the shared agent guidance in `~/dotfiles/docs/agent-guidance.md`". That line is a plain-text pointer, not an `@`-import. The index it points to (`docs/agent-guidance.md:8-16`) lists four "Required Loads" (branches-and-worktrees, read-before-designing, verification, and instruction-integrity, which is required "every session") plus about 15 conditional loads. All of them are prose pointers too.

The load-rate numbers below show that agents almost never read those files. P1 rests on that load rate alone: required rules that reach close to 0% of sessions are not in force. The audit also saw several failures that match the unloaded rules (skills documenting non-existent CLI commands, a stale plugin cache, agents offering gate bypasses), but the verifier noted that this causal link is inferred, not demonstrated.

## Evidence

Re-verified on 2026-09-23 against dotfiles `main` @ `81f35e1`.

- `codex/AGENTS.md:16` has a plain-text pointer to `~/dotfiles/docs/agent-guidance.md` and no `@`.
- `docs/agent-guidance.md:8-50` lists every leaf as a prose pointer, and none of them is imported.
- Load-rate scan of the Shatter project transcripts (`~/.claude/projects/-home-ketan-project-shatter/*.jsonl` and `*/subagents/*.jsonl`). It counts tool_use inputs whose `file_path` or command references `agent-guidance/` or `code-writing-guidance`:
  - Top-level sessions: 0/87 with the narrow pattern, 4/87 with a broad pattern that also counts the index file and Bash mentions.
  - Subagent transcripts: 1/166 (narrow) and 6/166 (broad).
- `wc -w codex/AGENTS.md` returns 1146.
- The hook-provenance sentence at `codex/AGENTS.md:23` says the worktree hook is "registered from `~/project/bento`". In fact `readlink -f ~/.claude/hooks/bento/require-worktree.sh` resolves to `~/.claude/plugins/cache/bento/bento/2.3.84/hooks/scripts/require-worktree.sh`, the installed plugin cache.

## Related open work (coordinate; do not duplicate)

- **dotfiles#18** puts a Definition of Done in `codex/AGENTS.md`, including the wiring-and-consumption rule and a pointer to drift-checks. The core file must not restate it.
- **dotfiles#20** converts guidance cross-references to absolute `~/dotfiles/docs/...` paths. That work is owned there.
- **dotfiles#23** caps `codex/AGENTS.md` at 800 words. This issue does not grow that file; the core file is injected separately.
- **dotfiles#21** covers the pending Codex render after guidance changes (relevant to `codex-render-composes-core-rules`).
- Closed #4 (Claude JSONL session lint) was closed as overscoped on 2026-09-07 and never built. No `claude/session-lint.py` exists. Do not assume that tooling.

## Acceptance criteria

- [ ] A core-rules file `~/dotfiles/docs/agent-guidance/core.md` exists, at most ~450 words, starting with a fixed marker line (for example `<!-- core-rules v1 -->`). It has one heading per rule, one or two imperative lines each, and a link to the leaf.
- [ ] Required Loads keep their force. For each of the four current Required Loads (branches-and-worktrees, read-before-designing, verification, instruction-integrity), `core.md` either states the leaf's essential constraints, or `docs/agent-guidance.md` keeps that leaf listed as a required load with its trigger. No Required Load becomes "optional depth" unless its essential constraints are in `core.md`.
- [ ] `core.md` also covers drift-checks, failing-checks and cross-repository boundaries. It must not duplicate #18's Definition of Done.
- [ ] Ownership of leaf summaries: `core.md` includes a one-line summary for each of these leaf rules that is on `main` when this issue lands: `never-recommend-bypass`, `background-wait-rule-and-hook`, `memory-lifecycle-rule`, `blocked-escalation`. Leaf rules that land later add their own line (each of those issues says so).
- [ ] Every Claude Code session receives `core.md` with no action by the agent. The mechanism is a SessionStart hook in `claude/settings.json` that prints the file, using a `$HOME/dotfiles/...` path, not `$DOTFILES` (see `hooks-dotfiles-env-unset`).
- [ ] Subagents: the closing comment states whether subagent sessions receive the injected context. If SessionStart does not reach subagents, either use a harness event that can add context to subagents, or record the gap explicitly in `docs/agent-guidance.md`.
- [ ] The hook-provenance note in `codex/AGENTS.md` (in whatever form survives #23) names the installed bento plugin cache instead of `~/project/bento`.
- [ ] Test: `claude/tests/test_core_rules_hook.py` extracts the SessionStart command from `claude/settings.json`, runs it under `env -i HOME=<tmp> PATH=/usr/bin:/bin` with `<tmp>/dotfiles` a copy of the repo, and asserts that stdout contains the marker line and every `##` heading of `core.md`. Run it with `python3 -m pytest claude/tests/test_core_rules_hook.py -q`. It must fail on the current tree (no such hook) and pass after the change.
- [ ] Delivery measurement, not read-call counts: over all top-level Claude Code sessions in `~/.claude/projects/*/` started in the 7 days after deployment, count the transcripts that contain the core marker line in hook-injected context. Success means 100% of top-level sessions. Report the subagent rate separately.

## Proof at close

The closing comment includes the red and green runs of the test, the delivery-measurement counts (numerator and denominator, and the script used), and an excerpt from one new session transcript that shows the injected marker.

## Suggested approach

1. Write `core.md` by condensing the leaves.
2. Add a SessionStart hook entry such as `cat "$HOME/dotfiles/docs/agent-guidance/core.md"`. Output from a SessionStart hook is added to context.
3. Update the "Required Loads" section of `docs/agent-guidance.md` to match the coverage criterion above.
4. Use a throwaway scan script for the measurement. Do not build the #4 toolchain.

## Out of scope

- Delivering the core rules to Codex (`codex-render-composes-core-rules`).
- Rewriting the leaf content.
- Absolute paths (#20), the Definition of Done (#18), the word budget itself (#23) and Codex render automation (#21).

## Dependencies

None blocking. Blocks `codex-render-composes-core-rules`.

## Source

Shatter audit 2026-09-22, findings plugins-06 (P1) and plugins-19 (P3; its path part is now owned by #20). The evidence is in `audits/2026-09-22/areas/plugins-guidance.md` on the shatter `audit-2026-09-22` branch.

---

<!-- FILE: 02-background-wait-rule-and-hook.md -->

---
slug: background-wait-rule-and-hook
kind: new
title: "Agents busy-wait on background work with alternating sleep-1 and ReadNotifications loops: add a waiting rule and a PreToolUse guard"
priority: P2
type: enhancement
labels: [enhancement, documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Agents busy-wait on background work with alternating sleep-1 and ReadNotifications loops: add a waiting rule and a PreToolUse guard

Part of #<epic>. Priority: P2 (the verifier lowered it from P1: it wastes turns and tokens but does not produce incorrect repository state). Type: enhancement. Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic.

## Problem

After starting background work, agents busy-wait instead of ending the turn. The dominant pattern alternates a bare `sleep 1; echo ok` Bash call with a `ReadNotifications` call, hundreds of times. The harness blocks long foreground sleeps, and its block text says "Do not chain shorter sleeps to work around this block." Agents chain short sleeps anyway. No global instruction says that ending the turn is the correct way to wait.

## Evidence

Shatter transcripts in `~/.claude/projects/-home-ketan-project-shatter/`, counted since 2026-09-04:

- 820 `sleep N[; echo x]` calls across 12 sessions, and 600 ReadNotifications polls.
- Session `87606e10-4dce-47bb-a273-768abd7be0a2` (claude-sonnet-5, 2026-09-19 to 09-21):
  - 1,575 tool calls. 609 were bare `sleep 1; echo ok` (description "No-op wait for code review notification") and 598 were ReadNotifications.
  - Adjacent tool-call pairs, recounted on 2026-09-23: `sleep` then `ReadNotifications` 596 times, `ReadNotifications` then `sleep` 487 times. The loop alternates; there are almost no back-to-back ReadNotifications runs.
  - 541 assistant turns begin "Still waiting". 934,138,123 cache-read tokens.
- Session `9f13ca23-bf49-4efb-abd0-ed3519e0dd38` made 200 `sleep 1` calls with the description "yield".
- The agent writes "I'll wait for the background task notification instead of polling." and then keeps polling.
- Contributing text: shatter `AGENTS.md:405-409` ("Team-lead liveness") tells the lead to "actively poll teammate liveness … never idle until the user notices". Re-verified at shatter `56c86168`.
- There is no waiting rule anywhere in `~/dotfiles/docs/agent-guidance/` (dotfiles @ `81f35e1`).

## Acceptance criteria

- [ ] A leaf `~/dotfiles/docs/agent-guidance/waiting.md` states the rule. After a `run_in_background` launch, or while waiting on a teammate or a notification, do one of these: end the turn; use a blocking wait (TaskOutput with block, or a Monitor until-loop); or run one bounded background until-loop. Never poll with short sleeps or repeated notification checks.
- [ ] If `docs/agent-guidance/core.md` (from `global-guidance-actually-loads`) is on `main` when this lands, this issue adds the rule's one-line summary to it. If not, that issue adds it. Nothing is added to `codex/AGENTS.md` (dotfiles#23 word cap).
- [ ] A PreToolUse hook `claude/wait_guard.py`, registered in `claude/settings.json` with a `$HOME/dotfiles/...` path, denies with a message that names `waiting.md`:
  - a Bash command that is only `sleep N`, or `sleep N` joined to `echo …`/`true`/`:` by `;`, `&&` or a newline;
  - a ReadNotifications call when the session's recent history is wait-only. Define "wait-class" calls as ReadNotifications and the bare-sleep Bash commands above (including denied ones). The guard keeps a per-session count of ReadNotifications calls since the last non-wait-class tool call. Only non-wait-class calls reset it. It denies the 4th.
- [ ] Commands that merely contain `sleep` (such as `sleep 2 && curl …`, or a `timeout` wrapper) are allowed and do not count as wait-class.
- [ ] Test `claude/tests/test_wait_guard.py`, run with `python3 -m pytest claude/tests/test_wait_guard.py -q` (`claude/tests/run.sh` discovers only `test_*.sh`, so it does not run Python tests). It feeds fixture PreToolUse JSON payloads under one session id and covers:
  - the motivating alternating sequence `sleep 1; echo ok` → ReadNotifications → `sleep 1; echo ok` → ReadNotifications → … : the sleeps are denied, and the 4th ReadNotifications is denied;
  - four adjacent ReadNotifications: the 4th is denied;
  - ReadNotifications ×3 → `Read` → ReadNotifications: allowed (a real tool call resets the count);
  - the allowed-`sleep` cases above;
  - two session ids do not share a counter.

## Proof at close

The closing comment includes the pytest output, and the guard's denial message as seen in one live session.

## Suggested approach

A small Python script that reads the PreToolUse JSON from stdin. It matches `tool_name == "Bash"` against an anchored regex. It keeps the counter in `${XDG_RUNTIME_DIR:-/tmp}/claude-wait-guard/<session_id>`. Model it on `claude/rtk_prefilter.py` and `claude/tests/test_rtk_prefilter.py`.

## Out of scope

- Rewording shatter `AGENTS.md:405-409` to mean event-driven liveness checks. That text lives in the shatter repo. The closing comment should point the maintainer at it; no issue for it exists in this audit.
- The bento check-unpushed Stop hook, which blocks turn end during long landings and pushes agents toward busy loops. That is drafted in bento as `check-unpushed-overcount-and-blocks` (source sessions-10).
- The shatter-local "no foreground sleep" line in str-qwua7.26.

## Maintainer decisions that apply

D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Dependencies

None blocking. Related: `global-guidance-actually-loads` (core summary) and `hooks-dotfiles-env-unset` (use `$HOME/dotfiles` paths).

## Source

Shatter audit 2026-09-22 finding sessions-01, in `audits/2026-09-22/areas/sessions.md`.

---

<!-- FILE: 03-never-recommend-bypass.md -->

---
slug: never-recommend-bypass
kind: new
title: "Agents offer hook or gate bypass as the (Recommended) option: forbid offering bypass options in failing-checks.md"
priority: P2
type: enhancement
labels: [documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Agents offer hook or gate bypass as the (Recommended) option: forbid offering bypass options in failing-checks.md

Part of #<epic>. Priority: P2. Type: enhancement. Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic.

## Problem

Sometimes a gate or hook fails for a reason unrelated to the diff. In those cases agents offer a bypass (`--no-verify`, `core.hooksPath=/dev/null`, "waive this gate") as the **(Recommended)** AskUserQuestion option. Every recorded time, the user declined and asked for the flake to be fixed or isolated instead.

`failing-checks.md` already forbids self-granted waivers ("Never grant yourself a waiver … let the user decide"). It says nothing about which options an agent may offer, so agents obey the letter and steer the user toward the bypass.

## Evidence

Verbatim quotes from Shatter transcripts in `~/.claude/projects/-home-ketan-project-shatter/`, confirmed by the verifier:

- `87606e10`, 2026-09-19T22:39: the agent offered "Bypass this push's hook with --no-verify (Recommended)". The user chose "Keep retrying the push".
- `87606e10`, 2026-09-19T15:58: the agent offered a bypass. The user answered: "the tests shouldn't be using a globally visible directory like that. file an issue to get filesystem isolation then fix it."
- `e724dbd8`, 2026-09-07T23:09: the agent offered "File the bug now, proceed with --no-verify landings (Recommended)".
- `9f13ca23`, 2026-09-19T16:04: the agent proposed "Waive this gate and land". The user chose "Hold off, fix the flake first".

Current text, re-verified at dotfiles @ `81f35e1`: `docs/agent-guidance/failing-checks.md` has 26 lines. `grep -in "recommended\|bypass\|no-verify"` on it finds nothing. The waiver bullet is the last bullet.

## Acceptance criteria

- [ ] `docs/agent-guidance/failing-checks.md` states the rule. When a gate or hook fails for a reason unrelated to the diff, the agent offers only these options: isolate or fix the cause; file it and pause; keep retrying with evidence. The agent never proposes a bypass (`--no-verify`, a `core.hooksPath` override, a skip environment variable, a gate waiver) as an option, recommended or not.
- [ ] The rule stays consistent with the existing waiver bullet: the agent presents the evidence and the user decides. The agent does not add a bypass to the choices it presents.
- [ ] The user's "filesystem isolation" answer is quoted as the canonical example.
- [ ] No text added by this issue describes how to bypass a hook, anywhere (D4).
- [ ] If `docs/agent-guidance/core.md` (from `global-guidance-actually-loads`) is on `main` when this lands, this issue adds the rule's one-line summary to it. If not, that issue adds it.

## Proof at close

The closing comment includes:

- the new bullet as it reads in `failing-checks.md`;
- `grep -n -i "no-verify\|hooksPath" ~/dotfiles/docs/agent-guidance/failing-checks.md`, where every match is inside the prohibition;
- if `core.md` exists, the rule's summary line in the output of the SessionStart core-rules hook (and in `~/.codex/AGENTS.md` once `codex-render-composes-core-rules` has landed). `codex/agents-sync.sh status` alone is not proof: it does not look at leaf files.

## Out of scope

- Enforcing this at the hook level (blocking `--no-verify`). That is drafted in bento as `git-guard-bypasses-and-false-positives`.
- Slow hooks are fixed at their root cause, not bypassed. For example, the shatter beads hook stall is fixed by retiring the JSONL import (D4).

## Maintainer decisions that apply

- D4 (2026-09-23): no timeout environment variable and no hook-bypass guidance anywhere.
- D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Dependencies

None blocking. Related: `global-guidance-actually-loads`, `blocked-escalation`.

## Source

Shatter audit 2026-09-22 finding sessions-06.

---

<!-- FILE: 04-first-party-plugin-autoupdate.md -->

---
slug: first-party-plugin-autoupdate
kind: new
title: "Installed shatter/refute plugins are 27 commits stale: enable autoUpdate for the shatterproof marketplace"
priority: P2
type: bug
labels: [bug]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Installed shatter/refute plugins are 27 commits stale: enable autoUpdate for the shatterproof marketplace

Part of #<epic>. Priority: P2. Type: bug. The staleness checker that was part of this draft is split out as `first-party-plugin-staleness-check`. Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic.

## Problem

Every session runs the `shatter@shatterproof` and `refute@shatterproof` plugins at version 0.1.1 from June. The source repo is 27 commits ahead; it declares shatter `0.1.12` and refute `0.1.2`. The `shatterproof` marketplace has not refreshed since 2026-06-19. It is the only first-party marketplace without `"autoUpdate": true`.

## Evidence

Re-verified on 2026-09-23:

- `~/.claude/plugins/installed_plugins.json` lists both `shatter@shatterproof` and `refute@shatterproof` at version `0.1.1`, gitCommitSha `efa59682…`, lastUpdated `2026-06-19T16:19Z`.
- `/home/ketan/project/shatter-agents` @ `119b807`: `.claude-plugin/marketplace.json` and the plugins' `plugin.json` files declare shatter `0.1.12` and refute `0.1.2`.
- In `~/.claude/plugins/known_marketplaces.json`, `shatterproof` has lastUpdated `2026-06-19T16:19:59Z` and no autoUpdate. By contrast:
  - `bento` has autoUpdate `true` and lastUpdated 2026-09-23.
  - `claude-plugins-official` and `claude-code-plugins` have no autoUpdate but refreshed on 2026-09-23.
  - `typesafe-ai` has no autoUpdate and refreshed on 2026-09-21.
- The marketplace clone `~/.claude/plugins/marketplaces/shatterproof` is at `efa5968` (2026-06-18).
- The remote is reachable: `git -C ~/.claude/plugins/marketplaces/shatterproof ls-remote origin HEAD` returns `119b8074…`, matching shatter-agents HEAD. `git rev-list --count efa59682..HEAD` returns 27. A fetch failure is therefore unlikely, and the missing refresh is the probable cause.
- The setting's source is `~/dotfiles/claude/hosts/pontoon/settings.json:36-41`, where the `shatterproof` entry under `extraKnownMarketplaces` has no `autoUpdate`. The `bento` entry at lines 29-35 has it. `claude/settings-sync.sh` renders the host overlay into `~/.claude/settings.json`, with the snapshot in `claude/settings.rendered.json`.

## Acceptance criteria

- [ ] The `shatterproof` entry in `claude/hosts/pontoon/settings.json` has `"autoUpdate": true` (and in any other host overlay that declares it). Other marketplaces are unchanged; whether `typesafe-ai` should auto-update is left to the maintainer.
- [ ] `claude/settings-sync.sh status` reports the render as current, and `~/.claude/settings.json` contains `autoUpdate: true` for `shatterproof`.
- [ ] In a fresh session after the change, `installed_plugins.json` shows `shatter@shatterproof` at the version in shatter-agents' `plugins/claude/shatter/.claude-plugin/plugin.json` and `refute@shatterproof` at the version in `plugins/claude/refute/.claude-plugin/plugin.json`, both with the shatter-agents HEAD sha of that day. If they do not refresh, the closing comment records the diagnosis (for example, the marketplace fetch output) and the issue stays open.

## Proof at close

Paste both `installed_plugins.json` entries and the two `plugin.json` versions into the closing comment.

## Out of scope

- The staleness warning (`first-party-plugin-staleness-check`).
- Plugin content fixes. Current shatter-agents source still ships a `shatter-diff` skill for a command that does not exist. Under maintainer decision D2, the shatter-agents bucket withdraws that skill (`withdraw-shatter-diff-skill`). Refreshing before that lands exposes the skill for a while. The refresh should not wait for it.

## Maintainer decisions that apply

- D2: `shatter diff` is retired; the shatter-diff skill is withdrawn in shatter-agents.
- D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Dependencies

None blocking. Related: `first-party-plugin-staleness-check`, shatter-agents `withdraw-shatter-diff-skill`.

## Source

Shatter audit 2026-09-22 finding plugins-04.

---

<!-- FILE: 05-rtk-head-range-compound.md -->

---
slug: rtk-head-range-compound
kind: new
title: "rtk still replaces `head -N` output with a summary inside compound commands (follow-up to #11)"
priority: P2
type: bug
labels: [bug]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# rtk still replaces `head -N` output with a summary inside compound commands (follow-up to #11)

Part of #<epic>. Priority: P2. Type: bug. This is a follow-up to closed **#11** ("Narrow the rtk PreToolUse hook: skip find/pipelines/redirects/git ref reads; add a byte-identity smoke test").

## Problem

#11 added `claude/rtk_prefilter.py` (landed in `88e9cb5` on 2026-09-07). It passes redirects, `find` with `-not`/`-exec`, and git ref reads through unmodified. It does not cover exact-range reads whose output is shown to the agent. When `head -N` (and presumably `tail`, `sed -n` and similar reads) appears as a segment of a compound command, rtk still rewrites it. The agent then receives a token-compacted summary instead of the bytes it asked for, without being told.

## Reproduction

Re-reproduced on 2026-09-23, in a Claude Code session with the prefilter hook active. The hook is registered as `python3 $DOTFILES/claude/rtk_prefilter.py` at `~/dotfiles/claude/settings.json:285`.

```
wc -l ~/.claude/hooks/bento/require-worktree.sh; true; head -5 ~/.claude/hooks/bento/require-worktree.sh
```

Displayed output:

```
168
#!/usr/bin/env bash
# See hooks/references/hook-contract.md for the blocking contract
}
import json, os, sys
[164 more lines]
```

`/usr/bin/head -5` on the same file prints the true first five lines:

```
#!/usr/bin/env bash
# See hooks/references/hook-contract.md for the blocking contract
# (exit 2 = block, exit 1 = non-blocking failure, JSON decision shapes).
set -euo pipefail

```

It was reproduced twice on 2026-09-22 and once on 2026-09-23. The same summarising also happened to `head -20 claude/tests/run.sh` inside a `;` chain while this issue was being drafted.

## Acceptance criteria

- [ ] `claude/rtk_prefilter.py` passes these through unmodified, whether they are the whole command or any segment of a `;`, `&&`, `||` or `|` chain:
  - `head` or `tail` with an explicit `-n N` / `-N`;
  - `sed -n '<range>p'`;
  - `cat` of a named file.
- [ ] New cases in `claude/tests/test_rtk_prefilter.py`, run with `python3 -m pytest claude/tests/test_rtk_prefilter.py -q` (`claude/tests/run.sh` runs only `test_*.sh`), fail before the fix and pass after it. The closing comment includes both runs. The cases must cover at least:
  - `a; head -5 f; b`
  - `a && tail -n 3 f`
  - `sed -n '1,4p' f | cat`
  - `wc -l f; true; head -5 f` (the reproduction above)
- [ ] The reproduction command above prints the true first five lines in a live session. Paste the output into the closing comment.
- [ ] The closing comment says whether an upstream rtk report ("never replace requested ranges with summaries") is worth filing. The maintainer files it; the implementer does not.

## Suggested approach

`rtk_prefilter.py` has no shell tokenizer. `skip_reason()` (`:84-95`) runs word-bounded regexes over the whole raw command, deliberately unanchored to position so that chains, `$(...)` and multi-line scripts are covered (see the comment at `:66-82`). Follow that design: add one more whole-string regex for exact-range reads (`head`/`tail` with `-n N` or `-N`, `sed -n '...p'`, `cat <path>`) and return a skip reason when it matches. The prefilter then exits 0 with no output, so the command runs byte-identical, as it already does for its other skip cases. A false positive only costs rtk's token savings. Note that `|` pipelines are not currently a skip case (only `tee` is), so `sed -n '1,4p' f | cat` needs the new regex too.

## Out of scope

- The `find -not/-exec` case, which #11 already handles (`rtk_prefilter.py:50-91`).
- rtk's own rewrite logic in `~/.claude/hooks/rtk-rewrite.sh`, which the rtk tool owns.

## Maintainer decisions that apply

D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Dependencies

None blocking. Related: `hooks-dotfiles-env-unset`. The prefilter hook uses `$DOTFILES`, so in sessions where it is unset neither the prefilter nor rtk runs at all.

## Source

Shatter audit 2026-09-22 finding plugins-13. Prior issue: #11 (closed).

---

<!-- FILE: 06-memory-lifecycle-rule.md -->

---
slug: memory-lifecycle-rule
kind: new
title: "Agent memory stands in for a tracker and goes stale: add an issue-first, retire-on-close memory rule"
priority: P2
type: enhancement
labels: [documentation, enhancement]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Agent memory stands in for a tracker and goes stale: add an issue-first, retire-on-close memory rule

Part of #<epic>. Priority: P2. Type: enhancement. Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic.

## Problem

The global Self-Improvement Loop says only "update memory with the lesson". Nothing says a tooling or product bug belongs in the owning repo's tracker, and nothing says a memory must be retired when the underlying issue is fixed. Agent auto-memory therefore becomes a shadow tracker. Its entries outlive the bugs they describe, and later agents quote them as authority.

## Motivating example (partly cleaned up)

The Shatter project memory (`~/.claude/projects/-home-ketan-project-shatter/memory/`) shows the failure mode. State re-checked on 2026-09-23:

- `project_shatter_git_hook_test_corrupts_worktree.md` told agents to commit and push with `--no-verify` after the underlying bug (shatter str-jttrf) had been fixed. Agents quoted it verbatim to justify bypassing hooks (audit finding sessions-03). **Corrected on 2026-09-23**: it is now marked historical and no longer gives that advice.
- `project_shatter_gate_cache_and_bare_primary.md` (lines 3, 18 and 25) and its `MEMORY.md` index line **still** say the primary checkout has `core.bare = true` and tell agents not to run git there. `git -C /home/ketan/project/shatter config --show-origin core.bare` returns `file:.git/config false`. The audit's own prompt repeated the stale claim.
- `project_audit_2026_07_10_gate_state.md` **still** exists and is **still** missing from `MEMORY.md` (`grep -c 07_10 MEMORY.md` returns 0).

This issue is about the global rule that would have stopped the drift. Correcting the two remaining Shatter memories is the rule's first application and is part of closing it (see below).

## Evidence (current rule)

Re-verified on dotfiles @ `81f35e1`:

- `codex/AGENTS.md:57-62` "Self-Improvement Loop" says: "After corrections that reveal a recurring pattern, update memory with the lesson." It has no issue-first rule and no retirement rule.
- The memory audit toolchain was specified in #5, #6 and #7 (with #8 as packaging). All four were **closed as overscoped on 2026-09-07 without being built**: `claude/memory-index-audit.py` does not exist. No automated memory check exists today.
- dotfiles#23 caps Self-Improvement Loop at ≤ 12 words, so the full rule cannot live in `codex/AGENTS.md`.

## Acceptance criteria

- [ ] A leaf `~/dotfiles/docs/agent-guidance/memory.md` states these rules:
  1. A tooling or product bug goes to the owning repo's tracker first.
  2. Memory holds at most a one-line pointer with the issue ID and the date.
  3. When the issue closes, the memory is deleted or rewritten.
  4. A workaround memory, especially one containing bypass instructions such as `--no-verify` or `core.hooksPath`, must name the fixing issue and is removed when that issue closes.
- [ ] `memory.md` says where project facts belong: global per-project auto-memory or the repo's tracker memory (`bd remember`). It gives a one-line criterion for each.
- [ ] The Self-Improvement Loop in `codex/AGENTS.md` points to `memory.md` within #23's word budget.
- [ ] If `docs/agent-guidance/core.md` (from `global-guidance-actually-loads`) is on `main` when this lands, this issue adds the rule's one-line summary to it. If not, that issue adds it.
- [ ] A small check script (not the overscoped #5-#7 toolchain) scans `~/.claude/projects/*/memory/*.md`. It prints any file that:
  - contains `--no-verify` or `core.hooksPath` without naming an issue ID; or
  - is not linked from its directory's `MEMORY.md`.

  Its test, `claude/tests/test_memory_check.py` (run with `python3 -m pytest claude/tests/test_memory_check.py -q`), seeds one file of each kind plus one clean indexed file in a temp directory, and asserts that exactly the two bad files are reported.
- [ ] A real run of the check over `~/.claude/projects/*/memory/` reports `project_audit_2026_07_10_gate_state.md` as unindexed if that has not been fixed yet. After the fix, the stale `core.bare` claim is removed from `project_shatter_gate_cache_and_bare_primary.md` and its `MEMORY.md` line, and the 07_10 file is either indexed or deleted.

## Out of scope

- Editing Shatter memory files beyond the two still-stale ones named above.
- Checking whether a referenced issue is closed. That needs per-tracker access. If wanted, file it separately.

## Proof at close

The closing comment includes the pytest output, one real run of the check before and after the Shatter memory fixes, and the `memory.md` rule text.

## Maintainer decisions that apply

D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Dependencies

None blocking. Related: `global-guidance-actually-loads`, and dotfiles#23 (the word budget).

## Source

Shatter audit 2026-09-22 findings plugins-15 (P2; verifier: partially confirmed) and sessions-03.

---

<!-- FILE: 07-validators-fail-on-empty-extraction.md -->

---
slug: validators-fail-on-empty-extraction
kind: new
title: "Fail-closed guidance does not cover vacuous validators: require failure on empty extraction and a canary test"
priority: P3
type: enhancement
labels: [documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Fail-closed guidance does not cover vacuous validators: require failure on empty extraction and a canary test

Part of #<epic>. Priority: P3. Type: enhancement. Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic.

## Problem

`~/dotfiles/docs/code-writing-guidance/fail-closed-defaults.md` covers three cases: an uninstalled tool must fail, an empty allowlist means deny, and a missing expiry means reject. It says nothing about validators that pass vacuously, meaning they extract nothing and report green. Agents are told to make gates pass, but nothing tells them to prove that a gate can fail.

## Evidence

- dotfiles @ `81f35e1`: `fail-closed-defaults.md` is 6 lines long, and `grep -in "empty\|canary\|mutation"` matches only line 4, the allowlist and expiry rule. `validation-and-errors.md:6` only points back to it.
- In the Shatter repo, three separate controls stayed green for months while checking nothing:
  - `scripts/validate-protocol-registry.py`: TypeScript extraction returns empty sets, which are silently skipped.
  - Conformance `known_drifts` patterns are written as regexes but matched as substrings, so they can never match.
  - JSON schema checks only ever see hand-written fixtures.

  The shatter-side fixes are tracked in the shatter protocol-parity bucket.

## Acceptance criteria

- [ ] `fail-closed-defaults.md` (or `validation-and-errors.md`, with a pointer from the other) states these rules for any gate or validator that extracts facts from source:
  - It fails when an extraction that the gate's contract expects to be non-empty returns nothing (for example, a frontend that must declare message types yields zero).
  - Declarations whose contract says "this specific thing exists", such as known-drift entries, expected-failure entries or suppressions of a named current defect, are reported as stale when they no longer match anything. The gate fails or warns on stale entries, as the gate's own docs define.
  - Ordinary allowlist or permit entries are not required to match anything in a given repository or run. An entry that matches nothing is valid, and the rule must say so, so that implementers do not add false failures.
  - It carries a canary or mutation test showing that it goes red on a seeded defect.
- [ ] The text includes two short examples: a registry validator that finds 0 message types in a frontend fails, and its test deletes one handler and asserts the validator reports it; and an allowlist entry for a path absent from this repo is accepted without error.
- [ ] The existing rule "An empty allowlist means deny" (`fail-closed-defaults.md:4`) is left unchanged, and the new text does not contradict it.

## Proof at close

The closing comment quotes the new rule text and shows `grep -n "canary\|empty extraction\|stale" ~/dotfiles/docs/code-writing-guidance/*.md`.

## Maintainer decisions that apply

D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Out of scope

Fixing the Shatter validators themselves. That is tracked in the shatter protocol-parity bucket.

## Dependencies

None.

## Source

Shatter audit 2026-09-22 finding protocol-parity-21, in `areas/protocol-parity.md`.

---

<!-- FILE: 08-hooks-dotfiles-env-unset.md -->

---
slug: hooks-dotfiles-env-unset
kind: new
title: "Global hooks use $DOTFILES, which is unset in non-interactive sessions: 94 hook failures, skipped summaries, rtk prefilter not running"
priority: P3
type: bug
labels: [bug]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Global hooks use $DOTFILES, which is unset in non-interactive sessions: 94 hook failures, skipped summaries, rtk prefilter not running

Part of #<epic>. Priority: P3. Type: bug. Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic.

## Problem

Several hook commands in `~/dotfiles/claude/settings.json` start with `$DOTFILES/...`. `DOTFILES` is exported only by `bashrc:101`, which runs after the interactive-shell guard at `bashrc:3-4`. The settings `env` block does not set it. In teammate, remote and spawned sessions the variable is empty, and the hooks fail with `/bin/sh: 1: /claude/tmux-state.sh: not found`. Because those errors are non-blocking, neither the user nor the agent sees them.

## Evidence

Re-verified at dotfiles @ `81f35e1`, `claude/settings.json`:

- `:285` PreToolUse (Bash): `python3 $DOTFILES/claude/rtk_prefilter.py`. The source finding missed this one. When `DOTFILES` is unset the prefilter fails, so the #11 byte-identity protection and the rtk rewrite both silently do not run.
- `:334` SessionStart: `cat | $DOTFILES/claude/tmux-state.sh session-start`.
- `:362` Stop: `$DOTFILES/claude/tmux-state.sh stop`, `$DOTFILES/claude/generate_summary.py` and `$DOTFILES/claude/generate_session_name.py`. Session summaries and names silently do nothing.
- `:373` PermissionRequest and `:384` UserPromptSubmit: `$DOTFILES/claude/tmux-state.sh`.
- `:33-35` `env` sets only `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS`.
- In Shatter transcripts there are 94 `hook_non_blocking_error` attachments across 47 sessions (36 across 18 sessions since 2026-09-04). 23 transcript files contain `tmux-state.sh: not found`.

## Acceptance criteria

- [ ] No hook command in `claude/settings.json` or in any host overlay under `claude/hosts/` depends on `$DOTFILES`. The commands use `$HOME/dotfiles/...` (preferred, because it does not depend on `env` expansion order), or `DOTFILES` is set in the settings `env` block. `grep -n '\$DOTFILES' claude/settings.json claude/hosts/*/settings.json` returns nothing, or matches only the `env` definition.
- [ ] Test `claude/tests/test_hook_paths.py`, run with `python3 -m pytest claude/tests/test_hook_paths.py -q` (`claude/tests/run.sh` runs only `test_*.sh`). It is isolated and exercises only the affected hooks:
  - It builds a temp `HOME` containing `dotfiles/claude/` with **stub** versions of `tmux-state.sh`, `rtk_prefilter.py`, `generate_summary.py` and `generate_session_name.py`. Each stub appends its own name and arguments to a log file and exits 0. No real hook, `tmux`, `bd`, `rtk` or ollama call runs.
  - For each affected hook command (PreToolUse Bash, SessionStart `tmux-state.sh`, Stop, PermissionRequest, UserPromptSubmit), taken from the rendered settings, it runs `env -i HOME=<tmp> PATH=/usr/bin:/bin sh -c '<command>'` with a representative event payload on stdin. The Stop payload includes a `transcript_path` pointing at a temp file, so the summary and session-name branches run.
  - It asserts that every stub the command should reach appears in the log. Exit codes alone are not enough: the Stop command ends in `; true`, and its Python scripts run in the background.
  - It waits for background stubs, with a bounded timeout, before reading the log.
  - Unrelated SessionStart commands (`bd prime`, `ensure-dolt.sh`, `tmc/agent-track`) are not run.
- [ ] The test fails on the current tree (the stubs are never reached, because `$DOTFILES` expands to empty) and passes after the change.
- [ ] Optional: a SessionStart self-check warns once when any hook script path does not resolve.

## Proof at close

The closing comment includes the test's red and green output, and one fresh non-interactive session (for example a spawned teammate) whose transcript has no `hook_non_blocking_error` for these hooks.

## Maintainer decisions that apply

D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Out of scope

Bento's agent-env doctor (bento-m4y5) checking global hook paths. It could be extended later, but the root fix belongs here.

## Dependencies

None. New hooks from `background-wait-rule-and-hook`, `global-guidance-actually-loads` and `first-party-plugin-staleness-check` should use `$HOME/dotfiles` paths from the start.

## Source

Shatter audit 2026-09-22 finding sessions-14.

---

<!-- FILE: 09-falsification-probe-first.md -->

---
slug: falsification-probe-first
kind: new
title: "Planning guidance: experiment and benchmark plans must start with a falsification or upper-bound probe"
priority: P3
type: enhancement
labels: [documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Planning guidance: experiment and benchmark plans must start with a falsification or upper-bound probe

Part of #<epic>. Priority: P3. The verifier lowered this from P2: it is a process preference, not a defect, and the negative result it produced had value. Type: enhancement.

## Problem

`~/dotfiles/docs/agent-guidance/planning.md` is 9 lines long. It covers plan-then-approve, re-planning and verification steps. It says nothing about experiments. As a result, an agent can plan and land new public infrastructure for an experiment before it has checked that the experiment can show any effect.

## Evidence

Shatter session `c1689435-5dae-4c52-8fc4-930ec4e87246`, from 2026-09-21 20:19 to 2026-09-22 02:14:

- The session went from brainstorming to writing-plans to executing-plans. It then created and landed a 4-issue epic, str-hjrnp.1 to .4: a FrontierRanker trait, a DecisionOracle plus Jev adapter in shatter-llm, a bench runner and a report script. That is 11 commits on main in `4c4aca4e..85a08ddd`; the verifier counted 11, not the 16 the finding claimed.
- Afterwards the agent concluded: "even a ranker that knows the answer can't beat the heuristic at the current hook points".
- About the 60-execution budget, the agent said: "It's a number I chose, and it's arguably the weakest part of the benchmark… Ranking only influences drilling and bounded unroll, which fire when a frontier has stalled".
- The user asked: "er... explain further why jev cannot improve on the exploration frontier."

A cheap upper-bound run with a scripted perfect ranker on the existing code would have shown the ceiling before a new public trait and a crate dependency landed on main.

## Acceptance criteria

- [ ] `docs/agent-guidance/planning.md` states two rules for plans whose goal is an experiment, benchmark or "does X improve Y" question:
  - Task 1 is a falsification or upper-bound probe on the existing code, for example an oracle, a scripted perfect choice, or an ablation.
  - New public traits, new dependencies and landed infrastructure wait until the probe shows a measurable effect.
- [ ] It says that experiment code stays on an experiment branch or a bento expedition until the probe passes, and that a negative probe result is reported as the outcome.
- [ ] Proof at close: the closing comment shows the output of `grep -n "falsification\|upper-bound" ~/dotfiles/docs/agent-guidance/planning.md`.

## Maintainer decisions that apply

D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Out of scope

Re-evaluating the landed str-hjrnp work in shatter.

## Dependencies

None.

## Source

Shatter audit 2026-09-22 finding sessions-12 (verifier: partially confirmed, lowered to P3).

---

<!-- FILE: 10-blocked-escalation.md -->

---
slug: blocked-escalation
kind: new
title: "Sessions stall for hours on unseen questions and classifier denials: add escalation guidance"
priority: P3
type: enhancement
labels: [documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Sessions stall for hours on unseen questions and classifier denials: add escalation guidance

Part of #<epic>. Priority: P3. Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic. The verifier lowered this from P2 because part of the root cause is harness UI, outside first-party control. Type: enhancement.

## Problem

Two situations stall sessions for many hours: a question the user never sees, and an auto-mode classifier denial. In both, the agent waits silently. `~/dotfiles/docs/agent-guidance/questions.md` (7 lines) covers how often to ask and how to number questions. It says nothing about how to make a blocking question visible, or what a classifier denial means.

## Evidence

Shatter transcripts in `~/.claude/projects/-home-ketan-project-shatter/`:

- `455c2cd7-3b18-4803-a9ec-2e1e543c18c1`: the auto-mode classifier denied `git push --no-verify origin 7fbfa7ab:refs/heads/main` at 2026-09-08T13:34. The session then idled until the user wrote "I don't see a question--maybe it got pushed back" at 2026-09-09T15:44. The verifier confirmed this quote.
- `5617a8ed`: an AskUserQuestion at 2026-09-09T16:43 got an empty result. The next action came at 2026-09-10T18:57, after the user typed "try again". The verifier did not re-check these timestamps.
- Since 2026-09-04 there have been 14 classifier denials across 9 sessions. The verifier did not recount this.
- Earlier, the user said: "don't wait 21 hours.  wait 15 minutes." (`5f2377ef`, 2026-08-29).
- Re-verified at dotfiles @ `81f35e1`: `grep -in "PushNotification\|classifier\|blocked" docs/agent-guidance/questions.md` finds nothing.

## Acceptance criteria

- [ ] `docs/agent-guidance/questions.md` says what to do when the work is blocked on the user. Send a PushNotification (where available) with the one-line question, and restate the question in plain text at the end of the turn, not only inside AskUserQuestion.
- [ ] It says what an auto-mode classifier denial means. The action, as attempted, is not authorized. The agent:
  - stops that action and tells the user in plain text what was denied and why it was attempted, then asks how to proceed (with a PushNotification where available), instead of waiting silently;
  - may continue through a different path only when that path is already authorized for this task, for example the user already asked to land through the bento land-work flow and the denied command was a manual push that the flow replaces;
  - never uses another tool or command to get the same effect the denial blocked;
  - never retries with a bypass (`--no-verify`, a hooks-path override), consistent with `never-recommend-bypass` and D4.
- [ ] The text uses the `455c2cd7` case as its example: a denied `git push --no-verify … :refs/heads/main` was an unauthorized action that needed the user's decision, not a prompt to find another way to push.
- [ ] If `docs/agent-guidance/core.md` (from `global-guidance-actually-loads`) is on `main` when this lands, this issue adds the rule's one-line summary to it. If not, that issue adds it.

## Proof at close

The closing comment quotes the new `questions.md` text, and, if `core.md` exists, shows the summary line in the output of the SessionStart core-rules hook (and in `~/.codex/AGENTS.md` once `codex-render-composes-core-rules` has landed). `codex/agents-sync.sh status` alone is not proof: it does not look at leaf files.

## Maintainer decisions that apply

- D4 (2026-09-23): no hook-bypass guidance anywhere.
- D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Out of scope

Harness UI bugs where a question is not displayed.

## Dependencies

None blocking. Related: `global-guidance-actually-loads` and `never-recommend-bypass`.

## Source

Shatter audit 2026-09-22 finding sessions-13 (verifier: partially confirmed, lowered to P3).

---

<!-- FILE: 11-tool-precedence-vs-harness-mode.md -->

---
slug: tool-precedence-vs-harness-mode
kind: new
title: "Tool-Specific Notes never say the Read/Grep-first rule overrides the harness's bypass-mode Bash guidance (missed #10 acceptance check)"
priority: P3
type: enhancement
labels: [documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Tool-Specific Notes never say the Read/Grep-first rule overrides the harness's bypass-mode Bash guidance (missed #10 acceptance check)

Part of #<epic>. Priority: P3. Type: enhancement. Follow-up to closed **#10** ("Reconcile subagent, tools-vs-Bash, and RTK guidance into one paragraph"). Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic.

## Problem

Closed #10 had this acceptance check: the "How to act" section "states explicitly that this rule wins over harness auto-mode prompts that prefer Bash for reads, because it is user instruction (which the harness ranks above its defaults)". #10 was closed on 2026-09-07 as landed (`af7864c`). The current text does not contain that statement.

The two sources do not strictly contradict each other: the harness permits shell reads, and the user rule prefers dedicated tools. But the harness text is specific and recent in context, the user rule does not say it takes precedence, and agents follow the harness. #10 already decided the precedence; this issue only lands the missing sentence.

## Evidence

- `codex/AGENTS.md:131-138` (dotfiles @ `81f35e1`), Tool-Specific Notes: "prefer `Read`/`Grep`/`Glob` over Bash `cat`/`grep`/`find` pipelines for anything those tools cover natively" and "dedicated harness tools (`Read`, `Grep`, `Glob`, `Edit`) still take precedence over any shell equivalent". `grep -n -i "auto-mode\|auto mode\|bypass" codex/AGENTS.md` finds nothing. The only "harness" match (`:137`) refers to the tools, not to harness mode text.
- The harness's bypass-permissions text, as it appears in a Claude Code session's system context on 2026-09-23: "While bypass permissions mode is active: You can do much of your work through the Bash tool when it is the simpler route: read files with cat, head, or sed -n, search with grep and find … The choice is yours".
- Usage, indicative only (the verifier did not recount, and these counts do not show that each shell call could have used a dedicated tool): in Shatter transcripts since 2026-09-04, 655 Bash calls led by grep, cat, sed, find or head across 34 sessions, against Read 379, Grep 128 and Glob 11. #10 itself measured 886 non-piped `grep/cat/sed/find` calls where Read/Grep/Glob fit.
- The rtk `find -not/-exec` rejections cited by the source finding all predate #11 (`88e9cb5`, 2026-09-07) and are excluded. The live exact-range problem (`head`/`sed -n` summarised inside compound commands) is tracked in `rtk-head-range-compound`.

## Acceptance criteria

- [ ] Tool-Specific Notes (or its successor after #23's rewrite) states that the Read/Grep/Glob preference is user instruction and takes precedence over harness mode text that permits or encourages shell reads, including bypass-permissions mode. This implements #10's acceptance check and does not change the rule itself.
- [ ] The wording fits dotfiles#23's ≤ 150-word target for Tool-Specific Notes. If #23 has landed, `test/global-instructions-budget-test.sh` still passes.
- [ ] Relaxing the rule (for example "plain shell is acceptable for one-off reads") is **not** part of this issue. It would reverse #10's decision and needs a separate maintainer decision.

## Proof at close

The closing comment shows the new text, `wc -w codex/AGENTS.md`, and `grep -n -i "bypass\|harness mode" ~/.codex/AGENTS.md` finding the sentence in the rendered Codex file (not only `agents-sync.sh status`).

## Out of scope

- rtk `find` handling (fixed in #11).
- The shatter-local Tool-rules block (str-qwua7.26).
- Hook enforcement of tool choice.

## Maintainer decisions that apply

D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Dependencies

None blocking. Coordinate with dotfiles#23, which rewrites the same section under a word budget. Whichever lands second rebases onto the other. Related: `rtk-head-range-compound`.

## Source

Shatter audit 2026-09-22 finding sessions-16 (verifier: partially confirmed, P3). Reframed during the Codex cross-check of 2026-09-23.

---

<!-- FILE: 12-first-party-plugin-staleness-check.md -->

---
slug: first-party-plugin-staleness-check
kind: new
title: "Warn at SessionStart when an installed first-party plugin is behind its local source checkout"
priority: P2
type: enhancement
labels: [enhancement]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Warn at SessionStart when an installed first-party plugin is behind its local source checkout

Part of #<epic>. Priority: P2. Type: enhancement. Split from `first-party-plugin-autoupdate`, which fixes the missing `autoUpdate` setting. This issue adds the check that catches the next failure of any kind, including a refresh that silently fails. Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic.

## Problem

The shatter and refute plugins ran 27 commits and three months stale (0.1.1 installed; source declares shatter 0.1.12 and refute 0.1.2) and nothing warned. `autoUpdate` fixes one cause. It does not detect a refresh that fails, a marketplace that is removed from the settings, or a version that is never bumped.

## Evidence

See `first-party-plugin-autoupdate` for the installed and source versions, re-verified on 2026-09-23. No script in dotfiles @ `81f35e1` compares installed plugins with their sources. bento's agent-env doctor (bento-m4y5, `in_progress`, P1, checked with `bd show` on 2026-09-23) is about broken agent wiring, not plugin freshness.

## Acceptance criteria

- [ ] A dotfiles script, for example `claude/plugin_staleness.py`, is registered as a SessionStart hook with a `$HOME/dotfiles/...` path. For each enabled plugin in `~/.claude/plugins/installed_plugins.json` whose marketplace repo is also checked out under `~/project/<repo>`, it compares the installed `gitCommitSha` and `version` with the checkout's `HEAD` and that plugin's `plugin.json` version. It prints one warning line per plugin that is behind, naming the plugin, both versions and the commit count.
- [ ] It prints nothing and exits 0 when every plugin is current, when no local checkout exists, or when a file is missing. It must never block a session.
- [ ] The mapping from marketplace to local checkout is explicit (a small table in the script, or the marketplace's `source` URL matched against checkout remotes), not guessed from names.
- [ ] Test `claude/tests/test_plugin_staleness.py`, run with `python3 -m pytest claude/tests/test_plugin_staleness.py -q`, uses a temp `HOME` with a fixture `installed_plugins.json` and a temp git repo as the checkout:
  - an installed sha behind `HEAD` produces the warning;
  - an installed sha equal to `HEAD` produces no output;
  - a missing checkout produces no output and exit 0.

## Proof at close

The closing comment includes the pytest output, and the script's output against the real `~/.claude` before `first-party-plugin-autoupdate` lands (a warning) or, if it has already landed, after seeding a fixture (a warning) and against the real state (silent).

## Out of scope

- The `autoUpdate` setting (`first-party-plugin-autoupdate`).
- Extending bento's agent-env doctor (bento-m4y5). It could adopt this check later.

## Maintainer decisions that apply

D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Dependencies

None blocking. Related: `first-party-plugin-autoupdate`, `hooks-dotfiles-env-unset` (use `$HOME/dotfiles` paths).

## Source

Shatter audit 2026-09-22 finding plugins-04. Split out during the Codex cross-check of 2026-09-23.

---

<!-- FILE: 13-codex-render-composes-core-rules.md -->

---
slug: codex-render-composes-core-rules
kind: new
title: "Deliver the core-rules file to Codex: make agents-sync.sh render, status and adopt composition-aware"
priority: P1
type: bug
labels: [bug]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: [global-guidance-actually-loads]
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Deliver the core-rules file to Codex: make agents-sync.sh render, status and adopt composition-aware

Part of #<epic>. Priority: P1. Type: bug. Split from `global-guidance-actually-loads`, which creates `docs/agent-guidance/core.md` and injects it into Claude Code sessions. This issue is blocked by it.

## Problem

Codex sessions read only `~/.codex/AGENTS.md`, which `codex/agents-sync.sh render` copies from `codex/AGENTS.md`. The Required-Loads leaves are prose pointers there too, so Codex sessions get the same ~0% load rate as Claude sessions (see `global-guidance-actually-loads`). Codex has no SessionStart hook, so the core rules have to be in the rendered file.

The render/adopt contract cannot just append a file. At dotfiles @ `81f35e1`:

- `cmd_render` (`codex/agents-sync.sh:42-63`) copies `BASE` to both `TARGET` and `SNAPSHOT`.
- `cmd_status` (`:65-89`) reports `render vs snapshot: pending` whenever `SNAPSHOT` differs from `BASE`. If render appended `core.md`, status would report pending forever.
- `cmd_adopt` (`:91-114`) copies the whole live `TARGET` back into `BASE`. After a composed render it would copy `core.md`'s content into `codex/AGENTS.md`, and the next render would append it a second time.

Also, `render vs snapshot: ok` compares only `BASE` and `SNAPSHOT`. It says nothing about the referenced leaves, and it can be `ok` while `live vs snapshot` is `drifted`. It is not proof that a rule reached Codex.

## Acceptance criteria

- [ ] `render` writes `BASE` followed by a delimited block (begin and end marker lines) containing `core.md` to `TARGET`, and writes the same composed output to `SNAPSHOT`.
- [ ] `status` compares `SNAPSHOT` with the composed output that render would produce now (`BASE` plus the current `core.md`). A change to `core.md` alone reports `pending`.
- [ ] `adopt` strips the delimited core block from the live file before writing `BASE`. Live edits inside the block are reported and not written into `BASE` (the fix belongs in `core.md`).
- [ ] Alternatively, if the maintainer prefers that `core.md`'s rules be written directly into `codex/AGENTS.md` within dotfiles#23's 800-word budget, the closing comment records that choice, and the content criterion below still applies.
- [ ] Round-trip tests in a new `test/agents-sync-compose-test.sh`, run with `bash test/agents-sync-compose-test.sh` (the `test/*-test.sh` scripts are standalone; there is no shared runner). Each case sets `HOME` to a temp directory:
  - render, then adopt, leaves `BASE` byte-identical;
  - render, adopt and render again gives exactly one core block in `TARGET`;
  - editing `core.md` makes `status` exit 1 with `pending`;
  - a live edit outside the block is adopted into `BASE`, and a live edit inside it is not.

  The tests must fail on the current script.
- [ ] Content check: after `render`, `~/.codex/AGENTS.md` contains the core marker line and every `##` heading of `core.md`.

## Proof at close

The closing comment includes the red and green test runs, `grep -c '<core marker>' ~/.codex/AGENTS.md` returning 1, and the marker found in a Codex session started after the render: `grep -l '<core marker>' ~/.codex/sessions/<yyyy>/<mm>/<dd>/rollout-*.jsonl` (rollout files record the loaded instructions).

## Out of scope

- Automating or surfacing the render after guidance changes (dotfiles#21).
- The contents of `core.md` (`global-guidance-actually-loads`).

## Dependencies

Blocked by `global-guidance-actually-loads` (creates `core.md`). Coordinate with dotfiles#21 and dotfiles#23, which touch the same render path and file.

## Source

Shatter audit 2026-09-22, finding plugins-06. Split out during the Codex cross-check of 2026-09-23.
