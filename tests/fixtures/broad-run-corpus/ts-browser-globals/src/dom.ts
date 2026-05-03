// Functions that reference browser-only globals (window, document,
// localStorage). The Node-based harness has no DOM, so these should
// produce a clean failure or skip — never a silent success and never
// a "completed_functions = 0" denominator collapse.

export function readFlag(key: string): string | null {
    if (typeof window === "undefined") return null;
    return window.localStorage.getItem(key);
}

export function readTitle(): string {
    if (typeof document === "undefined") return "";
    return document.title;
}

export function isProduction(): boolean {
    if (typeof window === "undefined") return false;
    return window.location.hostname === "prod.example.com";
}
