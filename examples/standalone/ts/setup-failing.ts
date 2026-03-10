/**
 * Deliberately failing setup fixture for E2E tests.
 *
 * setup() always throws to test failure-skip behavior:
 * when setup fails, dependent functions should be skipped.
 *
 * Used by: e2e_concolic::setup_failure_skips_dependents
 */

export function setup(_scope: string, _parentContext?: unknown): never {
  throw new Error("Intentional setup failure for testing");
}

export function teardown(_scope: string, _context: unknown): void {
  // Should never be called since setup always fails.
  throw new Error("Teardown called after failed setup — this should not happen");
}
