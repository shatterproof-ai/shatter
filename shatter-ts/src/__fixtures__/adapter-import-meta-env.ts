/**
 * Adapter fixture: import.meta.env usage (Vite-style).
 *
 * These functions read import.meta.env properties that are populated at build
 * time in Vite/Astro projects. Without an adapter providing env values, they
 * resolve to undefined and fall through to defaults.
 *
 * getApiBase: reads import.meta.env.VITE_API_BASE.
 *   - env value present → returns it
 *   - env value absent  → returns fallback
 *
 * isProduction: reads import.meta.env.MODE.
 *   - MODE === "production" → true
 *   - otherwise             → false
 *
 * getFeatureFlag: reads a dynamic env key via bracket access.
 *   - env value is "true"  → true
 *   - env value is truthy  → true
 *   - env value is falsy   → false
 */

export function getApiBase(fallback: string): string {
  const base = import.meta.env?.VITE_API_BASE;
  if (base) {
    return base;
  }
  return fallback;
}

export function isProduction(): boolean {
  const mode = import.meta.env?.MODE;
  if (mode === "production") {
    return true;
  }
  return false;
}

export function getFeatureFlag(flagName: string): boolean {
  const envRecord = import.meta.env as Record<string, string | undefined> | undefined;
  const value = envRecord?.[`VITE_FLAG_${flagName}`];
  if (value) {
    return true;
  }
  return false;
}
