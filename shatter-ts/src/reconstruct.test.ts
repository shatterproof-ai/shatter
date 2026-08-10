import { reconstructValue } from "./reconstruct.js";
import { STUB_INPUT_TAG, resetStubRotationForTests } from "./opaque-stub-registry.js";

describe("reconstructValue — opaque-param stub tag (str-syj9b)", () => {
  beforeEach(() => {
    resetStubRotationForTests();
  });

  it("builds a chainable recording stub from a stub-input tag", () => {
    const value = reconstructValue({
      __complex_type: STUB_INPUT_TAG,
      stub_key: "K",
      overrides: { "locator.count": [0, 2] },
    }) as { locator: (s: string) => { count: () => number } };

    expect(value.locator("#a").count()).toBe(0);
    expect(value.locator("#a").count()).toBe(2);
    expect(value.locator("#a").count()).toBe(0);
  });

  it("tolerates a stub tag with no overrides", () => {
    const value = reconstructValue({ __complex_type: STUB_INPUT_TAG, stub_key: "K" });
    expect(() => (value as { anything: () => unknown }).anything()).not.toThrow();
  });

  it("leaves ordinary values untouched", () => {
    expect(reconstructValue(42)).toBe(42);
    expect(reconstructValue({ a: 1, b: "x" })).toEqual({ a: 1, b: "x" });
  });
});

// str-ya5dx: the input generator now emits `closure` complex values for
// callable params. reconstructValue must turn that wire envelope into a
// callable stub so target code doing `cb()` executes instead of crashing
// with `x is not a function`.
describe("reconstructValue closure stubs (str-ya5dx)", () => {
  it("builds a callable stub for a closure value", () => {
    const fn = reconstructValue({ __complex_type: "closure", variant: "identity" });
    expect(typeof fn).toBe("function");
  });

  it("identity variant returns its first argument and records calls", () => {
    const fn = reconstructValue({
      __complex_type: "closure",
      variant: "identity",
    }) as ((...a: unknown[]) => unknown) & { calls: unknown[][] };
    expect(fn("a", "b")).toBe("a");
    expect(fn(7)).toBe(7);
    expect(fn.calls).toEqual([["a", "b"], [7]]);
  });

  it("constant variant returns a fixed value", () => {
    const fn = reconstructValue({
      __complex_type: "closure",
      variant: "constant",
    }) as (...a: unknown[]) => unknown;
    expect(fn(1, 2, 3)).toBe(0);
  });

  it("thrower variant throws when invoked", () => {
    const fn = reconstructValue({
      __complex_type: "closure",
      variant: "thrower",
    }) as (...a: unknown[]) => unknown;
    expect(() => fn()).toThrow();
  });

  it("defaults to identity for absent/unknown variants", () => {
    const fn = reconstructValue({ __complex_type: "closure" }) as (
      ...a: unknown[]
    ) => unknown;
    expect(fn("x")).toBe("x");
  });

  it("a reconstructed closure survives being called by target code", () => {
    // Simulates a `people.map(fn)`-style call site: the param is invoked with
    // real arguments and must not throw `is not a function`.
    const cb = reconstructValue({
      __complex_type: "closure",
      variant: "identity",
    }) as (x: number) => number;
    expect([1, 2, 3].map(cb)).toEqual([1, 2, 3]);
  });
});
