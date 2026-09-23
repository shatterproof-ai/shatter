---
slug: validator-optional-command-warning
kind: new
title: "validate-protocol-registry prints a permanent get_invocation_plan 'may be unimplemented' warning although the matrix marks it not_implemented for Rust"
priority: P3
type: chore
labels: [protocol, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# validate-protocol-registry prints a permanent get_invocation_plan 'may be unimplemented' warning although the matrix marks it not_implemented for Rust

## Problem

Every run of the registry validator prints the same warning about an intended, documented gap. A warning that is always there teaches agents and humans to ignore validator warnings, including real ones.

## Evidence (re-verified 2026-09-23 at 56c86168)

- `python3 scripts/validate-protocol-registry.py` prints:
  ```
  Warnings:
    shatter-rust/src/protocol.rs + handler.rs:
      commands: 'get_invocation_plan' in registry but not found in shatter-rust (may be unimplemented)

  All checks passed (with informational warnings).
  ```
- `protocol/parity-matrix.yaml:175-185`: `get_invocation_plan` has `status: optional` with `frontends: {typescript: not_implemented, go: implemented, rust: not_implemented}`.
- The warning is emitted in `validate()` at `scripts/validate-protocol-registry.py:663-666`. The validator never reads the matrix.
- Audit finding prior-24 (confirmed, P3).

## Acceptance criteria

- [ ] The validator reads `protocol/parity-matrix.yaml` `commands.<cmd>.frontends.<fe>` and suppresses the "may be unimplemented" warning when that frontend's status is `not_implemented` or `not_supported` for an optional command. A missing command whose matrix status is `implemented` stays reported, and should be a hard error; coordinate with parity-dispatch-reconciliation.
- [ ] A clean run on current main prints no warnings.
- [ ] Unit test in `scripts/test_validate_protocol_registry.py`: a command absent from source and marked `not_implemented` produces no warning, and one marked `implemented` does. Proof at close: the first test fails before the change and passes after it.
- [ ] `task parity` passes when forced to execute.

## Suggested approach

Load the matrix with the same YAML loader `validate-parity.py` uses, and pass per-frontend status into `validate()`.

## Out of scope

TS extraction (validator-ts-extraction-empty, which is blocked by this issue so that re-pointing TS does not add a TS `get_invocation_plan` warning).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.7 (closed), validator-ts-extraction-empty, parity-dispatch-reconciliation.

Size: S. Priority: P3. Type: chore. Labels: protocol, parity, audit. Parent: Epic: Audit 2026-09-22 findings.
