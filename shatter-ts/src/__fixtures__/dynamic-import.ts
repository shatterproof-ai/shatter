/**
 * Test fixture: module using dynamic import() expressions.
 * Exercises the ESM import race fix (str-4hay).
 *
 * When transpiled with module: CommonJS, TypeScript preserves import()
 * expressions. Without the fix, these can race against require() calls
 * for the same module, causing "Cannot require() ES Module ... not yet
 * fully loaded" crashes.
 */

export async function loadPath(): Promise<string> {
  const pathMod = await import("node:path");
  return pathMod.join("/tmp", "test");
}

export async function loadMultiple(): Promise<string[]> {
  const [pathMod, fsMod] = await Promise.all([
    import("node:path"),
    import("node:fs"),
  ]);
  return [pathMod.sep, typeof fsMod.readFileSync];
}

export function syncAdd(a: number, b: number): number {
  return a + b;
}
