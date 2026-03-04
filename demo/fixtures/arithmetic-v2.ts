// arithmetic-v2: evolved classifyNumber for spec-diff demo.
// Compared to v1: adds a "large" threshold at n > 1000, producing new branches
// ("large-even", "large-odd") and narrowing the preconditions for "positive-even"
// and "positive-odd" to 0 < n <= 1000.
//
// EXPECTED BRANCHES (6):
//   1. n < 0                → "negative"
//   2. n === 0              → "zero"
//   3. n > 1000, even       → "large-even"
//   4. n > 1000, odd        → "large-odd"
//   5. 0 < n <= 1000, even  → "positive-even"
//   6. 0 < n <= 1000, odd   → "positive-odd"

export function classifyNumber(n: number): string {
    if (n < 0) {
        return "negative";
    }
    if (n === 0) {
        return "zero";
    }
    if (n > 1000) {
        if (n % 2 === 0) {
            return "large-even";
        }
        return "large-odd";
    }
    if (n % 2 === 0) {
        return "positive-even";
    }
    return "positive-odd";
}
