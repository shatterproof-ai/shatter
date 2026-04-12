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
 */

import { loadModuleExports } from "./executor.js";
import { REACT_HOOK_ADAPTER_ID } from "./react-hook-recognizer.js";
import type {
  InvocationContext,
  InvocationHook,
  InvocationOutcome,
  RuntimeHookFactory,
  RuntimeHooks,
} from "./runtime-hooks.js";
import type { ErrorInfo } from "./protocol.js";

// ---------------------------------------------------------------------------
// Scenario schema
// ---------------------------------------------------------------------------

interface HookCallableReturnScenario {
  kind: "hook_callable_return";
  /** Optional path to the callable property on the hook's return value.
   *  When absent, the adapter scans the return value for the first callable. */
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

function createReactHookInvocationHook(): InvocationHook {
  return {
    id: REACT_HOOK_ADAPTER_ID,

    invoke(ctx: InvocationContext): InvocationOutcome {
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
      const scenario = ctx.invocationModel.scenario_schema;
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
