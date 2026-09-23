# Bundle: dotfiles-global-guidance (audit 2026-09-22)

- **Bucket:** dotfiles-global-guidance. The theme is global agent guidance and hooks: loading the required rules, waiting behaviour, framing of bypass options, plugin auto-update, rtk, the memory lifecycle, validators and escalation.
- **Repo / tracker:** dotfiles, via `gh -R ketang/dotfiles` (GitHub Issues; the repo has no `.beads`). GitHub has no priority field, so each body states its priority in the text. The only labels available are the GitHub defaults (bug, documentation, enhancement, ...).
- **Parent epic:** "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)". Each body starts with "Part of #<epic>", and the filer substitutes the epic's number.
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
- **D4:** `never-recommend-bypass` and `blocked-escalation` never recommend a bypass. Nothing in this bucket proposes hook-bypass guidance.
- **D2:** `first-party-plugin-autoupdate` notes that the refreshed plugin will temporarily ship the `shatter-diff` skill until shatter-agents `withdraw-shatter-diff-skill` lands.
- D1, D3, D5 and D6 do not change any draft in this bucket, apart from D6's "nothing filed".

## Changes from the old drafts (other-first-party/01-11), found during re-verification

1. **Overlap with open dotfiles issues #18, #20, #21 and #23, all filed on 2026-09-23 from the zolem review.**
   - #20 owns the absolute-path fix, so it was removed from `global-guidance-actually-loads`.
   - #18 inlines a Definition of Done that includes the wiring rule, so it is not duplicated.
   - #23 caps `codex/AGENTS.md` at 800 words: Self-Improvement Loop ≤ 12 words, Tool-Specific Notes ≤ 150. So the rules now live in leaves plus a SessionStart-injected core file, not inline.
   - #21 covers the pending Codex render, so the closing proofs check `agents-sync.sh status`.
2. **Closed #4-#8 were closed as overscoped on 2026-09-07 and never built.** No `session-lint.py` or `memory-index-audit.py` exists. The acceptance criteria that said "extend the #4 / #5-#7 tooling" became small, standalone scripts.
3. **`hooks-dotfiles-env-unset`:** the PreToolUse `rtk_prefilter.py` hook (`settings.json:285`) also uses `$DOTFILES`. When the variable is unset, neither the #11 protection nor rtk runs.
4. **`first-party-plugin-autoupdate`:** the shatterproof remote is reachable (`ls-remote` returns 119b807), and marketplaces without autoUpdate refreshed today. So the missing autoUpdate is the likely cause. The setting's source is `claude/hosts/pontoon/settings.json:36-41`.
5. **`tool-precedence-vs-harness-mode`:** every rtk `find -not/-exec` rejection predates #11 (2026-08-27 to 09-07), so that acceptance criterion was dropped.
6. **`rtk-head-range-compound`:** re-reproduced on 2026-09-23.
7. **`memory-lifecycle-rule`:** the Shatter memory files were corrected on 2026-09-23. They remain as the motivating example only.
8. **Priorities follow the verifier:** background-wait P2, falsification-probe-first P3, blocked-escalation P3.

## Contents

| # | Slug | Title | P | Type | Blocked by |
|---|---|---|---|---|---|
| 01 | global-guidance-actually-loads | Global Required-Loads guidance almost never loads: deliver a short core-rules set to every Claude and Codex session | P1 | bug | - |
| 02 | background-wait-rule-and-hook | Agents busy-wait on background work with sleep-1/echo and ReadNotifications loops: add a waiting rule and a PreToolUse guard | P2 | enhancement | - |
| 03 | never-recommend-bypass | Agents mark hook or gate bypass as the (Recommended) option: add a framing rule to failing-checks.md | P2 | enhancement | - |
| 04 | first-party-plugin-autoupdate | Installed shatter/refute plugins are 27 commits stale: enable autoUpdate for first-party marketplaces and add a staleness check | P2 | bug | - |
| 05 | rtk-head-range-compound | rtk still replaces `head -N` output with a summary inside compound commands (follow-up to #11) | P2 | bug | - |
| 06 | memory-lifecycle-rule | Agent memory stands in for a tracker and goes stale: add an issue-first, retire-on-close memory rule | P2 | enhancement | - |
| 07 | validators-fail-on-empty-extraction | Fail-closed guidance does not cover vacuous validators: require failure on empty extraction and a canary test | P3 | enhancement | - |
| 08 | hooks-dotfiles-env-unset | Global hooks use $DOTFILES, which is unset in non-interactive sessions: 94 hook failures, skipped summaries, rtk prefilter not running | P3 | bug | - |
| 09 | falsification-probe-first | Planning guidance: experiment and benchmark plans must start with a falsification or upper-bound probe | P3 | enhancement | - |
| 10 | blocked-escalation | Sessions stall for hours on unseen questions and classifier denials: add escalation guidance | P3 | enhancement | - |
| 11 | tool-precedence-vs-harness-mode | Global Read/Grep-first rule contradicts the harness bypass-mode text: state one rule that acknowledges both | P3 | enhancement | - |


---

<!-- FILE: 01-global-guidance-actually-loads.md -->

---
slug: global-guidance-actually-loads
kind: new
title: "Global Required-Loads guidance almost never loads: deliver a short core-rules set to every Claude and Codex session"
priority: P1
type: bug
labels: [bug, documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Global Required-Loads guidance almost never loads: deliver a short core-rules set to every Claude and Codex session

Part of #<epic>. Priority: P1. Type: bug.

## Problem

The only auto-loaded global file is `codex/AGENTS.md`. `~/.claude/CLAUDE.md` imports it with `@../codex/AGENTS.md`, and Codex reads the rendered copy `~/.codex/AGENTS.md`. At line 16 it says "Follow the shared agent guidance in `~/dotfiles/docs/agent-guidance.md`". That line is a plain-text pointer, not an `@`-import. The index it points to lists four "Required Loads" (branches-and-worktrees, read-before-designing, verification, and instruction-integrity, which is required "every session") plus about 15 conditional loads. All of them are prose pointers too, and agents almost never follow them.

The unloaded rules include drift-checks, failing-checks, instruction-integrity and cross-repository-boundaries. Their absence lines up with several 2026-09-22 Shatter audit findings: plugin skills that document CLI commands which do not exist, a plugin cache three months stale, and agents offering gate bypasses. The verifier notes that the causal link is inferred, not demonstrated.

## Evidence

Re-verified on 2026-09-23 against dotfiles `main` @ `81f35e1`.

- `codex/AGENTS.md:16` has a plain-text pointer to `~/dotfiles/docs/agent-guidance.md` and no `@`.
- `docs/agent-guidance.md:8-50` lists every leaf as a prose pointer, and none of them is imported.
- Load-rate scan of the Shatter project transcripts (`~/.claude/projects/-home-ketan-project-shatter/*.jsonl` and `*/subagents/*.jsonl`). It counts tool_use inputs whose `file_path` or command references `agent-guidance/` or `code-writing-guidance`:
  - Top-level sessions: 0/87 with the narrow pattern, 4/87 with a broad pattern that also counts the index file and Bash mentions.
  - Subagent transcripts: 1/166 (narrow) and 6/166 (broad).
  - Either way the load rate is close to zero.
- `wc -w codex/AGENTS.md` returns 1146.
- The hook-provenance sentence at `codex/AGENTS.md:23` says the worktree hook is "registered from `~/project/bento`". In fact `readlink -f ~/.claude/hooks/bento/require-worktree.sh` resolves to `~/.claude/plugins/cache/bento/bento/2.3.84/hooks/scripts/require-worktree.sh`, which is the installed plugin cache.

## Related open work (coordinate; do not duplicate)

- **dotfiles#18** puts a Definition of Done in `codex/AGENTS.md`. It already covers the wiring-and-consumption rule and a pointer to drift-checks. This issue must not restate those.
- **dotfiles#20** converts guidance cross-references to absolute `~/dotfiles/docs/...` paths. That work is owned there and is **removed from this issue**. It had been part of this issue's source finding, plugins-19.
- **dotfiles#23** caps `codex/AGENTS.md` at 800 words (Branches And Worktrees ≤ 30 words, Self-Improvement Loop ≤ 12 words). Inlining 6–8 rules into `codex/AGENTS.md` would break that budget, so the delivery mechanism below avoids growing that file.
- **dotfiles#21** covers the pending Codex render after guidance changes. Any change that must reach Codex depends on a render.
- Closed #4 (Claude JSONL session lint) was **closed as overscoped on 2026-09-07 and never built**. No `claude/session-lint.py` exists. Do not assume that tooling.

## Acceptance criteria

- [ ] A single core-rules file exists, for example `~/dotfiles/docs/agent-guidance/core.md` with at most ~350 words. It holds one or two lines per rule, each linking its leaf. At minimum it covers verification essentials, drift-checks, instruction-integrity, failing-checks (including the bypass-framing rule from `never-recommend-bypass`), cross-repository boundaries, and waiting for background work (from `background-wait-rule-and-hook`). The set must not duplicate the Definition of Done from #18.
- [ ] Every Claude Code session receives the core file with no action by the agent. The recommended way is a SessionStart hook in `claude/settings.json` that prints it as additional context, using a `$HOME/dotfiles/...` path and not `$DOTFILES` (see `hooks-dotfiles-env-unset`). If the maintainer prefers to inline the rules in `codex/AGENTS.md`, they must fit #23's 800-word budget.
- [ ] The issue states how Codex sessions receive the same rules, for example by appending the file during `codex/agents-sync.sh render`. After the change, `codex/agents-sync.sh status` reports `render vs snapshot: ok`.
- [ ] The hook-provenance note in `codex/AGENTS.md` (in whatever form survives #23) names the installed bento plugin cache instead of `~/project/bento`.
- [ ] Proof at close: a test under `claude/tests/` runs the SessionStart hook command and asserts that its output contains every core-rule heading. The closing comment includes the load-rate scan (the same patterns as above) re-run over sessions started after the change, and the transcript of one new session showing the injected context.

## Suggested approach

1. Write `core.md` by condensing the leaves. Keep each bullet imperative and short, and link the leaf for depth.
2. Add a SessionStart hook entry such as `cat "$HOME/dotfiles/docs/agent-guidance/core.md"`. Output from a SessionStart hook is added to context.
3. Change `docs/agent-guidance.md` so its "Required Loads" section says the core set is already loaded and the leaves are optional depth.
4. Measure again after about a week of sessions, using a throwaway scan script. Do not build the #4 toolchain.

## Out of scope

- Rewriting the leaf content.
- Absolute paths (#20), the Definition of Done (#18), the word budget itself (#23) and Codex render automation (#21).

## Dependencies

None blocking. `never-recommend-bypass`, `background-wait-rule-and-hook`, `blocked-escalation` and `memory-lifecycle-rule` each write a leaf rule, and this issue's core file summarises them. Whichever lands second adds the one-line summary.

## Source

Shatter audit 2026-09-22, findings plugins-06 (P1) and plugins-19 (P3; its path part is now owned by #20). The evidence is in `audits/2026-09-22/areas/plugins-guidance.md` on the shatter `audit-2026-09-22` branch.


---

<!-- FILE: 02-background-wait-rule-and-hook.md -->

---
slug: background-wait-rule-and-hook
kind: new
title: "Agents busy-wait on background work with sleep-1/echo and ReadNotifications loops: add a waiting rule and a PreToolUse guard"
priority: P2
type: enhancement
labels: [enhancement, documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Agents busy-wait on background work with sleep-1/echo and ReadNotifications loops: add a waiting rule and a PreToolUse guard

Part of #<epic>. Priority: P2 (the verifier lowered it from P1: it wastes turns and tokens but does not produce incorrect repository state). Type: enhancement.

## Problem

After starting background work, agents busy-wait instead of ending the turn. They loop over `sleep 1; echo ok` and repeated `ReadNotifications` calls, burning hundreds of turns and a very large number of cache-read tokens. The harness blocks long foreground sleeps, and its block text says "Do not chain shorter sleeps to work around this block." Agents chain short sleeps anyway. No global instruction says that ending the turn is the correct way to wait.

## Evidence

Shatter transcripts in `~/.claude/projects/-home-ketan-project-shatter/`, counted since 2026-09-04:

- 820 `sleep N[; echo x]` calls across 12 sessions, and 600 ReadNotifications polls.
- Session `87606e10-4dce-47bb-a273-768abd7be0a2` (claude-sonnet-5, 2026-09-19 to 09-21):
  - 1,575 tool calls. 609 were bare `sleep 1; echo ok` (description "No-op wait for code review notification") and 598 were ReadNotifications. The verifier recounted these.
  - 541 assistant turns begin "Still waiting".
  - 934,138,123 cache-read tokens.
- Session `9f13ca23-bf49-4efb-abd0-ed3519e0dd38` made 200 `sleep 1` calls with the description "yield".
- The agent writes "I'll wait for the background task notification instead of polling." and then keeps polling.
- Contributing text: shatter `AGENTS.md:405-409` ("Team-lead liveness") tells the lead to "actively poll teammate liveness … never idle until the user notices". Re-verified at shatter `56c86168`.
- There is no waiting rule anywhere in `~/dotfiles/docs/agent-guidance/` (dotfiles @ `81f35e1`).

## Acceptance criteria

- [ ] A leaf `~/dotfiles/docs/agent-guidance/waiting.md` states the rule. After a `run_in_background` launch, or while waiting on a teammate or a notification, do one of these:
  - end the turn;
  - use a blocking wait (TaskOutput with block, or a Monitor until-loop);
  - run one bounded background until-loop.

  Never use `sleep` ≤ 5 s or `sleep … ; echo …` loops. Never make more than 3 consecutive ReadNotifications calls with no other tool call in between.
- [ ] The rule's one-line summary is in the core-rules file from `global-guidance-actually-loads`. It is not added to `codex/AGENTS.md`, because dotfiles#23 caps that file's word count.
- [ ] A PreToolUse hook, registered in `claude/settings.json` with a `$HOME/dotfiles/...` path, denies two things, each with a message that names `waiting.md`:
  - a Bash command that is only `sleep N` or `sleep N; echo …` / `sleep N && echo …`;
  - the 4th consecutive ReadNotifications with no other tool call in between.
- [ ] Commands that merely contain `sleep`, such as `sleep 2 && curl …` inside a real script or a `timeout` wrapper, are allowed.
- [ ] Proof at close: `claude/tests/test_wait_guard.py`, run by `claude/tests/run.sh`, feeds fixture PreToolUse JSON payloads for the allowed and denied cases, including the ReadNotifications counter across four payloads with one session id. The closing comment includes the run output.

## Suggested approach

A small Python script (`claude/wait_guard.py`) that reads the PreToolUse JSON from stdin. It matches `tool_name == "Bash"` against an anchored regex. For ReadNotifications it keeps a per-session counter in `${XDG_RUNTIME_DIR:-/tmp}/claude-wait-guard/<session_id>`. Any other tool call resets the counter. Model it on `claude/rtk_prefilter.py` and `claude/tests/test_rtk_prefilter.py`.

## Out of scope

- Rewording shatter `AGENTS.md:405-409` to mean event-driven liveness checks. That belongs to the shatter repo, and whoever implements this issue should file it there once the rule lands.
- The bento check-unpushed Stop hook, which blocks turn end during long landings and pushes agents toward busy loops. That is tracked in bento as `check-unpushed-overcount-and-blocks` (source sessions-10).
- The shatter-local "no foreground sleep" line in str-qwua7.26.

## Dependencies

None blocking. Related: `global-guidance-actually-loads` (the core summary) and `hooks-dotfiles-env-unset` (use `$HOME/dotfiles` paths).

## Source

Shatter audit 2026-09-22 finding sessions-01, in `audits/2026-09-22/areas/sessions.md`.


---

<!-- FILE: 03-never-recommend-bypass.md -->

---
slug: never-recommend-bypass
kind: new
title: "Agents mark hook or gate bypass as the (Recommended) option: add a framing rule to failing-checks.md"
priority: P2
type: enhancement
labels: [documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Agents mark hook or gate bypass as the (Recommended) option: add a framing rule to failing-checks.md

Part of #<epic>. Priority: P2. Type: enhancement.

## Problem

Sometimes a gate or hook fails for a reason unrelated to the diff. In those cases agents offer a bypass (`--no-verify`, `core.hooksPath=/dev/null`, "waive this gate") as the **(Recommended)** AskUserQuestion option. Every recorded time, the user declined and asked for the flake to be fixed or isolated instead.

`failing-checks.md` already forbids self-granted waivers ("Never grant yourself a waiver … let the user decide"). It does not say how to frame the options, so agents obey the letter and steer the user toward the bypass.

## Evidence

Verbatim quotes from Shatter transcripts in `~/.claude/projects/-home-ketan-project-shatter/`, confirmed by the verifier:

- `87606e10`, 2026-09-19T22:39: the agent offered "Bypass this push's hook with --no-verify (Recommended)". The user chose "Keep retrying the push".
- `87606e10`, 2026-09-19T15:58: the agent offered a bypass. The user answered: "the tests shouldn't be using a globally visible directory like that. file an issue to get filesystem isolation then fix it."
- `e724dbd8`, 2026-09-07T23:09: the agent offered "File the bug now, proceed with --no-verify landings (Recommended)".
- `9f13ca23`, 2026-09-19T16:04: the agent proposed "Waive this gate and land". The user chose "Hold off, fix the flake first".

The rule's current text, re-verified at dotfiles @ `81f35e1`: `docs/agent-guidance/failing-checks.md` has 26 lines. `grep -in "recommended\|bypass\|no-verify"` on it finds nothing. The waiver bullet is the last bullet.

## Acceptance criteria

- [ ] `docs/agent-guidance/failing-checks.md` states the rule. When a gate or hook fails for a reason unrelated to the diff, the recommended option is to isolate or fix the cause, or to file it and pause. A bypass (`--no-verify`, `core.hooksPath` override, gate waiver, skip env var) may be listed only as a non-default option. It is never marked Recommended, never listed first, and never presented as the default.
- [ ] The user's "filesystem isolation" answer is quoted as the canonical example.
- [ ] The rule's one-line summary appears in the core-rules file from `global-guidance-actually-loads`. If that issue has not landed yet, it picks the line up.
- [ ] Proof at close: the closing comment shows the output of `grep -n "Recommended" ~/dotfiles/docs/agent-guidance/failing-checks.md` and of `codex/agents-sync.sh status`, with `render vs snapshot: ok` so Codex has the rule.

## Out of scope

- Enforcing this at the hook level (blocking `--no-verify`). That is tracked in bento as `git-guard-bypasses-and-false-positives`.
- Any guidance that recommends bypassing a hook. Per maintainer decision D4 (2026-09-23), no bypass guidance is written anywhere. Slow hooks are fixed at their root cause instead. For example, the shatter beads hook stall is fixed by retiring the JSONL import.

## Dependencies

None blocking. Related: `global-guidance-actually-loads`.

## Source

Shatter audit 2026-09-22 finding sessions-06.


---

<!-- FILE: 04-first-party-plugin-autoupdate.md -->

---
slug: first-party-plugin-autoupdate
kind: new
title: "Installed shatter/refute plugins are 27 commits stale: enable autoUpdate for first-party marketplaces and add a staleness check"
priority: P2
type: bug
labels: [bug]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Installed shatter/refute plugins are 27 commits stale: enable autoUpdate for first-party marketplaces and add a staleness check

Part of #<epic>. Priority: P2. Type: bug.

## Problem

Every session runs the `shatter@shatterproof` and `refute@shatterproof` plugins at 0.1.1 from June. The source repo is at 0.1.12, 27 commits ahead. The `shatterproof` marketplace has not refreshed since 2026-06-19. It is the only first-party marketplace without `"autoUpdate": true`, and nothing warns when an installed first-party plugin falls behind its source.

## Evidence

Re-verified on 2026-09-23:

- `~/.claude/plugins/installed_plugins.json` lists `shatter@shatterproof` and `refute@shatterproof` at version `0.1.1`, sha `efa59682`, lastUpdated `2026-06-19T16:19Z`.
- In `~/.claude/plugins/known_marketplaces.json`, `shatterproof` has lastUpdated `2026-06-19T16:19:59Z` and no autoUpdate. By contrast:
  - `bento` has autoUpdate `true` and lastUpdated 2026-09-23.
  - `claude-plugins-official` and `claude-code-plugins` have no autoUpdate but refreshed on 2026-09-23.
  - `typesafe-ai` has no autoUpdate and refreshed on 2026-09-21.
- The marketplace clone `~/.claude/plugins/marketplaces/shatterproof` is at `efa5968` (2026-06-18).
- The remote is reachable: `git -C ~/.claude/plugins/marketplaces/shatterproof ls-remote origin HEAD` returns `119b8074…`, which matches `/home/ketan/project/shatter-agents` HEAD `119b807`. `git rev-list --count efa59682..HEAD` returns 27. A fetch failure is therefore unlikely, and the missing refresh is the probable cause.
- The source of the setting is `~/dotfiles/claude/hosts/pontoon/settings.json:36-41`, where the `shatterproof` entry under `extraKnownMarketplaces` has no `autoUpdate`. The `bento` entry at lines 29-35 has it. `claude/settings-sync.sh` renders the host overlay into `~/.claude/settings.json`, with the snapshot in `claude/settings.rendered.json`. The base `claude/settings.json` does not contain the entry.

## Acceptance criteria

- [ ] Every first-party marketplace in the host settings (`shatterproof` and any other non-Anthropic first-party entry) has `"autoUpdate": true`, and `claude/settings-sync.sh status` reports the render as current.
- [ ] Proof at close: in a fresh session after the change, `installed_plugins.json` shows the shatter and refute plugins at the `shatter-agents` HEAD sha and version current at that time. Paste the entry into the closing comment. If they do not refresh, diagnose the refresh failure and record it here before closing.
- [ ] A staleness check exists. For each enabled plugin whose marketplace repo is also checked out under `~/project/<repo>`, it compares the installed sha or version with the checkout's HEAD and `plugin.json`, and warns when the plugin is behind. Put it in either:
  - a dotfiles script run at SessionStart, with a test that seeds an older sha and asserts the warning; or
  - an issue filed against bento's agent-env doctor (bento-m4y5), linked from here.

## Suggested approach

Add `"autoUpdate": true` to the `shatterproof` block in `claude/hosts/pontoon/settings.json`, re-render with `claude/settings-sync.sh render`, then start a new session. The staleness check is the more robust half, because it also catches refresh failures.

## Out of scope

- Plugin content fixes. Current shatter-agents source still ships a `shatter-diff` skill for a command that does not exist. Under maintainer decision D2, the shatter-agents bucket withdraws that skill (`withdraw-shatter-diff-skill`). Refreshing the install before that lands exposes the skill for a while. The refresh should not wait for it.

## Dependencies

None blocking. Related: shatter-agents `withdraw-shatter-diff-skill`.

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
- [ ] Proof at close: new cases in `claude/tests/test_rtk_prefilter.py` fail before the fix and pass after it. The closing comment includes both runs. The cases must cover at least:
  - `a; head -5 f; b`
  - `a && tail -n 3 f`
  - `sed -n '1,4p' f | cat`
  - `wc -l f; true; head -5 f` (the reproduction above)
- [ ] The reproduction command above prints the true first five lines in a live session. Paste the output into the closing comment.
- [ ] Optional: an upstream rtk issue is filed asking that requested ranges never be replaced with summaries, and it is linked here.

## Suggested approach

Split the command on top-level `;`, `&&`, `||` and `|` using the prefilter's existing tokenizer, if it has one. If any segment is an exact-range read, exit 0 with no output so the command runs byte-identical, as the prefilter already does for its other skip cases.

## Out of scope

- The `find -not/-exec` case, which #11 already handles (`rtk_prefilter.py:50-91`).
- rtk's own rewrite logic in `~/.claude/hooks/rtk-rewrite.sh`, which the rtk tool owns.

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

Part of #<epic>. Priority: P2. Type: enhancement.

## Problem

The global Self-Improvement Loop says only "update memory with the lesson". Nothing says a tooling or product bug belongs in the owning repo's tracker, and nothing says a memory must be retired when the underlying issue is fixed. Agent auto-memory therefore becomes a shadow tracker. Its entries outlive the bugs they describe, and later agents quote them as authority.

## Motivating example (already cleaned up, no action requested)

The Shatter project memory (`~/.claude/projects/-home-ketan-project-shatter/memory/`) showed the failure mode during the 2026-09-22 audit:

- `project_shatter_git_hook_test_corrupts_worktree.md` still told agents to commit and push with `--no-verify` after the underlying bug (shatter str-jttrf) had been fixed. Agents quoted it verbatim to justify bypassing hooks (audit finding sessions-03).
- `project_shatter_gate_cache_and_bare_primary.md` claimed the primary checkout has `core.bare=true`. `git -C /home/ketan/project/shatter config --show-origin core.bare` returned `file:.git/config false`. The audit's own prompt repeated the stale claim.
- `project_audit_2026_07_10_gate_state.md` was not indexed in `MEMORY.md`.

These files were corrected on 2026-09-23. This issue is about the global rule that would have stopped the drift, not about those files.

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
- [ ] The Self-Improvement Loop in `codex/AGENTS.md` points to `memory.md` within #23's word budget. The rule's summary line is in the core-rules file from `global-guidance-actually-loads`.
- [ ] A small check script (not the overscoped #5-#7 toolchain) scans `~/.claude/projects/*/memory/*.md`. It prints any file that:
  - contains `--no-verify` or `core.hooksPath` without naming an issue ID; or
  - is not linked from its directory's `MEMORY.md`.

  Proof at close: its test seeds one file of each kind and asserts that both are reported, and the closing comment includes one real run.

## Out of scope

- Editing specific Shatter memory files. That was done on 2026-09-23.
- Checking whether a referenced issue is closed. That needs per-tracker access. If wanted, file it separately.

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

Part of #<epic>. Priority: P3. Type: enhancement.

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

- [ ] `fail-closed-defaults.md` (or `validation-and-errors.md`, with a pointer from the other) states two rules for any gate or validator that extracts facts from source:
  - It must fail when an expected extraction is empty, or when a declared pattern, allowlist entry or known-drift entry never matches.
  - It must carry a canary or mutation test showing that it goes red on a seeded defect.
- [ ] The rule includes one short example, for example: "a registry validator that finds 0 message types in a frontend fails; its test deletes one handler and asserts the validator reports it."
- [ ] Proof at close: the closing comment shows the output of `grep -n "canary\|empty extraction" ~/dotfiles/docs/code-writing-guidance/*.md`.

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

Part of #<epic>. Priority: P3. Type: bug.

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

- [ ] No hook command in `claude/settings.json` or in any host overlay under `claude/hosts/` depends on `$DOTFILES`. Either the commands use `$HOME/dotfiles/...`, or `DOTFILES` is set in the settings `env` block. The first is preferred because it does not depend on `env` expansion order. `grep -n '\$DOTFILES' claude/settings.json claude/hosts/*/settings.json` returns nothing, or matches only the `env` definition.
- [ ] A test under `claude/tests/` runs every hook command from the rendered settings with `env -i HOME=$HOME PATH=/usr/bin:/bin sh -c '<command>' </dev/null` and asserts that none exits 127 or prints `not found`. It must fail on the current tree.
- [ ] Proof at close: the closing comment includes the test's before and after output, plus one fresh non-interactive session (for example a spawned teammate) whose transcript has no `hook_non_blocking_error` for these hooks.
- [ ] Optional: a SessionStart self-check warns once when any hook script path does not resolve.

## Out of scope

Bento's agent-env doctor (bento-m4y5) checking global hook paths. It could be extended later, but the root fix belongs here.

## Dependencies

None. New hooks from `background-wait-rule-and-hook` and `global-guidance-actually-loads` should use `$HOME/dotfiles` paths from the start.

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

Part of #<epic>. Priority: P3. The verifier lowered this from P2 because part of the root cause is harness UI, outside first-party control. Type: enhancement.

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
- [ ] It says that an auto-mode classifier denial means "use the sanctioned tool or path", for example the bento land-work driver for pushes to main. It does not mean "wait", and it does not mean "retry with a bypass" (consistent with `never-recommend-bypass`).
- [ ] The summary line is in the core-rules file from `global-guidance-actually-loads`.
- [ ] Proof at close: the closing comment shows the output of `grep -n "PushNotification\|classifier" ~/dotfiles/docs/agent-guidance/questions.md` and `codex/agents-sync.sh status` reporting `render vs snapshot: ok`.

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
title: "Global Read/Grep-first rule contradicts the harness bypass-mode text: state one rule that acknowledges both"
priority: P3
type: enhancement
labels: [documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Global Read/Grep-first rule contradicts the harness bypass-mode text: state one rule that acknowledges both

Part of #<epic>. Priority: P3. The verifier notes this is mostly a conflict of preferences with little effect on correctness. Type: enhancement.

## Problem

The global rule and the harness instructions give opposite defaults:

- `codex/AGENTS.md` "Tool-Specific Notes" (lines 129-137 at dotfiles @ `81f35e1`) says to prefer `Read`/`Grep`/`Glob` over Bash `cat`/`grep`/`find`.
- Claude Code's bypass-permissions harness text explicitly allows reading and searching with `cat`, `grep`, `sed` and `find` through Bash.

Agents follow the harness. Neither source says which one wins, and the global rule reads as ignored boilerplate.

## Evidence

Counts from Shatter transcripts since 2026-09-04 (the verifier did not recount them):

- 655 Bash calls led by grep, cat, sed, find or head, across 34 sessions: grep 372, cat 151, sed 70, find 58.
- Dedicated tools: Read 379, Grep 128, Glob 11.
- All-time totals: 1,470 shell-led calls against 1,429 dedicated-tool calls.

The source finding also cited rtk rejecting `find -not/-exec`. **That part is resolved.** Every occurrence of `rtk find does not support compound predicates` in the Shatter transcripts is dated between 2026-08-27 and 2026-09-07, before #11 landed (`88e9cb5`, 2026-09-07). `claude/rtk_prefilter.py:50-91` now passes those forms through, so it is excluded here.

The exact-range read problem, where `head`/`sed -n` output is summarized in compound commands, is a live reason to prefer `Read`. It is tracked in `rtk-head-range-compound`.

## Acceptance criteria

- [ ] The Tool-Specific Notes bullet (or its successor after #23's rewrite) acknowledges the harness's bypass mode and states the rule that matters:
  - use `Read` for files you will edit, or when you need exact byte ranges;
  - use `Grep`/`Glob` for multi-file search;
  - plain shell is acceptable for one-off reads and for pipelines that the dedicated tools cannot express.
- [ ] The wording fits dotfiles#23's ≤ 150-word target for Tool-Specific Notes. If #23 has landed, it must not break `test/global-instructions-budget-test.sh`.
- [ ] Proof at close: the closing comment shows the new bullet, the output of `wc -w codex/AGENTS.md`, and `codex/agents-sync.sh status` reporting `render vs snapshot: ok`.

## Out of scope

- rtk `find` handling (fixed in #11).
- The shatter-local Tool-rules block (str-qwua7.26).
- Hook enforcement of tool choice.

## Dependencies

None blocking. Coordinate with dotfiles#23, which rewrites the same section under a word budget. Whichever lands second rebases onto the other. Related: `rtk-head-range-compound`.

## Source

Shatter audit 2026-09-22 finding sessions-16 (verifier: partially confirmed, P3).
