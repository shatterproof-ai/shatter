# shatter-rust: crate type registry rebuilt on every analyze (O(N²) parses) and two independent Axum extractor classifiers

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | refactor |
| priority | P3 |
| labels | rust-frontend,performance,axum,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | none |
| source findings | frontend-rust-12, frontend-rust-14 |

<!-- body -->
## Problem

Two design inefficiencies in shatter-rust.

## Current code facts / evidence

- `shatter-rust/src/analyzer.rs:83`, `:201` call `build_crate_type_registry(file_path)` (`:907-952`) on every analyze: walks src/ and syn-parses every file; no cache.
- `shatter-rust/src/adapters.rs:389-470` AxumExtractorKind (12 kinds, by ParamInfo.type_name) vs `executor.rs:1796-1838` AxumExtractor (5 kinds, re-parses type strings with syn) used at executor.rs:2167, 2195, 2222, 2452, 2776, 4989, 6571, 6693.

## Acceptance criteria

- Registry cached on the Handler keyed by crate root + cheap fingerprint (max mtime, file count), invalidated on teardown/shutdown; multi-file scan timing recorded.
- One classifier in adapters.rs returns kind + inner type; executor consumes it; test asserts agreement for every type in AXUM_EXTRACTOR_TYPES.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: frontend-rust-12, frontend-rust-14 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: none
