// Imports via `@util/*` alias declared in tsconfig.json:paths.
// Exercises str-jeen.28 (TS path alias resolution) — the analyzer must
// resolve `@util/clamp` against tsconfig instead of treating it as missing.
import { clamp } from "@util/clamp";

export function clampPercent(value: number): number {
  return clamp(value, 0, 100);
}
