// Test fixtures for integer type refinement heuristic.
// Each function documents which signals should fire and the expected outcome.

// Signal 1 (comparison x2): n < 0, n > 100 — two integer comparison literals
// Expected: 1 signal category (comparison), needs naming or another to reach 2
// With param name "n" (not in naming list) → 1 signal → stays float
export function clampValue(n: number): number {
  if (n < 0) return 0;
  if (n > 100) return 100;
  return n;
}

// Signal 1 (comparison count <= 0) + Signal 4 (naming "count")
// Expected: 2 signals → refine to int
export function factorial(count: number): number {
  if (count <= 0) return 1;
  let result = 1;
  for (let i = count; i > 1; i--) {
    result *= i;
  }
  return result;
}

// Signal 4 only (naming "index")
// Expected: 1 signal → stays float (needs 2)
export function getAtIndex(index: number): number {
  return index;
}

// Fractional veto: .toFixed() suppresses all signals
// Even though amount < 0 is a comparison signal, the veto kills it
export function formatCurrency(amount: number): string {
  if (amount < 0) return "-" + formatCurrency(-amount);
  return amount.toFixed(2);
}

// Signal 1 (comparison) + Signal 2 (Math.floor coercion) + Signal 4 (naming "pageIndex")
// Expected: 3 signals → refine to int
export function paginate(pageIndex: number, totalItems: number): number {
  if (pageIndex < 0) return 0;
  return Math.floor(totalItems / 10);
}

// Signal 2 only (Math.floor coercion) — no naming signal for "value"
// Expected: 1 signal → stays float
export function truncateValue(value: number): number {
  return Math.floor(value);
}

// Signal 1 (comparison n > 0) + Signal 2 (bitwise coercion n | 0)
// Expected: 2 signals → refine to int
export function bitwiseCoerce(n: number): number {
  if (n > 0) return n | 0;
  return 0;
}

// Signal 1 (comparison count > 0) + Signal 4 (naming "count")
// Expected: 2 signals → refine to int
export function countdown(count: number): number[] {
  if (count < 0) return [];
  const results: number[] = [];
  let remaining = count;
  while (remaining > 0) {
    results.push(remaining);
    remaining -= 1;
  }
  return results;
}

// Fractional veto: param % 1 (checking for fractional part)
// Expected: veto → stays float
export function isWhole(x: number): boolean {
  return x % 1 === 0;
}

// Fractional veto: Math.round(param)
// Expected: veto → stays float
export function roundIt(x: number): number {
  if (x > 0) return Math.round(x);
  return 0;
}

// Signal 5: JSDoc @param {integer}
// + Signal 1 (comparison n > 0)
// Expected: 2 signals → refine to int
/**
 * Check if a value is positive.
 * @param {integer} n - The integer to check
 * @returns true if positive
 */
export function jsdocInteger(n: number): boolean {
  return n > 0;
}

// No signals at all — plain number arithmetic with no integer clues
// Expected: stays float
export function multiply(a: number, b: number): number {
  return a * b;
}

// Signal 2 (bitwise coercion num | 0) + Signal 4 (naming "num")
// Expected: 2 signals → refine to int
export function coerceAndUse(num: number): string {
  const safe = num | 0;
  return String(safe);
}
