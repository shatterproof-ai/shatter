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

  it("does not mark handle params when no registry entry matches", () => {
    const root = makeProject({ "handler.ts": HANDLER_TS });
    const functions = analyzeFile(path.join(root, "handler.ts"), "go", root);
    const stubParams = extractStubParams(functions);
    expect(stubParams).toEqual([]);
  });
});
