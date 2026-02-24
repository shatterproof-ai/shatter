export function format(value: string | number): string {
  if (typeof value === "string") {
    return value;
  }
  return value.toFixed(2);
}

export function nullable(x: number | null): number {
  return x ?? 0;
}

export function optional(x?: number): number {
  return x ?? 0;
}

export function undefinable(x: string | undefined): string {
  return x ?? "";
}

export function complex(x: string | number | null): string {
  if (x === null) return "null";
  if (typeof x === "string") return x;
  return String(x);
}
