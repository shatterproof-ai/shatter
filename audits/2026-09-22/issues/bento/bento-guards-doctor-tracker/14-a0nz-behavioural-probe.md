---
slug: a0nz-behavioural-probe
kind: note-to-existing
title: "Note on bento-a0nz: audit grades need a production-caller check or behavioural probe; add a Rust reachability mechanism; build the CLI from the audited SHA"
priority: P3
type: note
labels: [audit, skills]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-a0nz
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-a0nz

Target: bento-a0nz (open). Action: add a comment; not a new issue. Source finding: core-23 (shatter audit 2026-09-22).

## Comment text

Addendum (shatter audit 2026-09-22, finding core-23).

Additional requirement for the audit skill: every graded component must cite either a production-caller check or one behavioural probe or E2E test. In the prior shatter audit, grades rested on reading code:

- it graded dead `clustering.rs` "Solid";
- it asserted solver timeouts that are off by default;
- it filed str-qwua7.49 on a wrong premise.

Rust gap: `catalog/skills/audit/references/static-analysis-tools.md` lists reachability tools for Go (deadcode) and TS (knip), but its Rust section has only clippy, fmt, cargo-audit and tarpaulin. Add a Rust mechanism, for example a module-reachability script (pub modules with zero references outside their own tests) or cargo-udeps plus a pub-item grep.

Also: before filing behavioural findings, build the CLI from the audited SHA and record `--version` and the SHA. Shatter's str-qwua7.12 was filed from a stale binary.

Proof for these additions at close: a test or fixture run showing the audit report template rejects a grade without a cited caller/probe, and the Rust reachability mechanism flagging a known-dead module in a fixture.
