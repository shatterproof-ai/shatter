/**
 * TSX fixture — tests that analyzer, instrumentor, and executor handle .tsx files.
 *
 * greetingLabel: 2 branches (name truthy → personalized, falsy → default).
 * statusBadge: 3 branches via ternary chain (active → "green", pending → "yellow", default → "gray").
 *
 * No actual JSX elements here — functions returning JSX require react/jsx-runtime
 * at execution time, so JSX-element tests use inline source in instrumentor tests only.
 */

export function greetingLabel(name: string): string {
  if (name) {
    return `<span>Hello, ${name}!</span>`;
  }
  return "<span>Hello, stranger!</span>";
}

export function statusBadge(status: string): string {
  const color = status === "active"
    ? "green"
    : status === "pending"
      ? "yellow"
      : "gray";
  return `<span class="${color}">${status}</span>`;
}
