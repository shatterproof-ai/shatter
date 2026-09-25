---
slug: a0nz-behavioural-probe
kind: note-to-existing
title: "Note on bento-a0nz: audit grades need claim-appropriate evidence (production reachability for wiring claims, default-configuration probes for behaviour claims); add a Rust reachability mechanism; build the CLI from the audited SHA"
priority: P3
type: note
labels: [audit, skills]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-a0nz
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-a0nz

Target: bento-a0nz (open, P1, "Add invocation/wiring + entrypoint-parity detection to the audit skill"). Action: add a comment; not a new issue. Source finding: core-23 (shatter audit 2026-09-22).

## Comment text

Addendum (shatter audit 2026-09-22, finding core-23).

In the prior shatter audit, grades rested on reading code, and three findings were wrong:

- it graded dead `clustering.rs` "Solid" (a wiring claim: the module had no production caller);
- it asserted solver timeouts that are off by default (a behaviour claim: the code exists but the default configuration never enables it);
- it filed str-qwua7.49 on a wrong premise.

Additional requirement for the audit skill: the evidence for each graded component must match the kind of claim.

- **Wiring or integration claims** ("X is used", "X is solid in production") need a production-reachability citation: a caller chain from a real entry point (CLI command, handler, exported API), not a test. A behavioural probe does not satisfy this, because a probe can exercise code that nothing in production reaches.
- **Behaviour claims** ("X times out", "X rejects Y") need a probe or E2E run **in the default configuration**, recording the command and the configuration used. A caller citation does not satisfy this, because it cannot show that a feature is enabled by default.

Where to put it: there is no executable report validator in the audit skill today (`catalog/skills/audit/` has prose instructions in SKILL.md and `references/quality-standards.md`, plus `scripts/audit-discover.py`). There is also no component-grade report template. Add this as a **review checklist item** in `references/quality-standards.md` and in the part of SKILL.md that describes the report's findings, rather than claiming anything "rejects" a grade. Building a validator is out of scope for this addendum.

Rust gap: `catalog/skills/audit/references/static-analysis-tools.md` lists reachability tools for Go (deadcode) and TS (knip), but its Rust section has only clippy, fmt, cargo-audit and tarpaulin. Add a Rust mechanism, for example a module-reachability script (pub modules with zero references outside their own tests) or cargo-udeps plus a pub-item grep.

Also: before filing behavioural findings, build the CLI from the audited SHA and record `--version` and the SHA. Shatter's str-qwua7.12 was filed from a stale binary.

Proof for these additions at close: (1) the checklist text in `quality-standards.md` and SKILL.md, with a grep test that it is present; (2) the Rust reachability mechanism run on a fixture crate with one dead pub module and one live one, flagging only the dead one.
