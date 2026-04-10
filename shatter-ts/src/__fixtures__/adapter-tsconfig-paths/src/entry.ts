/**
 * Adapter fixture: entry point using tsconfig path aliases.
 *
 * The imports below use path aliases defined in the adjacent tsconfig.json.
 * Without the tsconfig-paths adapter, module resolution will fail.
 *
 * formatValue: uses @lib/math and @utils aliases.
 *   - value < 0   → "negative: <clamped>"
 *   - value === 0 → "zero"
 *   - value > 0   → "positive: <clamped>"
 */

import { clamp } from "@lib/math";
import { capitalize } from "@utils";

export function formatValue(value: number, label: string): string {
  const clamped = clamp(value, -100, 100);
  const prefix = capitalize(label);

  if (value < 0) {
    return `${prefix}: negative ${clamped}`;
  }
  if (value === 0) {
    return `${prefix}: zero`;
  }
  return `${prefix}: positive ${clamped}`;
}
