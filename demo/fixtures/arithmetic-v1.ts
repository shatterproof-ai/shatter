// arithmetic-v1: baseline classifyNumber for spec-diff demo.
// v2 adds a "large" threshold at n > 1000, splitting positive numbers into
// small vs large categories — the diff shows added and changed behaviors.
//
// EXPECTED BRANCHES (4):
//   1. n < 0        → "negative"
//   2. n === 0      → "zero"
//   3. n > 0, even  → "positive-even"
//   4. n > 0, odd   → "positive-odd"

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
