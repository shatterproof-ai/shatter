/**
 * Session-level setup fixture for E2E tests.
 *
 * setup(scope) returns a session context with a connection string
 * and the scope it was called with. teardown(scope, ctx) validates
 * the context before cleaning up.
 *
 * Used by: e2e_concolic::setup_session_context_flows_to_execute
 */

const teardownLog: string[] = [];

export function setup(
  scope: string,
  _parentContext?: unknown,
): { sessionId: string; scope: string } {
  return { sessionId: "sess-42", scope };
}

export function teardown(scope: string, context: unknown): void {
  const ctx = context as { sessionId: string };
  if (!ctx.sessionId) {
    throw new Error("Missing sessionId in teardown context");
  }
  teardownLog.push(`teardown:${scope}`);
}
