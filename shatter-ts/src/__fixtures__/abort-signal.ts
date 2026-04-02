/**
 * Fixture for str-ed25: AbortSignal stub must expose event-target methods.
 * Exercises AbortController/AbortSignal globals inside the VM sandbox.
 */
export function useAbortSignal(): string {
  const controller = new AbortController();
  const { signal } = controller;

  const handler = () => {};
  signal.addEventListener("abort", handler);
  signal.removeEventListener("abort", handler);

  return signal.aborted ? "aborted" : "not-aborted";
}
