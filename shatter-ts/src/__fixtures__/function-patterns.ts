// FunctionExpression in variable declaration (not ArrowFunction)
export const square = function(x: number): number { return x * x; };

// Named default export function
export default function defaultGreet(name: string): string {
  return `Hello ${name}`;
}
