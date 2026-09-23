# Extend docs-smoke to resource-parameters, distribution, execution-adapters, PROJECT-LAYOUT and PROTOCOL.md

- Priority: P3
- Type: task
- Labels: docs,smoke,quality-gates
- Tracker action: new issue
- Related: str-qwua7.9, str-qwua7.9.1, str-qwua7.44
- Source findings: audit 2026-09-22 docs-19 (confirmed)

<!-- body -->
## Current code facts
- `scripts/docs-smoke.yaml:20-24` lists only README.md, QUICKSTART.md, SPEC.md and docs/INDEX.md.
- The audit ran docs-smoke with `docs/resource-parameters.md`, `docs/distribution.md`, `docs/execution-adapters.md` and `docs/PROJECT-LAYOUT.md` added, and all passed (4+4+3 blocks checked), so adding them costs nothing today.
- `PROTOCOL.md` has 22 JSON fences, none of which is validated.

## Acceptance criteria
- The four docs are added to `docs-smoke.yaml`, and `task docs-smoke` passes.
- PROTOCOL.md JSON examples are validated against `protocol/schemas/` (request and response schemas by `command`), or explicitly marked illustrative.
- A unit test in `scripts/test_docs_smoke.py` asserts that the doc list includes every doc linked from docs/INDEX.md's user section, or an explicit exclusion list.
