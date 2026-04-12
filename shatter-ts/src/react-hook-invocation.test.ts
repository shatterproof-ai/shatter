/**
 * Unit tests for the react-hook invocation adapter.
 *
 * Validates the mount-then-call execution path for React hooks using
 * generic invocation metadata (scenario_schema), not React-specific logic.
 */

import * as path from "node:path";

import { executeAdapterOwned } from "./executor.js";
import { findCallable, createReactHookFactory } from "./react-hook-invocation.js";
import { REACT_HOOK_ADAPTER_ID } from "./react-hook-recognizer.js";
import type { AdapterInvocationModel, InvocationContext } from "./runtime-hooks.js";

const FIXTURES_DIR = path.resolve(__dirname, "__fixtures__");
const HOOK_FIXTURE = path.join(FIXTURES_DIR, "adapter-hooks.tsx");

// ---------------------------------------------------------------------------
// findCallable
// ---------------------------------------------------------------------------

describe("findCallable", () => {
  it("returns null for primitives", () => {
    expect(findCallable(42)).toBeNull();
    expect(findCallable("hello")).toBeNull();
    expect(findCallable(null)).toBeNull();
    expect(findCallable(undefined)).toBeNull();
    expect(findCallable(true)).toBeNull();
  });

  it("returns the function when value is itself callable", () => {
    const fn = () => 42;
    const result = findCallable(fn);
    expect(result).not.toBeNull();
    expect(result!.fn).toBe(fn);
    expect(result!.key).toBeNull();
  });

  it("finds first callable property on an object", () => {
    const obj = { state: "on", toggle: () => {}, other: () => {} };
    const result = findCallable(obj);
    expect(result).not.toBeNull();
    expect(result!.key).toBe("toggle");
    expect(result!.fn).toBe(obj.toggle);
  });

  it("returns null for object with no callable properties", () => {
    const obj = { state: "on", count: 5 };
    expect(findCallable(obj)).toBeNull();
  });

  it("follows callable_path to a nested function", () => {
    const obj = { actions: { submit: () => "submitted" } };
    const result = findCallable(obj, ["actions", "submit"]);
    expect(result).not.toBeNull();
    expect(result!.key).toBe("actions.submit");
    expect(result!.fn()).toBe("submitted");
  });

  it("returns null when callable_path leads to non-function", () => {
    const obj = { actions: { count: 5 } };
    expect(findCallable(obj, ["actions", "count"])).toBeNull();
  });

  it("returns null when callable_path is broken", () => {
    const obj = { a: 1 };
    expect(findCallable(obj, ["missing", "path"])).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// InvocationHook via executeAdapterOwned
// ---------------------------------------------------------------------------

describe("react-hook invocation hook", () => {
  const factory = createReactHookFactory();

  function getHook() {
    const hooks = factory.createRuntimeHooks!(
      { id: REACT_HOOK_ADAPTER_ID },
      { phase: "execute" },
    );
    return hooks!.invocation_hooks![0]!;
  }

  const hookCallableReturnModel: AdapterInvocationModel = {
    kind: "adapter",
    adapter_id: REACT_HOOK_ADAPTER_ID,
    scenario_schema: { kind: "hook_callable_return" },
  };

  const noScenarioModel: AdapterInvocationModel = {
    kind: "adapter",
    adapter_id: REACT_HOOK_ADAPTER_ID,
  };

  it("mounts useToggle and returns structured result", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: hookCallableReturnModel,
      fileForExec: HOOK_FIXTURE,
      functionName: "useToggle",
      inputs: [true],
    });

    expect(result.thrown_error).toBeNull();
    // useToggle(true) → { state: "on", toggle: [Function] }
    // Functions are stripped by JSON serialization, but the object shape remains
    expect(result.return_value).toBeDefined();
    const rv = result.return_value as Record<string, unknown>;
    expect(rv.state).toBe("on");
  });

  it("mounts useToggle(false) and returns off state", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: hookCallableReturnModel,
      fileForExec: HOOK_FIXTURE,
      functionName: "useToggle",
      inputs: [false],
    });

    expect(result.thrown_error).toBeNull();
    const rv = result.return_value as Record<string, unknown>;
    expect(rv.state).toBe("off");
  });

  it("mounts useGreeting with non-callable return (graceful fallback)", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: hookCallableReturnModel,
      fileForExec: HOOK_FIXTURE,
      functionName: "useGreeting",
      inputs: ["Alice"],
    });

    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("Hello, Alice!");
  });

  it("mounts useGreeting with falsy name", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: hookCallableReturnModel,
      fileForExec: HOOK_FIXTURE,
      functionName: "useGreeting",
      inputs: [""],
    });

    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe("Hello, stranger!");
  });

  it("mounts useDebounced with delay > 0", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: hookCallableReturnModel,
      fileForExec: HOOK_FIXTURE,
      functionName: "useDebounced",
      inputs: [42, 100],
    });

    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe(42);
  });

  it("works without scenario_schema (mount only)", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: noScenarioModel,
      fileForExec: HOOK_FIXTURE,
      functionName: "useToggle",
      inputs: [true],
    });

    expect(result.thrown_error).toBeNull();
    const rv = result.return_value as Record<string, unknown>;
    expect(rv.state).toBe("on");
  });

  it("returns error for missing function", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: hookCallableReturnModel,
      fileForExec: HOOK_FIXTURE,
      functionName: "nonExistentHook",
      inputs: [],
    });

    expect(result.thrown_error).not.toBeNull();
    expect(result.thrown_error!.message).toContain("nonExistentHook");
    expect(result.thrown_error!.message).toContain("not found");
  });

  it("returns empty branch_path for adapter-owned execution", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: hookCallableReturnModel,
      fileForExec: HOOK_FIXTURE,
      functionName: "useToggle",
      inputs: [true],
    });

    expect(result.branch_path).toEqual([]);
    expect(result.lines_executed).toEqual([]);
    expect(result.path_constraints).toEqual([]);
  });

  it("captures performance metrics", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: hookCallableReturnModel,
      fileForExec: HOOK_FIXTURE,
      functionName: "useToggle",
      inputs: [true],
    });

    expect(result.performance.wall_time_ms).toBeGreaterThanOrEqual(0);
    expect(result.performance.cpu_time_us).toBeGreaterThanOrEqual(0);
  });
});

// ---------------------------------------------------------------------------
// Factory registration
// ---------------------------------------------------------------------------

describe("createReactHookFactory", () => {
  it("has the correct adapter ID", () => {
    const factory = createReactHookFactory();
    expect(factory.id).toBe(REACT_HOOK_ADAPTER_ID);
  });

  it("creates an invocation hook with matching ID", () => {
    const factory = createReactHookFactory();
    const hooks = factory.createRuntimeHooks!(
      { id: REACT_HOOK_ADAPTER_ID },
      { phase: "execute" },
    );
    expect(hooks!.invocation_hooks).toHaveLength(1);
    expect(hooks!.invocation_hooks![0]!.id).toBe(REACT_HOOK_ADAPTER_ID);
  });
});
