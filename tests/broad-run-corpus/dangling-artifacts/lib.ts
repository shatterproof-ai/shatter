// Tiny TS target. The gate runs `shatter scan` against this fixture, then
// walks the resulting JSON / markdown report and asserts every artifact
// path it references resolves on disk (str-jeen.4 — artifact references
// valid). The point is to detect the regression class where the report
// embeds paths into a temp dir that has already been cleaned up, leaving
// dangling references.
export function classify(value: number): string {
  if (value < 0) {
    return "negative";
  }
  if (value === 0) {
    return "zero";
  }
  return "positive";
}
