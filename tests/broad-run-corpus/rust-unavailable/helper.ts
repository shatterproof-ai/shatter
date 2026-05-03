// TS sibling so the fixture is a *mixed-language* run. When the gate
// invokes shatter with `PATH=""`, the Rust frontend is unavailable but the
// TypeScript frontend is available, exercising the kapow-class scenario:
// a mixed scan must record the Rust file as
// `skipped_by_unavailable_frontend` and exit zero rather than aborting.
export function describeBucket(value: number): string {
  if (value < 0) {
    return "below";
  }
  if (value < 50) {
    return "low";
  }
  if (value < 80) {
    return "mid";
  }
  return "high";
}
