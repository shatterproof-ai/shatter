---
slug: doctor-state-per-worktree
kind: new
title: "Resolve .agent-mode.local repo-wide (main working tree), with a locked writer, so linked-worktree sessions see collapsed doctor state and primary-checkout decisions"
priority: P2
type: bug
labels: [audit, hooks]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Resolve .agent-mode.local repo-wide (main working tree), with a locked writer, so linked-worktree sessions see collapsed doctor state and primary-checkout decisions

bento-rdtn.2 is closed but its goal is not met in linked worktrees (a note on rdtn.2 points here). Source findings: bento-08, plugins-08 (context), shatter audit 2026-09-22. Related, touching the same code: bento-m4y5 (in_progress; the agent-env doctor hook), bento-xy8m (open; `hook_bypass` reported as unknown). Coordinate with whichever is in flight.

## Problem

bento-rdtn.2 stores the doctor's seen/decided/remind_after keys in `.agent-mode.local` at `git rev-parse --show-toplevel`, which is per checkout. Launch-work requires work in linked worktrees, so every working session:

- gets the full dormancy paragraphs and the "prints once per repo" superpowers notice again;
- gets a fresh `.agent-mode.local` written into its worktree root;
- misses settings recorded in the primary checkout's copy, such as any `agent_env_doctor_skip_plugin`, `require_pushed=false`, `require_worktree=false` or `hook_bypass=allow` decision.

## Evidence (shatter)

- Primary `/home/ketan/project/shatter/.agent-mode.local`: `dangerous`, `agent_env_doctor_seen=bugshot,storystore`, `agent_env_doctor_superpowers_pointer_seen=true`.
- Linked worktree `audit-2026-09-22/.agent-mode.local` (created 2026-09-22 12:16): only the seen keys that session wrote.
- Running the doctor in the worktree prints the full text; in the primary it prints one-liners.

## Current code facts (bento origin/main @ 0b8d488; cited files unchanged since b1bb787)

- `catalog/hooks/bento/claude/scripts/agent-env-doctor.py` line 198: root resolved with `git -C <cwd> rev-parse --show-toplevel`; line 588: `config = root / ".agent-mode.local"`.
- Lines 1040-1088 `_rewrite_agent_mode_keys`: an **unlocked** read-modify-write that writes a fixed temp name `.agent-mode.local.tmp` and `replace`s it. Its docstring already names the race with a concurrent SessionStart. Today each checkout has its own file, so the race is mostly per checkout; moving every worktree onto one file makes it a real lost-update risk.
- Other readers with the same per-checkout scope: `require-worktree-git-guard.py` line 85 (`_read_agent_mode_keys`), `check-unpushed.py` lines 101-129 (`require_pushed`, `require_landed`), `require-worktree.sh`, `catalog/hooks/hygiene/claude/scripts/hygiene-check.py`, and the Codex copies of the doctor and check-unpushed.
- Outside bento: the dotfiles launcher `bashrc.agent-mode.sh` (`_agent_mode_repo_root`, line 29) reads the `dangerous` line from `--show-toplevel` too.

## Key scope table (the issue must keep this table in the skill/hook docs)

| Key | Scope after this issue |
|---|---|
| `agent_env_doctor_seen`, `agent_env_doctor_superpowers_pointer_seen`, `agent_env_doctor_skip_plugin`, decided/remind_after keys | repo-wide |
| `require_pushed`, `require_landed`, `require_worktree`, `hook_bypass` | repo-wide |
| `dangerous` / `mode=` (launcher) | unchanged; read by the dotfiles launcher, not by bento |

## Acceptance criteria

- One resolver, shared by the doctor (Claude and Codex), the git guard, `require-worktree.sh`, check-unpushed (both) and hygiene-check: when the absolute `git rev-parse --git-common-dir` ends in `/.git`, the file is `<its parent>/.agent-mode.local` (this holds even when the primary has `core.bare=true`, as shatter's does, so the existing primary copy keeps working); for any other common dir (a true bare repo), the file is `<git-common-dir>/bento/agent-mode.local`. Reads and writes both use it. Tests cover a normal repo, a linked worktree, a primary with `core.bare=true`, and a true bare repo with linked worktrees.
- **Locked writer.** `_rewrite_agent_mode_keys` takes an exclusive `fcntl.flock` on a sibling lock file for the whole read-modify-write, writes to a unique temp name (`tempfile.NamedTemporaryFile(dir=..., delete=False)`), and `os.replace`s it. Test: 8 processes started together, each adding a distinct key 20 times, end with all 8 keys present and no `.tmp` files left; the same test on the pre-fix writer loses keys (record that failing run).
- Behaviour tests: seen state written from one linked worktree collapses output in another linked worktree of the same repo; `hook_bypass=allow` and `require_pushed=false` set in the main copy are honoured in a linked worktree.
- **Migration preserves decisions.** On first run, an existing worktree-local `.agent-mode.local` is merged into the repo-wide file under the lock: keys missing from the repo-wide file are copied; on a conflicting value the repo-wide value wins and a one-line notice names the key and both values. Seen-lists (`agent_env_doctor_seen`) are unioned. The worktree-local file is then left in place but ignored, and the notice says so. Tests cover copy, conflict and union.
- Proof at close: the close note names the new tests with failing-then-passing runs, including the concurrent-writer test.

## Out of scope

- New doctor checks.
- The dotfiles launcher's `dangerous` lookup (a dotfiles change, if wanted).

## Priority / Type / Labels

P2 / bug / audit, hooks

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None hard. Coordinate with bento-m4y5 and bento-xy8m.
