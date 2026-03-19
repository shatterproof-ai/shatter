// Example 13: MC/DC known-answer functions for testing condition decomposition.
// Tests shatter's ability to decompose compound boolean decisions into individual
// conditions and verify independence pairs under MC/DC analysis.
//
// EXPECTED MC/DC ANALYSIS — compoundAnd:
//   Decision: if (a > 0 && b < 10)
//   Conditions: [a > 0, b < 10]
//   Independence pairs needed: 2
//   Expected witnesses:
//     a > 0: {a: 1, b: 5} (T,T→T) vs {a: -1, b: 5} (F,T→F)
//     b < 10: {a: 1, b: 5} (T,T→T) vs {a: 1, b: 15} (T,F→F)
//
// EXPECTED BRANCHES — compoundAnd:
//   1. a > 0 && b < 10  → "both"
//   2. otherwise        → "neither"
export function compoundAnd(a: number, b: number): string {
  if (a > 0 && b < 10) {
    return "both";
  }
  return "neither";
}

// EXPECTED MC/DC ANALYSIS — compoundOr:
//   Decision: if (x || y)
//   Conditions: [x, y]
//   Expected witnesses:
//     x: {x: true, y: false} (T,masked→T) vs {x: false, y: false} (F,F→F)
//     y: {x: false, y: true} (F,T→T) vs {x: false, y: false} (F,F→F)
//
// EXPECTED BRANCHES — compoundOr:
//   1. x || y  → "either"
//   2. !x && !y → "none"
export function compoundOr(x: boolean, y: boolean): string {
  if (x || y) {
    return "either";
  }
  return "none";
}

// EXPECTED MC/DC ANALYSIS — threeWayAnd:
//   Decision: if (a > 0 && b > 0 && c > 0)
//   Conditions: [a > 0, b > 0, c > 0]
//   Independence pairs needed: 3
//   Short-circuit: if a <= 0, then b and c are masked; if b <= 0, then c is masked.
//   Expected witnesses (unique-cause masking MC/DC):
//     a > 0: {a:1, b:1, c:1} (T,T,T→T) vs {a:-1, b:1, c:1} (F,masked,masked→F)
//     b > 0: {a:1, b:1, c:1} (T,T,T→T) vs {a:1, b:-1, c:1} (T,F,masked→F)
//     c > 0: {a:1, b:1, c:1} (T,T,T→T) vs {a:1, b:1, c:-1} (T,T,F→F)
//
// EXPECTED BRANCHES — threeWayAnd:
//   1. a > 0 && b > 0 && c > 0 → "all positive"
//   2. otherwise                → "not all positive"
export function threeWayAnd(a: number, b: number, c: number): string {
  if (a > 0 && b > 0 && c > 0) {
    return "all positive";
  }
  return "not all positive";
}
