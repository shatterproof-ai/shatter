// Uses browser globals (window, document, fetch). Exercises str-jeen.30 —
// TS browser-global handling: the analyzer should not crash on bare
// references to host-provided identifiers, and broad-run reports should
// classify these targets coherently rather than as analyze failures.

declare const window: { location: { hostname: string } };
declare const document: { title: string };
declare const fetch: (url: string) => Promise<{ ok: boolean }>;

export function isLocalHost(): boolean {
  return window.location.hostname === "localhost";
}

export function pageTitle(): string {
  if (document.title.length === 0) {
    return "untitled";
  }
  return document.title;
}

export async function probe(url: string): Promise<string> {
  const response = await fetch(url);
  return response.ok ? "ok" : "fail";
}
