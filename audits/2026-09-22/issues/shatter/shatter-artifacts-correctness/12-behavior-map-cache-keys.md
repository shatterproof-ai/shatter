---
slug: behavior-map-cache-keys
kind: new
title: "Behavior-map cache is stored under the bare function name but looked up by qualified id: unchanged re-scans never hit, and same-named functions in different files overwrite each other"
priority: P2
type: bug
labels: [cache, behavior-map, scan, revalidate, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Behavior-map cache is stored under the bare function name but looked up by qualified id: unchanged re-scans never hit, and same-named functions in different files overwrite each other

## Problem

Scan looks up the behavior-map cache by the qualified function id (file path + name, since str-fuhw). It stores maps with `cache.store(&behavior_map)`, which keys on `BehaviorMap.function_id` (the bare name) and records no fingerprint. The two keys never meet, so the incremental "skip unchanged functions" feature never triggers: an unchanged re-scan re-explores everything. Because the stored key is the bare name, `a.ts:classify` and `b.ts:classify` share one `classify.json`. The last writer wins, and `revalidate` or mocking can load another file's map.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- Lookup by qualified id: `shatter-core/src/scan_orchestrator.rs:4141-4142` (`cache.is_fresh(func_name, dfp)` / `cache.load(func_name)`). The same pattern appears at :1600-1601 (the non-progress scan path) and in the callee prefetch at :4280 (str-fuhw comment at :4270-4275).
- Store by bare name without fingerprint: `cache.store(&func_result.behavior_map)` at `scan_orchestrator.rs:3360` and `cache.store(&result.behavior_map)` at `:4931` (also `:1840`).
- `shatter-core/src/cache.rs:123` `store(&self, map)` keys on `map.function_id`. `store_with_fingerprint` (:134) exists, and its doc comment says scan should use it. `path_for(function_id)` (:291) maps the id directly to `<cache>/behavior-maps/<id>.json`.
- Repro (`audits/2026-09-22/cli-ux-transcripts/scan-cache-1.*`, `scan-cache-2.*`): two scans of an unchanged directory both report `0 expected skipped`. A March sample report showed `36 skipped (fingerprint match)`, so this used to work.
- Repro (`audits/2026-09-22/goals-runs/collide/`): exploring `a.ts:classify` and `b.ts:classify` together produces one `.shatter-cache/behavior-maps/classify.json` with `function_id: "classify"`. The `.inputs.json` sidecars are file-scoped (`a.ts/classify.inputs.json`, `b.ts/classify.inputs.json`). Also, `01-arithmetic.ts`, `arithmetic-v1.ts` and `arithmetic-v2.ts` all write the same `classifyNumber.json`.

## Acceptance criteria

- [ ] Store and load use the same key: the qualified id made project-relative (relative source path + function name), never an absolute path. Store records the deep fingerprint (`store_with_fingerprint`) on every scan and explore path. Grep every `cache.store(` call site.
- [ ] Old cache entries (bare-name files without fingerprints) are ignored and never served as a hit. They may be deleted lazily. There is no silent migration that could pair a map with the wrong file.
- [ ] CLI test: scan a fixture dir twice with no changes. The second run reports `expected_skipped == n` (all functions). At close, show the test failing on current `main` and passing after the fix.
- [ ] Test: two same-named functions in different files keep separate maps with their own return values.
- [ ] Stored maps record their source file (add the field if `BehaviorMap` lacks it). `revalidate` refuses, with exit 2 and a clear message, any map whose recorded source file differs from its `<SOURCE>` argument.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Make `BehaviorMapCache` take an explicit key (`store(key, map, fingerprint)`) instead of reading `map.function_id`. Derive the key in one helper shared by scan, explore and revalidate. Encode the key into a bounded, filesystem-safe file name (relative path segments + short hash). The same naming rule is needed in scan-artifact-filenames-abs-path, so share the helper if both are in flight.

## Out of scope

- Scan's `--seed` not being part of cache freshness (str-9m9o3).
- Factoring the restore-or-skip logic into a helper (str-4ajhz). It can follow this fix.
- Explore resume keying (explore-resume-options-key).

## Priority

P2: incremental scan is silently disabled, and cross-file map collisions can mislead mocking and revalidate.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-fuhw (closed; qualified lookup ids), str-bo4z.11 (closed; explore writes fingerprinted maps), str-9m9o3 (open), str-4ajhz (open), scan-artifact-filenames-abs-path, revalidate-return-values.

## References

Audit 2026-09-22 findings cli-ux-12 (partially verified P2) and goals-14 (verified P2). Source draft: `drafts/shatter-code/32-behavior-map-cache-keys.md`.
