// Example 17: Third-party npm package usage
// Tests shatter's handling of functions that import and call real npm packages.
// Unlike 12-external-deps.ts which uses Node builtins (fs, path), this file
// imports lodash — a third-party package declared in package.json.
//
// Uses:
//   - lodash (_.groupBy, _.pick, _.isEmpty, _.get)

import * as _ from "lodash";

// ── groupBy classification ───────────────────────────────────────────
//
// Groups items by a key extracted via lodash, then classifies the result.
// Exercises lodash's groupBy and size — real library calls that transform data.
//
// EXPECTED BRANCHES (4):
//   1. items is empty          → "empty"
//   2. all items in one group  → "uniform:<key>"
//   3. exactly two groups      → "binary"
//   4. more than two groups    → "diverse:<count>"
export function classifyGroups(
    items: Array<{ category: string; value: number }>
): string {
    if (_.isEmpty(items)) {
        return "empty";
    }

    const groups = _.groupBy(items, "category");
    const groupCount = _.size(groups);

    if (groupCount === 1) {
        const key = Object.keys(groups)[0];
        return `uniform:${key}`;
    }

    if (groupCount === 2) {
        return "binary";
    }

    return `diverse:${groupCount}`;
}

// ── nested object access ─────────────────────────────────────────────
//
// Uses lodash's _.get for safe deep property access, then validates the result.
// Exercises lodash's get with dotted paths — the core reason projects depend on it.
//
// EXPECTED BRANCHES (4):
//   1. path is empty          → "error:empty path"
//   2. path resolves to undefined/null → "missing"
//   3. resolved value is a string      → "string:<value>"
//   4. resolved value is not a string  → "other:<type>"
export function safeDeepGet(
    obj: Record<string, unknown>,
    path: string
): string {
    if (path.length === 0) {
        return "error:empty path";
    }

    const value = _.get(obj, path);

    if (value === undefined || value === null) {
        return "missing";
    }

    if (typeof value === "string") {
        return `string:${value}`;
    }

    return `other:${typeof value}`;
}

// ── pick and validate ────────────────────────────────────────────────
//
// Uses lodash's _.pick to extract a subset of fields, then validates
// that required fields are present. Exercises pick + isEmpty together.
//
// EXPECTED BRANCHES (4):
//   1. fields is empty            → "error:no fields"
//   2. picked result is empty     → "none"
//   3. some fields missing        → "partial:<count>/<total>"
//   4. all fields present         → "complete"
export function pickAndValidate(
    obj: Record<string, unknown>,
    fields: string[]
): string {
    if (fields.length === 0) {
        return "error:no fields";
    }

    const picked = _.pick(obj, fields);

    if (_.isEmpty(picked)) {
        return "none";
    }

    const pickedCount = Object.keys(picked).length;
    if (pickedCount < fields.length) {
        return `partial:${pickedCount}/${fields.length}`;
    }

    return "complete";
}
