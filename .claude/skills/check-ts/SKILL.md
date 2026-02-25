---
name: check-ts
description: Run TypeScript quality gates (npm test + tsc --noEmit). Use after making changes to shatter-ts/.
allowed-tools: Bash
disable-model-invocation: true
---

Run the following checks and examine the output for errors:

1. Run `npm test` in `shatter-ts/` and capture output
2. Run `npx tsc --noEmit` in `shatter-ts/` and capture output
3. Examine both outputs for failures, type errors, and warnings
4. Summarize results:
   - Number of tests passed/failed
   - Any type errors found
   - Suggested corrections for any failures
   - Overall PASS/FAIL status
