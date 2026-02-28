/**
 * Test fixture: a setup module that only exports setup() (no teardown).
 */

export function setup(functionName: string, mode: string): { ready: boolean } {
  return { ready: true };
}
