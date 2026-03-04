// Example 1: Pure arithmetic with branches
// Tests shatter's ability to explore numeric conditions.
//
// EXPECTED BRANCHES:
//   1. n < 0        → returns "negative"
//   2. n === 0      → returns "zero"
//   3. n > 0, even  → returns "positive-even"
//   4. n > 0, odd   → returns "positive-odd"

export function classifyNumber(n: number): string {
    if (n < 0) {
        return "negative";
    }
    if (n === 0) {
        return "zero";
    }
    if (n % 2 === 0) {
        return "positive-even";
    }
    return "positive-odd";
}

// A function with compound arithmetic conditions.
//
// EXPECTED BRANCHES:
//   1. a + b > 100 AND a * b > 1000  → "both-large"
//   2. a + b > 100 AND a * b <= 1000 → "sum-large"
//   3. a + b <= 100 AND a * b > 1000 → "product-large"
//   4. a + b <= 100 AND a * b <= 1000 → "both-small"

export function compareMagnitudes(a: number, b: number): string {
    const sum = a + b;
    const product = a * b;

    if (sum > 100) {
        if (product > 1000) {
            return "both-large";
        }
        return "sum-large";
    }
    if (product > 1000) {
        return "product-large";
    }
    return "both-small";
}
