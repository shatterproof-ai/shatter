---
slug: protocol-test-doubles-relocate
kind: new
title: "Move protocol/ test-double frontends into a subdirectory and delete the dead delayed-execute-frontend.sh"
priority: P3
type: chore
labels: [protocol, cleanup, tests, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Move protocol/ test-double frontends into a subdirectory and delete the dead delayed-execute-frontend.sh

## Problem

`protocol/` holds the normative protocol documents: GOVERNANCE.md, PARITY.md, registry.yaml, parity-matrix.yaml and schemas/. Fifteen shell test-double frontends sit at the same level, which makes the directory hard to scan. One of them is referenced by nothing.

## Evidence (re-verified 2026-09-23 at 56c86168)

- `ls protocol/*-frontend.sh | wc -l` → `15`: concolic-test, dead-after-handshake-once, delayed-execute, execute-exits-twice, failing-file, failing-generator, fixed-branch, generated-skip, id-mismatch, noop, observer-recording, slow-execute, slow, slow-once, unknown-branch-fuzz.
- `git grep -c delayed-execute -- . ':!audits' ':!.beads'` → `protocol/delayed-execute-frontend.sh:2`, which means only self-references. The file was last touched in 145e94a7 (2026-05-24).
- References to move: `git grep -F -- '-frontend.sh' -- . ':!audits' ':!.beads'` finds 62 matching lines across PROTOCOL.md, docs/plans/str-kapl-resilience-timeouts-memory.md, protocol/conformance/{conformance_cases,golden_cases}.yaml, shatter-cli/tests/scan_analysis_cache_identity.rs, shatter-core/src/{batch_analyze,explorer,frontend,orchestrator,scan_orchestrator}.rs, and the scripts themselves. Use `-F`: without it, `.` matches any character and pulls in unrelated prose.
- `PROTOCOL.md:602-608` documents `protocol/noop-frontend.sh` as the reference implementation for new frontend authors.
- Audit finding protocol-parity-20 (confirmed, P3).

## Acceptance criteria

- [ ] The test doubles live in `protocol/test-frontends/`. Every reference found by the grep above is updated, and the grep returns no `protocol/<name>-frontend.sh` paths outside the new directory.
- [ ] `noop-frontend.sh` either moves too, with PROTOCOL.md:602-608 updated, or stays in `protocol/` as the documented reference implementation. The close note records which.
- [ ] `protocol/delayed-execute-frontend.sh` is deleted.
- [ ] Taskfile `sources:` globs that cover `protocol/**/*` still include the moved files. Check `conformance` and any shatter-core test task.
- [ ] Proof at close: `task test-standard`, `task conformance` and `task e2e` pass when forced to execute (not checksum-cached); paste the gate lines.

## Out of scope

Rewriting the test doubles.

## Dependencies

- Blocked by: none.
- Related: none.

Size: S. Priority: P3. Type: chore. Labels: protocol, cleanup, tests, audit. Parent: Epic: Audit 2026-09-22 findings.
