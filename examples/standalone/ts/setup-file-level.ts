/**
 * File-level setup fixture for E2E tests.
 *
 * setup(scope) returns a file-scoped context containing the file path (scope).
 * teardown(scope, ctx) validates the context.
 *
 * Used by: e2e_concolic::setup_file_level_scoped_per_file
 */

export function setup(
  scope: string,
  _parentContext?: unknown,
): { fileScope: string; initialized: boolean } {
  return { fileScope: scope, initialized: true };
}

export function teardown(scope: string, context: unknown): void {
  const ctx = context as { fileScope: string };
  if (ctx.fileScope !== scope) {
    throw new Error(`Teardown scope mismatch: expected ${ctx.fileScope}, got ${scope}`);
  }
}
