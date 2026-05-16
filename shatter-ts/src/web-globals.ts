/**
 * Standard web/fetch/crypto globals exposed unconditionally to the VM
 * sandbox so common web targets (Vite + browser code) do not crash with
 * `ReferenceError: Headers is not defined` etc. These are all Node 18+
 * built-ins reused as-is — they are not browser stubs.
 *
 * Targets that need DOM stubs (window/document/localStorage) still opt
 * in via the browser-dom adapter; this module covers only the standard
 * platform globals that are part of Node's runtime.
 *
 * See: str-ysnp, str-jeen.71.
 */

/** Standard web/fetch/crypto globals to inject into every VM sandbox. */
export const WEB_GLOBALS: Record<string, unknown> = {
  // Fetch API
  fetch: globalThis.fetch,
  Headers: globalThis.Headers,
  Request: globalThis.Request,
  Response: globalThis.Response,
  FormData: globalThis.FormData,
  Blob: globalThis.Blob,
  File: globalThis.File,

  // URL / query string
  URL: globalThis.URL,
  URLSearchParams: globalThis.URLSearchParams,

  // Encoding
  TextEncoder: globalThis.TextEncoder,
  TextDecoder: globalThis.TextDecoder,
  atob: globalThis.atob,
  btoa: globalThis.btoa,

  // Web Crypto API
  crypto: globalThis.crypto,

  // Misc platform
  structuredClone: globalThis.structuredClone,
  performance: globalThis.performance,
  queueMicrotask: globalThis.queueMicrotask,
  Event: globalThis.Event,
  EventTarget: globalThis.EventTarget,
};

/**
 * Default Vite-style `import.meta.env` values. Targets that read
 * `import.meta.env.MODE` or `import.meta.env.DEV` see plausible
 * development-mode defaults; unknown VITE_* keys read as `undefined`.
 */
export const DEFAULT_IMPORT_META_ENV: Record<string, string | boolean> = {
  MODE: "development",
  DEV: true,
  PROD: false,
  SSR: false,
  BASE_URL: "/",
};
