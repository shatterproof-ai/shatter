# land.py: document canonical long-running invocation; emit a progress log path and heartbeat

- Filing action: new issue
- Priority: P2
- Type: feature
- Labels: audit, land-work, skills
- Parent: epic
- Links: related bento-rdtn.14
- Source findings: bento-06

---BODY---
## Problem

A land.py run takes 10-25 minutes: create_preview about 300 s, verify 266-578 s, merge_push 259-1108 s. The skill shows only `land-work/scripts/land.py --runtime <runtime>`. Agents run it however they like, and the Bash tool's 2-minute foreground limit forces improvisation. In shatter session 9f13ca23 it was run as `land.py --runtime claude 2>&1 | tail -100` 14 times, and 4 failed runs came back "exited with code 0" because of the pipe. land.py itself exits 1 on failure.

## Current code facts

- `catalog/skills/land-work/SKILL.md` about lines 258-262: the only invocation shown.
- land.py prints one line per step (rdtn.14) and a final JSON on stdout. It writes no progress file of its own and prints no heartbeat.

## Acceptance criteria

- SKILL.md has a "Running land.py" block that says:
  - run it with run_in_background (or `nohup`), stdout JSON to a file, stderr to a log;
  - check `$?` directly and never pipe through tail/head;
  - wait for the completion notification instead of polling.
- land.py creates a progress log under `$XDG_STATE_HOME/bento/land-work/` and prints its path as the first stderr line.
- land.py writes a heartbeat line to stderr and the log every 60 s during long steps, naming the current step and elapsed seconds.
- A test asserts the first line and the heartbeat, with a short interval via an env override.

## Out of scope

- Splitting merge_push into sub-steps. That is a separate issue in this epic.
