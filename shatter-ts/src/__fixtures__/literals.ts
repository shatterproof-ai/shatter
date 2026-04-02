// Fixture for literal extraction tests.

// File-level constants
const MAX_RETRIES = 3;
const THRESHOLD = 0.75;
const PREFIX = "v1";

// Enum declaration
enum Color {
  Red = "red",
  Green = "green",
  Blue = "blue",
}

export function classifyPriority(priority: string): number {
  if (priority === "express") return 3;
  if (priority === "economy") return 1;
  if (priority === "standard") return 2;
  return 0;
}

export function gradeScore(score: number): string {
  switch (score) {
    case 90: return "A";
    case 70: return "B";
    case 50: return "C";
    default: return "F";
  }
}

export function validateZip(s: string): boolean {
  return /^\d{5}$/.test(s);
}

export function greetWithDefault(name: string = "World"): string {
  return `Hello ${name}`;
}

export function noLiterals(x: number): number {
  return x + x;
}

export const classifyArrow = (s: string): string => {
  if (s === "admin") return "privileged";
  return "normal";
};

export function withDuplicates(s: string): boolean {
  return s === "ok" || s === "ok" || s === "ok";
}

export function checkStatus(obj: { status: string }): boolean {
  return obj.status === "active";
}

export function lookupBracket(m: Record<string, number>): number {
  return (m["priority"] ?? 0) + (m["weight"] ?? 0);
}

export function goDirection(dir: "north" | "south" | "east"): string {
  if (dir === "north") return "up";
  return "other";
}

export function useFileConsts(x: number): number {
  return x * MAX_RETRIES;
}

export function clampToFinite(x: 1e308 | 42 | 3.14): number {
  if (x > 100) return 100;
  return x;
}
