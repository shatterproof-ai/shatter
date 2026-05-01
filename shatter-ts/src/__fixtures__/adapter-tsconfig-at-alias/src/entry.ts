/**
 * Adapter fixture: entry point using the `@/` tsconfig path alias.
 *
 * The import below uses the literal `@/` alias defined in the adjacent
 * tsconfig.json (`paths: { "@/*": ["src/*"] }`). Without the
 * tsconfig-paths adapter, module resolution fails with a
 * MODULE_NOT_FOUND error and the helper falls back to the
 * unresolvable-module stub.
 *
 * describeNumber: three branches driven entirely by `value`.
 *   - value < 0  → "neg:negative"
 *   - value === 0 → "neg:zero" / "pos:zero" (treated as "zero" by helper)
 *   - value > 0  → "pos:positive"
 */

import { classifySign } from "@/lib/sign";

export function describeNumber(value: number): string {
  const sign = classifySign(value);
  if (value < 0) {
    return `neg:${sign}`;
  }
  return `pos:${sign}`;
}
