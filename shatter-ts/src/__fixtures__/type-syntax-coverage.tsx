/**
 * TS/TSX type syntax coverage fixture (str-jeen.11).
 *
 * Each exported function exercises a TypeScript construct that Kapow runs
 * showed surviving transpile and crashing V8 with errors like
 * "Unexpected identifier 'ButtonProps'". The instrumentor + executor
 * pipeline must produce executable JS for every function below — exercised
 * by `executor.test.ts` -> "type syntax coverage".
 *
 * Contracts covered:
 * - type-only imports
 * - interface props aliases
 * - generic functions
 * - generic React components
 * - JSX in TSX (uses the React shim, no real React runtime required)
 * - `satisfies` operator
 * - type-only re-exports
 * - test-helper-style type-only imports (testing-library RenderOptions)
 */

import type { ReactNode } from "react";
import React from "react";

// type-only re-export: pipeline must transpile this file even when callers
// would also re-export types from third-party packages.
export type { ReactNode };

export interface ButtonProps {
  label: string;
  variant?: "primary" | "secondary";
  children?: ReactNode;
}

export type Variant = ButtonProps["variant"];

export function classifyButton(props: ButtonProps): string {
  if (props.label.length > 3) return "long";
  return "short";
}

export function pickGeneric<T>(items: T[], idx: number): T | undefined {
  if (idx >= 0 && idx < items.length) return items[idx];
  return undefined;
}

export function GenericList<T extends string>(props: {
  items: T[];
  render: (x: T) => string;
}): string {
  if (props.items.length === 0) return "empty";
  return props.items.map(props.render).join(",");
}

export function HelloTsx(props: { name: string }): unknown {
  if (props.name) return <div>Hi {props.name}</div>;
  return <div>stranger</div>;
}

type Cfg = { mode: "a" | "b"; n: number };
export function checkSatisfies(x: number): string {
  const cfg = { mode: "a", n: x } satisfies Cfg;
  if (cfg.n > 0) return "pos";
  return "neg";
}

// Realistic test-helper type — `RenderOptions` is intentionally only used as
// a type. We don't actually require @testing-library/react at runtime; the
// import is type-only so transpile drops it entirely.
import type { RenderOptions } from "@testing-library/react";

export function makeRenderOptions(n: number): RenderOptions | null {
  if (n > 0) return {} as RenderOptions;
  return null;
}
