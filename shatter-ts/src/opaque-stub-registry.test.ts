import * as fc from "fast-check";
import { fastCheckParameters } from "./fast-check-config.js";
import {
  buildStubValue,
  deriveModuleSpecifier,
  getStubRegistry,
  matchStubKey,
  overridesForKey,
  parseTsRuntimeValues,
  resetStubRotationForTests,
  type StubOverrides,
  type StubPrimitive,
} from "./opaque-stub-registry.js";

beforeEach(() => {
  resetStubRotationForTests();
});

describe("deriveModuleSpecifier", () => {
  it("extracts a scoped package name", () => {
    expect(
      deriveModuleSpecifier("/proj/node_modules/@playwright/test/types/test.d.ts"),
    ).toBe("@playwright/test");
  });

  it("extracts an unscoped package name", () => {
    expect(deriveModuleSpecifier("/proj/node_modules/pg/lib/index.d.ts")).toBe("pg");
  });

  it("resolves to the innermost nested install", () => {
    expect(
      deriveModuleSpecifier("/p/node_modules/a/node_modules/@playwright/test/x.d.ts"),
    ).toBe("@playwright/test");
  });

  it("normalizes Windows separators", () => {
    expect(
      deriveModuleSpecifier("C:\\proj\\node_modules\\@playwright\\test\\index.d.ts"),
    ).toBe("@playwright/test");
  });

  it("returns null for a project-local declaration", () => {
    expect(deriveModuleSpecifier("/proj/src/handlers.ts")).toBeNull();
  });
});

describe("matchStubKey", () => {
  const registry = getStubRegistry(null);

  it("matches a built-in Playwright Page by module-qualified key", () => {
    expect(
      matchStubKey("Page", ["/x/node_modules/@playwright/test/types/test.d.ts"], registry),
    ).toBe("@playwright/test:Page");
  });

  it("matches a built-in Playwright Locator", () => {
    expect(
      matchStubKey("Locator", ["/x/node_modules/@playwright/test/types/test.d.ts"], registry),
    ).toBe("@playwright/test:Locator");
  });

  it("does not match Page declared in an unrelated module", () => {
    expect(matchStubKey("Page", ["/x/node_modules/other-lib/index.d.ts"], registry)).toBeNull();
  });

  it("falls back to a bare type-name key (config escape hatch)", () => {
    const extended = new Map(registry);
    extended.set("Page", { overrides: { count: [0, 1] } });
    expect(matchStubKey("Page", ["/proj/src/local.ts"], extended)).toBe("Page");
  });

  it("prefers a module-qualified key over a bare key", () => {
    const extended = new Map(registry);
    extended.set("Page", { overrides: { count: [9] } });
    expect(
      matchStubKey("Page", ["/x/node_modules/@playwright/test/types/test.d.ts"], extended),
    ).toBe("@playwright/test:Page");
  });

  it("returns null for an unregistered type", () => {
    expect(matchStubKey("Widget", ["/proj/src/local.ts"], registry)).toBeNull();
  });
});

describe("parseTsRuntimeValues", () => {
  it("returns empty when the section is absent", () => {
    expect(parseTsRuntimeValues({ functions: {} }).size).toBe(0);
  });

  it("parses a valid entry with scalar-list overrides", () => {
    const out = parseTsRuntimeValues({
      ts_runtime_values: {
        "@playwright/test:Page": {
          stub: "proxy",
          overrides: { "locator.count": [0, 1, 3], "locator.isVisible": [true, false] },
        },
      },
    });
    expect(out.get("@playwright/test:Page")?.overrides).toEqual({
      "locator.count": [0, 1, 3],
      "locator.isVisible": [true, false],
    });
  });

  it("registers an entry with no overrides", () => {
    const out = parseTsRuntimeValues({ ts_runtime_values: { MyHandle: { stub: "proxy" } } });
    expect(out.has("MyHandle")).toBe(true);
    expect(out.get("MyHandle")?.overrides).toEqual({});
  });

  it("skips non-scalar override entries but keeps scalar siblings", () => {
    const out = parseTsRuntimeValues({
      ts_runtime_values: { K: { overrides: { good: [1, 2], bad: [{ nested: true }], notList: 5 } } },
    });
    expect(out.get("K")?.overrides).toEqual({ good: [1, 2] });
  });

  it("ignores a malformed section without throwing", () => {
    expect(parseTsRuntimeValues({ ts_runtime_values: [1, 2, 3] }).size).toBe(0);
    expect(parseTsRuntimeValues("nonsense").size).toBe(0);
  });
});

describe("buildStubValue", () => {
  const pageOverrides: StubOverrides = overridesForKey(getStubRegistry(null), "@playwright/test:Page");

  it("chains arbitrary property access and calls without throwing", () => {
    const stub = buildStubValue("K", {}) as Record<string, () => Record<string, unknown>>;
    expect(() => {
      const r = (stub as unknown as { a: { b: (x: number) => { c: () => unknown } } }).a.b(1).c();
      void r;
    }).not.toThrow();
  });

  it("is not thenable: awaiting the stub yields the stub itself", async () => {
    const stub = buildStubValue("K", {});
    expect(await stub).toBe(stub);
  });

  it("returns rotating override scalars for a bare method key", () => {
    const stub = buildStubValue("K", { count: [0, 1, 3] }) as { count: () => number };
    expect([stub.count(), stub.count(), stub.count(), stub.count()]).toEqual([0, 1, 3, 0]);
  });

  it("returns rotating override scalars for a dotted call path (Page style)", async () => {
    const stub = buildStubValue("@playwright/test:Page", pageOverrides) as {
      locator: (sel: string) => { count: () => number; isVisible: () => boolean };
    };
    const first = await stub.locator("#a").count();
    const second = await stub.locator("#a").count();
    expect(first).toBe(0);
    expect(second).toBe(1);
    expect(typeof (await stub.locator("#a").isVisible())).toBe("boolean");
  });

  it("keeps the call chain alive after a non-override call", () => {
    // page.goto(url) has no override; the returned stub must still be chainable,
    // and a bare-name override (last-segment match) fires at any depth.
    const stub = buildStubValue("K", { count: [7] }) as {
      goto: (u: string) => { nav: { count: () => number } };
    };
    expect(stub.goto("/x").nav.count()).toBe(7);
  });

  it("rotates independently per override key", () => {
    const stub = buildStubValue("K", { count: [0, 5], flag: [true, false] }) as {
      count: () => number;
      flag: () => boolean;
    };
    expect(stub.count()).toBe(0);
    expect(stub.flag()).toBe(true);
    expect(stub.count()).toBe(5);
    expect(stub.flag()).toBe(false);
  });
});

describe("buildStubValue — property invariants", () => {
  it("never throws for any access/call chain and only ever returns configured override scalars", () => {
    const methodName = fc.constantFrom("locator", "count", "isVisible", "goto", "click", "first");
    fc.assert(
      fc.property(fc.array(fc.tuple(methodName, fc.boolean()), { maxLength: 8 }), (steps) => {
        resetStubRotationForTests();
        const overrides: StubOverrides = { count: [0, 1, 3], isVisible: [true, false] };
        const allowed = new Set<StubPrimitive>([0, 1, 3, true, false]);
        let node: unknown = buildStubValue("K", overrides);
        for (const [name, asCall] of steps) {
          const next = (node as Record<string, unknown>)[name];
          if (asCall && typeof next === "function") {
            const result = (next as () => unknown)();
            // A configured method returns a scalar; anything else stays a chainable object.
            if (typeof result !== "object" && typeof result !== "function") {
              expect(allowed.has(result as StubPrimitive)).toBe(true);
              node = buildStubValue("K", overrides);
            } else {
              node = result;
            }
          } else {
            node = next;
          }
        }
        return true;
      }),
      fastCheckParameters(300),
    );
  });
});
