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
