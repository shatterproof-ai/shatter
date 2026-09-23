---
slug: validator-reopen-note
kind: reopen-note
title: "Comment on closed str-qwua7.7 (do not reopen): the empty-extraction criterion is unmet for TS"
priority: P2
type: note
labels: [protocol, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [validator-ts-extraction-empty, validator-optional-command-warning]
existing_id: str-qwua7.7
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-qwua7.7 (do not reopen): the empty-extraction criterion is unmet for TS

Target: str-qwua7.7 (CLOSED at 0655458b; status re-checked with `bd show` on 2026-09-23). Action: add a comment only. Do not reopen, and do not change its status. `kind: reopen-note` is the filer's kind for "comment on a closed issue": `file-all.sh` posts it with `bd comments add` and only warns if the target is not closed. It never reopens. The follow-up work is tracked in the new issues validator-ts-extraction-empty and validator-optional-command-warning. `blocked_by` lists them only so that the filer creates them first and substitutes their str- ids for the placeholders below; no dependency edge is added to str-qwua7.7.

## Comment text

Audit 2026-09-22 (finding protocol-parity-15, verified; re-checked 2026-09-23 at 56c86168): this issue was closed with its title criterion unmet.

- The fix (4cf2165f) repointed only the Rust command extractor at `handler.rs` dispatch. `extract_ts` (`scripts/validate-protocol-registry.py:545-553`) still reads `type Command/ResponseStatus/ErrorCode` unions from `shatter-ts/src/protocol.ts`. Those unions moved to `shatter-ts/src/generated/protocol-enums.ts`, so all three TS sets are empty.
- `validate()` skips empty sets (`if not src_set: continue`, :647). The script exits 0 with `All checks passed (with informational warnings).` and checks nothing for TS. "Empty source extraction must fail" is therefore not true.
- The Rust statuses regex (:621) finds only some of the 11 statuses.
- The 4cf2165f message says the vocabulary cross-check "is no longer this script's job", but `protocol/GOVERNANCE.md:113-114` still describes a source-name parity layer for every frontend. That decision was never reconciled with the issue or with GOVERNANCE.
- The Rust `get_invocation_plan` "may be unimplemented" warning still prints on every run, although the matrix marks it `not_implemented` for Rust.

Follow-ups: <validator-ts-extraction-empty> (decide delete-vs-repoint; hard-fail on empty extraction, with a canary test) and <validator-optional-command-warning> (the validator consults the matrix, so intended gaps do not warn).
