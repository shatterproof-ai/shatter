import fc from "fast-check";

export const FAST_CHECK_NUM_RUNS_ENV = "SHATTER_FAST_CHECK_NUM_RUNS";

export function parseFastCheckNumRuns(
  value: string | undefined,
): number | undefined {
  if (value === undefined || value === "default") {
    return undefined;
  }
  if (!/^[1-9][0-9]*$/.test(value)) {
    throw new Error(
      `${FAST_CHECK_NUM_RUNS_ENV} must be "default" or a positive integer, got ${JSON.stringify(value)}`,
    );
  }
  return Number(value);
}

export function fastCheckParameters(defaultRuns: number): { numRuns: number } {
  return {
    numRuns:
      parseFastCheckNumRuns(process.env[FAST_CHECK_NUM_RUNS_ENV]) ?? defaultRuns,
  };
}

export function configureFastCheckFromEnv(): void {
  const numRuns = parseFastCheckNumRuns(
    process.env[FAST_CHECK_NUM_RUNS_ENV],
  );
  if (numRuns !== undefined) {
    fc.configureGlobal({ numRuns });
  }
}
