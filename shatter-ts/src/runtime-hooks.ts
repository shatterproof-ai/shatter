import type { ExecutionAdapter, ExecutionProfile } from "./protocol.js";
import type { ResolverAdapter } from "./executor.js";

export type RuntimeHookPhase = "execute" | "setup";

export interface RuntimeHookContext {
  phase: RuntimeHookPhase;
  project_root?: string | null;
  entry_file?: string;
  function_name?: string;
}

export interface RuntimeHooks {
  resolver_adapters: ResolverAdapter[];
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
}

export function resolveRuntimeHooks(
  executionProfile: ExecutionProfile | null | undefined,
  context: RuntimeHookContext,
  factories: readonly RuntimeHookFactory[] = [],
): RuntimeHooks {
  const hooks: RuntimeHooks = { resolver_adapters: [] };
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
