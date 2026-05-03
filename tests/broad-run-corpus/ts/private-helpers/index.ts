// Public function classifyAge re-uses an unexported helper roundDown.
// roundDown is a private module-internal helper; the analyzer should not list it
// as an independently scheduled target (str-jeen.9 territory).
function roundDown(years: number): number {
  if (years < 0) {
    return 0;
  }
  return Math.floor(years / 10) * 10;
}

export function classifyAge(years: number): string {
  const decade = roundDown(years);
  if (decade >= 60) {
    return "senior";
  }
  if (decade >= 18) {
    return "adult";
  }
  return "minor";
}
