/**
 * Fixtures for private (non-exported) top-level helpers.
 *
 * These are discovered by the analyzer as targets but are not present on
 * the original module's export surface. Shatter must still be able to
 * execute them after instrumentation. Regression for str-jeen.9.
 */

// Private top-level function declaration. Two branches.
function toggleValue(flag: boolean): string {
  if (flag) {
    return "on";
  }
  return "off";
}

// Private top-level arrow-function const. Three branches via ternary chain.
const classifyMagnitude = (n: number): string =>
  n > 10 ? "large" : n > 0 ? "small-positive" : "non-positive";

// Private top-level function expression bound to a const.
const squareValue = function (x: number): number {
  return x * x;
};

// Public component that does not reference the private helpers — keeps the
// export surface narrow so the discovery/execution mismatch is reproducible.
export function publicEntry(label: string): string {
  return `entry:${label}`;
}

// Touch the private bindings so TypeScript does not treat them as
// dead code if a stricter compiler is used. The references are inside a
// never-called function so they do not affect runtime semantics.
function _retainPrivateBindings(): unknown {
  return [toggleValue, classifyMagnitude, squareValue];
}
void _retainPrivateBindings;
