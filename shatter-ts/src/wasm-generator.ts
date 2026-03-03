/**
 * WASM generator support via Extism.
 *
 * Loads Extism WASM plugins and calls exported functions to generate
 * values. Plugins are cached by file path to avoid reloading on
 * repeated generate requests for the same .wasm file.
 */

import { createPlugin } from "@extism/extism";
import type { Plugin } from "@extism/extism";

/** Result shape expected from a WASM generator function's JSON output. */
export interface WasmGeneratorResult {
  value: unknown;
  id: string;
  recipe?: unknown;
}

const pluginCache = new Map<string, Plugin>();

/**
 * Load an Extism WASM plugin, returning a cached instance if available.
 *
 * The plugin is created with WASI disabled since generators only need
 * to process JSON input and produce JSON output.
 */
export async function loadWasmPlugin(wasmPath: string): Promise<Plugin> {
  const cached = pluginCache.get(wasmPath);
  if (cached) return cached;

  const plugin = await createPlugin({ wasm: [{ path: wasmPath }] });
  pluginCache.set(wasmPath, plugin);
  return plugin;
}

/**
 * Call a named function in the WASM plugin and parse the JSON result.
 *
 * The plugin function receives an optional JSON-encoded recipe as input
 * and must return JSON with `{ value, id, recipe? }` shape.
 */
export async function runWasmGenerator(
  plugin: Plugin,
  funcName: string,
  recipe?: unknown,
): Promise<WasmGeneratorResult> {
  const input = recipe != null ? JSON.stringify(recipe) : "";
  const output = await plugin.call(funcName, input);

  if (output === null) {
    throw new Error(
      `WASM generator function "${funcName}" returned null output`,
    );
  }

  const parsed: unknown = JSON.parse(output.text());

  if (
    typeof parsed !== "object" ||
    parsed === null ||
    !("value" in parsed) ||
    !("id" in parsed)
  ) {
    throw new Error(
      `WASM generator function "${funcName}" returned invalid JSON: ` +
      `expected { value, id, recipe? } but got ${output.text().slice(0, 200)}`,
    );
  }

  const result = parsed as Record<string, unknown>;
  return {
    value: result["value"],
    id: String(result["id"]),
    recipe: result["recipe"],
  };
}

/**
 * Close all cached plugins and clear the cache.
 * Called during shutdown to release WASM resources.
 */
export async function clearWasmCache(): Promise<void> {
  const closingPlugins = [...pluginCache.values()].map((p) => p.close());
  pluginCache.clear();
  await Promise.all(closingPlugins);
}

/** Visible for testing: number of cached plugins. */
export function wasmCacheSize(): number {
  return pluginCache.size;
}
