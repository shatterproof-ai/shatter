---
slug: docs-first-run-reopen-note
kind: reopen-note
title: "Comment on closed str-qwua7.8: sandbox remedy disables write protection; SPEC changelog backfill claims edits never made"
priority: P1
type: note
labels: [docs, sandbox, spec, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.8
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-qwua7.8

Target: `str-qwua7.8` (closed 2026-09-14 with the bare reason "Closed"). Action: add the comment below. Do not reopen. The remaining work is tracked in the two new issues it names.

## Comment text

> Audit 2026-09-22: two of this issue's acceptance criteria are unmet on main, although the issue is closed.
>
> 1. **Sandbox remedy (docs-01, P1).** The addendum required that the TS example "never treat the Go-only backend variable as proof of confinement". README.md:309-310 and 321-329, QUICKSTART.md:83-85, SPEC.md:606-609 and 622-624, the `refusal_message()` text (shatter-cli/src/host_writes.rs:98-115) and the `--allow-host-writes` help (shatter-cli/src/args.rs:170-171) all still present `SHATTER_SANDBOX_BACKEND` as the recommended remedy. Only README.md:309 notes "(Go frontend)", as a parenthetical on the Recommended line; none of them says that TS and Rust targets get no confinement from it. It is worse than a docs gap: `host_writes.rs:142-145` skips the throwaway-directory IsolationGuard whenever the variable is set, so a TS or Rust target that writes a relative path writes straight into the cwd. This was reproduced with `SHATTER_SANDBOX_BACKEND=docker shatter explore w.ts:touch`, which left `marker-*.txt` in the cwd. Tracked in the new issue **sandbox-backend-disables-guard** (<id of sandbox-backend-disables-guard>).
> 2. **SPEC changelog backfill (docs-05, P2).** The §8 rows added here claim section updates that were never made. Row 2026-08-10 / str-1fwt claims §2.8 and §2.9, but §2.8 has no `.gitignore` block or implicit-init text, and §2.9 doctor still describes only embed staleness. Row 2026-07-18 / str-mktn claims §3.6, which never mentions `shatter.config.json`. The header `Last updated: 2026-09-09` predates the SPEC commits of 09-14 (21981b1d, 8bd5a667, c8bceb32) and 09-20 (2de05fd9, str-nfg4y), and those have no rows. Tracked in the new issue **spec-changelog-backfill** (<id of spec-changelog-backfill>).
>
> Process note: this closed with no close reason listing verified acceptance items. Requiring one is str-qwua7.51.

## Filing note

The filer script must replace each `<id of slug>` placeholder with the ids assigned to sandbox-backend-disables-guard (this bucket, 01) and spec-changelog-backfill (bucket shatter-docs).
