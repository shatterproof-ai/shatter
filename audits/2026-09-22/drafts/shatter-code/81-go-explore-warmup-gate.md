# Go explore lacks the cold-build warmup gate scan has; at default parallelism under load Go explores time out en masse

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P3 |
| labels | go,explore,parity,performance,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-tbk9e |
| source findings | goals-12 |

<!-- body -->
## Problem

str-tbk9e added BuildWarmupGate (first harness build solo, then fan out) to scan only. Explore spawns 16 workers on a cold cache.

## Current code facts / evidence

- `BuildWarmupGate` appears only in shatter-core/src/scan_orchestrator.rs.
- Repro (load 150-200, confounded): 'Spawned 1 frontend session(s) for 18 target(s) (16 parallel worker(s))' → 'all 18 attempted target(s) failed (... timed_out=43)', each 'request timed out after 30s'; `-w 2 --request-timeout 180` completes.

## Acceptance criteria

- Warmup gate shared by explore and scan (or Go explore caps parallelism on a cold cache).
- E2E: explore several Go files on a cold GOCACHE succeeds (re-verify on a quiet host).

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: goals-12 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-tbk9e
