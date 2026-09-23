# Concolic explorer does not beat the default explorer on the project's hard examples; add a benchmark and investigate early worklist termination

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | task |
| priority | P2 |
| labels | concolic,benchmark,audit |
| parent | audit epic (draft 00) |
| blocked by | draft 22 |
| related | str-ior1, str-2fui, str-qwua7.6 |
| source findings | goals-08 |

<!-- body -->
## Problem

On 21 hard TS functions, concolic stopped after 21-35 iterations on most and scored 36.1% lines (164/454) vs an unverified 40.7% for default. Kapow notes from July recorded zero coverage delta. str-ior1 (re-baseline with --concolic) closed with reason 'Closed' and no data.

## Current code facts / evidence

- `audits/2026-09-22/goals-runs/ts-sub-concolic.err`: computeArea 21 iters 0/6 branches; matchRoute 21 iters 1/19; negotiateLanguage 23 iters 1/12; classifyStatus 21 iters 0/3.
- Default-baseline number not independently reproduced (must be re-measured with fresh artifacts; see draft 22 resume bug).
- Some losses are downstream of drafts 11 (sort split), 41 (TS ternary/switch) and 79 (discriminant literals).

## Acceptance criteria

- Benchmark task comparing default vs concolic on the examples corpus with fresh artifacts and fixed seeds; results committed/published per release.
- Root cause of concolic stopping at ~21 iterations (worklist exhaustion / plateau) documented and fixed or filed.
- README/SPEC positioning matches measured results.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: goals-08 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-ior1, str-2fui, str-qwua7.6
