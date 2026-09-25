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
