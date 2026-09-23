---
slug: seed-for-explore-and-run
kind: new
title: "Add --seed to explore and run (str-0m0vn wired it into scan only)"
priority: P2
type: feature
labels: [cli, seeds, reproducibility, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Add --seed to explore and run (str-0m0vn wired it into scan only)

## Problem

str-0m0vn's symptom was "There is no way to make `shatter scan` or `shatter explore` reproducible". It was closed after adding `--seed` to `scan` only, and its scan-only follow-ups (str-pbqyr, str-9m9o3) do not cover explore. `explore` and `run` still have no seed flag, so an explore or run result cannot be reproduced. That undermines bug reports, CI flake triage, and the D3 benchmark (concolic-vs-default-benchmark), which needs fixed seeds.

CLAUDE.md's parity rule ("When adding a new ... CLI flag ... grep for the parallel code path") was not applied to the explore/scan/run flag trio.

## Evidence

Re-verified 2026-09-23 with `target/debug/shatter` built at HEAD `56c86168` of `audit-2026-09-22`:

- `shatter scan --help | grep -cE -- '--seed( |$|<)'` → 2. The same check on `explore --help` and `run --help` → 0. Their only seed-related flags are `--seeds-dir` and `--no-seeds`, which control the cross-function seed pool, not RNG seeding.
- `shatter-cli/src/args.rs:977-979`: `--seed` (`pub(crate) seed: Option<u64>`) is defined only in the Scan args. Its doc comment cites str-0m0vn.
- Scan-only seed follow-ups already open: str-pbqyr, str-9m9o3 (scan cache ignores the seed) and str-v1tzz (report the seed and test its propagation).
- Finding cli-ux-08 (areas/cli-ux.md F8), verified at P2.

## Acceptance criteria

- [ ] `shatter explore <target> --seed N` and `shatter run --seed N` are accepted. The seed reaches the random explorer and the concolic orchestrator, both engine paths, in the same way scan's does. A test asserts propagation for each path (for example through the effective config or an injected RNG).
- [ ] A reproducibility test, modelled on the existing scan seed reproducibility test, runs `explore` twice on the same fixture with the same seed and fresh artifact directories, and asserts identical path sets and generated inputs. A run with a different seed is allowed to differ. The test is shown failing (flag rejected) before the change and passing after.
- [ ] The explore resume and artifact key includes the seed, so a resumed run with a different seed is not served stale results. Coordinate with explore-resume-options-key, which introduces the options-hash key. If that issue lands first, add the seed to its hash; if not, record the gap on it.
- [ ] Help text for `--seed` is the same on scan, explore and run, and carries no tracker IDs (see help-tracker-ids-lint).
- [ ] E2E suites pass (`cargo test --test e2e_concolic`, `e2e_concolic_go`, `e2e_concolic_rust`) because explorer and orchestrator wiring changes. `task affected` passes, and the close comment records the gates selected.

## Suggested approach

Define `--seed` once in a shared options struct flattened into scan, explore and run. That is the str-qwua7.20.1 direction; if help-hides-execution-flags adds an `ExecOptions` struct first, put it there. Thread it through the same config field scan uses so both engine paths pick it up.

## Out of scope

- The scan-specific seed bugs (str-pbqyr, str-9m9o3).
- Printing the seed in reports (str-v1tzz). Extend that issue to explore and run once this lands, rather than duplicating it here.

## Dependencies

- Blocked by: none.
- Related: str-0m0vn (closed; see seed-reopen-note), str-v1tzz, str-pbqyr, str-9m9o3, str-qwua7.20.1, explore-resume-options-key, help-hides-execution-flags, concolic-vs-default-benchmark (needs fixed seeds).

## Source

Audit 2026-09-22 finding cli-ux-08; old draft `drafts/shatter-code/30-seed-for-explore-and-run.md`.
