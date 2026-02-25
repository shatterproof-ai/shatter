export function sum(nums: number[]): number {
  return nums.reduce((a, b) => a + b, 0);
}

export function flatten(nested: string[][]): string[] {
  return nested.flat();
}
