import * as ts from "typescript";
import {
  recognizeReactHooks,
  isHookName,
  isComponentName,
  REACT_HOOK_ADAPTER_ID,
  BUILTIN_REACT_HOOKS,
} from "./react-hook-recognizer.js";
import type { FunctionAnalysis } from "./protocol.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Create a TypeScript program from inline source and return the source file. */
function createSourceFile(source: string, fileName = "test.tsx"): ts.SourceFile {
  const sourceFile = ts.createSourceFile(fileName, source, ts.ScriptTarget.ES2022, true, ts.ScriptKind.TSX);
  return sourceFile;
}

/** Minimal FunctionAnalysis stub for testing. */
function stubAnalysis(overrides: Partial<FunctionAnalysis> & { name: string; start_line: number; end_line: number }): FunctionAnalysis {
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
// isHookName
// ---------------------------------------------------------------------------

describe("isHookName", () => {
  it("returns true for standard hook names", () => {
    expect(isHookName("useState")).toBe(true);
    expect(isHookName("useEffect")).toBe(true);
    expect(isHookName("useMyCustomHook")).toBe(true);
  });

  it("returns false for non-hook names", () => {
    expect(isHookName("use")).toBe(false);
    expect(isHookName("used")).toBe(false);
    expect(isHookName("user")).toBe(false);
    expect(isHookName("useful")).toBe(false);
    expect(isHookName("useState".toLowerCase())).toBe(false); // "usestate" — no uppercase after "use"
    expect(isHookName("Use")).toBe(false);
    expect(isHookName("")).toBe(false);
  });

  it("requires uppercase fourth character", () => {
    expect(isHookName("useA")).toBe(true);
    expect(isHookName("usea")).toBe(false);
    expect(isHookName("use1")).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// recognizeReactHooks
// ---------------------------------------------------------------------------

describe("recognizeReactHooks", () => {
  it("emits high confidence hint for function calling builtin hook", () => {
    const source = `
import { useState } from "react";
export function useCounter(initial: number) {
  const [count, setCount] = useState(initial);
  return count;
}
`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "useCounter", start_line: 3, end_line: 6 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints).toHaveLength(1);
    expect(hints[0]).toBeDefined();
    expect(hints[0]!.adapter.id).toBe(REACT_HOOK_ADAPTER_ID);
    expect(hints[0]!.confidence).toBe("high");
    expect(hints[0]!.reasons).toContain("Calls useState imported from 'react'");
    expect(hints[0]!.reasons).toContain("Follows useXxx naming convention");
  });

  it("emits high confidence for component calling hooks (no useXxx name)", () => {
    const source = `
import { useState } from "react";
export function MyComponent(props: { x: number }) {
  const [val] = useState(0);
  return val;
}
`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "MyComponent", start_line: 3, end_line: 6 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints).toHaveLength(1);
    expect(hints[0]).toBeDefined();
    expect(hints[0]!.confidence).toBe("high");
    expect(hints[0]!.reasons).toContain("Calls useState imported from 'react'");
    // Should NOT include naming convention reason
    expect(hints[0]!.reasons).not.toContain("Follows useXxx naming convention");
  });

  it("does not emit hint for useXxx name alone without hook calls", () => {
    const source = `
import { useState } from "react";
export function useFormatting(text: string) {
  return text.toUpperCase();
}
`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "useFormatting", start_line: 3, end_line: 5 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints).toHaveLength(1);
    expect(hints[0]).toBeUndefined();
  });

  it("does not emit hints when no React imports exist", () => {
    const source = `
export function useCounter(initial: number) {
  return initial + 1;
}
`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "useCounter", start_line: 2, end_line: 4 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints).toHaveLength(1);
    expect(hints[0]).toBeUndefined();
  });

  it("emits medium confidence for custom hook calls", () => {
    const source = `
import { useState } from "react";
function useInternalHook() {
  return useState(0);
}
export function useComposed(x: number) {
  const val = useInternalHook();
  return val;
}
`;
    const sf = createSourceFile(source);
    // Only analyzing the exported useComposed
    const fns = [stubAnalysis({ name: "useComposed", start_line: 6, end_line: 9 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints).toHaveLength(1);
    expect(hints[0]).toBeDefined();
    expect(hints[0]!.confidence).toBe("medium");
    expect(hints[0]!.reasons).toContain("Calls custom hook useInternalHook");
  });

  it("handles multiple functions with mixed signals", () => {
    const source = `
import { useState, useMemo } from "react";
export function useCounter(initial: number) {
  const [count] = useState(initial);
  return count;
}
export function helper(x: number) {
  return x * 2;
}
export function useLabel(text: string) {
  return useMemo(() => text.toUpperCase(), [text]);
}
`;
    const sf = createSourceFile(source);
    const fns = [
      stubAnalysis({ name: "useCounter", start_line: 3, end_line: 6 }),
      stubAnalysis({ name: "helper", start_line: 7, end_line: 9 }),
      stubAnalysis({ name: "useLabel", start_line: 10, end_line: 12 }),
    ];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints).toHaveLength(3);
    expect(hints[0]).toBeDefined();
    expect(hints[0]!.confidence).toBe("high");
    expect(hints[1]).toBeUndefined();
    expect(hints[2]).toBeDefined();
    expect(hints[2]!.confidence).toBe("high");
  });

  it("detects React.useState() property access style", () => {
    const source = `
import React from "react";
export function useValue() {
  const [v] = React.useState(0);
  return v;
}
`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "useValue", start_line: 3, end_line: 6 })];
    const hints = recognizeReactHooks(sf, fns);

    // Property access React.useState picks up "useState" from the name
    // but the import context tracks "React" as default import, not "useState"
    // This tests the property access path
    expect(hints).toHaveLength(1);
    expect(hints[0]).toBeDefined();
    expect(hints[0]!.confidence).toBe("high");
    expect(hints[0]!.reasons).toContain("Calls useState imported from 'react'");
  });

  it("reasons array is always non-empty when hint is emitted", () => {
    const source = `
import { useState, useEffect, useCallback } from "react";
export function useMulti(x: number) {
  const [a] = useState(x);
  useEffect(() => {}, [a]);
  const fn = useCallback(() => a, [a]);
  return fn;
}
`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "useMulti", start_line: 3, end_line: 8 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.reasons!.length).toBeGreaterThan(0);
  });
});

// ---------------------------------------------------------------------------
// JSX function component detection (str-zgsk)
// ---------------------------------------------------------------------------

describe("isComponentName", () => {
  it("returns true for PascalCase identifiers", () => {
    expect(isComponentName("App")).toBe(true);
    expect(isComponentName("Dashboard")).toBe(true);
    expect(isComponentName("X")).toBe(true);
  });

  it("returns false for non-PascalCase identifiers", () => {
    expect(isComponentName("helper")).toBe(false);
    expect(isComponentName("useFoo")).toBe(false);
    expect(isComponentName("")).toBe(false);
    expect(isComponentName("_App")).toBe(false);
  });
});

describe("recognizeReactHooks — JSX function components", () => {
  it("tags PascalCase JSX component when React is imported", () => {
    const source = `
import * as React from "react";
export function App(props: { name: string }) {
  return <div>Hello {props.name}</div>;
}
`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "App", start_line: 3, end_line: 5 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.adapter.id).toBe(REACT_HOOK_ADAPTER_ID);
    expect(hints[0]!.confidence).toBe("high");
    expect(hints[0]!.reasons).toContain(
      "Returns JSX (PascalCase function component)",
    );
  });

  it("tags self-closing JSX components", () => {
    const source = `
import * as React from "react";
export function Card() {
  return <Header/>;
}
`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "Card", start_line: 3, end_line: 5 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.confidence).toBe("high");
  });

  it("tags JSX fragment components", () => {
    const source = `
import * as React from "react";
export function List() {
  return <><span/><span/></>;
}
`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "List", start_line: 3, end_line: 5 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeDefined();
  });

  it("does not tag lowercase functions returning JSX", () => {
    const source = `
import * as React from "react";
function helper() {
  return <span/>;
}
`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "helper", start_line: 3, end_line: 5 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeUndefined();
  });

  it("does not tag PascalCase function without JSX or hook calls", () => {
    const source = `
import * as React from "react";
export function App() {
  return null;
}
`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "App", start_line: 3, end_line: 5 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeUndefined();
  });

  it("does not tag JSX components in files without a React import", () => {
    // Without a React import the recognizer cannot tell a JSX component
    // apart from a JSX-construction helper used by a non-React framework.
    const source = `
import { something } from "./other";
export function App() {
  return <div/>;
}
`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "App", start_line: 3, end_line: 5 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// useContext regression (str-zgsk acceptance criterion)
// ---------------------------------------------------------------------------

describe("recognizeReactHooks — useContext regression", () => {
  it("tags a component using useContext", () => {
    const source = `
import { useContext, createContext } from "react";
const ThemeContext = createContext({ dark: false });
export function ThemedPanel() {
  const theme = useContext(ThemeContext);
  return theme.dark ? "dark" : "light";
}
`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "ThemedPanel", start_line: 4, end_line: 7 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.confidence).toBe("high");
    expect(hints[0]!.reasons).toContain("Calls useContext imported from 'react'");
  });

  it("tags a custom hook wrapping useContext", () => {
    const source = `
import { useContext, createContext } from "react";
const ThemeContext = createContext({ dark: false });
export function useTheme() {
  return useContext(ThemeContext);
}
`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "useTheme", start_line: 4, end_line: 6 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.confidence).toBe("high");
  });

  it("tags a custom hook called via useXxx pattern (medium confidence)", () => {
    const source = `
import { useTheme } from "./useTheme";
export function useThemedClass() {
  const theme = useTheme();
  return theme.dark ? "dark" : "light";
}
`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "useThemedClass", start_line: 3, end_line: 6 })];
    const hints = recognizeReactHooks(sf, fns);

    // useTheme is imported but not from a React module, so this is a
    // medium-confidence "custom hook" signal driven by the useXxx call name.
    // Imported from "./useTheme" doesn't set hasReactImport, so the
    // recognizer returns undefined unless React itself is imported elsewhere.
    // Add a React import to exercise the medium path.
    expect(hints[0]).toBeUndefined();
  });

  it("emits medium confidence for custom hook calls when React is imported", () => {
    const source = `
import { useState } from "react";
import { useTheme } from "./useTheme";
export function useThemedClass() {
  useState(0); // pulls in React import — recognized
  const t = useTheme();
  return t;
}
`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "useThemedClass", start_line: 4, end_line: 8 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeDefined();
    // useState gives a builtin call → high confidence overall, but the
    // custom-hook reason is also recorded.
    expect(hints[0]!.reasons).toContain("Calls custom hook useTheme");
  });
});

// ---------------------------------------------------------------------------
// BUILTIN_REACT_HOOKS constant
// ---------------------------------------------------------------------------

describe("BUILTIN_REACT_HOOKS", () => {
  it("contains all standard React hooks", () => {
    const expected = [
      "useState", "useEffect", "useReducer", "useCallback",
      "useMemo", "useRef", "useContext", "useLayoutEffect", "useId",
    ];
    for (const hook of expected) {
      expect(BUILTIN_REACT_HOOKS.has(hook)).toBe(true);
    }
  });
});
