/**
 * Value serialization: convert native JS values that JSON.stringify cannot
 * handle into __complex_type tagged JSON objects.
 *
 * This is the inverse of reconstruct.ts — that module converts tagged JSON
 * into native JS values for function inputs; this module converts native JS
 * values back to tagged JSON for protocol output.
 *
 * Currently handles: BigInt (the only native JS type that causes
 * JSON.stringify to throw).
 */

/**
 * JSON.stringify replacer that converts BigInt values to tagged objects.
 *
 * Usage: `JSON.stringify(value, serializeReplacer)`
 *
 * BigInt → `{ __complex_type: "big_int", value: "<decimal string>" }`
 */
export function serializeReplacer(_key: string, value: unknown): unknown {
  if (typeof value === "bigint") {
    return { __complex_type: "big_int", value: value.toString() };
  }
  return value;
}
