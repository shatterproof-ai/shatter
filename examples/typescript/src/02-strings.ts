// Example 2: String manipulation with conditionals
// Tests shatter's ability to reason about string properties.
//
// EXPECTED BRANCHES:
//   1. empty string         → "empty"
//   2. length === 1         → "single-char"
//   3. starts with "http"   → "url"
//   4. contains "@"         → "email-like"
//   5. all digits           → "numeric"
//   6. none of the above    → "text"

export function classifyString(s: string): string {
    if (s.length === 0) {
        return "empty";
    }
    if (s.length === 1) {
        return "single-char";
    }
    if (s.startsWith("http")) {
        return "url";
    }
    if (s.includes("@")) {
        return "email-like";
    }
    if (/^\d+$/.test(s)) {
        return "numeric";
    }
    return "text";
}

// String transformation with conditional logic.
//
// EXPECTED BRANCHES:
//   1. mode === "upper"   → uppercased input
//   2. mode === "lower"   → lowercased input
//   3. mode === "reverse" → reversed input
//   4. mode === "repeat"  AND count <= 0 → ""
//   5. mode === "repeat"  AND count > 0  → input repeated count times
//   6. unknown mode       → input unchanged

export function transformString(
    input: string,
    mode: string,
    count: number
): string {
    if (mode === "upper") {
        return input.toUpperCase();
    }
    if (mode === "lower") {
        return input.toLowerCase();
    }
    if (mode === "reverse") {
        return input.split("").reverse().join("");
    }
    if (mode === "repeat") {
        if (count <= 0) {
            return "";
        }
        return input.repeat(count);
    }
    return input;
}
