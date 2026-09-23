# Spec/properties YAML uses serde custom tags (`!AllEqual`) that standard YAML loaders reject

- Priority: P2
- Type: bug
- Labels: spec,serialization,properties
- Tracker action: new issue
- Related: str-qwua7.38 (notes preconditions are externally tagged enums)
- Source findings: audit 2026-09-22 artifacts-14 (confirmed)

<!-- body -->
## Problem
`shatter properties ts/01-arithmetic.ts:classifyNumber` emits YAML such as:
```yaml
- !AllEqual
  param_index: 0
  value: 0
```
Python `yaml.safe_load` fails: `ConstructorError: could not determine a constructor for the tag '!AllEqual'`. The JSON form of the same data is `{"AllEqual":{...}}`, which is fine.

## Current code facts
- YAML serialization lives in `shatter-core/src/spec.rs:684-790`. `Precondition` (and possibly other enums) is an externally tagged serde enum, which serde_yaml renders as a custom tag.
- The YAML tests only use string-contains assertions.

## Acceptance criteria
- Spec and properties YAML load with a strict YAML 1.2 safe loader (a test runs PyYAML `safe_load`, or a Rust YAML parser without tag support, on the output of every example).
- Enums use an internally tagged form (e.g. `{kind: all_equal, param_index: 0, value: 0}`) in both JSON and YAML, with a spec-bundle schema version bump and a changelog row. `spec-diff` must still read old bundles, or document that it no longer does.

## Scope
In: serialization shape and version bump. Out: redesigning the preconditions themselves.
