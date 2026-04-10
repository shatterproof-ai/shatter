/**
 * Fixture that uses import.meta.env, common in Vite-style projects.
 * Should not crash during execute — import.meta.env values resolve to
 * undefined, which is acceptable for exploration.
 */
export function getApiUrl(fallback: string): string {
  const url = import.meta.env?.VITE_API_URL;
  if (url) {
    return url;
  }
  return fallback;
}
