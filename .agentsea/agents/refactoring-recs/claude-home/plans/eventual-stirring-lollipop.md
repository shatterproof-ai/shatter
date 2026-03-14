# Plan: Wire verification into demo run loop

## Context
The demo runner (`web/demo/run.ts`) executes 16 demo steps but doesn't verify expected UI state after each step. The `verify()` function and `expect` arrays on steps already exist — they just need to be wired together.

## Changes — single file: `web/demo/run.ts`

1. **Import** `verify` from `./verify`
2. **Add accumulators** before the loop: `let totalPassed = 0, totalFailed = 0`
3. **After `await step.action(page)` (line 113)**, add:
   ```ts
   if (step.expect?.length) {
     const result = await verify(page, step.expect)
     totalPassed += result.passed
     totalFailed += result.failed
   }
   ```
4. **In the headless success block (line 137-139)**, replace the simple success message with a summary:
   ```ts
   if (opts.headless) {
     console.log(`\nVerification: ${totalPassed} passed, ${totalFailed} failed`)
     if (totalFailed > 0) {
       process.exit(1)
     }
     console.log('\n✅ All demo steps passed.\n')
   }
   ```

## Verification
```bash
cd web && pnpm build && pnpm lint && pnpm test
```
