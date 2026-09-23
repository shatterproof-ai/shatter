---
slug: spec-yaml-custom-tags
kind: new
title: "Spec/properties YAML uses serde custom tags (!AllEqual) that standard YAML loaders reject"
priority: P2
type: bug
labels: [spec, serialization, properties, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
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

Python `yaml.safe_load` rejects it: `ConstructorError: could not determine a constructor for the tag '!AllEqual'`. Any consumer that uses a safe YAML loader cannot read Shatter's spec or properties YAML. The JSON form of the same data is `{"AllEqual":{...}}`, which parses fine.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 56c86168):

- `shatter-core/src/equivalence.rs:85-105`: `pub enum Precondition { AllPositive{..}, AllNegative{..}, AllZero{..}, AllEqual{..}, SameType{..} }` with plain `#[derive(Serialize, Deserialize)]`, which makes it externally tagged. serde_yaml renders externally tagged enum variants as YAML custom tags.
- `shatter-core/src/spec.rs:680-786`: the YAML view structs (`SpecClassYaml` has `preconditions: &'a Vec<Precondition>` at :691), `format_spec_yaml` (:765) and `format_file_spec_yaml` (:773) serialize those enums directly.
- The YAML tests in spec.rs (for example `yaml_bundle_includes_version` at :2077) use string-contains assertions and never parse the output with a standard loader.
- Captured sample: `audits/2026-09-22/artifact-samples/properties.yaml` has `- !AllEqual` at lines 14, 49 and 88 (on branch `audit-2026-09-22` until the audit reports land).

Repro: `shatter properties <examples>/standalone/ts/01-arithmetic.ts:classifyNumber > p.yaml && python3 -c 'import yaml; yaml.safe_load(open("p.yaml"))'`.

## Acceptance criteria

- [ ] Spec YAML and properties YAML load with a strict safe loader. A test runs PyYAML `safe_load` (or a Rust YAML parser with no custom-tag support) on the YAML output for every known-answer example. It fails on current code and passes after the fix; record both runs in the close comment.
- [ ] Every serialized enum in the spec model (`Precondition`, and any other externally tagged enum reachable from `FunctionSpec`/`FileSpecBundle`) uses an internally tagged form, for example `{kind: all_equal, param_index: 0, value: 0}`, in both JSON and YAML.
- [ ] The spec bundle schema version is bumped and the spec changelog has a row for the change.
- [ ] `spec-diff` (now the only regression tool, maintainer decision D2) still reads bundles written with the old shape, proven by a test that diffs an old-shape fixture against a new-shape one. If backward reading is dropped instead, the changelog and the error message say so.

## Suggested approach

Add `#[serde(tag = "kind", rename_all = "snake_case")]` to the enums, plus a compatibility deserializer (untagged fallback) for the old externally tagged form. Coordinate the version bump with spec-preconditions-from-path-constraints and spec-json-shapes-compare so the schema changes once, not three times.

## Out of scope

Redesigning what the preconditions mean (see spec-preconditions-from-path-constraints).

## Related

str-qwua7.38 (its description notes that preconditions are externally tagged enums). Source finding: artifacts-14 (confirmed).
