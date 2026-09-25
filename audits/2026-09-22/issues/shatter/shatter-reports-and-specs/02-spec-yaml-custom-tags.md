---
slug: spec-yaml-custom-tags
kind: new
title: "Spec/properties YAML uses serde custom tags (!AllEqual) that standard YAML loaders reject"
priority: P2
type: bug
labels: [spec, serialization, properties, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [spec-json-shapes-compare]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Spec/properties YAML uses serde custom tags (`!AllEqual`) that standard YAML loaders reject

## Problem

`shatter properties ts/01-arithmetic.ts:classifyNumber` emits YAML like this:

```yaml
- !AllEqual
  param_index: 0
  value: 0
```

Python `yaml.safe_load` rejects it: `ConstructorError: could not determine a constructor for the tag '!AllEqual'`. Any consumer that uses a safe YAML loader cannot read Shatter's spec or properties YAML. The JSON form of the same data is `{"AllEqual":{...}}`, which parses fine but uses a different shape from the rest of the spec model (`SymConstraint`, for example, is already internally tagged with `kind`).

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 793f2b0b; code identical to 56c86168):

- `shatter-core/src/equivalence.rs:85-105`: `pub enum Precondition { AllPositive{..}, AllNegative{..}, AllZero{..}, AllEqual{..}, SameType{..} }` with plain `#[derive(Serialize, Deserialize)]`, which makes it externally tagged. serde_yaml renders externally tagged enum variants as YAML custom tags.
- `shatter-core/src/execution_record.rs:17-25`: `SymConstraint` already uses `#[serde(tag = "kind", rename_all = "snake_case")]`, the target shape.
- `shatter-core/src/spec.rs:675-786`: the YAML view structs (`SpecClassYaml` has `preconditions: &'a Vec<Precondition>` at :691), `format_spec_yaml` (:765) and `format_file_spec_yaml` (:773) serialize those enums directly. The views are serialize-only; no Shatter command reads YAML back.
- The YAML tests in spec.rs (for example `yaml_bundle_includes_version` at :2077) use string-contains assertions and never parse the output with a standard loader.
- Captured sample: `audits/2026-09-22/artifact-samples/properties.yaml` has `- !AllEqual` at lines 14, 49 and 88 (on branch `audit-2026-09-22` until the audit reports land).

Repro: `shatter properties <examples>/standalone/ts/01-arithmetic.ts:classifyNumber > p.yaml && python3 -c 'import yaml; yaml.safe_load(open("p.yaml"))'`.

## Compatibility contract (maintainer decision D2)

`spec-diff` is the only regression tool, and CI baselines are spec JSON written by older builds. Old JSON must stay readable; there is no "drop backward reading" option.

- This change bumps `SPEC_SCHEMA_VERSION` by one (to N+1, where N is the value when this lands) and adds one upgrade step to the shared spec reader from spec-json-shapes-compare (this issue is blocked by it). The step rewrites externally tagged enum values from version N into the new internally tagged form. A plain `#[serde(untagged)]` fallback on the enum is not enough on its own, because the reader must know which version it upgraded from.
- Legacy fixture: the version-N bundle is added to `shatter-core/tests/fixtures/spec-legacy/` before the shape changes.
- Mixed versions: `spec-diff old(vN).json new(vN+1).json` gives the same classes, verdicts and exit code as `spec-diff` on two vN+1 files produced from the same sources, and additionally reports the existing `SpecVersionMismatch` note.
- YAML is output-only. It gets the new shape with no compatibility path.

## Acceptance criteria

- [ ] Spec YAML and properties YAML load with a strict safe loader. A test runs PyYAML `safe_load` (or a Rust YAML parser configured to reject unknown tags) on the YAML output for TS `classifyNumber`, Go `ClassifyNumber` and Rust `classify_number`, and on a synthetic spec that contains every `Precondition` variant. Close-time proof: that test failing on current code and passing after the fix, both pasted into the close comment.
- [ ] Every enum reachable from `FunctionSpec` / `FileSpecBundle` / the YAML views that serializes externally tagged today uses `#[serde(tag = "kind", rename_all = "snake_case")]` (for example `{kind: all_equal, param_index: 0, value: 0}`) in both JSON and YAML. The close comment lists each enum changed; a test fails if any YAML output contains a `!` tag.
- [ ] `SPEC_SCHEMA_VERSION` is bumped by one, and its doc comment (`shatter-core/src/spec.rs:236-252`, which is the schema's changelog under the bump policy there; there is no separate spec changelog file) gets a `- vN+1:` line naming this issue.
- [ ] Legacy reading: `spec-diff` on the version-N legacy fixture against a fresh bundle of the same source exits 0 with no ADDED, REMOVED, PRECOND or CHANGED rows, and prints the version-mismatch note. `compare` on the legacy fixture and a fresh bundle reports 100% equivalent. Both tests are in the test suite, not only run once.
- [ ] `task affected` passes, and its `Gates selected` output is recorded in the close comment.

## Suggested approach

Change the serde attributes, add the upgrade step to the shared reader, and add a YAML-parse test helper that shells out to `python3 -c 'import yaml,sys; yaml.safe_load(sys.stdin)'` or uses a strict Rust parser. If spec-preconditions-from-path-constraints is in flight at the same time, land one first; the second bumps the version again and adds its own upgrade step and fixture.

## Out of scope

- Redesigning what the preconditions mean (spec-preconditions-from-path-constraints).
- Reading YAML specs.

## Related

str-qwua7.38 (its description notes that preconditions are externally tagged enums). Blocked by spec-json-shapes-compare (shared versioned reader). Source finding: artifacts-14 (confirmed).
