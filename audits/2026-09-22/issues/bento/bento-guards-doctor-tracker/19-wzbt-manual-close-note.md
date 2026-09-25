---
slug: wzbt-manual-close-note
kind: note-to-existing
title: "Note on bento-wzbt: a follow-up adds a close helper for non-landing closes (not reproducible, duplicate, superseded, wontfix) that reuses this contract's shared syntax"
priority: P2
type: note
labels: [audit, beads-issue-flow]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-wzbt
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-wzbt

Target: bento-wzbt (open, P2, "land-work: define the closure-note contract ..."). Action: add a comment. Scope unchanged. This draft gives the filer the edge close-reason-evidence blocked-by bento-wzbt.

## Comment text

Audit 2026-09-22 (shatter; finding prior-13). Closes that are not landings have no contract: in shatter, 17 of 49 closures since 2026-09-04 carry the reason "Closed" or nothing, and str-qwua7.14 was closed "Not reproducible against current main (e50fc399)" where e50fc399 is not an ancestor of origin/main. <id of close-reason-evidence> adds a beads-issue-flow close helper for those closes only (not reproducible at an ancestor SHA, duplicate of, superseded by, wontfix with a rationale), plus an audit report of non-conforming close reasons. It is blocked by this issue so that it reuses this contract's SHA and issue-id syntax and names this contract as the rule for landings; it does not touch land.py (bento-x4bm) or define a landing reason. Please mention the non-landing forms in `closure-contract.md` as "see beads-issue-flow close helper", so the two stay one system.
