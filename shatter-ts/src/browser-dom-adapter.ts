/**
 * Browser DOM adapter — provides lightweight browser API stubs as a
 * SandboxProvider so that browser-dependent code can execute in Node.js.
 *
 * The adapter injects minimal, functional stubs for window, document,
 * navigator, localStorage, sessionStorage, and related browser globals
 * into the VM sandbox. Stubs are sufficient for concolic execution —
 * they hold readable/writable state and return sensible defaults, but
 * do not implement full DOM semantics.
 *
 * Options allow pre-seeding values (e.g. window.innerWidth, document.title,
 * localStorage entries) for targeted exploration of browser-dependent branches.
 */

import { BROWSER_GLOBALS_ADAPTER_ID } from "./browser-globals-recognizer.js";
import type {
  RuntimeHookFactory,
  RuntimeHooks,
  SandboxProvider,
} from "./runtime-hooks.js";
import type { ExecutionAdapter } from "./protocol.js";

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

export interface BrowserDomAdapterOptions {
  /** Seed values for window properties. */
  window?: Record<string, unknown>;
  /** Seed values for document properties. */
  document?: Record<string, unknown>;
  /** Pre-populated localStorage entries. */
  localStorage?: Record<string, string>;
  /** Pre-populated sessionStorage entries. */
  sessionStorage?: Record<string, string>;
}

// ---------------------------------------------------------------------------
// Storage stub
// ---------------------------------------------------------------------------

/** In-memory Storage implementation (localStorage / sessionStorage). */
export function createStorageStub(
  seed?: Record<string, string>,
): Storage {
  const store = new Map<string, string>(
    seed ? Object.entries(seed) : [],
  );

  return {
    getItem(key: string): string | null {
      return store.get(key) ?? null;
    },
    setItem(key: string, value: string): void {
      store.set(key, String(value));
    },
    removeItem(key: string): void {
      store.delete(key);
    },
    clear(): void {
      store.clear();
    },
    key(index: number): string | null {
      const keys = [...store.keys()];
      return keys[index] ?? null;
    },
    get length(): number {
      return store.size;
    },
  };
}

// ---------------------------------------------------------------------------
// Noop helpers
// ---------------------------------------------------------------------------

const noop = (): void => {};

function createStubElement(): Record<string, unknown> {
  return {
    tagName: "DIV",
    className: "",
    id: "",
    style: {},
    children: [],
    childNodes: [],
    textContent: "",
    innerHTML: "",
    setAttribute: noop,
    getAttribute: () => null,
    removeAttribute: noop,
    addEventListener: noop,
    removeEventListener: noop,
    appendChild: noop,
    removeChild: noop,
    insertBefore: noop,
    replaceChild: noop,
    cloneNode() {
      return createStubElement();
    },
    querySelector: () => null,
    querySelectorAll: () => [],
  };
}

// ---------------------------------------------------------------------------
// Browser globals construction
// ---------------------------------------------------------------------------

/**
 * Build the full set of browser API stubs, optionally seeded with
 * user-provided values. Exported for direct use in tests.
 */
export function createBrowserGlobalsSandbox(
  options?: BrowserDomAdapterOptions,
): Record<string, unknown> {
  const ls = createStorageStub(options?.localStorage);
  const ss = createStorageStub(options?.sessionStorage);

  const body = {
    className: "",
    style: {},
    appendChild: noop,
    removeChild: noop,
    insertBefore: noop,
    addEventListener: noop,
    removeEventListener: noop,
    querySelector: () => null,
    querySelectorAll: () => [],
    ...options?.document?.body as Record<string, unknown> | undefined,
  };

  const document: Record<string, unknown> = {
    title: "",
    body,
    head: createStubElement(),
    documentElement: createStubElement(),
    createElement: () => createStubElement(),
    createTextNode: () => ({ textContent: "" }),
    createDocumentFragment: () => ({ children: [], appendChild: noop }),
    getElementById: () => null,
    getElementsByClassName: () => [],
    getElementsByTagName: () => [],
    querySelector: () => null,
    querySelectorAll: () => [],
    addEventListener: noop,
    removeEventListener: noop,
    ...options?.document,
    // Restore body after spread so user body seeds merge correctly
    ...(options?.document?.body ? { body } : {}),
  };

  const location: Record<string, unknown> = {
    href: "about:blank",
    origin: "",
    protocol: "about:",
    host: "",
    hostname: "",
    port: "",
    pathname: "/blank",
    search: "",
    hash: "",
    assign: noop,
    replace: noop,
    reload: noop,
  };

  const history: Record<string, unknown> = {
    length: 1,
    state: null,
    pushState: noop,
    replaceState: noop,
    back: noop,
    forward: noop,
    go: noop,
  };

  const navigator: Record<string, unknown> = {
    userAgent: "shatter/1.0",
    language: "en-US",
    languages: ["en-US"],
    platform: "shatter",
    cookieEnabled: false,
    onLine: true,
  };

  const matchMedia = (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: noop,
    removeListener: noop,
    addEventListener: noop,
    removeEventListener: noop,
    dispatchEvent: () => false,
  });

  const stubObserver = () => ({
    observe: noop,
    unobserve: noop,
    disconnect: noop,
    takeRecords: () => [],
  });

  const window: Record<string, unknown> = {
    // Core properties — user options can override these
    innerWidth: 1024,
    innerHeight: 768,
    outerWidth: 1024,
    outerHeight: 768,
    devicePixelRatio: 1,
    ...options?.window,

    // Cross-references (set after spread so they aren't overwritten)
    document,
    navigator,
    location,
    history,
    localStorage: ls,
    sessionStorage: ss,

    // APIs
    matchMedia,
    requestAnimationFrame: (cb: () => void) => setTimeout(cb, 0),
    cancelAnimationFrame: (id: number) => clearTimeout(id),
    getComputedStyle: () => ({}),
    scrollTo: noop,
    scrollBy: noop,
    addEventListener: noop,
    removeEventListener: noop,
    dispatchEvent: () => false,
    postMessage: noop,
    open: () => null,
    close: noop,

    // Observers
    ResizeObserver: stubObserver,
    IntersectionObserver: stubObserver,
    MutationObserver: stubObserver,

    // Network
    XMLHttpRequest: function XMLHttpRequest() {
      return {
        open: noop,
        send: noop,
        setRequestHeader: noop,
        abort: noop,
        addEventListener: noop,
        removeEventListener: noop,
        readyState: 0,
        status: 0,
        responseText: "",
      };
    },

    // Dialogs
    alert: noop,
    confirm: () => true,
    prompt: () => "",
  };

  // Self-reference
  window.window = window;
  window.self = window;
  window.globalThis = window;

  return {
    window,
    document,
    navigator,
    location,
    history,
    localStorage: ls,
    sessionStorage: ss,
    matchMedia,
    requestAnimationFrame: window.requestAnimationFrame,
    cancelAnimationFrame: window.cancelAnimationFrame,
    ResizeObserver: stubObserver,
    IntersectionObserver: stubObserver,
    MutationObserver: stubObserver,
    XMLHttpRequest: window.XMLHttpRequest,
    alert: noop,
    confirm: () => true,
    prompt: () => "",
  };
}

// ---------------------------------------------------------------------------
// SandboxProvider
// ---------------------------------------------------------------------------

function createBrowserDomSandboxProvider(
  options?: BrowserDomAdapterOptions,
): SandboxProvider {
  return {
    id: BROWSER_GLOBALS_ADAPTER_ID,
    augmentSandbox(sandbox: Record<string, unknown>): void {
      const globals = createBrowserGlobalsSandbox(options);
      for (const [key, value] of Object.entries(globals)) {
        sandbox[key] = value;
      }
    },
  };
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

export function createBrowserDomFactory(): RuntimeHookFactory {
  return {
    id: BROWSER_GLOBALS_ADAPTER_ID,
    createRuntimeHooks(adapter: ExecutionAdapter): Partial<RuntimeHooks> {
      const options = adapter.options as BrowserDomAdapterOptions | undefined;
      return {
        sandbox_providers: [createBrowserDomSandboxProvider(options)],
      };
    },
  };
}
