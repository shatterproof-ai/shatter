// A module mixing exported public functions with non-exported (private)
// helpers. Shatter must be able to instrument exported entry points
// even when those depend on file-local helpers; harness generation that
// requires the helper to be exported (or that fails on closure capture)
// will surface here as build_failed.

function clamp(n: number, lo: number, hi: number): number {
    if (n < lo) return lo;
    if (n > hi) return hi;
    return n;
}

function describe(n: number): string {
    if (n === 0) return "zero";
    return n > 0 ? "pos" : "neg";
}

export function classify(input: number): string {
    const bounded = clamp(input, -100, 100);
    return describe(bounded);
}

export function describePair(a: number, b: number): string {
    return describe(a) + ":" + describe(b);
}
