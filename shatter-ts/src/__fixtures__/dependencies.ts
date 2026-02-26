import { helperAdd, helperFormat } from "./deps-helper";

export function usesExternal(a: number, b: number): string {
  const sum = helperAdd(a, b);
  return helperFormat(sum);
}

export function usesExternalMultipleTimes(a: number, b: number, c: number): number {
  const ab = helperAdd(a, b);
  const abc = helperAdd(ab, c);
  return abc;
}

export function noExternalDeps(a: number, b: number): number {
  return a + b;
}
