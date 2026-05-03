// Imports a package that is intentionally NOT in package.json. A
// preflight that catches missing dependencies will surface this as a
// failed function with a "missing dependency" or build_failed reason,
// rather than silently dropping the function from the denominator.
//
// @ts-ignore intentional: fixture asserts shatter's preflight catches
// the missing dep before tsc would.
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore
import { clone } from "lodash";

export function dup<T>(x: T): T {
    return clone(x) as T;
}

export function pickPositive(n: number): number {
    if (n > 0) return n;
    return 0;
}
