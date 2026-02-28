// Example 11: Opaque type handling
// Tests shatter's ability to distinguish between functions that can be
// meaningfully explored (primitive/structured params) and those that
// cannot (opaque handles like DB connections, sockets, streams).
//
// Shatter should SKIP functions whose parameters are entirely opaque
// (no primitive fields to vary), and should NOT SKIP functions that
// have a mix of opaque and primitive parameters.

// --- SHOULD BE SKIPPED: entirely opaque parameters ---

interface DatabaseConnection {
    query(sql: string): Promise<unknown[]>;
    execute(sql: string): Promise<void>;
    close(): Promise<void>;
}

// OPAQUE: The only parameter is a database connection handle.
// There are no primitive inputs to vary — the function's behavior
// depends entirely on external database state.
//
// EXPECTED: Shatter should skip this function.
export async function processWithDbConnection(
    db: DatabaseConnection
): Promise<string> {
    const rows = await db.query("SELECT COUNT(*) FROM users");
    if (!rows || rows.length === 0) {
        return "no-data";
    }
    return "has-data";
}

interface Socket {
    write(data: Buffer): boolean;
    end(): void;
    destroyed: boolean;
}

// OPAQUE: The only parameter is a socket handle.
// Behavior depends on network state, not on input values.
//
// EXPECTED: Shatter should skip this function.
export function processWithSocket(sock: Socket): string {
    if (sock.destroyed) {
        return "socket-closed";
    }
    const ok = sock.write(Buffer.from("ping"));
    if (ok) {
        return "sent";
    }
    return "backpressure";
}

// --- SHOULD NOT BE SKIPPED: has explorable primitive parameters ---

// MIXED: Takes an opaque DB connection BUT also primitive params
// (tableName: string, limit: number) that control branching.
// The primitive params have meaningful exploration even though
// the DB connection is opaque.
//
// EXPECTED BRANCHES (6):
//   1. tableName is empty                        → throws Error("empty table name")
//   2. tableName contains non-alphanumeric chars → throws Error("invalid table name")
//   3. limit <= 0                                → throws Error("limit must be positive")
//   4. limit > 1000                              → capped at 1000
//   5. result is empty                           → "no-results"
//   6. result has rows                           → "has-results"
//
// EXPECTED: Shatter should NOT skip this function. It should explore
// the primitive parameters while treating the DB connection as opaque.
//
// DIFFICULTY: Medium. Requires satisfying string regex constraints
// and numeric range checks simultaneously.
export async function processWithMixedParams(
    db: DatabaseConnection,
    tableName: string,
    limit: number
): Promise<string> {
    if (tableName.length === 0) {
        throw new Error("empty table name");
    }
    if (!/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(tableName)) {
        throw new Error("invalid table name");
    }
    if (limit <= 0) {
        throw new Error("limit must be positive");
    }

    const effectiveLimit = limit > 1000 ? 1000 : limit;
    const rows = await db.query(
        `SELECT * FROM ${tableName} LIMIT ${effectiveLimit}`
    );

    if (!rows || rows.length === 0) {
        return "no-results";
    }
    return "has-results";
}

// PURE PRIMITIVES: No opaque types at all. Complex branching over
// numeric and string inputs demonstrates what full exploration looks like.
//
// EXPECTED BRANCHES (8):
//   1. values is empty                           → "empty"
//   2. all values are 0                          → "all-zero"
//   3. any value is NaN                          → "contains-nan"
//   4. any value is negative                     → "contains-negative"
//   5. mode === "sum"                            → returns sum
//   6. mode === "avg"                            → returns average
//   7. mode === "max"                            → returns max
//   8. unknown mode                              → throws Error("unknown mode")
//
// EXPECTED: Shatter should NOT skip this function.
//
// DIFFICULTY: Medium. Requires generating arrays with specific numeric
// properties and matching mode strings.
export function computeFromPrimitives(
    values: number[],
    mode: string
): number {
    if (values.length === 0) {
        throw new Error("empty");
    }

    if (values.some(v => isNaN(v))) {
        throw new Error("contains-nan");
    }

    if (values.every(v => v === 0)) {
        return 0;
    }

    if (values.some(v => v < 0)) {
        throw new Error("contains-negative");
    }

    if (mode === "sum") {
        return values.reduce((a, b) => a + b, 0);
    }
    if (mode === "avg") {
        return values.reduce((a, b) => a + b, 0) / values.length;
    }
    if (mode === "max") {
        return Math.max(...values);
    }

    throw new Error("unknown mode");
}
