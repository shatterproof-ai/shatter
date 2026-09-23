---
slug: close-reason-evidence
kind: new
title: "beads-issue-flow: close helper for closes that are not landings (not reproducible, duplicate, wontfix, superseded) with typed reason validation and SHA ancestry checks, plus a report of non-conforming close reasons"
priority: P2
type: feature
labels: [audit, beads-issue-flow]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [wzbt-manual-close-note]
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# beads-issue-flow: close helper for closes that are not landings (not reproducible, duplicate, wontfix, superseded) with typed reason validation and SHA ancestry checks, plus a report of non-conforming close reasons

Source finding: prior-13 (shatter audit 2026-09-22). Related: bento-1qry, bento-1bl, bento-v57, bento-m4en.

**Ownership boundary with the closure group** (bento-wzbt, then bento-sy49 and bento-79j2, then bento-bo9c, then bento-x4bm):

- Closing an issue **after a landing** is owned by that group. bento-wzbt defines the closure-note contract and the close reason for landings; bento-x4bm makes `land.py` validate the note and close the issue itself. This issue does not touch `land.py` and does not define a landing reason format.
- This issue owns the **other** closes, which today have no contract at all: not reproducible, duplicate, wontfix, and superseded. It reuses wzbt's contract module or grammar file for anything shared (SHA and issue-id syntax) rather than defining a second one.

## Problem

Closures that are not landings carry no verifiable evidence, and one was checked against a commit that is not on main.

## Evidence (shatter)

- 17 of 49 closures since 2026-09-04 have the reason "Closed" or an empty reason, including str-qwua7.8, .9, .15, .27, .32, .41, .56 and str-2tyfk (empty).
- str-qwua7.14's reason is "Not reproducible against current main (e50fc399)". `git merge-base --is-ancestor e50fc399 origin/main` is false: e50fc399 is a stray fixture "init" commit.
- By contrast, land.py-era reasons (str-qwua7.4, str-vr7vq, str-gjsb2) carry the SHA, the gate and the review result.

## Current code facts (bento origin/main @ 0b8d488)

- `catalog/skills/beads-issue-flow/SKILL.md`: the ancestry rule (`git merge-base --is-ancestor <merge-sha> <integration-branch>`, line 138) and the Closure Checklist (line 174) are guidance only (bento-1bl, bento-v57, closed).
- `land.py` has no close step today (bento-x4bm adds it); `land-work-verify-landing.py` `_check_issue_status` (lines 77-130) only warns.
- `bd close` accepts any reason, including an empty one. Bento has no hook on `bd close`.

## Reason grammar (validated by type)

| Type | Form | Validation |
|---|---|---|
| not reproducible | `not reproducible at <sha> (<command>)` | `<sha>` is 7-40 hex and `git merge-base --is-ancestor <sha> <remote>/<primary>` succeeds; `<command>` is non-empty |
| duplicate | `duplicate of <id>` | `bd show <id>` succeeds and `<id>` differs from the issue being closed |
| superseded | `superseded by <id>[: <text>]` | as for duplicate |
| wontfix | `wontfix: <rationale>` | rationale has at least 3 words (so `wontfix: obsolete` is rejected and `wontfix: obsolete after str-81xiw removal` passes) |

Anything that matches none of these forms is rejected, whatever its length. The helper refuses a landing-style reason (`<sha> landed on ...`) and points to land-work, which owns those.

## Acceptance criteria

- A helper, `catalog/skills/beads-issue-flow/scripts/close.py <id> --reason "..."`, validates the reason against the table and then runs `bd close <id> --reason ...`. Ancestry and id-resolution failures refuse the close unless `--force --justification "<text>"` is given; the justification is appended to the reason.
- Tests, table-driven, cover for each type at least one valid reason and one invalid reason, including: `Closed`, an empty reason, 60 characters of free prose (rejected), `wontfix: obsolete` (rejected), `not reproducible at e50fc399 (...)` against a fixture where that SHA is not an ancestor (rejected), and the forced path.
- beads-issue-flow's Closure Checklist tells agents to use the helper for every close that is not a landing, and names bento-wzbt's contract for landings.
- **Enforcement is stated honestly.** The skill says the helper is the supported path and that a direct `bd close` bypasses it. To detect bypasses, `close.py --audit --since <date>` lists closed issues whose reason matches neither this grammar nor wzbt's landing form. A test covers the audit on a stubbed `bd` output.
- Proof at close: the close note names the tests with failing-then-passing runs; includes `close.py --audit --since 2026-09-04` output against shatter (expected: at least the 17 bare or empty reasons listed above); and this issue's own close reason follows wzbt's landing contract.

## Out of scope

- Closes that follow a landing, and `land.py` (bento-wzbt, bento-x4bm).
- A PreToolUse guard on `bd close` (possible follow-up on top of bento-l01v's segmenter).
- Retroactively fixing old close reasons.

## Priority / Type / Labels

P2 / feature / audit, beads-issue-flow

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

Blocked by bento-wzbt (the contract whose shared grammar this reuses), via the note draft wzbt-manual-close-note. Blocks followups-as-siblings (which extends the helper).
