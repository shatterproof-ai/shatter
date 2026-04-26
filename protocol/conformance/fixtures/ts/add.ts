// Conformance fixture for the typescript frontend's outcome shape (str-hy9b.A5).
// A trivially executable function so `execute` lands on the success path and
// the response carries `outcome.status = "completed"`.
export function add(a: number, b: number): number {
  return a + b;
}
