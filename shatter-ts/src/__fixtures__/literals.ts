// Fixture for literal extraction tests.

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
