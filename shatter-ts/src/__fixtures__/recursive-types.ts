/**
 * Fixture: recursive and deeply nested generic/conditional types.
 *
 * These trigger infinite recursion in convertType when there is no
 * depth limit or cycle detection. The analyzer must handle them
 * gracefully — returning {kind: "unknown"} for the recursive part
 * rather than blowing the stack.
 */

// Direct self-reference via interface
export interface TreeNode {
  value: number;
  left: TreeNode | null;
  right: TreeNode | null;
}

export function traverseTree(root: TreeNode): number {
  return root.value;
}

// Mutual recursion
export interface Odd {
  value: number;
  next: Even;
}
export interface Even {
  value: number;
  next: Odd;
}

export function processOdd(node: Odd): number {
  return node.value;
}

// Deeply nested generic type
export type DeepNested<T> = {
  data: T;
  children: DeepNested<T>[];
};

export function readDeep(tree: DeepNested<string>): string {
  return tree.data;
}

// Conditional type with recursive reference
export type JsonValue =
  | string
  | number
  | boolean
  | null
  | JsonValue[]
  | { [key: string]: JsonValue };

export function parseJson(input: JsonValue): string {
  return String(input);
}
