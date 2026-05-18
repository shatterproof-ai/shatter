// Fixtures for str-yb7q: ensure structural type info reaches input generation
// instead of degrading to {kind: "unknown"} or {kind: "object", fields: []}.

export function sumPair(pair: [number, number]): number {
  return pair[0] + pair[1];
}

export function labelTuple(t: readonly [string, number]): string {
  return `${t[0]}:${t[1]}`;
}

interface Row {
  id: number;
}

export function countRowsByKey(table: Record<string, Row[]>): number {
  let total = 0;
  for (const key of Object.keys(table)) {
    total += table[key]!.length;
  }
  return total;
}

export function arrayLikeLength(xs: ArrayLike<number>): number {
  return xs.length;
}

export function constrainedGeneric<T extends number[]>(xs: T): number {
  return xs.length;
}

export function nestedRows(data: { rows: Row[] }): number {
  return data.rows.filter((r) => r.id > 0).length;
}

export function nestedArrays(items: number[][]): number {
  return items.reduce((acc, row) => acc + row.length, 0);
}
