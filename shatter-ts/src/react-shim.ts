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

const shimRegistry: Record<string, Record<string, unknown>> = {
  "react": reactModule,
  "react/jsx-runtime": jsxRuntimeModule,
  "react/jsx-dev-runtime": jsxDevRuntimeModule,
};

/**
 * Returns the mock module for the given React module name, or undefined
 * if the name is not a React module.
 */
export function getReactShim(moduleName: string): Record<string, unknown> | undefined {
  return shimRegistry[moduleName];
}
