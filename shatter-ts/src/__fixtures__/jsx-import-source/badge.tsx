/**
 * Custom-jsxImportSource fixture — exercises `jsxImportSource: "preact"`
 * declared in the sibling `tsconfig.json`. The automatic JSX transform
 * emits `require("preact/jsx-runtime")`; the executor's resolver adapter
 * routes that to the bundled React shim so this file can execute without
 * a real Preact runtime installed.
 *
 * Two branches: severity controls className and label text.
 */

export function Badge(props: { severity: string; count: number }) {
  if (props.severity === "high" && props.count > 0) {
    return <span className="badge-high">High: {props.count}</span>;
  }
  return <span className="badge-low">Low</span>;
}
