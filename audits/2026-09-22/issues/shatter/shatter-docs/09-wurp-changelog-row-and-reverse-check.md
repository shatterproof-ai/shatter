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
> 2. **Reverse check:** every backticked `--flag` in SPEC §2 and §2.11 that is presented as a *Shatter* flag exists in clap for that command. This would have caught `--failure-threshold`. It must be context-aware, because SPEC legitimately names other tools' flags: for example `SPEC.md:176` (the explore flag table) says `--memory-limit` maps to Node's `--max-old-space-size`. Either (a) check only flag-table first columns and `shatter <cmd> …` usage lines, or (b) check every backticked flag but keep an explicit `external_flags:` allowlist (flag, SPEC location, owning tool, reason), seeded with `--max-old-space-size`. Unknown flags fail; allowlisted ones do not.
> 3. **Changelog-row rule**, as two separate requirements so neither can be satisfied alone:
>    - (a) *Row required:* if `shatter-cli/src/args.rs` or SPEC §2 changed relative to `origin/main`, SPEC §8 must gain at least one new row in that diff. Editing only the `Last updated` header does **not** satisfy this.
>    - (b) *Header consistent:* the `Last updated` date must equal the newest §8 row's date (a static check, which can run everywhere). Several rows may share a date, so same-day changes simply add another row with the same date; the header is unchanged in that case and still passes.
>    - Exemption: a commit trailer such as `Spec-Changelog: none (<reason>)` waives (a) only, never (b). The gate prints the reason.
>    (a) is diff-relative, so it belongs in `task affected` / pre-push and PR CI, not in static `check-static`. (b) can live in `check-static` (spec-changelog-backfill adds a first version of it in docs-smoke; reuse it rather than duplicate).
> 4. Take the inventory from clap itself (a `#[test]` walking `Cli::command()` subcommands and arguments), not by parsing help text.
>
> **Proof at close:** on the pre-fix HEAD, the gate reports **at least** the drift listed above (`--failure-threshold`, doctor `-d/--directory`, list-targets `--scope`), and every other item it reports is either real drift fixed in the same change or added to an allowlist with a reason — list them in the close note. It does not flag `--max-old-space-size`. It passes after the fix. A fixture diff that changes `args.rs` and only the `Last updated` header fails rule (a); a fixture with a new row but a stale header fails rule (b). drift-patrol's `cli-surface-drift` slot (`scripts/drift-patrol.py:437`) turns from PENDING into a real check. Show all of these with forced, direct runs, not cached `task` results.
>
> Note on D2 (2026-09-23): snapshot `shatter diff` is being retired (retire-snapshot-diff). The allowlist must not carry a `diff` entry once that lands.
