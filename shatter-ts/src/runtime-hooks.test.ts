import * as fs from "node:fs";
import * as path from "node:path";

import { executeFunction } from "./executor.js";
import { loadSetupModule, runSetup } from "./setup-loader.js";
import { resolveRuntimeHooks, type RuntimeHookFactory } from "./runtime-hooks.js";

const FIXTURES_DIR = path.resolve(__dirname, "__fixtures__");
const RUNTIME_HOOK_FIXTURE = path.join(FIXTURES_DIR, "runtime-hook-setup.ts");
const TSCONFIG_PATHS_DIR = path.join(FIXTURES_DIR, "tsconfig-paths-runtime");
const TSCONFIG_PATHS_SETUP = path.join(TSCONFIG_PATHS_DIR, "src", "setup.ts");
const TSCONFIG_PATHS_EXECUTE = path.join(TSCONFIG_PATHS_DIR, "src", "execute.ts");

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
    fs.mkdirSync(path.join(TSCONFIG_PATHS_DIR, "src", "lib"), { recursive: true });
    fs.writeFileSync(
      path.join(TSCONFIG_PATHS_DIR, "tsconfig.json"),
      JSON.stringify({
        compilerOptions: {
          baseUrl: ".",
          paths: {
            "@app/*": ["src/*"],
          },
        },
      }, null, 2),
    );
    fs.writeFileSync(
      path.join(TSCONFIG_PATHS_DIR, "src", "lib", "math.ts"),
      `export function add(a: number, b: number): number {
  return a + b;
}
`,
    );
    fs.writeFileSync(
      TSCONFIG_PATHS_SETUP,
      `import { add } from "@app/lib/math";
export function setup(scope: string) {
  return { scope, answer: add(40, 2) };
}
export function teardown() {}
`,
    );
    fs.writeFileSync(
      TSCONFIG_PATHS_EXECUTE,
      `import { add } from "@app/lib/math";
export function usesAlias(): number {
  return add(20, 22);
}
`,
    );
  });

  afterAll(() => {
    if (fs.existsSync(RUNTIME_HOOK_FIXTURE)) {
      fs.unlinkSync(RUNTIME_HOOK_FIXTURE);
    }
    fs.rmSync(TSCONFIG_PATHS_DIR, { recursive: true, force: true });
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

  it("resolves tsconfig path aliases for setup modules", async () => {
    const runtimeHooks = resolveRuntimeHooks(
      {
        adapters: [{ id: "ts/module-resolution/tsconfig-paths", apply: "required" }],
      },
      {
        phase: "setup",
        project_root: TSCONFIG_PATHS_DIR,
        entry_file: TSCONFIG_PATHS_SETUP,
      },
    );
    const setupModule = loadSetupModule(TSCONFIG_PATHS_SETUP, runtimeHooks.resolver_adapters);
    const setupContext = await runSetup(setupModule, "alias-scope", "function");

    expect(setupContext).toEqual({ scope: "alias-scope", answer: 42 });
  });

  it("resolves tsconfig path aliases for execute modules", async () => {
    const runtimeHooks = resolveRuntimeHooks(
      {
        adapters: [{ id: "ts/module-resolution/tsconfig-paths", apply: "required" }],
      },
      {
        phase: "execute",
        project_root: TSCONFIG_PATHS_DIR,
        entry_file: TSCONFIG_PATHS_EXECUTE,
      },
    );
    const result = await executeFunction(
      TSCONFIG_PATHS_EXECUTE,
      "usesAlias",
      [],
      undefined,
      true,
      runtimeHooks.resolver_adapters,
    );

    expect(result.thrown_error).toBeNull();
    expect(result.return_value).toBe(42);
  });
});
