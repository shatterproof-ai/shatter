// Fixture for scripts/docs-smoke.py's real-execution smoke command (str-qwua7.9).
//
// This mirrors the function shown in QUICKSTART.md "2. Explore One Function"
// so the smoke command exercises the same shape of invocation a new user
// would actually run, in an in-repo location docs-smoke can rely on without
// the external examples checkout (scripts/examples_checkout.py).
export function calculateShipping(weight: number, country: string): number {
  if (weight <= 0) throw new Error("invalid weight");
  if (country === "US") return weight < 5 ? 5.99 : 12.99;
  return weight < 5 ? 15.99 : 29.99;
}
