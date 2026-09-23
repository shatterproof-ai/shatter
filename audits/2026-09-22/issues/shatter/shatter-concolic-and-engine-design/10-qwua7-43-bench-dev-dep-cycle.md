---
slug: qwua7-43-bench-dev-dep-cycle
kind: note-to-existing
title: "Note on str-qwua7.43: bench_frontier_ranking.rs deepened the core -> shatter-llm dev-dependency cycle"
priority: P2
type: note
labels: [audit-2026-09-22, architecture, agents]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.43
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.43: bench_frontier_ranking.rs deepened the core -> shatter-llm dev-dependency cycle

**Target:** `str-qwua7.43` (open, decided 2026-09-05). Action: append the comment below. Do not create a new issue.

## Comment text

Audit 2026-09-22 note (finding frontend-rust-09; evidence `audits/2026-09-22/areas/frontend-rust.md`).

**What changed since this issue was decided:**

- The Jev frontier-ranking benchmark work (str-hjrnp.3, closed) added `shatter-core/tests/bench_frontier_ranking.rs`. It imports `shatter_llm::{DecisionFrontierRanker, JevAdapter, MockSeedOracle, ReplayDecisionOracle}` and `shatter_llm::jev::JevConfig` (lines 34-35), and calls `shatter_llm::build_oracle` (line 304).
- Its plan (`docs/superpowers/plans/2026-09-21-jev-frontier-ranking-benchmark.md`, around line 1037) explicitly said to add shatter-llm to core's `[dev-dependencies]`. That contradicts this issue's decision.
- Core now has two llm-dependent test files: `e2e_llm_oracle.rs` and `bench_frontier_ranking.rs`. The dev-dependency is still at `shatter-core/Cargo.toml:44` (`shatter-llm = { path = "../shatter-llm" }`).
- str-6nul9 (landed in 20692b08, merged 70465921) excluded `bench_frontier_ranking` from `core:test-ignored`'s sweep. The gate timeout is gone, but the dependency edge remains.

**Scope addition:**

- [ ] Move `bench_frontier_ranking.rs`, together with `e2e_llm_oracle.rs`, into `shatter-llm/tests/` or a dedicated bench crate. Then drop the dev-dependency at `shatter-core/Cargo.toml:44`.
- [ ] Update the Taskfile targets that name the test by crate: `bench-frontier-reference` (`Taskfile.yml:852`), `bench-frontier` (`:865`), and any `-p shatter-core --test bench_frontier_ranking` references. Also update the str-6nul9 exclusion, so the benchmark stays out of gate sweeps in its new home.
- [ ] Proof at close:
  - `cargo tree -p shatter-core -e dev | grep shatter-llm` prints nothing;
  - `task bench-frontier-reference` still runs from the new location (command + output).

The process gap (planning did not check open decided issues touching the same dependency edge) is filed separately as planning-rules-location-and-open-decisions (bucket shatter-agent-guidance-and-repo-hygiene) and is not part of this issue.
