---
name: check-all
description: Run the full repository quality suite and report a unified summary. Use for explicit full diagnostics, landing, or CI—not routine pre-commit verification.
allowed-tools: Bash
disable-model-invocation: true
---

Run the governed full suite and capture its output:

```bash
task check
```

This is the landing/CI/fail-safe tier. Routine branch verification uses
`task affected`, which selects the necessary language, protocol, E2E, and demo
gates from the diff.

Use the `check-static`, `check-unit`, and `check-integration` stage output to
populate the language rows below. Do not rerun their underlying commands bare;
the task graph owns build ordering, full property-test budgets, and machine-wide
heavyweight admission.

## Examine & Report

Examine all outputs for failures, warnings, and errors. Report a unified summary:

```
| Language   | Tests | Lint/Vet | Status |
|------------|-------|----------|--------|
| Rust       | ...   | ...      | PASS/FAIL |
| TypeScript | ...   | ...      | PASS/FAIL |
| Go         | ...   | ...      | PASS/FAIL |

Overall: PASS/FAIL
```

Include error details and suggested corrections for any failures.
