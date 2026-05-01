/**
 * Adapter fixture helper: classifier resolved via the `@/*` tsconfig alias.
 *
 * Imported by `src/entry.ts` as `@/lib/sign`. Without the tsconfig-paths
 * adapter, the import is a bare specifier that Node cannot resolve.
 */

export function classifySign(value: number): string {
  if (value < 0) {
    return "negative";
  }
  if (value === 0) {
    return "zero";
  }
  return "positive";
}
