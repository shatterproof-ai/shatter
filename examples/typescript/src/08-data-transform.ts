// Example 8: Data transformation and config merging
// Tests shatter's ability to reason about deeply nested objects, type coercion,
// and multi-step data pipelines. Common in application startup, ETL, and
// configuration management code.

// Deep-merge two config objects with special rules.
//
// EXPECTED BRANCHES (10):
//   1. both base and override are empty          → returns {}
//   2. override has key not in base              → key is added
//   3. base has key not in override              → key is preserved
//   4. both have same key, both objects          → recursive merge
//   5. both have same key, both arrays, strategy "append" → concatenated
//   6. both have same key, both arrays, strategy "replace" → override wins
//   7. both have same key, override is null      → key is removed
//   8. both have same key, type mismatch         → override wins
//   9. both have same key, same primitive type   → override wins
//  10. nested merge depth exceeds maxDepth       → throws Error("max depth exceeded")
//
// DIFFICULTY: Hard. Requires generating nested objects with specific type
// combinations at various depths. Random generation rarely produces matching
// nested structures with the right type relationships.

type ConfigValue = string | number | boolean | null | ConfigObject | ConfigValue[];
interface ConfigObject {
    [key: string]: ConfigValue;
}

type ArrayStrategy = "append" | "replace";

export function mergeConfig(
    base: ConfigObject,
    override: ConfigObject,
    arrayStrategy: ArrayStrategy,
    maxDepth: number,
    currentDepth: number = 0
): ConfigObject {
    if (currentDepth > maxDepth) {
        throw new Error("max depth exceeded");
    }

    const result: ConfigObject = {};

    // Copy base keys
    for (const key of Object.keys(base)) {
        const baseVal = base[key];
        if (!(key in override)) {
            result[key] = baseVal;
            continue;
        }

        const overrideVal = override[key];

        // null in override means delete the key
        if (overrideVal === null) {
            continue;
        }

        // Both are objects: recursive merge
        if (
            isConfigObject(baseVal) &&
            isConfigObject(overrideVal)
        ) {
            result[key] = mergeConfig(
                baseVal,
                overrideVal,
                arrayStrategy,
                maxDepth,
                currentDepth + 1
            );
            continue;
        }

        // Both are arrays: apply strategy
        if (Array.isArray(baseVal) && Array.isArray(overrideVal)) {
            if (arrayStrategy === "append") {
                result[key] = [...baseVal, ...overrideVal];
            } else {
                result[key] = overrideVal;
            }
            continue;
        }

        // Type mismatch or same primitive type: override wins
        result[key] = overrideVal;
    }

    // Add keys from override not in base
    for (const key of Object.keys(override)) {
        if (!(key in base) && override[key] !== null) {
            result[key] = override[key];
        }
    }

    return result;
}

function isConfigObject(val: ConfigValue): val is ConfigObject {
    return typeof val === "object" && val !== null && !Array.isArray(val);
}

// ETL-style record transformation with validation and normalization.
//
// EXPECTED BRANCHES (8):
//   1. record missing "id" field                 → "rejected: missing id"
//   2. record missing "type" field               → "rejected: missing type"
//   3. type === "user", missing "email"          → "rejected: user needs email"
//   4. type === "user", email invalid format     → "rejected: invalid email"
//   5. type === "user", valid email              → normalized user record
//   6. type === "order", missing "amount"        → "rejected: order needs amount"
//   7. type === "order", amount <= 0             → "rejected: non-positive amount"
//   8. type === "order", valid amount            → normalized order record
//   9. unknown type                              → "rejected: unknown type"
//
// DIFFICULTY: Medium. Requires generating objects with specific field
// combinations. The email validation regex adds string constraint solving.

interface RawRecord {
    [key: string]: string | number | boolean | undefined;
}

interface TransformResult {
    status: "accepted" | "rejected";
    reason?: string;
    normalized?: Record<string, string | number | boolean>;
}

export function transformRecord(record: RawRecord): TransformResult {
    if (!record["id"]) {
        return { status: "rejected", reason: "missing id" };
    }

    if (!record["type"]) {
        return { status: "rejected", reason: "missing type" };
    }

    const type = String(record["type"]);

    if (type === "user") {
        const email = record["email"];
        if (!email) {
            return { status: "rejected", reason: "user needs email" };
        }

        const emailStr = String(email);
        // Simple email check: must contain @ with text on both sides
        if (!/^[^@]+@[^@]+\.[^@]+$/.test(emailStr)) {
            return { status: "rejected", reason: "invalid email" };
        }

        return {
            status: "accepted",
            normalized: {
                id: String(record["id"]),
                type: "user",
                email: emailStr.toLowerCase(),
            },
        };
    }

    if (type === "order") {
        const amount = record["amount"];
        if (amount === undefined || amount === "") {
            return { status: "rejected", reason: "order needs amount" };
        }

        const numAmount = Number(amount);
        if (numAmount <= 0 || isNaN(numAmount)) {
            return { status: "rejected", reason: "non-positive amount" };
        }

        return {
            status: "accepted",
            normalized: {
                id: String(record["id"]),
                type: "order",
                amount: Math.round(numAmount * 100) / 100,
            },
        };
    }

    return { status: "rejected", reason: "unknown type" };
}
