import type { TypeInfo } from "./protocol.js";

// str-pjlc1: the shared TypeInfo union variant carries an optional
// `enum_values` value domain (Go const sets today; TS string-literal-union
// emission is a tracked follow-up). These tests pin the wire shape so the field
// round-trips and stays omitted for plain type unions.
describe("TypeInfo union enum_values", () => {
  it("round-trips a union carrying an enum value domain", () => {
    const ti: TypeInfo = {
      kind: "union",
      variants: [{ kind: "str" }],
      enum_values: ["RED", "GREEN", "BLUE"],
    };
    const decoded = JSON.parse(JSON.stringify(ti)) as TypeInfo;
    expect(decoded).toEqual(ti);
    if (decoded.kind === "union") {
      expect(decoded.enum_values).toEqual(["RED", "GREEN", "BLUE"]);
    }
  });

  it("accepts a numeric enum value domain", () => {
    const ti: TypeInfo = {
      kind: "union",
      variants: [{ kind: "int" }],
      enum_values: [0, 1, 2],
    };
    const decoded = JSON.parse(JSON.stringify(ti)) as TypeInfo;
    expect(decoded).toEqual(ti);
  });

  it("omits enum_values for a plain type union", () => {
    const ti: TypeInfo = {
      kind: "union",
      variants: [{ kind: "str" }, { kind: "int" }],
    };
    expect(JSON.stringify(ti)).not.toContain("enum_values");
  });
});
