---
slug: qwua7-10-allowlist-issue-links-note
kind: note-to-existing
title: "Note on str-qwua7.10: require a tracker issue on every gauntlet allowlist entry, not only the two new expiring ones"
priority: P1
type: bug
labels: [gauntlet, quality-gates, audit-2026-09-22]
parent_epic: ""
blocked_by: []
existing_id: str-qwua7.10
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.10: require a tracker issue on every gauntlet allowlist entry, not only the two new expiring ones

Target: **str-qwua7.10** (open, P1, "Demo gates must fail on 0% coverage, all-throwing functions and lifecycle-helper clusters"; verified with `bd show` on 2026-09-23). Action: append the comment below. Do not change the priority. This note replaces the allowlist half of the earlier draft known-answer-ratchet-and-ts-discriminants, because str-qwua7.10 already owns the allowlist schema (it adds a required `expires:` date and two entries that cite their issue ids), and a second issue changing the same file's schema would split ownership.

## Comment text

**Audit 2026-09-22 note (finding goals-07): extend the allowlist schema change to every entry.**

This issue adds a required `expires:` field to `demo/gauntlet-scan-allowlist.yaml` and two new entries that cite their issue ids. The 2026-09-22 audit found that the existing entries have the same problem the expiry is meant to fix: the file has 113 lines and one `str-` reference, it has not changed since 2026-05-08 (commits 8734407f and 398e4a7e), and entries such as `computeArea` (:33) and `routeRequest` (:38) give "union-input synthesis" as the reason with no issue.

Proposed additions to the acceptance checks:

- Every `expected_failures` entry (existing and new) carries an `issue:` field with a tracker id, alongside `expires:`. The checker fails on an entry without one. For the existing entries, file or reuse issues (the 2026-09-22 audit filed ts-union-discriminant-literals for `computeArea`).
- A checker test covers an entry with no `issue:` field and shows the gate failing.

Related new audit issues: known-answer-ratchet-and-ts-discriminants (a separate ratchet gate over the examples' expected outcomes, which does not touch the allowlist schema) and ts-union-discriminant-literals.
