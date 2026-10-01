export function spin(n: number): number {
  if (n > 3) {
    while (true) { n = n + 1; }
  }
  return n;
}
export function ok(a: number): string {
  if (a > 1) { return "big"; }
  return "small";
}
