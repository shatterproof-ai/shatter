/**
 * Lightweight React mock for executing components in the Shatter sandbox.
 *
 * React components are plain functions (props → JSX element tree). The real
 * react/jsx-runtime converts JSX syntax into createElement calls at compile
 * time. These mocks return plain element-like objects so the concolic engine
 * can execute components and explore their conditional rendering branches
 * without a full React runtime.
 *
 * Hooks are stubbed to return deterministic values:
 * - useState/useReducer: return initial state (no re-renders)
 * - useEffect/useLayoutEffect: noop (side effects skipped)
 * - useMemo: calls factory (may contain branches worth exploring)
 * - useCallback: returns the callback unchanged
 */

/** Symbol matching React's internal element marker. */
const REACT_ELEMENT_TYPE = Symbol.for("react.element");

/** Static ID returned by useId() — deterministic for concolic execution. */
const STATIC_USE_ID = ":r0:";

/** Module specifiers intercepted by the require wrapper. */
export const REACT_MODULE_NAMES = new Set([
  "react",
  "react/jsx-runtime",
  "react/jsx-dev-runtime",
]);

/**
 * React-family specifiers aliased onto the Shatter shim in Node's *native*
 * module cache (see `installNativeReactAliases` in `executor.ts`) so that
 * dependencies loaded from `node_modules` via `createRequire` receive the shim
 * on their transitive `require('react')` &c. instead of the project's real
 * React (whose hook dispatcher is null outside a renderer → null-dispatcher
 * crashes).
 *
 * This is a superset of `REACT_MODULE_NAMES`: the latter governs only in-sandbox
 * target/instrumented-code resolution (JSX runtime), whereas native aliasing
 * also covers `react-dom` / `react-dom/client` which dependencies pull in.
 */
export const NATIVE_REACT_ALIAS_NAMES: readonly string[] = [
  "react",
  "react-dom",
  "react-dom/client",
  "react/jsx-runtime",
  "react/jsx-dev-runtime",
];

// ── JSX element construction ────────────────────────────────────────

interface ReactElement {
  $$typeof: symbol;
  type: unknown;
  props: Record<string, unknown>;
  key: string | null;
}

function createElementObject(
  type: unknown,
  props: Record<string, unknown> | null,
  key?: string | null,
): ReactElement {
  return {
    $$typeof: REACT_ELEMENT_TYPE,
    type,
    props: { ...props, key: key ?? null },
    key: key ?? null,
  };
}

function jsx(type: unknown, props: Record<string, unknown>, key?: string): ReactElement {
  return createElementObject(type, props, key);
}

function createElement(
  type: unknown,
  props: Record<string, unknown> | null,
  ...children: unknown[]
): ReactElement {
  const merged = { ...props };
  if (children.length === 1) {
    merged.children = children[0];
  } else if (children.length > 1) {
    merged.children = children;
  }
  return createElementObject(type, merged, (props as Record<string, unknown>)?.key as string);
}

// ── Hook stubs ──────────────────────────────────────────────────────

const noop = (): void => {};

function useState<T>(initialState: T | (() => T)): [T, (v: T) => void] {
  const value = typeof initialState === "function" ? (initialState as () => T)() : initialState;
  return [value, noop as (v: T) => void];
}

function useReducer<S>(_reducer: unknown, initialState: S): [S, (action: unknown) => void] {
  return [initialState, noop as (action: unknown) => void];
}

function useMemo<T>(factory: () => T, _deps?: unknown[]): T {
  return factory();
}

function useCallback<T>(callback: T, _deps?: unknown[]): T {
  return callback;
}

function useRef<T>(initialValue: T): { current: T } {
  return { current: initialValue };
}

function useContext<T>(context: { _currentValue?: T }): T | undefined {
  return context?._currentValue;
}

function useId(): string {
  return STATIC_USE_ID;
}

/**
 * Store-subscription hook (React 18). Third-party state libraries loaded via the
 * native-alias path call this — notably zustand v5's `useStore`
 * (`react.js` → `useSyncExternalStore`). Deterministic stub: return the current
 * snapshot without subscribing, so the store's value flows into the component
 * for concolic exploration.
 */
function useSyncExternalStore<T>(
  _subscribe: unknown,
  getSnapshot: () => T,
  getServerSnapshot?: () => T,
): T {
  const snapshot = getSnapshot ?? getServerSnapshot;
  return typeof snapshot === "function" ? snapshot() : (undefined as T);
}

/** Pass-through: expose the value immediately (no deferral). */
function useDeferredValue<T>(value: T): T {
  return value;
}

/** Non-pending transition that runs its callback synchronously. */
function useTransition(): [boolean, (cb: () => void) => void] {
  return [false, (cb: () => void) => (typeof cb === "function" ? cb() : undefined)];
}

/**
 * Stub of React.createContext. Returns an object exposing `_currentValue`
 * (read by the `useContext` stub above), a `Provider` that updates
 * `_currentValue` from its `value` prop and renders its children, and a
 * `Consumer` that supports the render-prop form. Deterministic and
 * side-effect-free, matching the rest of the shim — no subscriber
 * notification, no fiber tree, no re-render scheduling.
 */
interface ReactContext<T> {
  _currentValue: T;
  Provider: (props: { value?: T; children?: unknown }) => unknown;
  Consumer: (props: { children?: unknown }) => unknown;
  displayName?: string;
}

function createContext<T>(defaultValue: T): ReactContext<T> {
  const context: ReactContext<T> = {
    _currentValue: defaultValue,
    Provider: (props) => {
      if (props && "value" in props) {
        context._currentValue = props.value as T;
      }
      return props?.children;
    },
    Consumer: (props) => {
      const children = props?.children;
      return typeof children === "function"
        ? (children as (v: T) => unknown)(context._currentValue)
        : null;
    },
  };
  return context;
}

/** Pass-through wrapper — returns the component unchanged. */
function forwardRef(render: unknown): unknown {
  return render;
}

/** Pass-through wrapper — returns the component unchanged. */
function memo(component: unknown): unknown {
  return component;
}

const Fragment = Symbol.for("react.fragment");

// ── Assembled module mocks ──────────────────────────────────────────

const reactModule = {
  useState,
  useReducer,
  useEffect: noop,
  useLayoutEffect: noop,
  useMemo,
  useCallback,
  useRef,
  useContext,
  useId,
  useSyncExternalStore,
  useInsertionEffect: noop,
  useImperativeHandle: noop,
  useDebugValue: noop,
  useDeferredValue,
  useTransition,
  createContext,
  createElement,
  Fragment,
  forwardRef,
  memo,
  Children: {
    map: (children: unknown, fn: (child: unknown, index: number) => unknown) =>
      Array.isArray(children) ? children.map(fn) : children != null ? [fn(children, 0)] : [],
    forEach: (children: unknown, fn: (child: unknown, index: number) => void) => {
      if (Array.isArray(children)) children.forEach(fn);
      else if (children != null) fn(children, 0);
    },
    count: (children: unknown) => (Array.isArray(children) ? children.length : children != null ? 1 : 0),
    only: (children: unknown) => children,
    toArray: (children: unknown) => (Array.isArray(children) ? children : children != null ? [children] : []),
  },
  default: undefined as unknown,
};
// CommonJS default export — transpiled `import React from 'react'` reads `module.exports.default`
reactModule.default = reactModule;

const jsxRuntimeModule = {
  jsx,
  jsxs: jsx,
  Fragment,
};

const jsxDevRuntimeModule = {
  jsxDEV: jsx,
  Fragment,
};

// react-dom shim. Third-party dependencies (e.g. Mantine portals) may
// transitively `require('react-dom')` / `require('react-dom/client')`. The
// concolic sandbox never renders to a real DOM, so these are pass-throughs /
// no-ops — just enough surface for a dependency's module top level to load
// without reaching for a live renderer.
const reactDomModule = {
  createPortal: (children: unknown) => children,
  render: noop,
  hydrate: noop,
  unmountComponentAtNode: () => false,
  findDOMNode: () => null,
  flushSync: <T>(fn: () => T): T | undefined =>
    typeof fn === "function" ? fn() : undefined,
  unstable_batchedUpdates: <A>(fn: (a: A) => void, a: A) => fn(a),
  createRoot: () => ({ render: noop, unmount: noop }),
  hydrateRoot: () => ({ render: noop, unmount: noop }),
  version: "0.0.0-shatter-shim",
  default: undefined as unknown,
};
reactDomModule.default = reactDomModule;

const reactDomClientModule = {
  createRoot: reactDomModule.createRoot,
  hydrateRoot: reactDomModule.hydrateRoot,
  default: undefined as unknown,
};
reactDomClientModule.default = reactDomClientModule;

const shimRegistry: Record<string, Record<string, unknown>> = {
  "react": reactModule,
  "react/jsx-runtime": jsxRuntimeModule,
  "react/jsx-dev-runtime": jsxDevRuntimeModule,
  "react-dom": reactDomModule,
  "react-dom/client": reactDomClientModule,
};

/**
 * Returns the mock module for the given React module name, or undefined
 * if the name is not a React module.
 */
export function getReactShim(moduleName: string): Record<string, unknown> | undefined {
  return shimRegistry[moduleName];
}
