// Example 12: External dependencies and auto-mocking
// Tests shatter's automatic mock generation for functions that depend on
// external modules: filesystem I/O, network calls, database queries, and
// pure utility libraries.
//
// Shatter classifies each dependency and generates appropriate mocks:
//   - fs/path        → filesystem I/O stubs (returns "", null, etc.)
//   - axios/fetch    → network stubs (returns {status: 200, data: {}})
//   - pg/prisma      → database stubs (returns {rows: []})
//   - lodash/dayjs   → pure utility passthrough (no mock needed)

import * as fs from "fs";
import * as path from "path";

// ── Filesystem dependency ──────────────────────────────────────────────
//
// EXPECTED BRANCHES (4):
//   1. configPath is empty          → throws Error("empty config path")
//   2. file does not exist          → returns "missing"
//   3. file content is empty        → returns "empty"
//   4. file has content             → returns parsed content length
//
// MOCKS: fs.existsSync → true/false, fs.readFileSync → string
//
// DIFFICULTY: Easy. Two string branches plus mock return values.
export function loadConfig(configPath: string): string {
    if (configPath.length === 0) {
        throw new Error("empty config path");
    }

    const fullPath = path.resolve(configPath);

    if (!fs.existsSync(fullPath)) {
        return "missing";
    }

    const content = fs.readFileSync(fullPath, "utf-8");
    if (content.length === 0) {
        return "empty";
    }

    return `loaded:${content.length}`;
}

// ── Network dependency (simulated via function signature) ──────────────
//
// This function takes a `fetchFn` parameter typed to simulate an HTTP client.
// In real code this would be `axios.get` or `fetch`. Shatter should detect
// the external dependency and generate a network-category mock.
//
// EXPECTED BRANCHES (4):
//   1. url is empty                 → throws Error("empty url")
//   2. response status >= 400       → returns "error:<status>"
//   3. response has no data         → returns "no-data"
//   4. response has data            → returns "ok:<key count>"
//
// DIFFICULTY: Easy. Numeric comparison on status plus null check on data.
export function fetchAndProcess(
    url: string,
    response: { status: number; data: Record<string, unknown> | null }
): string {
    if (url.length === 0) {
        throw new Error("empty url");
    }

    if (response.status >= 400) {
        return `error:${response.status}`;
    }

    if (!response.data) {
        return "no-data";
    }

    return `ok:${Object.keys(response.data).length}`;
}

// ── Database dependency (simulated via parameter) ──────────────────────
//
// EXPECTED BRANCHES (5):
//   1. table is empty               → throws Error("empty table")
//   2. table has invalid chars       → throws Error("invalid table name")
//   3. rows is empty                → returns "no-rows"
//   4. rows has one item            → returns "single:<value>"
//   5. rows has multiple items      → returns "multiple:<count>"
//
// DIFFICULTY: Easy. String validation plus array length checks.
export function queryTable(
    table: string,
    rows: Array<{ id: number; value: string }>
): string {
    if (table.length === 0) {
        throw new Error("empty table");
    }

    if (!/^[a-zA-Z_]\w*$/.test(table)) {
        throw new Error("invalid table name");
    }

    if (rows.length === 0) {
        return "no-rows";
    }

    if (rows.length === 1) {
        return `single:${rows[0].value}`;
    }

    return `multiple:${rows.length}`;
}

// ── Pure utility usage ─────────────────────────────────────────────────
//
// Uses path.join (filesystem category) but the logic is pure string
// manipulation. Demonstrates that even when a function imports a utility
// module, Shatter can still explore it meaningfully.
//
// EXPECTED BRANCHES (3):
//   1. parts is empty               → returns ""
//   2. any part contains ".."       → throws Error("path traversal")
//   3. normal parts                 → returns joined path
//
// DIFFICULTY: Easy. Array emptiness check plus string search.
export function buildSafePath(parts: string[]): string {
    if (parts.length === 0) {
        return "";
    }

    for (const part of parts) {
        if (part.includes("..")) {
            throw new Error("path traversal");
        }
    }

    return path.join(...parts);
}
