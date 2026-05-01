/**
 * Adapter fixture helper: TSX target inside a referenced UI project.
 *
 * The enclosing project sets `jsx: "react-jsx"` in its tsconfig.json.
 * After project-reference merging, the workspace-root analyzer should
 * inherit that JSX setting and parse this file successfully.
 */

export function buildGreeting(name: string): string {
  if (name.length === 0) {
    return "Hello, stranger!";
  }
  return `Hello, ${name}!`;
}
