/**
 * Regression fixture for str-ysnp + str-jeen.71: standard web/fetch/crypto
 * globals are available in the VM sandbox by default (no adapter opt-in).
 */

export function buildHeaders(token: string): string {
  const h = new Headers();
  h.set("Authorization", `Bearer ${token}`);
  return h.get("Authorization") ?? "";
}

export function buildRequest(url: string): string {
  const r = new Request(url, { method: "POST" });
  return r.method;
}

export function buildResponse(body: string): string {
  const r = new Response(body, { status: 200 });
  return String(r.status);
}

export function hasFetch(): boolean {
  return typeof fetch === "function";
}

export function randomUuidLength(): number {
  return crypto.randomUUID().length;
}

export function viteMode(): string {
  // Reading import.meta.env should not throw a ReferenceError, and `env`
  // must be defined (Vite defaults supplied by the sandbox).
  const mode = import.meta.env?.MODE;
  return typeof mode === "string" ? mode : "unknown";
}

export function readVitePosthogKey(fallback: string): string {
  const key = import.meta.env?.VITE_POSTHOG_KEY;
  return typeof key === "string" ? key : fallback;
}
