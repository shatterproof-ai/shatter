/**
 * Unit tests for the react-hook invocation adapter.
 *
 * Validates the mount-then-call execution path for React hooks using
 * generic invocation metadata (scenario_schema), not React-specific logic.
 */

import * as path from "node:path";

import { executeAdapterOwned } from "./executor.js";
import {
  findCallable,
  createReactHookFactory,
  isRerenderScenario,
  HookExecutionContext,
  UnsupportedEffectError,
  type RerenderOutcome,
} from "./react-hook-invocation.js";
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
// HookExecutionContext
// ---------------------------------------------------------------------------

describe("HookExecutionContext", () => {
  it("tracks useState initial state on first render", () => {
    const ctx = new HookExecutionContext();
    ctx.beginRender();
    const [value, setter] = ctx.useState(42);
    expect(value).toBe(42);
    expect(typeof setter).toBe("function");
  });

  it("evaluates lazy initializer on first render", () => {
    const ctx = new HookExecutionContext();
    ctx.beginRender();
    const [value] = ctx.useState(() => "lazy");
    expect(value).toBe("lazy");
  });

  it("preserves state across renders without updates", () => {
    const ctx = new HookExecutionContext();
    ctx.beginRender();
    const [v1] = ctx.useState(10);
    expect(v1).toBe(10);

    // Second render — same value, no updates
    ctx.beginRender();
    const [v2] = ctx.useState(99); // initial ignored
    expect(v2).toBe(10);
  });

  it("applies pending updates on rerender", () => {
    const ctx = new HookExecutionContext();
    ctx.beginRender();
    const [v1, setter] = ctx.useState("a");
    expect(v1).toBe("a");

    setter("b");
    expect(ctx.hasPendingUpdates()).toBe(true);
    expect(ctx.applyPendingUpdates()).toBe(true);

    ctx.beginRender();
    const [v2] = ctx.useState("ignored");
    expect(v2).toBe("b");
  });

  it("supports functional updater form", () => {
    const ctx = new HookExecutionContext();
    ctx.beginRender();
    const [v1, setter] = ctx.useState(5);
    expect(v1).toBe(5);

    setter((prev: number) => prev + 10);
    ctx.applyPendingUpdates();

    ctx.beginRender();
    const [v2] = ctx.useState(0);
    expect(v2).toBe(15);
  });

  it("tracks multiple useState slots independently", () => {
    const ctx = new HookExecutionContext();
    ctx.beginRender();
    const [a, setA] = ctx.useState("x");
    const [b, setB] = ctx.useState(100);
    expect(a).toBe("x");
    expect(b).toBe(100);

    setA("y");
    // Don't update B
    ctx.applyPendingUpdates();

    ctx.beginRender();
    const [a2] = ctx.useState("ignored");
    const [b2] = ctx.useState(0);
    expect(a2).toBe("y");
    expect(b2).toBe(100); // unchanged
  });

  it("returns false from applyPendingUpdates when nothing queued", () => {
    const ctx = new HookExecutionContext();
    ctx.beginRender();
    ctx.useState(1);
    expect(ctx.applyPendingUpdates()).toBe(false);
    expect(ctx.hasPendingUpdates()).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// isRerenderScenario type guard
// ---------------------------------------------------------------------------

describe("isRerenderScenario", () => {
  it("accepts valid rerender scenario", () => {
    expect(isRerenderScenario({ kind: "hook_rerender" })).toBe(true);
    expect(isRerenderScenario({ kind: "hook_rerender", max_rerenders: 3 })).toBe(true);
    expect(isRerenderScenario({ kind: "hook_rerender", callable_path: ["toggle"] })).toBe(true);
  });

  it("rejects non-rerender scenarios", () => {
    expect(isRerenderScenario({ kind: "hook_callable_return" })).toBe(false);
    expect(isRerenderScenario(null)).toBe(false);
    expect(isRerenderScenario(undefined)).toBe(false);
    expect(isRerenderScenario("hook_rerender")).toBe(false);
    expect(isRerenderScenario({})).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// hook_rerender scenario via executeAdapterOwned
// ---------------------------------------------------------------------------

describe("hook_rerender scenario", () => {
  const factory = createReactHookFactory();

  function getHook() {
    const hooks = factory.createRuntimeHooks!(
      { id: REACT_HOOK_ADAPTER_ID },
      { phase: "execute" },
    );
    return hooks!.invocation_hooks![0]!;
  }

  function rerenderModel(
    overrides?: Partial<{ max_rerenders: number; callable_path: string[] }>,
  ): AdapterInvocationModel {
    return {
      kind: "adapter",
      adapter_id: REACT_HOOK_ADAPTER_ID,
      scenario_schema: { kind: "hook_rerender", ...overrides },
    };
  }

  it("useToggle(true) → toggle → rerender shows state flip", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: rerenderModel(),
      fileForExec: HOOK_FIXTURE,
      functionName: "useToggle",
      inputs: [true],
    });

    expect(result.thrown_error).toBeNull();
    const outcome = result.return_value as RerenderOutcome;
    expect(outcome.renders).toHaveLength(2);
    expect(outcome.renders[0]!.render_index).toBe(0);
    expect(outcome.renders[0]!.value).toMatchObject({ state: "on" });
    expect(outcome.renders[1]!.render_index).toBe(1);
    expect(outcome.renders[1]!.value).toMatchObject({ state: "off" });
    expect(outcome.rerender_count).toBe(1);
  });

  it("useToggle(false) → toggle → rerender shows off→on", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: rerenderModel(),
      fileForExec: HOOK_FIXTURE,
      functionName: "useToggle",
      inputs: [false],
    });

    expect(result.thrown_error).toBeNull();
    const outcome = result.return_value as RerenderOutcome;
    expect(outcome.renders).toHaveLength(2);
    expect(outcome.renders[0]!.value).toMatchObject({ state: "off" });
    expect(outcome.renders[1]!.value).toMatchObject({ state: "on" });
  });

  it("useCounter(0) → increment → rerender shows count 0→1", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: rerenderModel(),
      fileForExec: HOOK_FIXTURE,
      functionName: "useCounter",
      inputs: [0],
    });

    expect(result.thrown_error).toBeNull();
    const outcome = result.return_value as RerenderOutcome;
    expect(outcome.renders).toHaveLength(2);
    expect(outcome.renders[0]!.value).toMatchObject({ count: 0 });
    expect(outcome.renders[1]!.value).toMatchObject({ count: 1 });
  });

  it("useCounter with multiple rerenders shows sequential increments", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: rerenderModel({ max_rerenders: 3 }),
      fileForExec: HOOK_FIXTURE,
      functionName: "useCounter",
      inputs: [0],
    });

    expect(result.thrown_error).toBeNull();
    const outcome = result.return_value as RerenderOutcome;
    expect(outcome.renders).toHaveLength(4);
    expect(outcome.renders[0]!.value).toMatchObject({ count: 0 });
    expect(outcome.renders[1]!.value).toMatchObject({ count: 1 });
    expect(outcome.renders[2]!.value).toMatchObject({ count: 2 });
    expect(outcome.renders[3]!.value).toMatchObject({ count: 3 });
    expect(outcome.rerender_count).toBe(3);
  });

  it("useCounter with callable_path targets decrement", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: rerenderModel({ callable_path: ["decrement"] }),
      fileForExec: HOOK_FIXTURE,
      functionName: "useCounter",
      inputs: [5],
    });

    expect(result.thrown_error).toBeNull();
    const outcome = result.return_value as RerenderOutcome;
    expect(outcome.renders).toHaveLength(2);
    expect(outcome.renders[0]!.value).toMatchObject({ count: 5 });
    expect(outcome.renders[1]!.value).toMatchObject({ count: 4 });
  });

  it("useGreeting (no callable) returns single render", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: rerenderModel(),
      fileForExec: HOOK_FIXTURE,
      functionName: "useGreeting",
      inputs: ["Bob"],
    });

    expect(result.thrown_error).toBeNull();
    const outcome = result.return_value as RerenderOutcome;
    expect(outcome.renders).toHaveLength(1);
    expect(outcome.renders[0]!.value).toBe("Hello, Bob!");
    expect(outcome.rerender_count).toBe(0);
  });

  it("max_rerenders: 0 returns only initial render", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: rerenderModel({ max_rerenders: 0 }),
      fileForExec: HOOK_FIXTURE,
      functionName: "useToggle",
      inputs: [true],
    });

    expect(result.thrown_error).toBeNull();
    const outcome = result.return_value as RerenderOutcome;
    expect(outcome.renders).toHaveLength(1);
    expect(outcome.rerender_count).toBe(0);
  });

  it("returns error for missing function", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: rerenderModel(),
      fileForExec: HOOK_FIXTURE,
      functionName: "nonExistent",
      inputs: [],
    });

    expect(result.thrown_error).not.toBeNull();
    expect(result.thrown_error!.message).toContain("nonExistent");
  });

  it("returns empty branch_path for rerender execution", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: rerenderModel(),
      fileForExec: HOOK_FIXTURE,
      functionName: "useToggle",
      inputs: [true],
    });

    expect(result.branch_path).toEqual([]);
    expect(result.lines_executed).toEqual([]);
    expect(result.path_constraints).toEqual([]);
  });

  it("strips functions from render snapshots", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: rerenderModel(),
      fileForExec: HOOK_FIXTURE,
      functionName: "useToggle",
      inputs: [true],
    });

    const outcome = result.return_value as RerenderOutcome;
    const firstRender = outcome.renders[0]!.value as Record<string, unknown>;
    // toggle should be stripped to "[Function]"
    expect(firstRender["toggle"]).toBe("[Function]");
  });
});

// ---------------------------------------------------------------------------
// HookExecutionContext — effect tracking
// ---------------------------------------------------------------------------

describe("HookExecutionContext effects", () => {
  it("useEffect registers and flushEffects runs callbacks", () => {
    const ctx = new HookExecutionContext();
    const log: string[] = [];
    ctx.beginRender();
    ctx.useEffect(() => { log.push("effect-1"); });
    ctx.useEffect(() => { log.push("effect-2"); });
    expect(log).toEqual([]);
    ctx.flushEffects();
    expect(log).toEqual(["effect-1", "effect-2"]);
  });

  it("useLayoutEffect runs before passive effects", () => {
    const ctx = new HookExecutionContext();
    const log: string[] = [];
    ctx.beginRender();
    ctx.useEffect(() => { log.push("passive"); });
    ctx.useLayoutEffect(() => { log.push("layout"); });
    ctx.flushEffects();
    expect(log).toEqual(["layout", "passive"]);
  });

  it("cleanup runs on re-render before new callback", () => {
    const ctx = new HookExecutionContext();
    const log: string[] = [];

    // First render
    ctx.beginRender();
    ctx.useEffect(() => {
      log.push("effect-1");
      return () => { log.push("cleanup-1"); };
    });
    ctx.flushEffects();
    expect(log).toEqual(["effect-1"]);

    // Second render — no deps means fire every render
    ctx.beginRender();
    ctx.useEffect(() => {
      log.push("effect-2");
      return () => { log.push("cleanup-2"); };
    });
    ctx.flushEffects();
    expect(log).toEqual(["effect-1", "cleanup-1", "effect-2"]);
  });

  it("unchanged deps skip re-run", () => {
    const ctx = new HookExecutionContext();
    const log: string[] = [];
    const dep = "stable";

    ctx.beginRender();
    ctx.useEffect(() => { log.push("run"); }, [dep]);
    ctx.flushEffects();
    expect(log).toEqual(["run"]);

    // Second render — same deps
    ctx.beginRender();
    ctx.useEffect(() => { log.push("run-again"); }, [dep]);
    ctx.flushEffects();
    expect(log).toEqual(["run"]); // not re-run
  });

  it("changed deps trigger re-run", () => {
    const ctx = new HookExecutionContext();
    const log: string[] = [];

    ctx.beginRender();
    ctx.useEffect(() => { log.push("run-1"); }, ["a"]);
    ctx.flushEffects();

    ctx.beginRender();
    ctx.useEffect(() => { log.push("run-2"); }, ["b"]);
    ctx.flushEffects();
    expect(log).toEqual(["run-1", "run-2"]);
  });

  it("empty deps [] only runs on mount", () => {
    const ctx = new HookExecutionContext();
    const log: string[] = [];

    ctx.beginRender();
    ctx.useEffect(() => { log.push("mount"); }, []);
    ctx.flushEffects();
    expect(log).toEqual(["mount"]);

    // Second render — should not re-run
    ctx.beginRender();
    ctx.useEffect(() => { log.push("should-not-run"); }, []);
    ctx.flushEffects();
    expect(log).toEqual(["mount"]);
  });

  it("no deps array runs every render", () => {
    const ctx = new HookExecutionContext();
    const log: string[] = [];

    ctx.beginRender();
    ctx.useEffect(() => { log.push("r1"); });
    ctx.flushEffects();

    ctx.beginRender();
    ctx.useEffect(() => { log.push("r2"); });
    ctx.flushEffects();

    ctx.beginRender();
    ctx.useEffect(() => { log.push("r3"); });
    ctx.flushEffects();
    expect(log).toEqual(["r1", "r2", "r3"]);
  });

  it("async callback throws UnsupportedEffectError", () => {
    const ctx = new HookExecutionContext();
    ctx.beginRender();
    ctx.useEffect(() => {
      return Promise.resolve() as unknown as void;
    }, []);
    expect(() => ctx.flushEffects()).toThrow(UnsupportedEffectError);
  });

  it("layout cleanup runs before layout callbacks, passive cleanup before passive callbacks", () => {
    const ctx = new HookExecutionContext();
    const log: string[] = [];

    // First render
    ctx.beginRender();
    ctx.useLayoutEffect(() => { log.push("layout-cb-1"); return () => { log.push("layout-cleanup-1"); }; });
    ctx.useEffect(() => { log.push("passive-cb-1"); return () => { log.push("passive-cleanup-1"); }; });
    ctx.flushEffects();
    expect(log).toEqual(["layout-cb-1", "passive-cb-1"]);

    // Second render — no deps = re-fire
    log.length = 0;
    ctx.beginRender();
    ctx.useLayoutEffect(() => { log.push("layout-cb-2"); });
    ctx.useEffect(() => { log.push("passive-cb-2"); });
    ctx.flushEffects();
    expect(log).toEqual([
      "layout-cleanup-1", "layout-cb-2",
      "passive-cleanup-1", "passive-cb-2",
    ]);
  });

  it("mixed useState and useEffect in same context", () => {
    const ctx = new HookExecutionContext();
    const log: string[] = [];

    ctx.beginRender();
    const [val, setVal] = ctx.useState(10);
    ctx.useEffect(() => { log.push(`effect-val-${val}`); }, [val]);
    ctx.flushEffects();
    expect(log).toEqual(["effect-val-10"]);

    setVal(20);
    ctx.applyPendingUpdates();

    ctx.beginRender();
    const [val2] = ctx.useState(0);
    ctx.useEffect(() => { log.push(`effect-val-${val2}`); }, [val2]);
    ctx.flushEffects();
    expect(log).toEqual(["effect-val-10", "effect-val-20"]);
  });
});

// ---------------------------------------------------------------------------
// hook_rerender scenario with effects
// ---------------------------------------------------------------------------

describe("hook_rerender scenario with effects", () => {
  const factory = createReactHookFactory();

  function getHook() {
    const hooks = factory.createRuntimeHooks!(
      { id: REACT_HOOK_ADAPTER_ID },
      { phase: "execute" },
    );
    return hooks!.invocation_hooks![0]!;
  }

  function rerenderModel(
    overrides?: Partial<{ max_rerenders: number; callable_path: string[] }>,
  ): AdapterInvocationModel {
    return {
      kind: "adapter",
      adapter_id: REACT_HOOK_ADAPTER_ID,
      scenario_schema: { kind: "hook_rerender", ...overrides },
    };
  }

  it("useDocTitle effect callback executes during rerender scenario", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: rerenderModel({ max_rerenders: 0 }),
      fileForExec: HOOK_FIXTURE,
      functionName: "useDocTitle",
      inputs: ["My Page"],
    });

    expect(result.thrown_error).toBeNull();
    const outcome = result.return_value as RerenderOutcome;
    expect(outcome.renders).toHaveLength(1);
    expect(outcome.renders[0]!.value).toBe("My Page");
  });

  it("useAsyncEffect throws UnsupportedEffectError in rerender scenario", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: rerenderModel({ max_rerenders: 0 }),
      fileForExec: HOOK_FIXTURE,
      functionName: "useAsyncEffect",
      inputs: [],
    });

    expect(result.thrown_error).not.toBeNull();
    expect(result.thrown_error!.error_type).toBe("UnsupportedEffectError");
    expect(result.thrown_error!.message).toContain("returned a Promise");
  });

  it("useDebounced effect runs without error in rerender scenario", async () => {
    const result = await executeAdapterOwned({
      hook: getHook(),
      invocationModel: rerenderModel({ max_rerenders: 0 }),
      fileForExec: HOOK_FIXTURE,
      functionName: "useDebounced",
      inputs: [42, 100],
    });

    expect(result.thrown_error).toBeNull();
    const outcome = result.return_value as RerenderOutcome;
    expect(outcome.renders).toHaveLength(1);
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
