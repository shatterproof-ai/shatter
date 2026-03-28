export function simpleIf(x: number): string {
  if (x > 0) {
    return "positive";
  }
  return "non-positive";
}

export function ifElse(x: number): string {
  if (x > 0) {
    return "positive";
  } else {
    return "non-positive";
  }
}

export function ifElseIf(x: number): string {
  if (x > 0) {
    return "positive";
  } else if (x < 0) {
    return "negative";
  } else {
    return "zero";
  }
}

export function switchCase(x: number): string {
  switch (x) {
    case 1:
      return "one";
    case 2:
      return "two";
    default:
      return "other";
  }
}

export function ternary(x: number): string {
  return x > 0 ? "positive" : "negative";
}

export function logicalAnd(x: number, y: number): boolean {
  return x > 0 && y > 0;
}

export function logicalOr(x: number, y: number): boolean {
  return x > 0 || y > 0;
}

export function nestedBranches(x: number, y: number): string {
  if (x > 0) {
    if (y > 0) {
      return "both positive";
    }
    return "x positive only";
  }
  return "x non-positive";
}

export function whileLoop(x: number): number {
  let sum = 0;
  let i = 0;
  while (i < x) {
    sum += i;
    i++;
  }
  return sum;
}

export function forLoop(x: number): number {
  let sum = 0;
  for (let i = 0; i < x; i++) {
    sum += i;
  }
  return sum;
}

export function mixedBranches(x: number, y: number): string {
  if (x > 0) {
    switch (y) {
      case 1:
        return "x positive, y is 1";
      case 2:
        return "x positive, y is 2";
    }
    return x > 10 ? "big positive" : "small positive";
  }
  return "non-positive";
}

// Loop induction variable analysis fixtures

export function forLoopCanonical(n: number): number {
  let sum = 0;
  for (let i = 0; i < n; i++) {
    sum += i;
  }
  return sum;
}

export function forLoopStepTwo(n: number): number {
  let sum = 0;
  for (let i = 0; i < n; i += 2) {
    sum += i;
  }
  return sum;
}

export function forLoopBodyModifiesI(n: number): number {
  let sum = 0;
  for (let i = 0; i < n; i++) {
    i = 5;
    sum += i;
  }
  return sum;
}

export function forLoopNoCondition(n: number): number {
  let sum = 0;
  for (let i = 0; ; i++) {
    if (i >= n) break;
    sum += i;
  }
  return sum;
}

export function forLoopFloatInit(n: number): number {
  let sum = 0;
  for (let i = 0.5; i < n; i++) {
    sum += i;
  }
  return sum;
}
