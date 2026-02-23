// Example 4: Functions with error throwing
// Tests shatter's ability to discover error paths and distinguish normal vs exceptional returns.

// EXPECTED BRANCHES:
//   1. denominator === 0                   → throws Error("division by zero")
//   2. !isFinite(numerator)                → throws Error("non-finite numerator")
//   3. result is integer                   → returns integer result
//   4. result is not integer               → returns float result

export function safeDivide(numerator: number, denominator: number): number {
    if (denominator === 0) {
        throw new Error("division by zero");
    }
    if (!isFinite(numerator)) {
        throw new Error("non-finite numerator");
    }
    const result = numerator / denominator;
    if (Number.isInteger(result)) {
        return result;
    }
    return result;
}

// EXPECTED BRANCHES:
//   1. items is empty                → throws Error("empty array")
//   2. any item < 0                  → throws Error("negative value")
//   3. all items === 0               → returns { sum: 0, avg: 0, max: 0 }
//   4. normal case, max > 100        → returns stats with "high-max" flag
//   5. normal case, max <= 100       → returns stats without flag

export function computeStats(items: number[]): {
    sum: number;
    avg: number;
    max: number;
    flag?: string;
} {
    if (items.length === 0) {
        throw new Error("empty array");
    }

    let sum = 0;
    let max = items[0];

    for (const item of items) {
        if (item < 0) {
            throw new Error("negative value");
        }
        sum += item;
        if (item > max) {
            max = item;
        }
    }

    const avg = sum / items.length;

    if (max === 0) {
        return { sum: 0, avg: 0, max: 0 };
    }

    if (max > 100) {
        return { sum, avg, max, flag: "high-max" };
    }

    return { sum, avg, max };
}
