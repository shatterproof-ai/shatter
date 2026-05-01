/**
 * TSX variant of private-helpers.ts. Discovery picks up the private
 * `formatGreeting` helper alongside the exported component.
 *
 * Regression for str-jeen.9 — Shatter previously failed to execute
 * private TSX helpers because the instrumented module surfaced only the
 * exported component on `module.exports`.
 *
 * No real JSX elements: rendering JSX requires `react/jsx-runtime` at
 * execution time, which is out of scope for this regression.
 */

// Private top-level helper. Two branches.
function formatGreeting(name: string): string {
  if (name.length > 0) {
    return `<span>Hello, ${name}!</span>`;
  }
  return "<span>Hello, stranger!</span>";
}

export function ComponentName(props: { label: string }): string {
  return `component:${props.label}`;
}

function _retainPrivateBindings(): unknown {
  return formatGreeting;
}
void _retainPrivateBindings;
