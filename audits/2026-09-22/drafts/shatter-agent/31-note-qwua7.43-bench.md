# Note for str-qwua7.43: bench_frontier_ranking.rs deepened the core→shatter-llm dev-dep cycle

- Priority: P2
- Type: note
- Labels: agents,architecture
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: note-to-append (target str-qwua7.43)
- Source findings: frontend-rust-09
- Action: append as notes to str-qwua7.43 (no new issue)
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
Audit 2026-09-22 (evidence `audits/2026-09-22/areas/frontend-rust.md`):

- Since this issue was filed, the Jev benchmark work (str-hjrnp.3, closed)
  added `shatter-core/tests/bench_frontier_ranking.rs`, which imports
  `shatter_llm::{DecisionFrontierRanker, JevAdapter, ...}`. Its plan
  (`docs/superpowers/plans/2026-09-21-jev-frontier-ranking-benchmark.md`
  ~line 1037) explicitly said to add shatter-llm to core's dev-dependencies,
  contradicting this decided issue.
- Scope addition: move `bench_frontier_ranking.rs` (with
  `e2e_llm_oracle.rs`) into `shatter-llm/tests/` or a dedicated bench crate
  before dropping the dev-dep at `shatter-core/Cargo.toml:44`.
- The benchmark is also swept into `core:test-ignored` and times out at 120 s
  (tracked by str-6nul9); moving it resolves both.
- Process follow-up filed separately (planning must check open decisions).
