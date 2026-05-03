// Mixed-language tree: TS file alongside Rust sources whose frontend
// is currently unavailable in the standard build. Scan must report
// the Rust files with an honest unavailable status (or skip them with
// a clear reason) without aborting the whole run.

export function tsClassify(n: number): string {
    if (n < 0) return "neg";
    if (n === 0) return "zero";
    return "pos";
}
