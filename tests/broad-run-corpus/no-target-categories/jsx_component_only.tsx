// NoTargetReason::JsxComponentOnly — TSX file containing only JSX component
// definitions.
declare const React: { createElement: (...args: unknown[]) => unknown };

export function Banner(props: { text: string }): unknown {
  return React.createElement("div", { className: "banner" }, props.text);
}

export function Spinner(): unknown {
  return React.createElement("span", { className: "spin" });
}
