# Conformance harness: timeouts cascade into misattributed failures, known_drifts can never match, summary arithmetic is wrong, 8/18 cases have no cross-frontend check

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | conformance,protocol,parity,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qe9pp, str-uoclg, str-qwua7.34 |
| source findings | prior-05, protocol-parity-02, gates-10 |

<!-- body -->
## Problem

The conformance harness under-reports and mis-reports. A slow reply is read as the next case's response; `known_drifts` patterns are regexes matched as substrings so they never match; the summary line multiplies frontends by cases but prints an executed-check count; and no success-path analyze/execute case runs on more than one frontend, so structural cross-checks never see side_effects/conditions.

## Current code facts / evidence

- `protocol/conformance/conformance_harness.py:88-107` `send()` waits once (`COMMAND_TIMEOUT_S`, :31) and reads one line with no id match; on None the loop at :609-615 `continue`s on the same process. Under load: `go / shutdown -- id: expected 99, got 20` after 4 Go timeouts; reruns pass.
- `conformance_harness.py:667-676`: `any(pat in d for pat in known_drift_patterns)`; patterns in `conformance_cases.yaml:13-24` are `side_effects.*thrown_error` etc. First entry cites `side-effect-thrown-error-placement`, not a parity-matrix ID.
- `conformance_harness.py:682` prints `Tested 4 frontends x 18 cases = 42 checks` (4x18=72); log shows 8 `cross-check: SKIP only 1 frontend responded`.
- Every analyze/execute success case in `conformance_cases.yaml:302-563` lists a single frontend.

## Acceptance criteria

- `send()` reads until a line whose `id` equals the request id (discarding stale lines); on timeout the frontend is killed, respawned and re-handshaken before the next case; one timeout yields one failure.
- known_drifts use `re.search`, each carries a `divergence_id` resolved against parity-matrix `allowed_divergences`, and an entry that matches nothing in a run is reported (or the mechanism is deleted and docs point at allowed_divergences).
- Summary prints executed / skipped / cross-checked counts explicitly.
- At least one analyze and one execute success case runs on all language frontends against equivalent fixtures so structural comparison covers the execute response.
- Harness unit tests cover the id-mismatch and regex-drift paths.

## Suggested approach

Fix send()/respawn first (it causes flaky red drift-patrol runs), then drift matching, then add cross-frontend cases.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: prior-05, protocol-parity-02, gates-10 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qe9pp, str-uoclg, str-qwua7.34
