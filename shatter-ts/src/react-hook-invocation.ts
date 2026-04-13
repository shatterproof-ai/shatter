/**
 * React hook invocation adapter — mounts a hook and optionally invokes
 * a returned callable. Driven entirely by generic invocation metadata
 * (scenario_schema on InvocationModel), not React-specific core logic.
 *
 * Scenario: hook_callable_return
 *   1. Load the target module (React shims applied for .tsx)
 *   2. Mount the hook with provided inputs
 *   3. Inspect the return value for callable properties
 *   4. If a callable is found, invoke it (exercises returned-function branches)
 *   5. Return the mount result as the invocation outcome
 *
 * Scenario: hook_rerender
 *   1. Load the target module with a stateful React shim (useState tracks state)
 *   2. Mount the hook (initial render)
 *   3. Find and invoke a callable on the return value (triggers state setter)
 *   4. Apply pending state updates and re-invoke the hook (rerender)
 *   5. Repeat up to max_rerenders times
 *   6. Return per-render snapshots as structured outcome
 */

import { loadModuleExports, type ResolverAdapter } from "./executor.js";
import { REACT_HOOK_ADAPTER_ID } from "./react-hook-recognizer.js";
import { REACT_MODULE_NAMES, getReactShim } from "./react-shim.js";
import type {
  InvocationContext,
  InvocationHook,
  InvocationOutcome,
  RuntimeHookFactory,
  RuntimeHooks,
} from "./runtime-hooks.js";
import type { ErrorInfo } from "./protocol.js";

// ---------------------------------------------------------------------------
// Scenario schemas
// ---------------------------------------------------------------------------

interface HookCallableReturnScenario {
  kind: "hook_callable_return";
  /** Optional path to the callable property on the hook's return value.
   *  When absent, the adapter scans the return value for the first callable. */
  callable_path?: string[];
}

interface HookRerenderScenario {
  kind: "hook_rerender";
  /** Max rerenders after initial mount. Default 1, capped at MAX_RERENDERS. */
  max_rerenders?: number;
  /** Optional path to the callable that triggers state changes. */
  callable_path?: string[];
}

function isCallableReturnScenario(
  schema: unknown,
): schema is HookCallableReturnScenario {
  return (
    typeof schema === "object" &&
    schema !== null &&
    (schema as Record<string, unknown>).kind === "hook_callable_return"
  );
}

export function isRerenderScenario(
  schema: unknown,
): schema is HookRerenderScenario {
  return (
    typeof schema === "object" &&
    schema !== null &&
    (schema as Record<string, unknown>).kind === "hook_rerender"
  );
}

// ---------------------------------------------------------------------------
// Callable discovery
// ---------------------------------------------------------------------------

interface FoundCallable {
  fn: (...args: unknown[]) => unknown;
  key: string | null;
}

/**
 * Locate a callable on `value`. Checks the value itself first, then
 * follows `callablePath` if given, and falls back to scanning own
 * enumerable properties for the first function.
 */
export function findCallable(
  value: unknown,
  callablePath?: string[],
): FoundCallable | null {
  // Value is itself a function
  if (typeof value === "function") {
    return { fn: value as (...args: unknown[]) => unknown, key: null };
  }

  if (typeof value !== "object" || value === null) {
    return null;
  }

  const obj = value as Record<string, unknown>;

  // Follow explicit path
  if (callablePath && callablePath.length > 0) {
    let current: unknown = obj;
    for (const segment of callablePath) {
      if (typeof current !== "object" || current === null) return null;
      current = (current as Record<string, unknown>)[segment];
    }
    if (typeof current === "function") {
      return {
        fn: current as (...args: unknown[]) => unknown,
        key: callablePath.join("."),
      };
    }
    return null;
  }

  // Scan for first callable property
  for (const key of Object.keys(obj)) {
    if (typeof obj[key] === "function") {
      return { fn: obj[key] as (...args: unknown[]) => unknown, key };
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// Hook execution context — stateful useState tracking across renders
// ---------------------------------------------------------------------------

const MAX_RERENDERS = 10;
const DEFAULT_RERENDERS = 1;

/**
 * Tracks useState state slots across simulated renders. Each useState call
 * in the hook is assigned a slot by call order (same rule as real React).
 * Setters queue updates; `applyPendingUpdates` flushes them for the next
 * render pass.
 */
export class HookExecutionContext {
  private stateSlots: unknown[] = [];
  private pendingUpdates = new Map<number, unknown>();
  private callIndex = 0;

  /** Reset the hook call counter before each render pass. */
  beginRender(): void {
    this.callIndex = 0;
  }

  /** Stateful useState implementation. Slot index determined by call order. */
  useState<T>(initialState: T | (() => T)): [T, (v: T | ((prev: T) => T)) => void] {
    const slotIndex = this.callIndex++;
    if (this.stateSlots.length <= slotIndex) {
      const value = typeof initialState === "function"
        ? (initialState as () => T)()
        : initialState;
      this.stateSlots.push(value);
    }
    const currentValue = this.stateSlots[slotIndex] as T;
    const setter = (v: T | ((prev: T) => T)): void => {
      const next = typeof v === "function"
        ? (v as (prev: T) => T)(this.stateSlots[slotIndex] as T)
        : v;
      this.pendingUpdates.set(slotIndex, next);
    };
    return [currentValue, setter];
  }

  /** Flush queued state updates into slots. Returns true if any were applied. */
  applyPendingUpdates(): boolean {
    if (this.pendingUpdates.size === 0) return false;
    for (const [idx, val] of this.pendingUpdates) {
      this.stateSlots[idx] = val;
    }
    this.pendingUpdates.clear();
    return true;
  }

  /** Whether any setter was called since the last applyPendingUpdates. */
  hasPendingUpdates(): boolean {
    return this.pendingUpdates.size > 0;
  }
}

// ---------------------------------------------------------------------------
// Stateful React shim — delegates useState to HookExecutionContext
// ---------------------------------------------------------------------------

/**
 * Build a ResolverAdapter that intercepts React module imports and provides
 * a stateful useState backed by the given HookExecutionContext. All other
 * hooks behave identically to the default noop shim.
 */
export function createStatefulReactShimAdapter(
  ctx: HookExecutionContext,
): ResolverAdapter {
  const noop = (): void => {};

  const statefulReactModule: Record<string, unknown> = {
    ...getReactShim("react")!,
    useState: <T>(initialState: T | (() => T)) => ctx.useState(initialState),
    default: undefined as unknown,
  };
  statefulReactModule["default"] = statefulReactModule;

  const shimRegistry: Record<string, Record<string, unknown>> = {
    "react": statefulReactModule,
    "react/jsx-runtime": getReactShim("react/jsx-runtime")!,
    "react/jsx-dev-runtime": getReactShim("react/jsx-dev-runtime")!,
  };

  return {
    id: "ts/react-shim/stateful",
    resolveModule({ module_id }) {
      if (REACT_MODULE_NAMES.has(module_id)) {
        return { kind: "resolved", value: shimRegistry[module_id] };
      }
      return { kind: "continue" };
    },
  };
}

// ---------------------------------------------------------------------------
// Render snapshot
// ---------------------------------------------------------------------------

/** One render observation in a rerender sequence. */
export interface RenderSnapshot {
  render_index: number;
  value: unknown;
}

/** Structured outcome for hook_rerender scenarios. */
export interface RerenderOutcome {
  renders: RenderSnapshot[];
  rerender_count: number;
}

// ---------------------------------------------------------------------------
// Invocation hook
// ---------------------------------------------------------------------------

function buildErrorInfo(e: unknown): ErrorInfo {
  const err = e as {
    constructor?: { name?: string };
    message?: string;
    stack?: string;
  };
  return {
    error_type: err.constructor?.name ?? "Error",
    message: String(err.message ?? e),
    stack: err.stack ?? null,
  };
}

function stripFunctions(value: unknown): unknown {
  if (typeof value === "function") return "[Function]";
  if (typeof value !== "object" || value === null) return value;
  if (Array.isArray(value)) return value.map(stripFunctions);
  const result: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
    result[k] = typeof v === "function" ? "[Function]" : stripFunctions(v);
  }
  return result;
}

function executeRerenderScenario(
  ctx: InvocationContext,
  scenario: HookRerenderScenario,
): InvocationOutcome {
  const hookCtx = new HookExecutionContext();
  const shimAdapter = createStatefulReactShimAdapter(hookCtx);

  // Load module with stateful shim (bypasses cache since custom resolverAdapters)
  let moduleExports: Record<string, unknown>;
  try {
    moduleExports = loadModuleExports(ctx.fileForExec, [shimAdapter]);
  } catch (e: unknown) {
    return { thrownError: buildErrorInfo(e) };
  }

  const hookFn = moduleExports[ctx.functionName];
  if (typeof hookFn !== "function") {
    return {
      thrownError: {
        error_type: "Error",
        message: `Hook "${ctx.functionName}" not found in exports of ${ctx.fileForExec}. ` +
          `Available exports: ${Object.keys(moduleExports).join(", ")}`,
        stack: null,
      },
    };
  }

  const maxRerenders = Math.min(
    Math.max(scenario.max_rerenders ?? DEFAULT_RERENDERS, 0),
    MAX_RERENDERS,
  );
  const renders: RenderSnapshot[] = [];

  // Initial render
  hookCtx.beginRender();
  let lastResult: unknown;
  try {
    lastResult = hookFn(...ctx.inputs);
  } catch (e: unknown) {
    return { thrownError: buildErrorInfo(e) };
  }
  renders.push({ render_index: 0, value: stripFunctions(lastResult) });

  // Action-rerender loop
  for (let i = 0; i < maxRerenders; i++) {
    const found = findCallable(lastResult, scenario.callable_path);
    if (!found) break;

    try {
      found.fn();
    } catch {
      // Callable errors are non-fatal — same as hook_callable_return
      break;
    }

    if (!hookCtx.applyPendingUpdates()) break;

    hookCtx.beginRender();
    try {
      lastResult = hookFn(...ctx.inputs);
    } catch (e: unknown) {
      return { thrownError: buildErrorInfo(e) };
    }
    renders.push({ render_index: i + 1, value: stripFunctions(lastResult) });
  }

  const outcome: RerenderOutcome = {
    renders,
    rerender_count: renders.length - 1,
  };
  return { returnValue: outcome };
}

function createReactHookInvocationHook(): InvocationHook {
  return {
    id: REACT_HOOK_ADAPTER_ID,

    invoke(ctx: InvocationContext): InvocationOutcome {
      const scenario = ctx.invocationModel.scenario_schema;

      // Rerender scenario — stateful execution
      if (isRerenderScenario(scenario)) {
        return executeRerenderScenario(ctx, scenario);
      }

      // All other scenarios use the default noop React shim
      // 1. Load module (React shims applied automatically for .tsx)
      let moduleExports: Record<string, unknown>;
      try {
        moduleExports = loadModuleExports(ctx.fileForExec);
      } catch (e: unknown) {
        return { thrownError: buildErrorInfo(e) };
      }

      // 2. Resolve hook function
      const hookFn = moduleExports[ctx.functionName];
      if (typeof hookFn !== "function") {
        return {
          thrownError: {
            error_type: "Error",
            message: `Hook "${ctx.functionName}" not found in exports of ${ctx.fileForExec}. ` +
              `Available exports: ${Object.keys(moduleExports).join(", ")}`,
            stack: null,
          },
        };
      }

      // 3. Mount the hook (call with inputs)
      let mountResult: unknown;
      try {
        mountResult = hookFn(...ctx.inputs);
      } catch (e: unknown) {
        return { thrownError: buildErrorInfo(e) };
      }

      // 4. If scenario is hook_callable_return, find and invoke callable
      if (isCallableReturnScenario(scenario)) {
        const found = findCallable(mountResult, scenario.callable_path);
        if (found) {
          try {
            found.fn();
          } catch {
            // Callable invocation errors are not fatal — the mount
            // result is still the primary observation. With the React
            // shim, setState is a noop so callables may behave oddly.
          }
        }
        // Return the mount result regardless — it describes hook behavior.
        return { returnValue: mountResult };
      }

      // No recognized scenario — return mount result directly
      return { returnValue: mountResult };
    },
  };
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

export function createReactHookFactory(): RuntimeHookFactory {
  return {
    id: REACT_HOOK_ADAPTER_ID,
    createRuntimeHooks(): Partial<RuntimeHooks> {
      return {
        invocation_hooks: [createReactHookInvocationHook()],
      };
    },
  };
}
