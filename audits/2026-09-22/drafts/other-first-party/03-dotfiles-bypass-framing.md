# Never mark a hook or gate bypass as the recommended option in AskUserQuestion

## Filing metadata

- tracker/repo: dotfiles
- action: create new issue
- type: enhancement
- priority: P2
- labels: documentation
- parent: repo epic (see INDEX)
- dedupe relation: new
- source findings: sessions-06

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

When a gate or hook fails for a reason unrelated to the diff, agents offer a bypass (`--no-verify`, "waive this gate") as the **(Recommended)** AskUserQuestion option. The user has declined it every time and asked for the flake to be fixed instead. Verbatim examples from Shatter transcripts:

- `87606e10` 2026-09-19T22:39: "Bypass this push's hook with --no-verify (Recommended)". The user chose "Keep retrying the push".
- `87606e10` 2026-09-19T15:58: a bypass was offered. The user replied: "the tests shouldn't be using a globally visible directory like that. file an issue to get filesystem isolation then fix it."
- `e724dbd8` 2026-09-07T23:09: "File the bug now, proceed with --no-verify landings (Recommended)".
- `9f13ca23` 2026-09-19T16:04: the agent proposed "Waive this gate and land". The user chose "Hold off, fix the flake first".

`~/dotfiles/docs/agent-guidance/failing-checks.md` covers accept/wait decisions but has no rule about how to frame bypass options.

## Acceptance criteria

- [ ] `failing-checks.md` states: when a gate fails for a reason unrelated to the diff, the recommended option is to isolate or fix the flake, or file it and pause. A bypass may be listed, but never marked Recommended and never offered as the default.
- [ ] The user's "filesystem isolation" answer is included as the canonical example.
- [ ] The rule is reachable in every session. Either it is inlined per the guidance-loading issue, or it is in a leaf that issue makes load.

## Out of scope

Hook-level enforcement of bypasses, which is tracked in bento as git-guard work.

## Source

Shatter audit 2026-09-22 finding sessions-06.
