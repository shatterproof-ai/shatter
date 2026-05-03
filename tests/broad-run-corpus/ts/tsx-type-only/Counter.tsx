// TSX fixture exercising the JSX transform path and a callable export
// alongside it (str-jeen.29). The analyzer should target `nextCount` while
// `Counter` (a JSX component-only export) is classified jsx_component_only.
import type { ReactNode } from "./types";

export function nextCount(current: number, increment: number): number {
  if (increment <= 0) {
    return current;
  }
  if (current + increment > 100) {
    return 100;
  }
  return current + increment;
}

export function Counter(props: { value: number }): ReactNode {
  return { tag: "span", text: String(props.value) } as unknown as ReactNode;
}
