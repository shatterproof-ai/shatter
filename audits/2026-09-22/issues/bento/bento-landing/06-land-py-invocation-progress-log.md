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
