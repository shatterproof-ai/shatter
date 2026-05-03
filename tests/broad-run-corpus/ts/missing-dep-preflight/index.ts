// Imports a declared-but-uninstalled package. Preflight should detect the
// missing node_modules entry (str-jeen.26) and surface `preflight_failed`
// rather than aborting the entire scan.
import { mystery } from "definitely-not-installed-pkg";

export function describe(value: number): string {
  if (value < 0) {
    return "negative";
  }
  return mystery(value);
}
