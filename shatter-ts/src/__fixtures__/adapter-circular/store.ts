/**
 * Companion half of the str-aatcq import cycle — see component.ts.
 * Requires component.ts back at runtime (a genuine value dependency, not
 * type-only) so requiring either file transitively re-requires the other.
 *
 * Uses `require()` rather than a static `import` for the back-edge: an ES
 * import here would make ts-jest's whole-program type-check (used only to
 * run *this test suite*, not by the production loadModule/vm.transpileModule
 * path this fixture actually exercises) descend into component.ts's own
 * `import ... from "react"` and fail with "Cannot find module 'react'" —
 * this project intentionally has no real `react` dependency, since target
 * code runs against the bundled shim (`react-shim.ts`), not a real React
 * install. `require()` returns `any`, so ts-jest has nothing further to
 * resolve. Not a workaround for the bug under test: the runtime require
 * cycle this fixture reproduces is identical either way.
 */
const { useBookmarkButton } = require("./component.js") as {
  useBookmarkButton: (bookmarked: boolean) => unknown;
};

export const STORE_LABEL = "Bookmarked";

/** Referenced only to keep the require live (not tree-shaken). */
export function describeBookmarkButton(): string {
  return typeof useBookmarkButton;
}
