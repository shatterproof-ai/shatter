---
slug: validator-ts-extraction-empty
kind: new
title: "validate-protocol-registry: TS source extraction is silently empty, which str-qwua7.7 was closed without fixing"
priority: P2
type: bug
labels: [protocol, parity, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [validator-optional-command-warning]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# validate-protocol-registry: TS source extraction is silently empty, which str-qwua7.7 was closed without fixing

## Problem

str-qwua7.7 ("validate-protocol-registry.py: empty source extraction must fail; point extractors at real sources") was closed at 0655458b. Its merged fix (4cf2165f) repointed only the Rust command extractor. The TS extractor still returns empty sets for commands, statuses and error codes, and `validate()` silently skips empty sets. The script therefore reports success while checking nothing for TS. Its title criterion, that empty extraction must fail, is unmet.

The 4cf2165f commit message says the vocabulary cross-check "is no longer this script's job". GOVERNANCE, by contrast, still describes a source-name parity layer for every frontend, so the code, the closed issue and the governance doc disagree about what the script is for.

## Evidence (re-verified 2026-09-23 at 56c86168)

- `scripts/validate-protocol-registry.py:545-553` `extract_ts` reads `type Command = …`, `type ResponseStatus = …` and `type ErrorCode = …` unions from `shatter-ts/src/protocol.ts`. Those unions no longer exist there: `/usr/bin/grep -n "type Command\|type ResponseStatus\|type ErrorCode" shatter-ts/src/protocol.ts` finds nothing, because they moved to `shatter-ts/src/generated/protocol-enums.ts:26,42,59`. `extract_ts_union` returns `set()` when there is no match (:556-562).
- `validate()` (:641-669) does `if not src_set: continue` (:647), so the empty TS sets pass silently. The Rust `statuses` regex (:620-621) matches only some statuses. The audit dump found 5 of 11.
- `python3 scripts/validate-protocol-registry.py` → exit 0, `All checks passed (with informational warnings).`, with no TS line at all.
- `protocol/GOVERNANCE.md:113-114`: "Source-name parity layer. Cross-checks command, response status, and error code names against source files in core and every frontend."
- `bd show str-qwua7.7` → CLOSED.
- Audit finding protocol-parity-15 (confirmed, P2). The verifier corrected the dedupe: str-qwua7.7 is closed, not open, so this needs a new issue rather than a note on an open one.

## Acceptance criteria

- [ ] Decide and record in the close note:
  - (a) delete the TS layer and the statuses layer, because generated-enum `--check` plus the language sync tests cover vocabulary, and keep command extraction only where it reads real dispatch;
  - or (b) re-point TS extraction at the real sources: `handlers.ts` dispatch for commands, and the generated or handwritten status and error-code definitions.

  Either way, GOVERNANCE's description matches the result (see governance-md-omits-matrix).
- [ ] For every extractor that remains, an empty result for any category and any frontend is a hard error (non-zero exit) naming the frontend and the category.
- [ ] Canary test in `scripts/test_validate_protocol_registry.py`: point each remaining extractor at a fixture with the relevant definitions removed and assert a non-zero exit. The canary fails against today's script (paste the output) and passes after the fix.
- [ ] Rust status extraction either finds all 11 statuses or is removed under option (a).
- [ ] `task parity` passes when forced to execute; record the output.

## Suggested approach

Option (a) is probably right. Vocabulary is already guarded by `protocol-codegen.py --check`. "Advertised vs dispatched" belongs to parity-dispatch-reconciliation, which adds dispatch extractors to `validate-parity.py`. Share one dispatch-extractor helper so there are not two regex sets. Land after validator-optional-command-warning: re-pointing TS command extraction would otherwise add a new permanent TS `get_invocation_plan` warning, because the matrix marks it `not_implemented` for TS.

## Out of scope

Dispatch-vs-matrix reconciliation in `validate-parity.py` (parity-dispatch-reconciliation).

## Dependencies

- Blocked by: validator-optional-command-warning.
- Related: str-qwua7.7 (closed; see validator-reopen-note), parity-dispatch-reconciliation, governance-md-omits-matrix.

Size: S. Priority: P2. Type: bug. Labels: protocol, parity, quality-gates, audit. Parent: Epic: Audit 2026-09-22 findings.
