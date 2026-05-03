// TSX with a type-only import. Shatter's TS frontend must accept .tsx
// extensions and must tolerate `import type` which is erased at runtime.
import type { Settings } from "./types";

export function describeSettings(s: Settings): string {
    if (s.threshold < 0) return "invalid";
    if (s.mode === "off") return "off";
    if (s.threshold === 0) return "default";
    return "active";
}
