---
slug: spec-json-shapes-compare
kind: new
title: "Three incompatible spec JSON shapes; compare rejects the --spec-out bundle (the only clean producer)"
priority: P2
type: bug
labels: [spec, cli, artifacts, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Three incompatible spec JSON shapes; `compare` rejects the `--spec-out` bundle (the only clean producer)

## Problem

Shatter writes specs in three shapes:

- `explore --spec-out` writes a versioned `FileSpecBundle` (`{version, file, functions: [...]}`).
- `explore --spec-json` on stdout emits a bare `FunctionSpec`, after the markdown report (str-qwua7.11).
- `properties` emits a YAML list of bundles.

`compare` only deserializes a bare `FunctionSpec`, so it cannot read the `--spec-out` file, which is the only producer that writes a clean file. Users who follow the documented flow (explore TS and Go with `--spec-out`, then `compare`) get a parse error. `spec-diff` accepts bundles but, given a TS and a Go bundle, prints "Added functions: ClassifyNumber / Removed functions: classifyNumber" with no hint that `compare` is the right tool.

This is the code half of finding docs-03. The docs half (SPEC §5 producer/consumer table) is spec-s5-contract-table-and-samples, which this issue blocks.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 56c86168):

- `shatter-cli/src/commands/compare.rs:19-22`: both inputs are parsed with `serde_json::from_str::<shatter_core::spec::FunctionSpec>`.
- `shatter-core/src/spec.rs:252` (bundle version constant), `:259` (`pub struct FileSpecBundle`), `:304` (`pub struct FunctionSpec`).
- Captured output (on branch `audit-2026-09-22` until the audit reports land):
  - `audits/2026-09-22/artifact-samples/compare-ts-go.txt`: `Error: failed to parse spec A '.../ts-spec-out.json': missing field `function_name` at line 172 column 1` (exit 2).
  - `compare-ts-go-bare.txt`: with hand-extracted bare specs, `4 of 4 shared behaviors match — 100% equivalent`.
- The verifier reproduced `compare s.json s.json` on a `--spec-out` bundle exiting 2 while `spec-diff s.json s.json` exits 0.

Repro: `shatter explore --spec-out ts.json <examples>/standalone/ts/01-arithmetic.ts:classifyNumber`, the same for `go/01-arithmetic.go:ClassifyNumber` into `go.json`, then `shatter compare ts.json go.json`.

## Acceptance criteria

- [ ] One shared spec reader in `shatter-core` accepts a bundle, a bare spec and a bundle list. `compare`, `spec-diff`, `stale` and `revalidate` (where they read specs) all use it; the close comment lists each consumer switched.
- [ ] `compare --function A[=B]` selects functions from multi-function bundles. With one function per side, no flag is needed.
- [ ] `spec-diff` suggests `compare` when the added and removed function names differ only by case or language convention.
- [ ] A CLI round-trip test runs `explore --spec-out` for a TS and a Go known-answer function, then `compare` on the two files, and asserts success and "4 of 4". It fails on current code; record both runs.
- [ ] Error messages for an unrecognized shape name the accepted shapes.

## Suggested approach

Add `spec::read_specs(path) -> Vec<FunctionSpec>` (or similar) in core that detects the shape, then switch the consumers. Coordinate any schema-version change with spec-yaml-custom-tags and spec-preconditions-from-path-constraints.

## Out of scope

- The markdown-before-JSON stdout mixing of `--spec-json` (str-qwua7.11).
- SPEC documentation of the artifact table (spec-s5-contract-table-and-samples).
- The retired snapshot `shatter diff` command (maintainer decision D2; retire-snapshot-diff).

## Related

str-wfqh, str-nq20, str-qwua7.11; blocks spec-s5-contract-table-and-samples. Source findings: artifacts-09 (confirmed), docs-03 (code half).
