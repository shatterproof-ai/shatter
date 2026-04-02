import { serializeReplacer } from "./serialize.js";
import { reconstructValue } from "./reconstruct.js";

describe("serializeReplacer", () => {
  it("converts BigInt to tagged object", () => {
    const result = JSON.parse(JSON.stringify(42n, serializeReplacer));
    expect(result).toEqual({ __complex_type: "big_int", value: "42" });
  });

  it("converts negative BigInt", () => {
    const result = JSON.parse(JSON.stringify(-999n, serializeReplacer));
    expect(result).toEqual({ __complex_type: "big_int", value: "-999" });
  });

  it("converts zero BigInt", () => {
    const result = JSON.parse(JSON.stringify(0n, serializeReplacer));
    expect(result).toEqual({ __complex_type: "big_int", value: "0" });
  });

  it("preserves large BigInt precision", () => {
    const large = 99999999999999999999999999999999n;
    const result = JSON.parse(JSON.stringify(large, serializeReplacer));
    expect(result).toEqual({
      __complex_type: "big_int",
      value: "99999999999999999999999999999999",
    });
  });

  it("passes non-BigInt values through unchanged", () => {
    const obj = { a: 1, b: "hello", c: null, d: true, e: [1, 2] };
    const result = JSON.parse(JSON.stringify(obj, serializeReplacer));
    expect(result).toEqual(obj);
  });

  it("handles nested BigInt in objects", () => {
    const obj = { x: 1, big: 123n, nested: { inner: 456n } };
    const result = JSON.parse(JSON.stringify(obj, serializeReplacer));
    expect(result).toEqual({
      x: 1,
      big: { __complex_type: "big_int", value: "123" },
      nested: { inner: { __complex_type: "big_int", value: "456" } },
    });
  });

  it("handles BigInt in arrays", () => {
    const arr = [1n, 2, 3n];
    const result = JSON.parse(JSON.stringify(arr, serializeReplacer));
    expect(result).toEqual([
      { __complex_type: "big_int", value: "1" },
      2,
      { __complex_type: "big_int", value: "3" },
    ]);
  });

  it("roundtrips BigInt through serialize → reconstruct", () => {
    const original = 123456789012345678901234567890n;
    const serialized = JSON.parse(JSON.stringify(original, serializeReplacer));
    const reconstructed = reconstructValue(serialized);
    expect(reconstructed).toBe(original);
  });

  it("roundtrips nested BigInt through serialize → reconstruct", () => {
    const original = { result: 42n, items: [1n, 2n] };
    const serialized = JSON.parse(JSON.stringify(original, serializeReplacer));
    const reconstructed = reconstructValue(serialized) as Record<string, unknown>;
    expect(reconstructed["result"]).toBe(42n);
    expect(reconstructed["items"]).toEqual([1n, 2n]);
  });
});
