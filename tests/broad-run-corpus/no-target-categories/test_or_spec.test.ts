// NoTargetReason::TestOrSpec — a TypeScript test file. Recognized by name
// pattern (`*.test.ts`).
declare function describe(label: string, fn: () => void): void;
declare function it(label: string, fn: () => void): void;
declare function expect(actual: unknown): { toBe: (expected: unknown) => void };

describe("placeholder", () => {
  it("is a placeholder", () => {
    expect(1 + 1).toBe(2);
  });
});
