/**
 * Setup and generator module loader for the Shatter TypeScript frontend.
 *
 * Loads user-provided setup/teardown and generator files using the same
 * vm.createContext + TypeScript transpile pattern as executor.ts.
 */

import * as ts from "typescript";
import * as fs from "node:fs";
import * as vm from "node:vm";
import * as path from "node:path";
import { createRequire } from "node:module";
import type { SetupMode, GeneratorKind } from "./protocol.js";

/** A loaded setup module with its exports available for calling. */
export interface SetupModule {
  exports: Record<string, unknown>;
  filePath: string;
}

/** A loaded generator module with its exports available for calling. */
export interface GeneratorModule {
  exports: Record<string, unknown>;
  filePath: string;
}

/**
 * Transpile and load a TypeScript/JavaScript file, returning its exports.
 */
function loadAndTranspile(filePath: string): Record<string, unknown> {
  const absolutePath = path.resolve(filePath);
  const source = fs.readFileSync(absolutePath, "utf-8");
  const result = ts.transpileModule(source, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.CommonJS,
      esModuleInterop: true,
      strict: true,
    },
    fileName: absolutePath,
  });

  const targetRequire = createRequire(absolutePath);
  const moduleExports: Record<string, unknown> = {};
  const moduleObj = { exports: moduleExports };

  const sandbox = vm.createContext({
    module: moduleObj,
    exports: moduleExports,
    require: targetRequire,
    console,
    process,
    Buffer,
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
    __filename: absolutePath,
    __dirname: path.dirname(absolutePath),
  });

  vm.runInContext(result.outputText, sandbox, { filename: absolutePath });

  const finalExports = (sandbox as Record<string, unknown>)["module"] as {
    exports: Record<string, unknown>;
  };
  return finalExports.exports;
}

/**
 * Load a setup module file. The file should export a `setup()` function
 * and optionally a `teardown()` function.
 */
export function loadSetupModule(file: string): SetupModule {
  const exports = loadAndTranspile(file);
  return { exports, filePath: path.resolve(file) };
}

/**
 * Run the setup() export from a loaded setup module.
 *
 * Calls `module.setup(functionName, mode)` and returns the setup_context.
 */
export function runSetup(
  setupModule: SetupModule,
  functionName: string,
  mode: SetupMode,
): unknown {
  const setupFn = setupModule.exports["setup"];
  if (typeof setupFn !== "function") {
    throw new Error(
      `Setup file "${setupModule.filePath}" does not export a setup() function. ` +
      `Available exports: ${Object.keys(setupModule.exports).join(", ")}`,
    );
  }
  return (setupFn as (fn: string, mode: string) => unknown)(functionName, mode);
}

/**
 * Run the teardown() export from a loaded setup module.
 *
 * Calls `module.teardown(functionName, setupContext)`.
 */
export function runTeardown(
  setupModule: SetupModule,
  functionName: string,
  setupContext: unknown,
): void {
  const teardownFn = setupModule.exports["teardown"];
  if (typeof teardownFn !== "function") {
    throw new Error(
      `Setup file "${setupModule.filePath}" does not export a teardown() function. ` +
      `Available exports: ${Object.keys(setupModule.exports).join(", ")}`,
    );
  }
  (teardownFn as (fn: string, ctx: unknown) => void)(functionName, setupContext);
}

/**
 * Load a generator module file. The file should export named functions
 * that produce values when called.
 */
export function loadGeneratorModule(file: string): GeneratorModule {
  const exports = loadAndTranspile(file);
  return { exports, filePath: path.resolve(file) };
}

/**
 * Run a named generator function from a loaded generator module.
 *
 * For `kind: "type_name"`, `name` is the type name (e.g. "User").
 * For `kind: "param_name"`, `name` is the parameter name (e.g. "authToken").
 *
 * The module should export a function with that name.
 */
export function runGenerator(
  generatorModule: GeneratorModule,
  name: string,
  kind: GeneratorKind,
): unknown {
  const genFn = generatorModule.exports[name];
  if (typeof genFn !== "function") {
    throw new Error(
      `Generator file "${generatorModule.filePath}" does not export a "${name}" function ` +
      `(kind: ${kind}). Available exports: ${Object.keys(generatorModule.exports).join(", ")}`,
    );
  }
  return (genFn as () => unknown)();
}
