import * as ts from "typescript";
import fc from "fast-check";
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

  it("tags JSX components in .tsx files without a React import (automatic JSX runtime, str-cd4ur)", () => {
    // React 17+ automatic JSX runtime: components need no `react` import.
    // The .tsx extension plus JSX in the body is the decided recognition
    // predicate — do NOT consult tsconfig.
    const source = `
import { something } from "./other";
export function App() {
  return <div>{something}</div>;
}
`;
    const sf = createSourceFile(source, "App.tsx");
    const fns = [stubAnalysis({ name: "App", start_line: 3, end_line: 5 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.adapter.id).toBe(REACT_HOOK_ADAPTER_ID);
    expect(hints[0]!.reasons).toContain(
      "Returns JSX (PascalCase function component)",
    );
  });

  it("does not tag JSX components in non-.tsx/.jsx files without a React import (str-cd4ur)", () => {
    // Extension gate: a .ts file (no JSX runtime) with a JSX-shaped body and
    // no React import stays unrecognized — the extension predicate fails.
    const source = `
export function App() {
  return <div/>;
}
`;
    const sf = createSourceFile(source, "app.ts");
    const fns = [stubAnalysis({ name: "App", start_line: 2, end_line: 4 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// str-cd4ur: automatic JSX runtime, third-party hooks, arrow/HOC unwrapping
// ---------------------------------------------------------------------------

describe("recognizeReactHooks — automatic JSX runtime (str-cd4ur)", () => {
  it("recognizes a .jsx component with no react import", () => {
    const source = `
export function Banner(props) {
  return <section>{props.title}</section>;
}
`;
    const sf = createSourceFile(source, "Banner.jsx");
    const fns = [stubAnalysis({ name: "Banner", start_line: 2, end_line: 4 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.confidence).toBe("high");
  });

  it("does not recognize a PascalCase non-component util (no JSX, no hooks)", () => {
    // Negative: PascalCase alone must never trigger recognition.
    const source = `
export function FormatCurrency(cents: number) {
  return "$" + (cents / 100).toFixed(2);
}
`;
    const sf = createSourceFile(source, "money.tsx");
    const fns = [stubAnalysis({ name: "FormatCurrency", start_line: 2, end_line: 4 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeUndefined();
  });
});

describe("recognizeReactHooks — third-party hooks from any module (str-cd4ur)", () => {
  it("recognizes a component using only @mantine/hooks-style third-party hooks (no react import)", () => {
    // Mirrors kapow: no `react` import; a useXxx imported from a non-react
    // package is a hook-usage signal.
    const source = `
import { useDisclosure } from "@mantine/hooks";
import { Modal } from "@mantine/core";
export function Dialog() {
  const [opened, handlers] = useDisclosure(false);
  return <Modal opened={opened} onClose={handlers.close} />;
}
`;
    const sf = createSourceFile(source, "Dialog.tsx");
    const fns = [stubAnalysis({ name: "Dialog", start_line: 4, end_line: 7 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.adapter.id).toBe(REACT_HOOK_ADAPTER_ID);
    expect(hints[0]!.reasons).toContain("Calls custom hook useDisclosure");
    // A JSX component must render under the adapter even though it also calls
    // a store/third-party hook: high confidence so it is auto-applied (active),
    // not merely suggested. This is the dominant kapow pattern (str-cd4ur).
    expect(hints[0]!.confidence).toBe("high");
    expect(hints[0]!.reasons).toContain(
      "Returns JSX (PascalCase function component)",
    );
  });

  it("classifies a JSX component that also calls a store hook (no react import) as high confidence", () => {
    // Mirrors kapow's CardFieldPicker: `export function X()` in a .tsx file,
    // no react import, calls a `useXxxStore` selector, returns JSX. Must be
    // high-confidence so the react-hook adapter is auto-applied.
    const source = `
import { useCardFieldStore } from "@/stores/cardFieldStore";
export function CardFieldPicker() {
  const fields = useCardFieldStore((s) => s.visibleFields);
  return <div>{fields.length}</div>;
}
`;
    const sf = createSourceFile(source, "CardFieldPicker.tsx");
    const fns = [stubAnalysis({ name: "CardFieldPicker", start_line: 3, end_line: 6 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.confidence).toBe("high");
  });

  it("recognizes a custom hook (no JSX, no react import) wrapping a third-party hook", () => {
    // A .ts custom hook file that imports its hook from a store/third-party
    // module — no JSX, so the extension predicate cannot help; the imported
    // useXxx call is the signal.
    const source = `
import { useStore } from "@/stores/thing";
export function useThing() {
  const value = useStore((s) => s.value);
  return value * 2;
}
`;
    const sf = createSourceFile(source, "useThing.ts");
    const fns = [stubAnalysis({ name: "useThing", start_line: 3, end_line: 6 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.reasons).toContain("Calls custom hook useStore");
  });

  it("does not recognize a locally-defined useXxx call that is not imported (no react import)", () => {
    // Guard against over-broadening: a useXxx call whose callee is neither a
    // react hook nor imported from any module is not a recognition signal.
    const source = `
export function usePlain(x: number) {
  return useLocalThing(x);
  function useLocalThing(n: number) { return n + 1; }
}
`;
    const sf = createSourceFile(source, "usePlain.ts");
    const fns = [stubAnalysis({ name: "usePlain", start_line: 2, end_line: 5 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeUndefined();
  });
});

describe("recognizeReactHooks — arrow and HOC-wrapped components (str-cd4ur)", () => {
  it("recognizes a const-arrow component (no react import)", () => {
    const source = `
export const Panel = (props: { label: string }) => {
  return <div>{props.label}</div>;
};
`;
    const sf = createSourceFile(source, "Panel.tsx");
    // Arrow spans the initializer: `(props...) => { ... }` on lines 2-4.
    const fns = [stubAnalysis({ name: "Panel", start_line: 2, end_line: 4 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.reasons).toContain(
      "Returns JSX (PascalCase function component)",
    );
  });

  it("recognizes a memo-wrapped component by unwrapping the call expression", () => {
    const source = `
import { memo } from "react";
export const Card = memo(function Card(props: { n: number }) {
  return <article>{props.n}</article>;
});
`;
    const sf = createSourceFile(source, "Card.tsx");
    // Analysis line range points at the whole `memo(...)` call declaration.
    const fns = [stubAnalysis({ name: "Card", start_line: 3, end_line: 5 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.reasons).toContain(
      "Returns JSX (PascalCase function component)",
    );
  });

  it("recognizes a forwardRef-wrapped arrow component by unwrapping", () => {
    const source = `
import { forwardRef } from "react";
export const Input = forwardRef((props: { name: string }, ref) => {
  return <input name={props.name} ref={ref} />;
});
`;
    const sf = createSourceFile(source, "Input.tsx");
    const fns = [stubAnalysis({ name: "Input", start_line: 3, end_line: 5 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeDefined();
  });
});

describe("recognizeReactHooks — recognition invariants (str-cd4ur property)", () => {
  it("recognizes any PascalCase JSX component and never a lowercase one, in a .tsx file with no react import", () => {
    const pascal = fc
      .tuple(
        fc.constantFrom(..."ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
        fc.stringMatching(/^[a-zA-Z]{0,10}$/),
      )
      .map(([head, rest]) => head + rest);
    const lower = fc
      .tuple(
        fc.constantFrom(..."abcdefghijklmnopqrstuvwxyz"),
        fc.stringMatching(/^[a-zA-Z]{0,10}$/),
      )
      .map(([head, rest]) => head + rest)
      // exclude useXxx names — those are hook-shaped, a separate signal
      .filter((n) => !isHookName(n));

    fc.assert(
      fc.property(pascal, lower, (Comp, helper) => {
        const source = `
export function ${Comp}() {
  return <div>x</div>;
}
export function ${helper}() {
  return <div>y</div>;
}
`;
        const sf = createSourceFile(source, "gen.tsx");
        const fns = [
          stubAnalysis({ name: Comp, start_line: 2, end_line: 4 }),
          stubAnalysis({ name: helper, start_line: 5, end_line: 7 }),
        ];
        const hints = recognizeReactHooks(sf, fns);
        expect(hints[0]).toBeDefined();
        expect(hints[1]).toBeUndefined();
      }),
      { numRuns: 100 },
    );
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

  it("tags a custom hook called via useXxx pattern imported from any module (medium confidence, str-cd4ur)", () => {
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

    // useTheme is imported (from any module, here "./useTheme") and matches the
    // useXxx naming convention, so it is a hook-usage signal even without a
    // React import (str-cd4ur, change #2). Medium confidence: no builtin call.
    expect(hints[0]).toBeDefined();
    expect(hints[0]!.confidence).toBe("medium");
    expect(hints[0]!.reasons).toContain("Calls custom hook useTheme");
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

describe("recognizeReactHooks — namespace imports (str-zgsk)", () => {
  it("detects React.useState via `import * as React from \"react\"`", () => {
    const source = `
import * as React from "react";
export function useCounter(initial: number) {
  const [count, setCount] = React.useState(initial);
  return count;
}
`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "useCounter", start_line: 3, end_line: 6 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.adapter.id).toBe(REACT_HOOK_ADAPTER_ID);
    expect(hints[0]!.confidence).toBe("high");
    expect(hints[0]!.reasons).toContain("Calls useState imported from 'react'");
  });

  it("detects React.useContext-only custom hooks via namespace import", () => {
    // Reproduces pickpackit's `useTheme = () => React.useContext(Ctx)` pattern.
    // Before namespace-import handling this returned undefined and the hook
    // executed raw, producing "Invalid hook call" warnings.
    const source = `
import * as React from "react";
const Ctx = React.createContext({ dark: false });
export function useTheme() {
  return React.useContext(Ctx);
}
`;
    const sf = createSourceFile(source);
    const fns = [stubAnalysis({ name: "useTheme", start_line: 4, end_line: 6 })];
    const hints = recognizeReactHooks(sf, fns);

    expect(hints[0]).toBeDefined();
    expect(hints[0]!.reasons).toContain("Calls useContext imported from 'react'");
  });
});

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
