/**
 * Kapow-shaped regression fixture for str-jeen.69.
 *
 * Models a web/e2e/*.spec.ts file: top-level imports from an external
 * test framework (Playwright-style — resolved to a stub by the
 * executor's sandboxRequire), top-level helper functions discovered
 * by the analyzer as targets, and top-level `test(...)` calls that
 * run at module load time.
 *
 * Reproduces the failure class where the analyzer discovers a private
 * helper (e.g. `loginAs`) but the executor cannot find it on
 * module.exports because the top-level test(...) call interrupts
 * module initialization before the str-jeen.9 trailer can run.
 */

import { test, expect } from "@playwright/test";

// Private helper discovered by the analyzer. Two branches.
function loginAs(role: string): { role: string; perms: string[] } {
  if (role === "admin") {
    return { role, perms: ["read", "write", "admin"] };
  }
  return { role, perms: ["read"] };
}

// Top-level test() invocation: at module init this calls into the
// stubbed Playwright module. The stubbed function may throw or
// short-circuit; either way it runs BEFORE the trailer at end of file.
test("login as admin grants admin perms", async () => {
  const result = loginAs("admin");
  expect(result.perms).toContain("admin");
});

function _retainPrivateBindings(): unknown {
  return loginAs;
}
void _retainPrivateBindings;
