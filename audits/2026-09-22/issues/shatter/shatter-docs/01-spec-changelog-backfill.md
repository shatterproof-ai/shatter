---
slug: spec-changelog-backfill
kind: new
title: "SPEC changelog claims §2.8/§2.9/§3.6 updates that were never made; the 09-14 and 09-20 CLI changes have no rows"
priority: P2
type: bug
labels: [docs, spec, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# SPEC changelog claims §2.8/§2.9/§3.6 updates that were never made; the 09-14 and 09-20 CLI changes have no rows

## Problem

The §8 changelog in `SPEC.md` says some sections were updated when they were not. Several recent CLI-visible changes have no row at all, and the "Last updated" header is stale. Readers and agents who use the changelog to find out what changed are sent to sections that still describe the old behaviour.

str-qwua7.8 was supposed to fix this. It was closed on 2026-09-14 with the bare reason "Closed", and its acceptance criteria were not met. A reopen note on str-qwua7.8 (slug `docs-first-run-reopen-note`, bucket shatter-cli-runtime-output) points to this issue. This issue is the fix; the note is only the record.

## Evidence

Re-verified against `audit-2026-09-22` at 56c86168 (2026-09-23):

- `SPEC.md:3` reads `Last updated: 2026-09-09`. `git log --format='%h %cs %s' -- SPEC.md` shows later SPEC commits that have no §8 row:
  - `8bd5a667` and `21981b1d`, 2026-09-14 (str-qwua7.8)
  - `c8bceb32`, 2026-09-14 (str-qwua7.9.2)
  - `2de05fd9`, 2026-09-20 (str-nfg4y, which corrected the spec-diff help and SPEC text)
- str-qwua7.15 changed help output on 2026-09-14 and also has no row.
- `SPEC.md:1178`, row 2026-08-10 (str-1fwt), claims sections "2.8, 2.9", but neither section describes that change:
  - §2.8 `shatter init` (`SPEC.md:475-493`) does not describe the `.gitignore` block that init manages, or implicit init.
  - §2.9 `shatter doctor` (`SPEC.md:532-536`) describes only the check for a stale embedded frontend.
  - `target/debug/shatter doctor --help` documents `-d, --directory <DIRECTORY>` and a check that "any configured output path (cache, seeds, artifacts, report) … `.gitignore` fails to cover (str-1fwt)", and says it exits non-zero on that condition.
- `SPEC.md:1181`, row 2026-07-18 (str-mktn), claims sections "2.9, 3.6". The only mention of `shatter.config.json` outside the changelog is `SPEC.md:326` (the `run` scope). §3.6 Configuration (`SPEC.md:715`ff.) never names `shatter.config.json` and does not give its precedence relative to `.shatter/config.yaml` and `--set`.

## Acceptance criteria

- [ ] §2.8 documents the `.gitignore` block that `shatter init` manages, and implicit init. It links the str-qwua7.58 decision.
- [ ] §2.9 `shatter doctor` documents `-d/--directory`, the gitignore-coverage check, the project-configuration report (if doctor prints one) and the exit codes, and matches `shatter doctor --help` at the time of the change.
- [ ] §3.6 documents `shatter.config.json`: what it holds, and its precedence relative to `.shatter/config.yaml` and `--set`.
- [ ] §8 has rows for str-qwua7.8, str-qwua7.9.2, str-qwua7.15 and str-nfg4y. The `Last updated` header equals the date of the newest row.
- [ ] Every existing §8 row's "Section" column has been spot-checked against `git show <commit> -- SPEC.md` for the commit that row describes. False claims are corrected, and the PR description lists the rows that were checked.
- [ ] A check fails when the `Last updated` date in `SPEC.md` is older than the newest §8 row date. It can live in `scripts/docs-smoke.py` or a small script called from `task docs`.
  - Proof at close: a unit test (for example in `scripts/test_docs_smoke.py`) that fails on a fixture with a stale header and passes on a current one.
  - Proof at close: the check run directly (not through a cached `task` wrapper), with its output pasted into the close note.
- [ ] The close reason lists each acceptance item and how it was verified. A bare "Closed" is what went wrong with str-qwua7.8.

## Suggested approach

1. Rewrite §2.8, §2.9 (doctor) and §3.6 from the current `--help` output and the code: `shatter-cli/src/commands/init.rs`, the doctor command, and config loading.
2. Walk §8 top to bottom with `git log -p -- SPEC.md`, fixing section claims as you go.
3. Add the header-date check.

The broader mechanical gate (every clap flag documented in SPEC, every SPEC flag present in clap, a changelog row required when `args.rs` changes) is str-wurp. Do not build it here.

## Out of scope

- The CLI-surface drift gate (str-wurp).
- SPEC §5 (spec-s5-contract-table-and-samples) and §6 (spec-s6-layout-and-checkpoint).
- Removing `shatter diff` from SPEC §2.6, §2.11 and §5.5. That belongs to retire-snapshot-diff (D2), which adds its own §8 row.

## Dependencies

- Blocked by: none.
- Related: str-qwua7.8 (closed, reopen note), str-wurp, str-nfg4y, str-qwua7.15, str-qwua7.51 (require a close reason at landing), retire-snapshot-diff (also edits SPEC §8).

## Source

Audit 2026-09-22, finding docs-05 (confirmed, P2). Evidence is in `audits/2026-09-22/areas/docs.md`.
