// Example 17: Functions that branch on mock return values.
// Tests dynamic mock discovery — the concolic explorer should generate varied
// mock return values per iteration to discover all branches gated by external
// dependency behavior.
//
// Uses named imports (not namespace import) so the instrumentor can rewrite
// individual symbols for mock interception.
//
// Branch conditions use prefix/length/truthiness checks (not exact string
// equality) so that random mock value generation can discover branches
// without needing to guess specific magic strings.

import { readFileSync, existsSync } from "fs";

// ── Status-code branching via string length ─────────────────────────────
//
// Reads a config file and branches on the length of the content.
// Random mock string generation naturally produces varied lengths,
// making all branches reachable via dynamic mocking.
//
// EXPECTED BRANCHES (4):
//   1. length === 0  → "empty"
//   2. length < 5    → "short"
//   3. length < 15   → "medium"
//   4. length >= 15  → "long"
//
// MOCKS: readFileSync returns a string of varying length.
export function classifyStatus(configPath: string): string {
    const status = (readFileSync(configPath, "utf-8") as string).trim();

    if (status.length === 0) {
        return "empty";
    }
    if (status.length < 5) {
        return "short";
    }
    if (status.length < 15) {
        return "medium";
    }
    return "long";
}

// ── Result/Error branching ─────────────────────────────────────────────
//
// Reads a config file; branches on whether the file exists (existsSync)
// and on the length of the content when it does (readFileSync).
//
// EXPECTED BRANCHES (3):
//   1. existsSync returns falsy    → "missing"
//   2. file exists, content truthy → "loaded"
//   3. file exists, content falsy  → "empty-config"
//
// MOCKS: existsSync → boolean, readFileSync → string.
// Dynamic mocking varies boolean returns to explore both exist/not-exist paths.
export function loadOrDefault(filePath: string): string {
    if (!existsSync(filePath)) {
        return "missing";
    }

    const content = readFileSync(filePath, "utf-8") as string;
    if (content) {
        return "loaded";
    }

    return "empty-config";
}

// ── Loop with mock-per-iteration ───────────────────────────────────────
//
// Reads multiple config files in a loop. Each iteration calls readFileSync,
// and the branch depends on whether the content starts with "#".
//
// EXPECTED BRANCHES (3):
//   1. all files start with "#"    → "all-comments"
//   2. no files start with "#"     → "no-comments"
//   3. mixed                       → "mixed"
//
// MOCKS: readFileSync returns different content per call.
// Dynamic mocking cycles through return_values, so varied values per call
// produce different branch outcomes.
export function classifyConfigs(paths: string[]): string {
    if (paths.length === 0) {
        return "no-comments";
    }

    let commentCount = 0;
    for (const p of paths) {
        const content = readFileSync(p, "utf-8") as string;
        if (content.startsWith("#")) {
            commentCount++;
        }
    }

    if (commentCount === paths.length) {
        return "all-comments";
    }
    if (commentCount === 0) {
        return "no-comments";
    }
    return "mixed";
}
