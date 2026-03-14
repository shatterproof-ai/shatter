import * as fc from "fast-check";
import { shannonEntropy, MIN_BUFFER_SIZE } from "./entropy";

describe("shannonEntropy", () => {
  it("returns 0 for empty buffer", () => {
    expect(shannonEntropy(Buffer.alloc(0))).toBe(0.0);
  });

  it("returns 0 for buffer below MIN_BUFFER_SIZE", () => {
    expect(shannonEntropy(Buffer.alloc(MIN_BUFFER_SIZE - 1, 0x42))).toBe(0.0);
  });

  it("returns 0 for constant bytes", () => {
    const data = Buffer.alloc(256, 0x42);
    expect(shannonEntropy(data)).toBe(0.0);
  });

  it("returns ~8.0 for uniform distribution", () => {
    const data = Buffer.from(Array.from({ length: 256 }, (_, i) => i));
    const e = shannonEntropy(data);
    expect(e).toBeCloseTo(8.0, 2);
  });

  it("returns ~1.0 for two equally frequent values", () => {
    const data = Buffer.alloc(256);
    data.fill(0, 0, 128);
    data.fill(1, 128, 256);
    const e = shannonEntropy(data);
    expect(e).toBeCloseTo(1.0, 2);
  });

  it("computes for exactly MIN_BUFFER_SIZE bytes", () => {
    const data = Buffer.from(Array.from({ length: MIN_BUFFER_SIZE }, (_, i) => i));
    expect(shannonEntropy(data)).toBeGreaterThan(0);
  });
});

describe("shannonEntropy properties", () => {
  it("entropy is always in [0, 8]", () => {
    fc.assert(
      fc.property(
        fc.uint8Array({ minLength: 0, maxLength: 1024 }),
        (arr) => {
          const e = shannonEntropy(Buffer.from(arr));
          return e >= 0.0 && e <= 8.0;
        },
      ),
    );
  });

  it("short buffers always return 0", () => {
    fc.assert(
      fc.property(
        fc.uint8Array({ minLength: 0, maxLength: MIN_BUFFER_SIZE - 1 }),
        (arr) => {
          return shannonEntropy(Buffer.from(arr)) === 0.0;
        },
      ),
    );
  });
});
