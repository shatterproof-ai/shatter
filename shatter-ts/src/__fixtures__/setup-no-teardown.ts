/**
 * Test fixture: a setup module that only exports setup() (no teardown).
 */

export function setup(scope: string, _parentContext?: unknown): { ready: boolean } {
  return { ready: true };
}
