# Concolic refine phase drops prepare_id/execution_profile and discards the paths it reaches; no shared Execute builder

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | concolic,orchestrator,refactor,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qwua7.5, str-qwua7.6, str-inct |
| source findings | core-06, core-16 (context; dup-open str-qwua7.5) |

<!-- body -->
## Problem

`refine_boundaries_async` builds its own Execute requests without `prepare_id`/`execution_profile` (dropping TS execution adapters and forcing re-preparation) and uses results only to update boundary witnesses, never coverage or raw_results. The root cause is >8 inline `Command::Execute` constructions per engine, each re-deciding fields (also the cause of str-qwua7.5's capture bug).

## Current code facts / evidence

- `shatter-core/src/orchestrator.rs:2321-2420` refine; `:2372-2383` sends `prepare_id: None, execution_profile: None`.
- Concolic Classify artifact: boundary_results true_witness `[0.5000037571385455]` (20 executions) while unique_paths=2; report 4/5 lines.
- Capture literals hard-coded at orchestrator.rs:1694, 2380, 2701, 2714, 3103, 3643, 3687, 3741 (str-qwua7.5).

## Acceptance criteria

- A single helper builds every Execute request from the engine config (prepare_id, execution_profile, setup_context, capture) and is used by all sites in both engines.
- Refine executions flow through normal observation/coverage accounting.
- Test: a TS execution-adapter target keeps its execution_profile during refine; a path reached only in refine appears in the report.

## Suggested approach

Build the helper, migrate sites, then close str-qwua7.5 with it (append the scope note there).

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: core-06, core-16 (context; dup-open str-qwua7.5) (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.5, str-qwua7.6, str-inct
