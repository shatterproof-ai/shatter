export function add(a: number, b: number): number {
  return a + b;
}

export function greet(name: string): string {
  return `Hello, ${name}!`;
}

export function isPositive(n: number): boolean {
  return n > 0;
}

export function identity(x: bigint): bigint {
  return x;
}
