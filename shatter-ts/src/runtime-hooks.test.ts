import * as fs from "node:fs";
import * as path from "node:path";

import { loadSetupModule, runSetup } from "./setup-loader.js";
import { resolveRuntimeHooks, type RuntimeHookFactory } from "./runtime-hooks.js";

const FIXTURES_DIR = path.resolve(__dirname, "__fixtures__");
const RUNTIME_HOOK_FIXTURE = path.join(FIXTURES_DIR, "runtime-hook-setup.ts");

describe("runtime hook layer", () => {
  beforeAll(() => {
    fs.writeFileSync(
      RUNTIME_HOOK_FIXTURE,
      `const virtualValue = require("@virtual/value");
export function setup(scope: string) {
  return { scope, answer: virtualValue.answer };
}
export function teardown() {}
`,
    );
  });

  afterAll(() => {
    if (fs.existsSync(RUNTIME_HOOK_FIXTURE)) {
      fs.unlinkSync(RUNTIME_HOOK_FIXTURE);
    }
  });

  it("builds resolver-backed runtime hooks in execution-profile order", async () => {
    const factories: RuntimeHookFactory[] = [
      {
        id: "test.rewrite",
        createRuntimeHooks() {
          return {
            resolver_adapters: [
              {
                id: "test.rewrite",
                resolveModule({ module_id }) {
                  if (module_id === "@virtual/value") {
                    return { kind: "rewrite", module_id: "virtual:value" };
                  }
                  return { kind: "continue" };
                },
              },
            ],
          };
        },
      },
      {
        id: "test.resolve",
        createRuntimeHooks() {
          return {
            resolver_adapters: [
              {
                id: "test.resolve",
                resolveModule({ module_id }) {
                  if (module_id === "virtual:value") {
                    return { kind: "resolved", value: { answer: 42 } };
                  }
                  return { kind: "continue" };
                },
              },
            ],
          };
        },
      },
    ];

    const runtimeHooks = resolveRuntimeHooks(
      {
        adapters: [
          { id: "test.rewrite", apply: "auto" },
          { id: "test.resolve", apply: "required" },
        ],
      },
      { phase: "setup", entry_file: RUNTIME_HOOK_FIXTURE },
      factories,
    );
    const setupModule = loadSetupModule(RUNTIME_HOOK_FIXTURE, runtimeHooks.resolver_adapters);
    const setupContext = await runSetup(setupModule, "my-scope", "function");

    expect(runtimeHooks.resolver_adapters.map((adapter) => adapter.id)).toEqual([
      "test.rewrite",
      "test.resolve",
    ]);
    expect(setupContext).toEqual({ scope: "my-scope", answer: 42 });
  });

  it("ignores disabled adapters before support checks", () => {
    const hooks = resolveRuntimeHooks(
      { adapters: [{ id: "test.unsupported", apply: "disabled" }] },
      { phase: "execute" },
    );
    expect(hooks.resolver_adapters).toEqual([]);
  });

  it("fails explicitly for unsupported active adapters", () => {
    expect(() =>
      resolveRuntimeHooks(
        { adapters: [{ id: "test.unsupported", apply: "required" }] },
        { phase: "execute" },
      ),
    ).toThrow("execution adapter not supported by TypeScript frontend: test.unsupported");
  });
});
