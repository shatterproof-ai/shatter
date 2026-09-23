---
slug: conformance-known-drifts-matching
kind: new
title: "Conformance known_drifts can never match: patterns are tested as substrings and the structural comparator erases the values they name"
priority: P2
type: bug
labels: [conformance, protocol, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Conformance known_drifts can never match: patterns are tested as substrings and the structural comparator erases the values they name

Split out of conformance-harness-correctness (Codex cross-check: that draft bundled transport recovery, divergence policy, summary reporting and cross-language fixtures).

## Problem

`protocol/conformance/conformance_cases.yaml` declares `known_drifts` patterns so that accepted cross-frontend differences are reported as warnings instead of failures. The mechanism cannot work, for two independent reasons:

1. The patterns are regexes, but `conformance_harness.py` tests them as substrings, so a pattern such as `side_effects.*thrown_error` never matches.
2. Even with `re.search`, the drift strings come from `extract_structure()`, which replaces every string value with `"string"` and inspects only the first array element. Discriminator values that the patterns name (`ite` in `condition.*ite`, the `thrown_error` kind of a side effect) are erased before comparison. Two frontends that differ only in those values produce identical skeletons and no drift string at all.

Fixing only the regex would make the unit test pass while real drifts still go unreported. Agents are told (GOVERNANCE, the frontend-parity skill) to record accepted differences in `known_drifts`, so the dead mechanism also misdirects them away from `allowed_divergences` in `parity-matrix.yaml`.

## Evidence (re-verified 2026-09-23 at 56c86168; unchanged at 793f2b0b)

- `conformance_harness.py:675`: `if any(pat in d for pat in known_drift_patterns):` (substring test). Patterns are read at `:667-668`.
- `conformance_cases.yaml:16-24` patterns: `side_effects.*thrown_error`, `side_effects.*global_mutation`, `condition.*ite`. On a sample drift string, the substring test gives [False, False, False] and `re.search` gives [True, False, False].
- `conformance_harness.py:223-243` `extract_structure()`: `dict` → keys recursed; `list` → `array(<first element>)`; `str` → `"string"`. It is applied to each response at `:384` before comparison.
- The first known_drifts entry cites `side-effect-thrown-error-placement`, which is not a matrix ID. That dangling citation is already owned by str-qwua7.34 (open); do not duplicate its fix here.
- Audit finding protocol-parity-02 (confirmed, P1 → P2; drift-matching part).

## Acceptance criteria

- [ ] The close note records one of two options, because governance-md-omits-matrix and parity-guidance-skill-and-template word their guidance from it:
  - **Keep:** matching uses `re.search`. Each entry carries a `divergence_id` that `validate-parity.py` resolves against `allowed_divergences`, so an unresolvable id fails `task parity`. Reuse str-qwua7.34's resolution check if it has landed. The comparator keeps the values of discriminator fields (at least `kind`, `status`, `type` and enumerated fields named in the registry) and compares every array element, or a per-kind multiset, rather than the first element only. A run lists each known_drifts entry that matched nothing.
  - **Delete:** `known_drifts` and its matcher are removed. A cross-frontend drift then fails the case unless the difference is registered in `allowed_divergences`, and the harness reads that registry to downgrade the failure to a warning.
- [ ] Tests use the real comparator on populated responses, not a hand-written drift string. Fixture pairs of real-shaped `execute`/`analyze` responses from two frontends cover: (a) same shape but a different side-effect `kind` in the second array element; (b) a condition whose node kind differs (`ite` vs a non-ite kind); (c) a difference registered as intended (keep: a known_drifts entry; delete: an `allowed_divergences` entry). The tests assert that (a) and (b) are reported as drift, and that (c) is downgraded to a warning with its divergence id.
- [ ] Proof at close: tests (a) and (b) fail against the pre-fix harness (no drift reported) and pass after; paste both runs.
- [ ] `task conformance` and `task parity` pass on the unmutated tree; paste the output of runs that executed (not checksum-cached).

## Out of scope

- Transport recovery and the summary line (conformance-harness-correctness).
- Adding cross-frontend execute cases (conformance-cross-frontend-execute-cases, which depends on this).
- Purging dangling divergence IDs elsewhere (str-qwua7.34).

## Dependencies

- Blocked by: none.
- Blocks: conformance-cross-frontend-execute-cases.
- Related: str-qwua7.34 (open; dangling divergence IDs, including the known_drifts citation), governance-md-omits-matrix, parity-guidance-skill-and-template.

Size: M. Priority: P2. Type: bug. Labels: conformance, protocol, parity, audit. Parent: Epic: Audit 2026-09-22 findings.
