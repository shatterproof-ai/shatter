// str-knf0v analyzer fixture: parameters whose declared type resolves to a
// literal-union alias or a TS enum. The analyzer must emit an `enum_values`
// value domain on the param's union TypeInfo so the core draws valid members.

export type Mode = "fast" | "slow" | "off";

// Literal-union alias parameter.
export function pickMode(m: Mode): string {
  switch (m) {
    case "fast":
      return "F";
    case "slow":
      return "S";
    default:
      return "O";
  }
}

// String enum parameter (multi-member → resolves to a union of string literals).
export enum Color {
  Red = "RED",
  Green = "GREEN",
  Blue = "BLUE",
}

export function classify(c: Color): string {
  switch (c) {
    case Color.Red:
      return "warm";
    case Color.Green:
      return "cool-green";
    case Color.Blue:
      return "cool-blue";
    default:
      return "invalid";
  }
}

// Numeric enum parameter (multi-member → union of numeric literals; forward
// member values only, never the runtime reverse-mapping names).
export enum Level {
  Low = 1,
  Mid = 2,
  High = 3,
}

export function rank(l: Level): number {
  switch (l) {
    case Level.Low:
      return 10;
    case Level.Mid:
      return 20;
    case Level.High:
      return 30;
    default:
      return 0;
  }
}

// Single-member numeric enum (resolves to a lone enum literal, not a union).
export enum Solo {
  Only = 7,
}

export function solo(s: Solo): number {
  return s === Solo.Only ? 1 : 0;
}
