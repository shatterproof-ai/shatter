# SPEC changelog claims §2.8/§2.9/§3.6 updates that never happened; 09-14 and 09-20 CLI changes have no rows

- Priority: P2
- Type: bug
- Labels: docs,spec
- Tracker action: new issue (str-qwua7.8 was closed with bare reason "Closed" and its acceptance criteria unmet)
- Related: str-qwua7.8, str-nfg4y, str-qwua7.15, str-wurp, str-qwua7.51
- Source findings: audit 2026-09-22 docs-05 (confirmed)

<!-- body -->
## Problem
SPEC.md's §8 changelog backfill says sections were updated that were not. Recent CLI-visible changes have no rows. The "Last updated" header is stale.

## Evidence / current code facts
- `SPEC.md:3` header reads `Last updated: 2026-09-09`, but SPEC was committed on 2026-09-14 (21981b1d, 8bd5a667, c8bceb32) and 2026-09-20 (2de05fd9, str-nfg4y).
- Row `SPEC.md:1178` (2026-08-10, str-1fwt) claims sections 2.8 and 2.9. §2.8 (`SPEC.md:475-493`) does not describe the `.gitignore` block that `init` manages or implicit init. §2.9 doctor (`SPEC.md:532-536`) mentions only embed staleness, while `shatter doctor --help` documents `-d/--directory` and a `.gitignore` coverage check.
- Row `SPEC.md:1181` (2026-07-18, str-mktn) claims §3.6, yet the only mention of `shatter.config.json` in SPEC is at line 326 (run) and in the changelog.
- str-nfg4y and str-qwua7.15 (help-output change, 09-14) have no changelog rows.

## Acceptance criteria
- §2.8 documents the `.gitignore` block that init manages and implicit init (link the str-qwua7.58 decision).
- §2.9 doctor documents `-d/--directory`, the gitignore check, the configuration report, and exit codes, matching `shatter doctor --help`.
- §3.6 documents `shatter.config.json` and its precedence relative to `.shatter/config.yaml` and `--set`.
- §8 has rows for str-nfg4y and str-qwua7.15, and the header date equals the newest row.
- Every existing §8 row's section list is spot-checked against `git log -p SPEC.md` for that commit, and false claims are corrected.

## Suggested approach
Rewrite each section from the current `--help` and code. Add a small check to `task docs` that fails when SPEC.md's header date is older than the newest §8 row. The larger args-vs-SPEC gate is str-wurp.

## Scope
In: SPEC text accuracy for §2.8, §2.9, §3.6 and §8. Out: the mechanical CLI-surface gate (str-wurp), and the §6 layout (separate issue).
