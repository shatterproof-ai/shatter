import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { analyzeFile, extractStubParams } from "./analyzer.js";
import { clearStubRegistryCacheForTests } from "./opaque-stub-registry.js";

/**
 * End-to-end (within the analyzer) coverage of the opaque-param stub pipeline
 * (str-syj9b): a handle param is recognised, a binding is recorded, and the
 * emitted TypeInfo is scrubbed to a plain empty object so the core no longer
 * skips the function. Uses a project-local `Page` type registered via
 * `.shatter/config.yaml` `ts_runtime_values`, which also exercises the
 * config-extensibility acceptance without needing the real `@playwright/test`.
 */
function makeProject(files: Record<string, string>): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "shatter-stub-"));
  for (const [rel, contents] of Object.entries(files)) {
    const full = path.join(root, rel);
    fs.mkdirSync(path.dirname(full), { recursive: true });
    fs.writeFileSync(full, contents);
  }
  return root;
}

const HANDLER_TS = `
interface Locator {
  count(): Promise<number>;
  isVisible(): Promise<boolean>;
}
interface Page {
  locator(selector: string): Locator;
  goto(url: string): Promise<void>;
}
export async function go(page: Page): Promise<string> {
  await page.goto("/");
  if ((await page.locator("#target").count()) > 0) {
    return "present";
  }
  return "absent";
}
`;

const CONFIG_YAML = `
ts_runtime_values:
  Page:
    stub: proxy
    overrides:
      "locator.count": [0, 2]
`;

describe("analyzer opaque-param stub detection (str-syj9b)", () => {
  beforeEach(() => {
    clearStubRegistryCacheForTests();
  });

  it("records a stub binding and scrubs the sentinel for a config-registered handle", () => {
    const root = makeProject({ "handler.ts": HANDLER_TS, ".shatter/config.yaml": CONFIG_YAML });
    const functions = analyzeFile(path.join(root, "handler.ts"), "go", root);
    const stubParams = extractStubParams(functions);

    expect(stubParams).toEqual([{ functionName: "go", paramIndex: 0, stubKey: "Page" }]);

    const go = functions.find((f) => f.name === "go");
    expect(go).toBeDefined();
    // The param that was a handle is now a plain empty object — the core will
    // synthesize `{}` and explore the function instead of skipping it.
    expect(go!.params[0]!.type).toEqual({ kind: "object", fields: [] });
    // No stub sentinel label leaks onto the wire.
    expect(JSON.stringify(go)).not.toContain("shatter-stub:");
  });

  it("binds a handle wrapped in a nullable/optional param (Page | undefined)", () => {
    const src = `
interface Locator { count(): Promise<number>; }
interface Page { locator(selector: string): Locator; }
export async function go(page?: Page): Promise<number> {
  if (!page) return -1;
  return page.locator("#x").count();
}
`;
    const root = makeProject({ "handler.ts": src, ".shatter/config.yaml": CONFIG_YAML });
    const functions = analyzeFile(path.join(root, "handler.ts"), "go", root);
    const stubParams = extractStubParams(functions);
    expect(stubParams).toEqual([{ functionName: "go", paramIndex: 0, stubKey: "Page" }]);
    // Sentinel scrubbed everywhere — nothing handle-shaped leaks to the wire.
    expect(JSON.stringify(functions.find((f) => f.name === "go"))).not.toContain("shatter-stub:");
  });

  it("binds a handle appearing as a union variant (Page | Widget)", () => {
    const src = `
interface Locator { count(): Promise<number>; }
interface Page { locator(selector: string): Locator; }
interface Widget { label: string; }
export function go(x: Page | Widget): string {
  return "id" in x ? "widget" : "page";
}
`;
    const root = makeProject({ "handler.ts": src, ".shatter/config.yaml": CONFIG_YAML });
    const functions = analyzeFile(path.join(root, "handler.ts"), "go", root);
    const stubParams = extractStubParams(functions);
    expect(stubParams).toEqual([{ functionName: "go", paramIndex: 0, stubKey: "Page" }]);
  });

  it("scrubs but does NOT bind a handle nested in an array (documented fallback)", () => {
    // The whole-argument overlay cannot substitute one stub for an array element,
    // so Locator[] gets the safe empty-object fallback and no binding. Tracked as
    // a follow-up for element/field-level stubbing.
    const src = `
interface Locator { count(): Promise<number>; }
export function go(locators: Locator[]): number {
  return locators.length;
}
`;
    const config = `
ts_runtime_values:
  Locator:
    overrides:
      count: [0, 2]
`;
    const root = makeProject({ "handler.ts": src, ".shatter/config.yaml": config });
    const functions = analyzeFile(path.join(root, "handler.ts"), "go", root);
    const stubParams = extractStubParams(functions);
    expect(stubParams).toEqual([]);
    const go = functions.find((f) => f.name === "go");
    // The array survives; its element is scrubbed to a plain empty object, and no
    // stub sentinel leaks to the wire.
    expect(go!.params[0]!.type).toEqual({
      kind: "array",
      element: { kind: "object", fields: [] },
    });
    expect(JSON.stringify(go)).not.toContain("shatter-stub:");
  });

  it("does not mark handle params when no registry entry matches", () => {
    const root = makeProject({ "handler.ts": HANDLER_TS });
    const functions = analyzeFile(path.join(root, "handler.ts"), "go", root);
    const stubParams = extractStubParams(functions);
    expect(stubParams).toEqual([]);
  });
});
