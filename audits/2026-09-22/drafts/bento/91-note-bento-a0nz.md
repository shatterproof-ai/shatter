# NOTE TO APPEND to bento-a0nz (duplicate-open; not a new issue)

---BODY---
## Addendum (shatter audit 2026-09-22, finding core-23)

Additional requirement for the audit skill: every graded component must cite either a production-caller check or one behavioural probe or E2E test. In the prior shatter audit, the grades rested on reading code:

- it graded dead `clustering.rs` "Solid";
- it asserted solver timeouts that are off by default;
- it filed str-qwua7.49 on a wrong premise.

Rust gap: `catalog/skills/audit/references/static-analysis-tools.md` lists reachability tools for Go (deadcode) and TS (knip), but its Rust section has only clippy, fmt, cargo-audit and tarpaulin. Add a Rust mechanism, for example a module-reachability script (pub modules with zero references outside their own tests) or cargo-udeps plus a pub-item grep.

Also: before filing behavioural findings, build the CLI from the audited SHA and record `--version`/SHA. Shatter's str-qwua7.12 was filed from a stale binary.
