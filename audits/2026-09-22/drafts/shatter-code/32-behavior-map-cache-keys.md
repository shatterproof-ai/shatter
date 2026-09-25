# Behavior-map cache keyed by bare function name: scan never hits on unchanged re-scan, and same-named functions in different files overwrite each other

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | cache,behavior-map,scan,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-bo4z.11, str-fuhw, str-9m9o3, str-4ajhz |
| source findings | cli-ux-12, goals-14 |

<!-- body -->
## Problem

Scan looks up the behavior-map cache by qualified id but stores by `BehaviorMap.function_id` (bare name) without the fingerprint, so an unchanged re-scan skips nothing. Because the store key is the bare name, `a.ts:classify` and `b.ts:classify` share one `classify.json`, and revalidate can replay another file's map.

## Current code facts / evidence

- `shatter-core/src/scan_orchestrator.rs:4139-4150` lookup via `cache.is_fresh(func_name…)` / `load(func_name)` (qualified per the str-fuhw comment at :4270).
- Store: `cache.store(&result.behavior_map)` at scan_orchestrator.rs:3360 and :4931 (keys on map.function_id, no fingerprint); `shatter-core/src/cache.rs:118-133` recommends `store_with_fingerprint`; `cache.rs:291-295` `path_for(function_id)`.
- Two unchanged scans both report '0 expected skipped'. A March sample report showed '36 skipped (fingerprint match)'.
- Exploring a.ts:classify + b.ts:classify yields one `.shatter-cache/behavior-maps/classify.json` (function_id 'classify'); inputs sidecars are file-scoped.

## Acceptance criteria

- Store and load use the same qualified id (file-relative path + function) and store the fingerprint.
- CLI test: scan twice unchanged → expected_skipped == n on the second run.
- Test: two same-named functions in different files keep separate maps.
- revalidate refuses a map whose source file differs from its target.

## Suggested approach

Unify keying; migrate or invalidate old cache entries.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S-M

## References

- Audit findings: cli-ux-12, goals-14 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-bo4z.11, str-fuhw, str-9m9o3, str-4ajhz
