---
slug: concolic-fuzz-rng-unseeded
kind: new
title: "Concolic plateau fuzz phase uses StdRng::from_os_rng and ignores --seed, so seeded concolic runs are not reproducible"
priority: P1
type: bug
labels: [audit-2026-09-22, concolic, orchestrator, reproducibility, seeds]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Concolic plateau fuzz phase uses StdRng::from_os_rng and ignores --seed, so seeded concolic runs are not reproducible

## Problem

`scan --seed N` promises that "the same seed over unchanged source yields the same exploration" (`shatter-cli/src/args.rs:970-979`, str-0m0vn). The concolic orchestrator seeds its main RNG from the config (`shatter-core/src/orchestrator.rs:2561-2563`: `Some(seed) => StdRng::seed_from_u64(seed)`). But when the loop hits a coverage plateau and enters the fuzz phase, it builds a fresh RNG from OS entropy:

```rust
// shatter-core/src/orchestrator.rs:3060
let mut fuzz_rng = StdRng::from_os_rng();
```

Any seeded concolic run that reaches the fuzz phase is therefore not reproducible. The audit's concolic run entered that phase eight times across 21 functions. This blocks the D3 benchmark (concolic-vs-default-benchmark), which needs fixed seeds per arm.

## Evidence

- `orchestrator.rs:3060` is the only `from_os_rng` outside the `None` branch at `:2563` (`grep -n "from_os_rng" shatter-core/src/orchestrator.rs`, audit HEAD).
- Tracker searches on 2026-09-23 (`bd search from_os_rng`, `bd search "fuzz phase seed"`) found no existing issue. str-0m0vn (closed) widened `--seed` to the exploration RNG. str-pbqyr and str-9m9o3 cover the scan cache ignoring the seed, not this.

## Acceptance criteria

- [ ] When `config.seed` is `Some`, the fuzz-phase RNG is derived deterministically from it (for example `seed_from_u64(seed ^ FUZZ_STREAM)`, or drawn from the main seeded RNG). When it is `None`, behaviour is unchanged.
- [ ] Every other RNG construction in `shatter-core/src` non-test code is listed in the close note, each marked as seeded from config or intentionally entropy-based.
- [ ] Test: the orchestrator is run twice with the same seed on a fixture that reliably enters the fuzz phase (assert the phase was entered, for example via the fuzz execution counter). The generated fuzz inputs are identical across the two runs. A third run with a different seed is allowed to differ. Proof: the test fails on current main (paste the diff of inputs) and passes after the fix.
- [ ] Forced (uncached) `task e2e` output at close.

## Out of scope

- Adding `--seed` to explore and run (seed-for-explore-and-run).
- Scan cache keying on the seed (str-pbqyr, str-9m9o3).

## Metadata

- Priority: P1 (blocks the D3 benchmark)
- Type: bug
- Labels: audit-2026-09-22, concolic, orchestrator, reproducibility, seeds
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Blocks: concolic-vs-default-benchmark
- Related: seed-for-explore-and-run, str-0m0vn (closed), str-pbqyr, str-9m9o3
- Source findings: Codex cross-check of concolic-vs-default-benchmark (2026-09-23)
- Decision refs: D3
