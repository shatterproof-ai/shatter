# NOTE on str-qwua7.6: explore_with_oracle keeps growing; add a function-length ratchet

| field | value |
|---|---|
| action | **append comment to existing issue `str-qwua7.6`** (no new issue) |
| suggested priority for target | P3 |
| source findings | core-15 |

<!-- body -->
**Audit 2026-09-22 note** (findings: core-15)



### Current code facts
- Sizes at 16794cef: `orchestrator::explore_with_oracle` 1,373 lines (1,307 at the 09-04 audit), `parallel_scan_with_progress` 1,347, `explore_function` 966, `run_layer_batched` 476.
- Shrink-selection block still duplicated: explorer.rs:1700-1740 ↔ orchestrator.rs:3561-3600.

### Proposed changes / acceptance additions
- Add a CI function-length ratchet (fail when a listed function grows).
- Plan phase extraction (probe, main loop, refine, shrink) after str-qwua7.6.1.
