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
// Hook execution context — stateful hook tracking across renders
// ---------------------------------------------------------------------------
//
// Deterministic effect scheduling model
// ======================================
//
// Real React runs effects in two phases after each commit:
//   1. Layout effects (useLayoutEffect) — synchronous, before browser paint
//   2. Passive effects (useEffect) — asynchronous, after browser paint
//
// This hook runner mirrors that ordering deterministically, without a real
// DOM or event loop. After each render pass:
//
//   1. Cleanup functions from *stale* layout effects run (in registration order)
//   2. New/changed layout effect callbacks run (in registration order)
//   3. Cleanup functions from *stale* passive effects run (in registration order)
//   4. New/changed passive effect callbacks run (in registration order)
//
// "Stale" means the effect's deps changed since the last render (or the
// effect has no deps array, meaning it runs every render).
//
// Dep comparison uses Object.is per-element, matching React's semantics.
// An empty deps array `[]` means mount-only — the effect fires on the first
// render and never again.
//
// Unsupported semantics that fail explicitly:
//   - Async effect callbacks (returning a Promise/thenable) throw
//     UnsupportedEffectError. React itself warns against this pattern.
//
// Intentional fidelity limits (documented, not failures):
//   - No real DOM exists, so useLayoutEffect callbacks that read layout
//     measurements will see undefined values. The ordering guarantee
//     (layout before passive) is preserved.
//   - Effects run synchronously in the same microtask as the render.
//     There is no simulated event loop or requestAnimationFrame.
// ---------------------------------------------------------------------------

const MAX_RERENDERS = 10;
const DEFAULT_RERENDERS = 1;

type EffectPhase = "layout" | "passive";

interface EffectSlot {
  phase: EffectPhase;
  callback: (() => void | (() => void)) | null;
  deps: unknown[] | undefined;
  prevDeps: unknown[] | undefined;
  cleanup: (() => void) | null;
  /** Whether this slot has ever been flushed (for mount-only detection). */
  flushed: boolean;
}

/**
 * Thrown when an effect callback returns a Promise/thenable. React warns
 * against async effects; the hook runner fails hard so the issue surfaces
 * immediately rather than causing silent mis-execution.
 */
export class UnsupportedEffectError extends Error {
  constructor(phase: EffectPhase, slotIndex: number) {
    super(
      `Unsupported: ${phase} effect at slot ${slotIndex} returned a Promise. ` +
        `Effect callbacks must be synchronous. Use a synchronous wrapper that ` +
        `calls the async function if the hook needs to trigger async work.`,
    );
    this.name = "UnsupportedEffectError";
  }
}

/**
 * Tracks useState state slots and useEffect/useLayoutEffect registrations
 * across simulated renders. Each hook call is assigned a slot by call order
 * (same rule as real React). State setters queue updates;
 * `applyPendingUpdates` flushes them for the next render pass.
 * `flushEffects` runs effect callbacks in deterministic phase order.
 */
export class HookExecutionContext {
  private stateSlots: unknown[] = [];
  private pendingUpdates = new Map<number, unknown>();
  private callIndex = 0;

  private effectSlots: EffectSlot[] = [];
  private effectCallIndex = 0;

  /** Reset hook call counters before each render pass. */
  beginRender(): void {
    this.callIndex = 0;
    this.effectCallIndex = 0;
  }

  /** Stateful useState implementation. Slot index determined by call order. */
  useState<T>(
    initialState: T | (() => T),
  ): [T, (v: T | ((prev: T) => T)) => void] {
    const slotIndex = this.callIndex++;
    if (this.stateSlots.length <= slotIndex) {
      const value =
        typeof initialState === "function"
          ? (initialState as () => T)()
          : initialState;
      this.stateSlots.push(value);
    }
    const currentValue = this.stateSlots[slotIndex] as T;
    const setter = (v: T | ((prev: T) => T)): void => {
      const next =
        typeof v === "function"
          ? (v as (prev: T) => T)(this.stateSlots[slotIndex] as T)
          : v;
      this.pendingUpdates.set(slotIndex, next);
    };
    return [currentValue, setter];
  }

  /**
   * Register a passive effect (useEffect). The callback is stored and
   * executed during the next `flushEffects()` call if deps changed.
   */
  useEffect(callback: () => void | (() => void), deps?: unknown[]): void {
    this.registerEffect("passive", callback, deps);
  }

  /**
   * Register a layout effect (useLayoutEffect). Same semantics as useEffect
   * but flushes in the layout phase (before passive effects).
   */
  useLayoutEffect(callback: () => void | (() => void), deps?: unknown[]): void {
    this.registerEffect("layout", callback, deps);
  }

  private registerEffect(
    phase: EffectPhase,
    callback: () => void | (() => void),
    deps: unknown[] | undefined,
  ): void {
    const slotIndex = this.effectCallIndex++;
    if (this.effectSlots.length <= slotIndex) {
      // First render — create slot
      this.effectSlots.push({
        phase,
        callback,
        deps,
        prevDeps: undefined,
        cleanup: null,
        flushed: false,
      });
    } else {
      // Re-render — update callback and deps, preserve cleanup from previous flush
      const slot = this.effectSlots[slotIndex]!;
      slot.prevDeps = slot.deps;
      slot.callback = callback;
      slot.deps = deps;
    }
  }

  /**
   * Flush registered effects in deterministic phase order:
   * layout cleanup → layout callbacks → passive cleanup → passive callbacks.
   *
   * Only effects whose deps changed (or that have no deps array) are flushed.
   * Mount-only effects (`deps: []`) fire on the first flush and are skipped
   * thereafter.
   *
   * @throws {UnsupportedEffectError} if any callback returns a thenable.
   */
  flushEffects(): void {
    // Partition by phase, preserving registration order within each phase
    const layout: number[] = [];
    const passive: number[] = [];
    for (let i = 0; i < this.effectSlots.length; i++) {
      const slot = this.effectSlots[i]!;
      if (!this.shouldFireEffect(slot)) continue;
      if (slot.phase === "layout") layout.push(i);
      else passive.push(i);
    }

    // Layout phase
    for (const i of layout) this.runCleanup(i);
    for (const i of layout) this.runCallback(i);

    // Passive phase
    for (const i of passive) this.runCleanup(i);
    for (const i of passive) this.runCallback(i);
  }

  private shouldFireEffect(slot: EffectSlot): boolean {
    if (slot.callback === null) return false;
    // First flush — always fire
    if (!slot.flushed) return true;
    // No deps array — fire every render
    if (slot.deps === undefined) return true;
    // Empty deps — mount-only, already flushed
    if (slot.deps.length === 0) return false;
    // Compare deps
    if (slot.prevDeps === undefined) return true;
    if (slot.deps.length !== slot.prevDeps.length) return true;
    for (let i = 0; i < slot.deps.length; i++) {
      if (!Object.is(slot.deps[i], slot.prevDeps[i])) return true;
    }
    return false;
  }

  private runCleanup(slotIndex: number): void {
    const slot = this.effectSlots[slotIndex]!;
    if (slot.cleanup) {
      slot.cleanup();
      slot.cleanup = null;
    }
  }

  private runCallback(slotIndex: number): void {
    const slot = this.effectSlots[slotIndex]!;
    if (!slot.callback) return;
    const result = slot.callback();
    // Check for async effect (unsupported)
    if (
      result != null &&
      typeof (result as { then?: unknown }).then === "function"
    ) {
      throw new UnsupportedEffectError(slot.phase, slotIndex);
    }
    slot.cleanup = (result as (() => void) | undefined) ?? null;
    slot.flushed = true;
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
 * stateful useState, useEffect, and useLayoutEffect backed by the given
 * HookExecutionContext. All other hooks behave identically to the default
 * noop shim.
 */
export function createStatefulReactShimAdapter(
  ctx: HookExecutionContext,
): ResolverAdapter {
  const statefulReactModule: Record<string, unknown> = {
    ...getReactShim("react")!,
    useState: <T>(initialState: T | (() => T)) => ctx.useState(initialState),
    useEffect: (cb: () => void | (() => void), deps?: unknown[]) =>
      ctx.useEffect(cb, deps),
    useLayoutEffect: (cb: () => void | (() => void), deps?: unknown[]) =>
      ctx.useLayoutEffect(cb, deps),
    default: undefined as unknown,
  };
  statefulReactModule["default"] = statefulReactModule;

  const shimRegistry: Record<string, Record<string, unknown>> = {
    react: statefulReactModule,
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

  // Load module with stateful shim (bypasses cache since custom resolverAdapters).
  // Prefer the instrumented loader so rerenders record real coverage; fall back
  // to the raw module when no instrumented source is available (str-26fhi).
  let moduleExports: Record<string, unknown>;
  try {
    moduleExports = ctx.loadInstrumentedExports
      ? ctx.loadInstrumentedExports([shimAdapter])
      : loadModuleExports(ctx.fileForExec, [shimAdapter]);
  } catch (e: unknown) {
    return { status: "runtime_failed", thrown_error: buildErrorInfo(e) };
  }

  const hookFn = moduleExports[ctx.functionName];
  if (typeof hookFn !== "function") {
    return {
      status: "unsupported",
      thrown_error: {
        error_type: "Error",
        message:
          `Hook "${ctx.functionName}" not found in exports of ${ctx.fileForExec}. ` +
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
    return { status: "runtime_failed", thrown_error: buildErrorInfo(e) };
  }
  renders.push({ render_index: 0, value: stripFunctions(lastResult) });

  // Flush effects registered during the initial render
  try {
    hookCtx.flushEffects();
  } catch (e: unknown) {
    return { status: "runtime_failed", thrown_error: buildErrorInfo(e) };
  }

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
      return { status: "runtime_failed", thrown_error: buildErrorInfo(e) };
    }
    renders.push({ render_index: i + 1, value: stripFunctions(lastResult) });

    // Flush effects registered during this rerender
    try {
      hookCtx.flushEffects();
    } catch (e: unknown) {
      return { status: "runtime_failed", thrown_error: buildErrorInfo(e) };
    }
  }

  const outcome: RerenderOutcome = {
    renders,
    rerender_count: renders.length - 1,
  };
  return { status: "completed", return_value: outcome };
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
      // 1. Load module (React shims applied automatically for .tsx).
      // Prefer the instrumented loader so the mount records real coverage;
      // fall back to the raw module when no instrumented source is available
      // (str-26fhi).
      let moduleExports: Record<string, unknown>;
      try {
        moduleExports = ctx.loadInstrumentedExports
          ? ctx.loadInstrumentedExports()
          : loadModuleExports(ctx.fileForExec);
      } catch (e: unknown) {
        return { status: "runtime_failed", thrown_error: buildErrorInfo(e) };
      }

      // 2. Resolve hook function
      const hookFn = moduleExports[ctx.functionName];
      if (typeof hookFn !== "function") {
        return {
          status: "unsupported",
          thrown_error: {
            error_type: "Error",
            message:
              `Hook "${ctx.functionName}" not found in exports of ${ctx.fileForExec}. ` +
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
        return { status: "runtime_failed", thrown_error: buildErrorInfo(e) };
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
        return { status: "completed", return_value: mountResult };
      }

      // No recognized scenario — return mount result directly
      return { status: "completed", return_value: mountResult };
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
