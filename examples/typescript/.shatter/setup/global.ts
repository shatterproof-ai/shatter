// Global setup module for Shatter example functions.
//
// Initializes a mock database connection and temp directory before exploration.
// The setup_context returned here is passed to each execute call, and the
// teardown function cleans up afterward.
//
// Usage in .shatter/config.yaml:
//   defaults:
//     setup: ./setup/global.ts
//     setup_mode: per_function

export function setup(functionName: string, mode: string): {
  db: string;
  tempDir: string;
  functionName: string;
  mode: string;
} {
  return {
    db: `mock_db_${functionName}`,
    tempDir: `/tmp/shatter-test-${Date.now()}`,
    functionName,
    mode,
  };
}

export function teardown(functionName: string, context: unknown): void {
  const ctx = context as { db: string; tempDir: string };
  if (!ctx.db) {
    throw new Error("teardown: missing db handle in context");
  }
  // In a real project this would close the DB connection and remove temp files.
}
