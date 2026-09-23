---
slug: spec-json-shapes-compare
kind: new
title: "compare rejects the --spec-out bundle (the only clean producer); add one versioned spec reader in core that owns legacy-schema reading"
priority: P2
type: bug
labels: [spec, cli, artifacts, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# `compare` rejects the `--spec-out` bundle (the only clean producer); add one versioned spec reader in core that owns legacy-schema reading

## Problem

Shatter writes spec JSON in two shapes, and its consumers disagree about which they accept:

- `explore --spec-out` writes a versioned `FileSpecBundle` (`{version, file, functions: [...]}`).
- `explore --spec-json` on stdout emits a bare `FunctionSpec`, after the markdown report (str-qwua7.11).
- `properties` (and `specify --yaml`) emit YAML through serialize-only view structs. No command reads that YAML back.

`compare` only deserializes a bare `FunctionSpec`, so it cannot read the `--spec-out` file, which is the only producer that writes a clean file. Users who follow the documented flow (explore TS and Go with `--spec-out`, then `compare`) get a parse error. `spec-diff` has its own private reader (`SpecInput` in `shatter-cli/src/commands/diff.rs`) that accepts both JSON shapes and keeps the bundle `version` for its schema-mismatch report, but `compare` does not share it. Given a TS and a Go bundle, `spec-diff` prints "Added functions: ClassifyNumber / Removed functions: classifyNumber" with no hint that `compare` is the right tool.

This issue also makes the shared reader the single owner of legacy-schema reading. spec-yaml-custom-tags and spec-preconditions-from-path-constraints both change the spec JSON schema; maintainer decision D2 makes `spec-diff` the only regression tool, so a spec written by an older build must stay readable. Both of those issues are blocked by this one and plug their upgrade step into this reader.

This is the code half of finding docs-03. The docs half (SPEC §5 producer/consumer table) is spec-s5-contract-table-and-samples, which this issue blocks.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 793f2b0b; code identical to 56c86168):

- `shatter-cli/src/commands/compare.rs:19-22`: both inputs are parsed with `serde_json::from_str::<shatter_core::spec::FunctionSpec>`.
- `shatter-cli/src/commands/diff.rs:43-89`: `enum SpecInput { Function(FunctionSpec), Bundle(FileSpecBundle) }` detects the shape by a top-level `functions` array, exposes `version()` (bundle version, `None` for a bare spec) and flattens with `into_specs()`. `SpecVersionMismatch` (:91-104) reports an old/new version difference without failing.
- `shatter-core/src/spec.rs:252` (`SPEC_SCHEMA_VERSION = 1`; bundles without the field deserialize as version 0), `:259` (`pub struct FileSpecBundle`), `:304` (`pub struct FunctionSpec`).
- `shatter-core/src/spec.rs:675-714`: the YAML views (`SpecClassYaml`, `FunctionSpecYaml`) are `#[derive(Serialize)]` only and replace `ClassifiedInvariant` with `SpecInvariant`. The YAML is therefore a different, lossy representation, not a second encoding of the JSON model.
- Captured output (on branch `audit-2026-09-22` until the audit reports land):
  - `audits/2026-09-22/artifact-samples/compare-ts-go.txt`: `Error: failed to parse spec A '.../ts-spec-out.json': missing field `function_name` at line 172 column 1` (exit 2).
  - `compare-ts-go-bare.txt`: with hand-extracted bare specs, `4 of 4 shared behaviors match — 100% equivalent`.
- The verifier reproduced `compare s.json s.json` on a `--spec-out` bundle exiting 2 while `spec-diff s.json s.json` exits 0.

Repro: `shatter explore --spec-out ts.json <examples>/standalone/ts/01-arithmetic.ts:classifyNumber`, the same for `go/01-arithmetic.go:ClassifyNumber` into `go.json`, then `shatter compare ts.json go.json`.

## Data contract for the shared reader

The reader must not flatten away information that consumers use. It returns a document, not a bare `Vec<FunctionSpec>`:

- `SpecDocument { source_path, shape: Bundle | BareFunction, schema_version: Option<u32>, file: Option<String>, functions: Vec<FunctionSpec> }`. `schema_version` is the bundle's `version` (0 for pre-versioning bundles) and `None` for a bare spec. `file` is the bundle's `file`.
- Functions are addressed by `(file, function_name)`. When two entries in one document share a name, selecting by bare name is an error that lists the qualified candidates. Nothing is silently chosen or merged.
- Input is JSON only. A YAML file (for example `properties` output) is rejected with an error that says YAML spec output is write-only and names the JSON producers (`explore --spec-out`, `explore --spec-json`).
- Version handling: versions `0..=SPEC_SCHEMA_VERSION` are read, upgrading older versions in memory through an ordered list of upgrade steps (empty today; spec-yaml-custom-tags and spec-preconditions-from-path-constraints each add one). A version newer than the binary's `SPEC_SCHEMA_VERSION` is an error naming both versions, exit code 2. The original `schema_version` is kept on the document after upgrade so `spec-diff` can still report `SpecVersionMismatch`.
- Legacy fixtures: `shatter-core/tests/fixtures/spec-legacy/` holds one real bundle and one bare spec per supported version (v0 = the current bundle with the `version` field removed, v1 = current). Each later schema bump adds its predecessor's fixture there.

## Acceptance criteria

- [ ] `shatter-core` exposes the reader described above (for example `spec::read_spec_document(path) -> Result<SpecDocument, SpecReadError>`). `SpecInput` is removed from `diff.rs`; `compare`, `spec-diff`, `stale` and `revalidate` (wherever they read spec JSON) all call the core reader. The close comment lists each consumer switched, with file:line.
- [ ] `compare` accepts bundles and bare specs. With one function per side, no flag is needed. With several, `compare --function A[=B]` selects them (qualified `file::name` accepted); an ambiguous or missing name is an error listing the candidates.
- [ ] `spec-diff` output and exit codes are unchanged for every existing `shatter-cli/tests` spec-diff test, and it still reports `SpecVersionMismatch` for a v0-vs-v1 pair (test on the legacy fixtures).
- [ ] `spec-diff` suggests `compare` when the only added and removed functions differ by case or by language naming convention (for example `classifyNumber` / `ClassifyNumber` / `classify_number`).
- [ ] A CLI round-trip test runs `explore --spec-out` on TS `classifyNumber` and Go `ClassifyNumber` with a fixed `--max-iterations` budget and `--no-seeds` (`explore` has no `--seed` flag today; only `scan` does), then `compare` on the two files, and asserts exit 0 and "4 of 4". Close-time proof: the test failing on current code (parse error) and passing after the fix, both pasted into the close comment.
- [ ] Reader unit tests cover: v0 bundle, v1 bundle, bare spec, a version newer than the binary (error names both versions), a YAML file (error names the JSON producers), duplicate function names in one bundle (bare-name selection errors), and malformed JSON (error names the accepted shapes). A proptest checks that serializing any generated `FileSpecBundle` and reading it back yields the same functions, `file` and version.

## Suggested approach

Move `SpecInput` into `shatter-core/src/spec.rs` (or a new `spec_io.rs`), extend it with the fields above, and switch the consumers. Keep the upgrade-step list as a plain `match` on version so each schema bump adds one arm and one fixture.

## Out of scope

- Reading YAML specs.
- The markdown-before-JSON stdout mixing of `--spec-json` (str-qwua7.11).
- SPEC documentation of the artifact table (spec-s5-contract-table-and-samples).
- The retired snapshot `shatter diff` command (maintainer decision D2; retire-snapshot-diff).

## Related

str-wfqh, str-nq20, str-qwua7.11; blocks spec-s5-contract-table-and-samples, spec-yaml-custom-tags and spec-preconditions-from-path-constraints. Source findings: artifacts-09 (confirmed), docs-03 (code half).
