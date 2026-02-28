/**
 * Test fixture: a setup module that exports setup() and teardown().
 *
 * setup() returns a context object; teardown() accepts it and cleans up.
 */

export function setup(functionName: string, mode: string): { db: string; functionName: string; mode: string } {
  return { db: "test_db_conn", functionName, mode };
}

export function teardown(functionName: string, context: unknown): void {
  // In a real module this would close connections, delete temp files, etc.
  const ctx = context as { db: string };
  if (!ctx.db) {
    throw new Error("Missing db in context");
  }
}
