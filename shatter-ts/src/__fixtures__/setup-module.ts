/**
 * Test fixture: a setup module that exports setup() and teardown().
 *
 * setup(scope, parentContext?) returns a context object;
 * teardown(scope, context) accepts it and cleans up.
 */

interface ParentContext {
  contexts: Array<{ level: string; context: unknown }>;
}

export function setup(
  scope: string,
  parentContext?: ParentContext | null,
): { db: string; scope: string; parentLevels: string[] } {
  const parentLevels = parentContext?.contexts?.map(e => e.level) ?? [];
  return { db: "test_db_conn", scope, parentLevels };
}

export function teardown(scope: string, context: unknown): void {
  const ctx = context as { db: string };
  if (!ctx.db) {
    throw new Error("Missing db in context");
  }
}
