---
slug: wurp-changelog-row-and-reverse-check
kind: note-to-existing
title: "Note on str-wurp: fold the flag-level, reverse (SPEC to clap) and changelog-row checks into its acceptance; flag-level drift keeps returning"
priority: P2
type: task
labels: [docs, spec, governance, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-wurp
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-wurp (open, P2: "Mechanical CLI-surface drift gate: SPEC.md and gauntlet coverage vs clap definitions")

Target: `str-wurp`. Post the text below as a comment. The maintainer may also choose to move the acceptance items into the description. Do not change status.

Context for the filer: str-wurp's **description** still has only command-level acceptance. The 2026-09-04 audit appended an "Acceptance checks (to append)" block in NOTES, covering flag-level, reverse-direction and changelog-row checks, but the description was never updated and nothing has landed.

## Comment text

> **Audit 2026-09-22 (finding docs-06): third audit in a row to find flag-level SPEC drift.**
>
> Verified at 56c86168, the drift this gate would flag today:
> - `SPEC.md:634` (§2.11 exit codes) still names a nonexistent `--failure-threshold`. `shatter scan --help` has only `--fail-on-failures [<PERCENT>]` and `--fail-on-setup-error`. This is the **reverse** direction: a flag named in SPEC that clap does not define.
> - §2.9 `shatter doctor` (`SPEC.md:532-536`) omits `-d/--directory`.
> - §2.9 `shatter list-targets` (`SPEC.md:499-505`) omits `--scope`.
> - SPEC's `Last updated: 2026-09-09` header and §8 changelog miss the 09-14 and 09-20 SPEC and CLI changes (str-qwua7.8, str-qwua7.9.2, str-qwua7.15, str-nfg4y). The prose fixes are filed separately as spec-changelog-backfill.
>
> Please make these part of this issue's **acceptance criteria**, not just notes:
> 1. **Forward check:** every non-hidden long flag of every subcommand appears as a backticked token in that command's SPEC §2 section, or in an allowlist (for example "shared with explore").
> 2. **Reverse check:** every backticked `--flag` in SPEC §2 and §2.11 exists in clap. This would have caught `--failure-threshold`.
> 3. **Changelog-row rule:** if `shatter-cli/src/args.rs` or SPEC §2 changed relative to `origin/main` and SPEC §8 gained no row (and the `Last updated` line did not change), fail, unless the commit carries an explicit trailer such as `Spec-Changelog: none (reason)`. This check is diff-relative, so it belongs in `task affected` / pre-push and PR CI, not in static `check-static`.
> 4. Take the inventory from clap itself (a `#[test]` walking `Cli::command()` subcommands and arguments), not by parsing help text.
>
> **Proof at close:** running the gate on the pre-fix HEAD reports exactly the drift listed above. It passes after the fix. drift-patrol's `cli-surface-drift` slot (`scripts/drift-patrol.py:437`) turns from PENDING into a real check. Show all three with forced runs, not cached `task` results.
>
> Note on D2 (2026-09-23): snapshot `shatter diff` is being retired (retire-snapshot-diff). The allowlist must not carry a `diff` entry once that lands.
