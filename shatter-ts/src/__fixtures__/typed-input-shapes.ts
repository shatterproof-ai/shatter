// Test fixtures for body-driven parameter shape refinement (str-yb7q).
// Each function documents which usage signals fire on its parameter.

// Array method on `any`-typed param → refine to array
export function sumPositives(items: any): number {
  return items.filter((x: number) => x > 0).length;
}

// Nested array under an object property → refine `data` to `{rows: array}`
export function countRows(data: any): number {
  return data.rows.filter((r: any) => r.active).length;
}

// for-of usage → array signal
export function joinNames(items: any): string {
  const names: string[] = [];
  for (const item of items) {
    names.push(item.name);
  }
  return names.join(",");
}

// Spread in array literal → array signal
export function spreadCopy(items: any): any[] {
  return [...items];
}

// Destructuring → array signal
export function firstTwo(items: any): [any, any] {
  const [a, b] = items;
  return [a, b];
}

// Multiple nested array fields under object → both refined
export function processReport(report: any): number {
  const rowSum = report.rows.length;
  const itemSum = report.items.map((i: any) => i).length;
  return rowSum + itemSum;
}

// Explicitly typed array param — refiner must leave it alone
export function sumExplicit(nums: number[]): number {
  return nums.filter((x) => x > 0).length;
}

// No body usage at all → refiner is a no-op
export function unused(_x: any): number {
  return 42;
}

// Param used only as property access (no array methods) → object with unknown leaves
export function readField(obj: any): string {
  return obj.label;
}

// Param with both array and string methods at root — array wins
// (this matches real cases like `.length` being shared)
export function mixedSignals(value: any): number {
  return value.length;
}
