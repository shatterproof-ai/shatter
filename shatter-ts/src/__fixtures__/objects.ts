export interface Point {
  x: number;
  y: number;
}

export function distance(a: Point, b: Point): number {
  return Math.sqrt((a.x - b.x) ** 2 + (a.y - b.y) ** 2);
}

export function makePoint(x: number, y: number): Point {
  return { x, y };
}

export function getLabel(item: { name: string; count: number }): string {
  return `${item.name}: ${item.count}`;
}
