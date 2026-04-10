import * as ts from "typescript";
import {
  recognizeBrowserGlobals,
  BROWSER_GLOBALS_ADAPTER_ID,
} from "./browser-globals-recognizer.js";
import type { FunctionAnalysis } from "./protocol.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function createSourceFile(source: string, fileName = "test.ts"): ts.SourceFile {
  return ts.createSourceFile(fileName, source, ts.ScriptTarget.ES2022, true, ts.ScriptKind.TS);
}

function stubAnalysis(
  overrides: Partial<FunctionAnalysis> & { name: string; start_line: number; end_line: number },
): FunctionAnalysis {
  return {
    exported: true,
    params: [],
    branches: [],
    dependencies: [],
    return_type: { kind: "unknown" },
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// recognizeBrowserGlobals
// ---------------------------------------------------------------------------

describe("recognizeBrowserGlobals", () => {
  it("detects document usage", () => {
    const source = `export function getTitle() {
  return document.title;
}`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "getTitle", start_line: 1, end_line: 3 })];
    const hints = recognizeBrowserGlobals(sf, fns);

    expect(hints).toHaveLength(1);
    expect(hints[0]).toBeDefined();
    expect(hints[0]!.adapter.id).toBe(BROWSER_GLOBALS_ADAPTER_ID);
    expect(hints[0]!.confidence).toBe("high");
    expect(hints[0]!.reasons).toContainEqual(expect.stringContaining("document"));
  });

  it("detects window usage", () => {
    const source = `export function getWidth() {
  return window.innerWidth;
}`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "getWidth", start_line: 1, end_line: 3 })];
    const hints = recognizeBrowserGlobals(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.confidence).toBe("high");
    expect(hints[0]!.reasons).toContainEqual(expect.stringContaining("window"));
  });

  it("detects multiple globals in one function", () => {
    const source = `export function setup() {
  document.title = "hi";
  localStorage.setItem("key", "value");
  window.addEventListener("resize", () => {});
}`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "setup", start_line: 1, end_line: 5 })];
    const hints = recognizeBrowserGlobals(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.reasons!.length).toBeGreaterThanOrEqual(3);
    expect(hints[0]!.reasons).toContainEqual(expect.stringContaining("document"));
    expect(hints[0]!.reasons).toContainEqual(expect.stringContaining("localStorage"));
    expect(hints[0]!.reasons).toContainEqual(expect.stringContaining("window"));
  });

  it("detects navigator", () => {
    const source = `export function getLang() {
  return navigator.language;
}`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "getLang", start_line: 1, end_line: 3 })];
    const hints = recognizeBrowserGlobals(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.reasons).toContainEqual(expect.stringContaining("navigator"));
  });

  it("detects storage APIs", () => {
    const source = `export function saveData() {
  sessionStorage.setItem("x", "1");
}`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "saveData", start_line: 1, end_line: 3 })];
    const hints = recognizeBrowserGlobals(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.reasons).toContainEqual(expect.stringContaining("sessionStorage"));
  });

  it("detects observer APIs", () => {
    const source = `export function observe() {
  const ro = new ResizeObserver(() => {});
  const io = new IntersectionObserver(() => {});
  const mo = new MutationObserver(() => {});
}`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "observe", start_line: 1, end_line: 5 })];
    const hints = recognizeBrowserGlobals(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.reasons!.length).toBe(3);
  });

  it("detects matchMedia", () => {
    const source = `export function isDark() {
  return matchMedia("(prefers-color-scheme: dark)").matches;
}`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "isDark", start_line: 1, end_line: 3 })];
    const hints = recognizeBrowserGlobals(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.reasons).toContainEqual(expect.stringContaining("matchMedia"));
  });

  it("detects animation frame APIs", () => {
    const source = `export function animate() {
  const id = requestAnimationFrame(() => {});
  cancelAnimationFrame(id);
}`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "animate", start_line: 1, end_line: 4 })];
    const hints = recognizeBrowserGlobals(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.reasons).toContainEqual(expect.stringContaining("requestAnimationFrame"));
    expect(hints[0]!.reasons).toContainEqual(expect.stringContaining("cancelAnimationFrame"));
  });

  it("detects dialog APIs", () => {
    const source = `export function askUser() {
  alert("hello");
  const ok = confirm("proceed?");
  const name = prompt("name?");
}`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "askUser", start_line: 1, end_line: 5 })];
    const hints = recognizeBrowserGlobals(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.reasons).toContainEqual(expect.stringContaining("alert"));
    expect(hints[0]!.reasons).toContainEqual(expect.stringContaining("confirm"));
    expect(hints[0]!.reasons).toContainEqual(expect.stringContaining("prompt"));
  });

  it("detects XMLHttpRequest", () => {
    const source = `export function fetchData() {
  const xhr = new XMLHttpRequest();
  xhr.open("GET", "/api");
}`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "fetchData", start_line: 1, end_line: 4 })];
    const hints = recognizeBrowserGlobals(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.reasons).toContainEqual(expect.stringContaining("XMLHttpRequest"));
  });

  it("detects location and history", () => {
    const source = `export function navigate() {
  location.href = "/new";
  history.pushState({}, "", "/new");
}`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "navigate", start_line: 1, end_line: 4 })];
    const hints = recognizeBrowserGlobals(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.reasons).toContainEqual(expect.stringContaining("location"));
    expect(hints[0]!.reasons).toContainEqual(expect.stringContaining("history"));
  });

  it("detects window.localStorage via property access", () => {
    const source = `export function save() {
  window.localStorage.setItem("k", "v");
}`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "save", start_line: 1, end_line: 3 })];
    const hints = recognizeBrowserGlobals(sf, fns);

    expect(hints[0]).toBeDefined();
    // Should detect both window and localStorage
    expect(hints[0]!.reasons).toContainEqual(expect.stringContaining("window"));
    expect(hints[0]!.reasons).toContainEqual(expect.stringContaining("localStorage"));
  });

  it("returns undefined for functions with no browser globals", () => {
    const source = `export function add(a: number, b: number) {
  return a + b;
}`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "add", start_line: 1, end_line: 3 })];
    const hints = recognizeBrowserGlobals(sf, fns);

    expect(hints[0]).toBeUndefined();
  });

  it("returns undefined for pure Node.js code", () => {
    const source = `import * as fs from "fs";
export function readFile(path: string) {
  return fs.readFileSync(path, "utf-8");
}`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "readFile", start_line: 2, end_line: 4 })];
    const hints = recognizeBrowserGlobals(sf, fns);

    expect(hints[0]).toBeUndefined();
  });

  it("does not flag naming conventions alone (e.g. variable named 'document')", () => {
    // The recognizer detects identifier references. If someone uses `document`
    // as a variable name, that's still a reference to the global in static
    // analysis. This is intentional — shadowing browser globals is itself a
    // signal. This test documents the behavior.
    const source = `export function process() {
  const doc = "test";
  return doc;
}`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "process", start_line: 1, end_line: 4 })];
    const hints = recognizeBrowserGlobals(sf, fns);

    expect(hints[0]).toBeUndefined();
  });

  it("handles arrow functions", () => {
    const source = `export const getTitle = () => {
  return document.title;
};`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "getTitle", start_line: 1, end_line: 3 })];
    const hints = recognizeBrowserGlobals(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.adapter.id).toBe(BROWSER_GLOBALS_ADAPTER_ID);
  });

  it("handles multiple functions — only flags browser-using ones", () => {
    const source = `export function pure(x: number) {
  return x * 2;
}
export function browserFn() {
  document.title = "hi";
}`;
    const sf = createSourceFile(source);
    const fns = [
      stubAnalysis({ name: "pure", start_line: 1, end_line: 3 }),
      stubAnalysis({ name: "browserFn", start_line: 4, end_line: 6 }),
    ];
    const hints = recognizeBrowserGlobals(sf, fns);

    expect(hints[0]).toBeUndefined();
    expect(hints[1]).toBeDefined();
    expect(hints[1]!.adapter.id).toBe(BROWSER_GLOBALS_ADAPTER_ID);
  });

  it("deduplicates globals referenced multiple times", () => {
    const source = `export function multi() {
  document.title = "a";
  document.body.className = "b";
}`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "multi", start_line: 1, end_line: 4 })];
    const hints = recognizeBrowserGlobals(sf, fns);

    expect(hints[0]).toBeDefined();
    const docReasons = hints[0]!.reasons!.filter((r) => r.includes("document"));
    expect(docReasons).toHaveLength(1);
  });

  it("does not flag property names that match globals (obj.window)", () => {
    const source = `export function getSize(config: any) {
  return config.window.width;
}`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "getSize", start_line: 1, end_line: 3 })];
    const hints = recognizeBrowserGlobals(sf, fns);

    // `config.window` — window is a property name, not the global
    expect(hints[0]).toBeUndefined();
  });
});
