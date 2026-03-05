/**
 * Value reconstruction: convert __complex_type tagged JSON into native JS objects.
 *
 * Called on input values before passing them to the function under test.
 * Each frontend has its own reconstruction module matching the types it declared
 * in its handshake capabilities.
 */

import logger from "./logger.js";

/**
 * Recursively reconstruct native JS values from __complex_type tagged JSON.
 *
 * Plain values (numbers, strings, booleans, null) pass through unchanged.
 * Arrays are reconstructed element-by-element.
 * Objects with a __complex_type tag are dispatched to type-specific constructors.
 * Plain objects are reconstructed field-by-field.
 */
export function reconstructValue(value: unknown): unknown {
  if (typeof value !== "object" || value === null) return value;
  if (Array.isArray(value)) return value.map(reconstructValue);

  const obj = value as Record<string, unknown>;
  const tag = obj["__complex_type"] as string | undefined;

  if (!tag) {
    // Plain object: reconstruct each field
    return Object.fromEntries(
      Object.entries(obj).map(([k, v]) => [k, reconstructValue(v)])
    );
  }

  switch (tag) {
    case "date":
    case "date_time":
      return new Date(obj["value"] as number);

    case "duration":
      // JS doesn't have a Duration type; return the millisecond value
      return (obj["ms"] as number) ?? (obj["value"] as number);

    case "time":
      // Return as a plain object with hour/minute/second/ms
      return {
        hour: obj["hour"] as number,
        minute: obj["minute"] as number,
        second: obj["second"] as number,
        ms: obj["ms"] as number,
      };

    case "reg_exp":
      return new RegExp(
        obj["source"] as string,
        (obj["flags"] as string) ?? ""
      );

    case "big_int":
      return BigInt(obj["value"] as string);

    case "url":
      return new URL(obj["value"] as string);

    case "buffer":
      return Buffer.from(
        obj["value"] as string,
        (obj["encoding"] as BufferEncoding) ?? "base64"
      );

    case "error": {
      const className = (obj["class"] as string) ?? "Error";
      const message = (obj["message"] as string) ?? "";
      switch (className) {
        case "TypeError": return new TypeError(message);
        case "RangeError": return new RangeError(message);
        case "SyntaxError": return new SyntaxError(message);
        case "ReferenceError": return new ReferenceError(message);
        case "URIError": return new URIError(message);
        case "EvalError": return new EvalError(message);
        default: return new Error(message);
      }
    }

    case "symbol":
      return Symbol((obj["description"] as string) ?? "");

    case "option":
      return (obj["present"] as boolean)
        ? reconstructValue(obj["value"])
        : undefined;

    case "uuid":
    case "path":
    case "email":
    case "mime_type":
    case "locale":
    case "sem_ver":
      // These are plain strings in JS
      return obj["value"] as string;

    default:
      // Unknown complex type: log warning and pass through as-is
      logger.warn("unknown __complex_type: %s, passing through", tag);
      return value;
  }
}
