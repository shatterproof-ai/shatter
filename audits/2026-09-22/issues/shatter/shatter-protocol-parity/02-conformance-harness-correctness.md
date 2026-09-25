---
slug: conformance-harness-correctness
kind: new
title: "Conformance harness: a timeout leaves the frontend process in use, so late replies are misattributed to later cases, and the summary line multiplies unrelated counts"
priority: P2
type: bug
labels: [conformance, protocol, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Conformance harness: a timeout leaves the frontend process in use, so late replies are misattributed to later cases, and the summary line multiplies unrelated counts

## Problem

`protocol/conformance/conformance_harness.py` mis-reports in two ways that make drift-patrol runs go red under load:

1. After a timeout it keeps using the same frontend process and reads one line per request with no id matching, so a late reply is read as the answer to the next case. One slow reply turns into a cascade of unrelated failures.
2. The summary prints `frontends × cases = <executed checks>`, which is not a product, so a reader cannot tell how many checks ran, were skipped or were cross-checked.

Cases are not independent: some depend on state an earlier case left in the frontend process. For example, `planner_runtime_value_go` (`conformance_cases.yaml:538`) runs `get_invocation_plan`, which needs the analysis cached by the preceding `analyze_runtime_value_go` (`:520`); the Go handler says so at `shatter-go/protocol/handler.go:1960` and looks it up in `cachedAnalyses` at `:1981`. A naive "respawn and re-handshake after a timeout" therefore produces a second, misleading failure on the dependent case. The fix must handle prerequisites explicitly.

This issue covers transport recovery and the summary only. The known_drifts matcher and the missing cross-frontend execute cases were split into conformance-known-drifts-matching and conformance-cross-frontend-execute-cases.

## Evidence (re-verified 2026-09-23 at 56c86168; unchanged at 793f2b0b)

- `conformance_harness.py:88` `send()` writes the request, calls `select` once with `COMMAND_TIMEOUT_S` (`:31`, 30 s), reads a single line, and does no id matching. When the response is `None`, the case loop (around `:608-615`) records the failure and continues on the same process.
- A drift-patrol run at load average 66/103/116 reported 4 Go timeouts and then `go / shutdown -- id: expected 99, got 20`. Reruns passed (`conformance_harness.py -f go` passed in 2.3 s; the full harness passed 42 checks).
- `conformance_harness.py:682`: `print(f"Tested {n_frontends} frontends x {len(cases)} cases = {total_checks} checks")`. The drift-patrol log printed `Tested 4 frontends x 18 cases = 42 checks` (4×18 = 72) and contained 8 lines of `cross-check: SKIP only 1 frontend responded`.
- Case-order dependency: `analyze_runtime_value_go` (`conformance_cases.yaml:520`) → `planner_runtime_value_go` (`:538`); `handler.go:1960` ("get_invocation_plan must have previously issued analyze for the target's …").
- Audit findings prior-05 (confirmed, P2), protocol-parity-02 (confirmed, P1 → P2; transport part), gates-10 (confirmed, P3; summary line).

## Acceptance criteria

- [ ] `send()` reads lines until one arrives whose `id` equals the request id, discarding and logging stale lines (with their ids), bounded by the case timeout.
- [ ] On a timeout, the frontend process is killed, respawned and re-handshaken before the next case.
- [ ] Prerequisites are explicit. Each case that depends on earlier state declares it (for example `requires: [analyze_runtime_value_go]` in `conformance_cases.yaml`). After a respawn, the harness either replays the declared prerequisites on the new process before the dependent case, or reports the dependent case as `BLOCKED (prerequisite <name> failed)`. A blocked case counts as neither a pass nor an independent failure. A test fails when a case uses cross-case state without declaring it. At minimum, audit every get_invocation_plan, prepare and execute case for an implicit dependency, and record the list in the close note.
- [ ] One slow reply produces exactly one failure. The cases that depend on it are reported as blocked or pass after replay, and the id-mismatch cascade is gone.
- [ ] The summary prints executed, passed, failed, blocked, skipped (with reason counts) and cross-checked counts separately, with no multiplication. The exit status is non-zero when any case failed or was blocked.
- [ ] Harness unit tests drive a stub frontend (a small script that speaks the protocol and can be told to delay one reply past the timeout, then send it late). They cover: (a) the stale-line discard; (b) respawn after timeout; (c) a dependent case after its prerequisite timed out (replayed or blocked, never misattributed); (d) the summary counts for a run with one timeout. Proof at close: each test fails against the pre-fix harness and passes after the fix; paste both runs.
- [ ] `task conformance` passes on the unmutated tree; paste the output of a run that executed (not checksum-cached).

## Suggested approach

Fix `send()` first; id matching alone removes the cascade. Then add respawn with prerequisite replay. Replaying declared prerequisites is cheaper for readers than blocked cases, but blocking is acceptable when a prerequisite is itself slow. Consider scaling `COMMAND_TIMEOUT_S` with a load-aware factor, as `run-heavy` does.

## Out of scope

- The Rust prepare timeout itself (str-qe9pp).
- known_drifts matching and the comparator (conformance-known-drifts-matching).
- Cross-frontend analyze/execute cases (conformance-cross-frontend-execute-cases).
- Per-command success cases (conformance-success-case-per-command).
- Validating responses against JSON schemas (protocol-schemas-reject-real-output).

## Dependencies

- Blocked by: none.
- Blocks: conformance-success-case-per-command and conformance-cross-frontend-execute-cases (both add cases that rely on the prerequisite mechanism).
- Related: str-qe9pp (open; Rust prepare timeout that triggers the id cascade), str-uoclg (closed; same cascade seen as a symptom).

Size: M. Priority: P2. Type: bug. Labels: conformance, protocol, parity, audit. Parent: Epic: Audit 2026-09-22 findings.
