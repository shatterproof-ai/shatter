/**
 * Adapter fixture helper: math utility inside a referenced project.
 *
 * Used to verify that targets in referenced projects are discoverable
 * and executable when the analyzer is pointed at the workspace root.
 */

export function clamp(value: number, min: number, max: number): number {
  if (value < min) {
    return min;
  }
  if (value > max) {
    return max;
  }
  return value;
}
