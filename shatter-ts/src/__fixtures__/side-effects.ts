// Test fixture for side effect capture: console output, thrown errors, and global state.

export function logsAndReturns(x: number): string {
  console.log("processing", x);
  console.warn("watch out");
  return `done: ${x}`;
}

export function throwsError(msg: string): never {
  console.error("about to throw");
  throw new Error(msg);
}

export function logsMultipleLevels(): void {
  console.log("log message");
  console.warn("warn message");
  console.error("error message");
  console.info("info message");
  console.debug("debug message");
}

export function throwsCustomError(): never {
  throw new TypeError("custom type error");
}

export function noSideEffects(a: number, b: number): number {
  return a + b;
}

let counter = 0;

export function incrementCounter(): number {
  counter++;
  return counter;
}

export { counter };
