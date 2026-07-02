import * as path from "node:path";

import * as ts from "typescript";

import type {
  ExecutionAdapter,
  ExecutionProfile,
  InvocationModel,
  InvocationOutcome,
} from "./protocol.js";
import type { ResolverAdapter } from "./executor.js";
import { ADAPTER_ID_IMPORT_META_ENV } from "./runtime-hints.js";
import { createBrowserDomFactory } from "./browser-dom-adapter.js";
import { createReactHookFactory } from "./react-hook-invocation.js";

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

/** The non-direct InvocationModel variant. The direct variant short-circuits
 *  to the existing executeFunction/executeInstrumented paths and never reaches
 *  an InvocationHook. */
export type AdapterInvocationModel = Extract<
  InvocationModel,
  { kind: "adapter" }
>;

/** Context handed to an InvocationHook describing the call to dispatch. */
export interface InvocationContext {
  readonly fileForExec: string;
  readonly functionName: string;
  readonly invocationModel: AdapterInvocationModel;
  readonly inputs: readonly unknown[];
  readonly capture: boolean;
  /**
   * When present, loads the target module's **instrumented** exports into a
   * live sandbox wired with coverage callbacks, so invoking the returned
   * exports records lines_executed / branch_path / path_constraints exactly
   * like a direct call. Hooks MUST prefer this over loading the raw module
   * when it is provided, and pass any scenario-specific resolver adapters
   * (e.g. a stateful React shim) through the `resolverAdapters` argument —
   * the same override semantics as `loadModuleExports`.
   *
   * Absent when no instrumented source is available for the target (the hook
   * then falls back to loading the raw module, yielding empty coverage).
   * Call it exactly once per invocation; coverage accumulates in the sandbox.
   */
  readonly loadInstrumentedExports?: (
    resolverAdapters?: ResolverAdapter[],
  ) => Record<string, unknown>;
}
export type { InvocationOutcome } from "./protocol.js";

/** Adapter-owned invocation hook. Mounts, scenario-drives, or otherwise
 *  invokes a target instead of calling the exported symbol directly.
 *  Resolved by `id`, which must equal `InvocationModel.adapter_id`. */
export interface InvocationHook {
  readonly id: string;
  invoke(
    context: InvocationContext,
  ): Promise<InvocationOutcome> | InvocationOutcome;
}

export interface RuntimeHooks {
  resolver_adapters: ResolverAdapter[];
  sandbox_providers: SandboxProvider[];
  invocation_hooks: InvocationHook[];
}

export interface RuntimeHookFactory {
  id: string;
  createRuntimeHooks?(
    adapter: ExecutionAdapter,
    context: RuntimeHookContext,
  ): Partial<RuntimeHooks> | null | undefined;
}

function mergeRuntimeHooks(
  target: RuntimeHooks,
  next: Partial<RuntimeHooks> | null | undefined,
): void {
  if (!next) return;
  if (next.resolver_adapters)
    target.resolver_adapters.push(...next.resolver_adapters);
  if (next.sandbox_providers)
    target.sandbox_providers.push(...next.sandbox_providers);
  if (next.invocation_hooks)
    target.invocation_hooks.push(...next.invocation_hooks);
}

/** Pure dispatcher selection used both by the execute handler and by
 *  property tests. Decides whether a given analysis routes to the direct
 *  path, an adapter-owned path, or an unsupported failure. */
export type InvocationStrategy =
  | { kind: "direct" }
  | { kind: "adapter"; hook: InvocationHook; model: AdapterInvocationModel }
  | { kind: "unsupported"; adapterId: string };

export function chooseInvocationStrategy(
  invocationModel: InvocationModel | undefined,
  hooks: ReadonlyArray<InvocationHook>,
): InvocationStrategy {
  if (!invocationModel || invocationModel.kind === "direct") {
    return { kind: "direct" };
  }
  const hook = hooks.find((h) => h.id === invocationModel.adapter_id);
  if (!hook) {
    return { kind: "unsupported", adapterId: invocationModel.adapter_id };
  }
  return { kind: "adapter", hook, model: invocationModel };
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

  return (
    ts.findConfigFile(searchDir, ts.sys.fileExists, "tsconfig.json") ?? null
  );
}

function hasPaths(options: ts.CompilerOptions): boolean {
  return !!options.paths && Object.keys(options.paths).length > 0;
}

/** Resolve a `references[].path` entry (relative to its parent tsconfig dir)
 *  to an absolute tsconfig file path. The entry may point at a tsconfig file
 *  directly, at a directory containing tsconfig.json, or at a path missing
 *  the `.json` suffix — TypeScript accepts all three forms. */
function resolveReferencedTsconfig(referencePath: string): string | null {
  if (ts.sys.fileExists(referencePath)) {
    return referencePath;
  }
  if (ts.sys.directoryExists(referencePath)) {
    return (
      ts.findConfigFile(referencePath, ts.sys.fileExists, "tsconfig.json") ??
      null
    );
  }
  const withJson = referencePath.endsWith(".json")
    ? referencePath
    : `${referencePath}.json`;
  if (ts.sys.fileExists(withJson)) {
    return withJson;
  }
  return null;
}

function getTsconfigResolutionState(
  tsconfigPath: string,
  visited: Set<string> = new Set(),
): TsconfigResolutionState {
  const cached = tsconfigStateCache.get(tsconfigPath);
  if (cached) {
    return cached;
  }
  if (visited.has(tsconfigPath)) {
    throw new Error(
      `tsconfig-paths adapter detected reference cycle at ${tsconfigPath}`,
    );
  }
  visited.add(tsconfigPath);

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

  if (hasPaths(parsed.options)) {
    const state = { compilerOptions: parsed.options };
    tsconfigStateCache.set(tsconfigPath, state);
    return state;
  }

  // Vite-style project layouts use a references-only root tsconfig.json that
  // points at sibling configs (e.g. tsconfig.app.json) holding the actual
  // baseUrl/paths. Walk references in declared order and adopt the first
  // referenced config that supplies paths.
  const references = parsed.projectReferences ?? [];
  const referenceErrors: string[] = [];
  for (const ref of references) {
    const refTsconfigPath = resolveReferencedTsconfig(ref.path);
    if (!refTsconfigPath) {
      continue;
    }
    try {
      const refState = getTsconfigResolutionState(refTsconfigPath, visited);
      tsconfigStateCache.set(tsconfigPath, refState);
      return refState;
    } catch (err) {
      referenceErrors.push(
        `${refTsconfigPath}: ${(err as Error).message}`,
      );
    }
  }

  const detail =
    referenceErrors.length > 0
      ? ` (tried references: ${referenceErrors.join("; ")})`
      : "";
  throw new Error(
    `tsconfig-paths adapter requires compilerOptions.paths in ${tsconfigPath}${detail}`,
  );
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
              if (
                !path.isAbsolute(resolvedFile) ||
                resolvedFile.endsWith(".d.ts")
              ) {
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
        invocation_hooks: [],
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
      const userEnv =
        (adapter.options as ImportMetaEnvOptions | undefined)?.env ?? {};
      const mergedEnv: Record<string, string | boolean> = {
        ...VITE_ENV_DEFAULTS,
        ...userEnv,
      };

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
                meta.env = { ...meta.env, ...mergedEnv };
              }
            },
          },
        ],
        invocation_hooks: [],
      };
    },
  };
}

export const DEFAULT_RUNTIME_HOOK_FACTORIES: readonly RuntimeHookFactory[] = [
  createTsconfigPathsFactory(),
  createImportMetaEnvFactory(),
  createReactHookFactory(),
  createBrowserDomFactory(),
];

export function resolveRuntimeHooks(
  executionProfile: ExecutionProfile | null | undefined,
  context: RuntimeHookContext,
  factories: readonly RuntimeHookFactory[] = DEFAULT_RUNTIME_HOOK_FACTORIES,
): RuntimeHooks {
  const hooks: RuntimeHooks = {
    resolver_adapters: [],
    sandbox_providers: [],
    invocation_hooks: [],
  };
  if (!executionProfile) {
    return hooks;
  }

  const factoryById = new Map(
    factories.map((factory) => [factory.id, factory]),
  );
  for (const adapter of executionProfile.adapters) {
    if (adapter.apply === "disabled") {
      continue;
    }

    const factory = factoryById.get(adapter.id);
    if (!factory) {
      throw new Error(
        `execution adapter not supported by TypeScript frontend: ${adapter.id}`,
      );
    }

    mergeRuntimeHooks(hooks, factory.createRuntimeHooks?.(adapter, context));
  }

  return hooks;
}
