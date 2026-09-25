# Move protocol/ test-double frontends into a subdirectory and delete dead delayed-execute-frontend.sh

- Priority: P3
- Type: chore
- Labels: protocol,cleanup,tests
- Tracker action: new issue
- Source findings: audit 2026-09-22 protocol-parity-20 (confirmed)

<!-- body -->
## Current code facts
- `protocol/` holds 15 `*-frontend.sh` test-double scripts next to the normative docs (PROTOCOL governance, registry, schemas).
- `protocol/delayed-execute-frontend.sh` is referenced nowhere except inside itself. It was last touched in 145e94a7 (2026-05-24).

## Acceptance criteria
- The test doubles move to `protocol/test-frontends/`, and every reference is updated (find them with `git grep -- '-frontend.sh'`, mostly shatter-core tests).
- `delayed-execute-frontend.sh` is deleted.
- `task test-standard` and `task e2e` pass.
