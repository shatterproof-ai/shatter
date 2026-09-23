# NOTE TO APPEND to bento-dyp7: route git hooks and land.py through the host admission governor; add a "machine busy" status line

- Filing action: note to append to existing open issue bento-dyp7 (host-wide weighted admission governor). Preview ownership lock is already bento-e583. Do not file a new issue.
- Priority: (inherits bento-dyp7; audit suggests P2)
- Source findings: sessions-07

---BODY---
## Addendum from shatter audit 2026-09-22

Observed load on the shared 32-core host while sessions from several projects (shatter, kapow, pickpackit, Codex) ran at once:

- load average 141-174;
- swap nearly full on 09-19;
- 49 `ps`, 23 `uptime`, 16 `pgrep` and 10 `free` calls in 15 sessions, spent diagnosing load by hand.

The gates that run inside git hooks (shatter pre-commit `cargo test`, pre-push `task affected`) and land.py's verifier do not go through `run-heavy`. `grep run-heavy` over shatter's hooks and `scripts/precommit-rust.sh` finds nothing.

Proposed additions to bento-dyp7's scope:
- The admission governor documents a hook entry point (`run-heavy -- <cmd>`) that consumer repos' git hooks can call, with an example.
- land.py's verify step goes through the governor and waits when the host is overloaded, instead of failing.
- land.py prints a one-line "machine busy: load X/cores, waiting for slot" status, so agents stop diagnosing load by hand.
