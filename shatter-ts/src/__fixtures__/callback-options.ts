// Fixture for testing per-field callable detection in convertObjectType.
//
// Expected branches / triggering inputs:
//   process("hello", { prefix: ">>", transform: undefined }) → ">>hello"  (transform omitted)
//   process("hello", { prefix: ">>", transform: (s) => s.toUpperCase() }) → ">>HELLO"
//
// Edge cases:
//   - transform is optional — engine must not pass a non-function value for it
//   - prefix is required — engine must always supply a string

export interface TransformOptions {
  transform?: (s: string) => string;
  prefix: string;
}

export function process(input: string, options: TransformOptions): string {
  let result = input;
  if (options.transform) {
    result = options.transform(result);
  }
  return options.prefix + result;
}

// Pure function parameter — must still be TypeInfo::Unknown (regression guard for
// the early-return path in convertObjectType that handles callable types directly).
export function applyFn(fn: (x: number) => number, value: number): number {
  return fn(value);
}
