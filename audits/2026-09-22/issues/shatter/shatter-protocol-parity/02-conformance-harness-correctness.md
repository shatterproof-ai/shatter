---
slug: conformance-harness-correctness
kind: new
title: "Conformance harness: timeouts cascade into misattributed failures, known_drifts can never match, summary arithmetic is wrong, analyze/execute success cases never cross-check"
priority: P2
type: bug
labels: [conformance, protocol, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Conformance harness: timeouts cascade into misattributed failures, known_drifts can never match, summary arithmetic is wrong, analyze/execute success cases never cross-check

## Problem

`protocol/conformance/conformance_harness.py` under-reports and mis-reports in four ways:

1. After a timeout it keeps using the same frontend process, so a late reply is read as the answer to the next case.
2. `known_drifts` patterns are regexes but are tested as substrings, so they never match.
3. The summary line prints `frontends × cases = <executed checks>`, which is not a product.
4. Every analyze/execute success case runs on a single frontend, so the structural cross-check never sees `side_effects` or conditions, which are the fields the drifts exist for.

## Evidence (re-verified 2026-09-23 at 56c86168)

- `conformance_harness.py:88-107` `send()` writes the request, calls `select` once with `COMMAND_TIMEOUT_S` (`:31`, 30 s), reads a single line, and does no id matching. When `resp is None`, the case loop (`:609-615`) records the failure and `continue`s on the same process. A drift-patrol run at load average 66/103/116 reported 4 Go timeouts and then `go / shutdown -- id: expected 99, got 20`. Reruns passed (`conformance_harness.py -f go` passed in 2.3 s; the full harness passed 42 checks).
- `conformance_harness.py:675`: `if any(pat in d for pat in known_drift_patterns):`. The patterns in `conformance_cases.yaml:16-24` are `side_effects.*thrown_error`, `side_effects.*global_mutation` and `condition.*ite`. On a sample drift string, the substring test gives [False, False, False] and `re.search` gives [True, False, False]. The first entry cites `side-effect-thrown-error-placement`, which does not exist in `parity-matrix.yaml`.
- `conformance_harness.py:682`: `print(f"Tested {n_frontends} frontends x {len(cases)} cases = {total_checks} checks")`. The drift-patrol log printed `Tested 4 frontends x 18 cases = 42 checks` (4×18 = 72) and contained 8 lines of `cross-check: SKIP only 1 frontend responded`.
- Every analyze/execute success case in `conformance_cases.yaml` (roughly :300-563, e.g. `execute_outcome_shape_go/ts/rust`) lists a single frontend. Only error, handshake, setup, teardown, generate and shutdown cases run on more than one frontend.
- Audit findings prior-05 (confirmed, P2), protocol-parity-02 (confirmed, P1 → P2), gates-10 (confirmed, P3; folded in here).

## Acceptance criteria

- [ ] `send()` reads lines until one arrives whose `id` equals the request id, discarding and logging stale lines. On a timeout, the frontend is killed, respawned and re-handshaken before the next case. One slow reply produces exactly one failure.
- [ ] known_drifts matching uses `re.search`. Every entry carries a `divergence_id` that `validate-parity.py` resolves against `allowed_divergences`. A run reports any entry that matched nothing. Alternative: delete known_drifts entirely and point GOVERNANCE, PARITY.md and the frontend-parity skill at `allowed_divergences`. The close note must say which option was chosen, because governance-md-omits-matrix and parity-guidance-skill-and-template follow it.
- [ ] The summary prints executed, skipped and cross-checked counts separately. No multiplication.
- [ ] At least one analyze success case and one execute success case run on every language frontend against equivalent fixtures, so the structural comparison covers the execute response (`side_effects`, `path_constraints`/conditions).
- [ ] Harness unit tests cover the id-mismatch/stale-line path, the respawn-after-timeout path and the regex-drift path, including an unmatched-drift report. Proof at close: each new test fails against the pre-fix harness and passes after the fix (paste both runs).
- [ ] `task conformance` passes after being forced to execute (not checksum-cached).

## Suggested approach

Fix `send()` and respawn first; this is what makes drift-patrol runs go red under load. Then fix drift matching (or delete it), then add the cross-frontend cases. Consider scaling `COMMAND_TIMEOUT_S` with a load-aware factor, as `run-heavy` does.

## Out of scope

- The Rust prepare timeout itself (str-qe9pp).
- Per-command dispatch coverage cases (parity-dispatch-reconciliation).
- Validating responses against JSON schemas (protocol-schemas-reject-real-output).

## Dependencies

- Blocked by: none.
- Related: str-qe9pp (open; Rust prepare timeout that triggers the id cascade), str-uoclg (closed; same cascade seen as a symptom), str-qwua7.34 (divergence-ID resolution), parity-guidance-skill-and-template, governance-md-omits-matrix.

Size: M. Priority: P2. Type: bug. Labels: conformance, protocol, parity, audit. Parent: Epic: Audit 2026-09-22 findings.
