import * as path from "node:path";
import type { TypeInfo } from "./protocol.js";
import { analyzeFile } from "./analyzer.js";

const fixtures = path.join(__dirname, "__fixtures__");

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

  // str-knf0v: the analyzer now EMITS enum_values (not just round-trips a
  // hand-built value). Pin that the emitted TypeInfo survives a JSON wire trip
  // with the value domain intact.
  it("round-trips an analyzer-emitted enum_values domain", () => {
    const results = analyzeFile(path.join(fixtures, "enum-values.ts"), "classify");
    const emitted = results[0]!.params[0]!.type;
    expect(emitted).toEqual({
      kind: "union",
      variants: [{ kind: "str" }],
      enum_values: ["RED", "GREEN", "BLUE"],
    });
    const decoded = JSON.parse(JSON.stringify(emitted)) as TypeInfo;
    expect(decoded).toEqual(emitted);
    expect(JSON.stringify(emitted)).toContain("enum_values");
  });
});
