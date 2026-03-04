/**
 * Async test fixtures for measureExecution/executeFunction async support.
 *
 * asyncAdd — resolves: returns sum of a + b
 * asyncThrows — rejects: throws Error("async boom")
 * asyncHangs — never resolves: hangs until timeout
 */

export async function asyncAdd(a: number, b: number): Promise<number> {
  return a + b;
}

export async function asyncThrows(): Promise<string> {
  throw new Error("async boom");
}

export async function asyncHangs(): Promise<string> {
  return new Promise<string>(() => {
    // intentionally never resolves
  });
}
