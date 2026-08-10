/**
 * Minimal ambient declaration for js-yaml (no `@types/js-yaml` is installed).
 *
 * Only `load` is used, and only to parse the trusted, project-local
 * `.shatter/config.yaml`. The full js-yaml surface is intentionally not
 * declared — add members here if a future caller needs them.
 */
declare module "js-yaml" {
  /** Parse a single YAML document into a JS value. Returns `undefined` for empty input. */
  export function load(input: string): unknown;
}
