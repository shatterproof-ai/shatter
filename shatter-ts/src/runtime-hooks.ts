import * as path from "node:path";

import * as ts from "typescript";

import type { ExecutionAdapter, ExecutionProfile } from "./protocol.js";
import type { ResolverAdapter } from "./executor.js";
import { ADAPTER_ID_IMPORT_META_ENV } from "./runtime-hints.js";

export type RuntimeHookPhase = "execute" | "setup";

export interface RuntimeHookContext {
  phase: RuntimeHookPhase;
  project_root?: string | null;
  entry_file?: string;
  function_name?: string;
}

export interface SandboxProvider {
  id: string;
  augmentSandbox(sandbox: Record<string, unknown>): void;
}

export interface RuntimeHooks {
  resolver_adapters: ResolverAdapter[];
  sandbox_providers: SandboxProvider[];
}

export interface RuntimeHookFactory {
  id: string;
  createRuntimeHooks?(
    adapter: ExecutionAdapter,
    context: RuntimeHookContext,
  ): RuntimeHooks | null | undefined;
}

function mergeRuntimeHooks(target: RuntimeHooks, next: RuntimeHooks | null | undefined): void {
  if (!next) return;
  target.resolver_adapters.push(...next.resolver_adapters);
  target.sandbox_providers.push(...next.sandbox_providers);
}

interface TsconfigResolutionState {
  compilerOptions: ts.CompilerOptions;
}

const tsconfigStateCache = new Map<string, TsconfigResolutionState>();

function isNonRelativeModule(moduleId: string): boolean {
  return !moduleId.startsWith(".") && !path.isAbsolute(moduleId);
}

function findTsconfigPath(context: RuntimeHookContext): string | null {
  const searchDir = context.project_root
    ? path.resolve(context.project_root)
    : context.entry_file
      ? path.dirname(path.resolve(context.entry_file))
      : null;
  if (!searchDir) {
    return null;
  }

  return ts.findConfigFile(searchDir, ts.sys.fileExists, "tsconfig.json") ?? null;
}

function getTsconfigResolutionState(tsconfigPath: string): TsconfigResolutionState {
  const cached = tsconfigStateCache.get(tsconfigPath);
  if (cached) {
    return cached;
  }

  const readResult = ts.readConfigFile(tsconfigPath, ts.sys.readFile);
  if (readResult.error) {
    throw new Error(`tsconfig-paths adapter could not read ${tsconfigPath}`);
  }

  const parsed = ts.parseJsonConfigFileContent(
    readResult.config,
    ts.sys,
    path.dirname(tsconfigPath),
    undefined,
    tsconfigPath,
  );
  if (parsed.errors.length > 0) {
    throw new Error(`tsconfig-paths adapter could not parse ${tsconfigPath}`);
  }
  if (!parsed.options.paths || Object.keys(parsed.options.paths).length === 0) {
    throw new Error(`tsconfig-paths adapter requires compilerOptions.paths in ${tsconfigPath}`);
  }

  const state = { compilerOptions: parsed.options };
  tsconfigStateCache.set(tsconfigPath, state);
  return state;
}

function createTsconfigPathsFactory(): RuntimeHookFactory {
  return {
    id: "ts/module-resolution/tsconfig-paths",
    createRuntimeHooks(_adapter, context) {
      const tsconfigPath = findTsconfigPath(context);
      if (!tsconfigPath) {
        throw new Error("tsconfig-paths adapter could not find tsconfig.json");
      }

      const state = getTsconfigResolutionState(tsconfigPath);
      return {
        resolver_adapters: [
          {
            id: "ts/module-resolution/tsconfig-paths",
            resolveModule({ module_id, importer_file }) {
              if (!isNonRelativeModule(module_id) || !importer_file) {
                return { kind: "continue" };
              }

              const resolved = ts.resolveModuleName(
                module_id,
                importer_file,
                state.compilerOptions,
                ts.sys,
              ).resolvedModule;
              if (!resolved) {
                return { kind: "continue" };
              }

              const resolvedFile = resolved.resolvedFileName;
              if (!path.isAbsolute(resolvedFile) || resolvedFile.endsWith(".d.ts")) {
                return { kind: "continue" };
              }
              if (resolvedFile.includes(`${path.sep}node_modules${path.sep}`)) {
                return { kind: "continue" };
              }

              return { kind: "rewrite", module_id: resolvedFile };
            },
          },
        ],
        sandbox_providers: [],
      };
    },
  };
}

/** Vite's standard import.meta.env defaults. */
const VITE_ENV_DEFAULTS: Record<string, string | boolean> = {
  MODE: "development",
  DEV: true,
  PROD: false,
  SSR: false,
  BASE_URL: "/",
};

interface ImportMetaEnvOptions {
  env?: Record<string, string>;
}

function createImportMetaEnvFactory(): RuntimeHookFactory {
  return {
    id: ADAPTER_ID_IMPORT_META_ENV,
    createRuntimeHooks(adapter) {
      const userEnv = (adapter.options as ImportMetaEnvOptions | undefined)?.env ?? {};
      const mergedEnv: Record<string, string | boolean> = { ...VITE_ENV_DEFAULTS, ...userEnv };

      return {
        resolver_adapters: [],
        sandbox_providers: [
          {
            id: ADAPTER_ID_IMPORT_META_ENV,
            augmentSandbox(sandbox) {
              const meta = sandbox["__shatter_import_meta"] as
                | { env?: Record<string, string | boolean> }
                | undefined;
              if (meta) {
                meta.env = { ...mergedEnv, ...meta.env };
              }
            },
          },
        ],
      };
    },
  };
}

const DEFAULT_RUNTIME_HOOK_FACTORIES: readonly RuntimeHookFactory[] = [
  createTsconfigPathsFactory(),
  createImportMetaEnvFactory(),
];

export function resolveRuntimeHooks(
  executionProfile: ExecutionProfile | null | undefined,
  context: RuntimeHookContext,
  factories: readonly RuntimeHookFactory[] = DEFAULT_RUNTIME_HOOK_FACTORIES,
): RuntimeHooks {
  const hooks: RuntimeHooks = { resolver_adapters: [], sandbox_providers: [] };
  if (!executionProfile) {
    return hooks;
  }

  const factoryById = new Map(factories.map((factory) => [factory.id, factory]));
  for (const adapter of executionProfile.adapters) {
    if (adapter.apply === "disabled") {
      continue;
    }

    const factory = factoryById.get(adapter.id);
    if (!factory) {
      throw new Error(`execution adapter not supported by TypeScript frontend: ${adapter.id}`);
    }

    mergeRuntimeHooks(hooks, factory.createRuntimeHooks?.(adapter, context));
  }

  return hooks;
}
