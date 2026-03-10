/**
 * Setup/teardown order tracking fixture for E2E tests.
 *
 * Both setup() and teardown() append to a shared log array stored in the
 * context. The test verifies that teardown calls occur in reverse order
 * relative to setup calls.
 *
 * Used by: e2e_concolic::setup_teardown_runs_in_reverse_order
 */

export function setup(
  scope: string,
  _parentContext?: unknown,
): { scope: string; setupTimestamp: number } {
  return { scope, setupTimestamp: Date.now() };
}

export function teardown(scope: string, context: unknown): void {
  const ctx = context as { scope: string };
  if (ctx.scope !== scope) {
    throw new Error(`Teardown scope mismatch: expected ${ctx.scope}, got ${scope}`);
  }
}
