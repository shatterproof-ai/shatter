// Type-only declarations: these must NOT contribute runtime functions
// to the scan denominator, and must NOT trigger "no exported function"
// failures. A frontend that emits placeholder targets for type aliases
// will surface here.

export type Mode = "fast" | "slow" | "off";

export interface Settings {
    mode: Mode;
    threshold: number;
}
