/**
 * Wire-typing tests for the optional `plan` (InvocationPlan) field on
 * PrepareRequest / ExecuteRequest (str-mhinv.2). TS types the field but does
 * not consume it — see `ts-rust-execute-plan-not-implemented` in
 * protocol/parity-matrix.yaml.
 */
import type {
  ExecuteRequest,
  InvocationPlan,
  PrepareRequest,
} from "./protocol.js";
import { PROTOCOL_VERSION } from "./protocol.js";

// Wire shape produced by the Rust core / Go planner, including
// receiver_field_plans (str-mhinv.1) and a runtime_value plan.
const planWire = {
  target_id: "example.com/pkg:(*queryResolver).Search",
  receiver_kind: "constructor:NewResolver",
  generic_type_args: ["int"],
  argument_plans: [
    { param_index: 0, param_name: "ctx", kind: "runtime_value", literal: "context.Background()", type_hint: "context.Context" },
    { param_index: 1, param_name: "q", kind: "literal", literal: "abc" },
  ],
  constructor_arg_plans: [{ param_index: 0, param_name: "n", kind: "zero" }],
  receiver_field_plans: [
    { path: ["Resolver", "SearchBackend"], kind: "runtime_value", literal: "stub()", type_hint: "Backend" },
    { path: ["Limit"], kind: "symbolic" },
  ],
  priority: 1,
  label: "ctor",
};

describe("InvocationPlan request typing", () => {
  it("ExecuteRequest with plan survives JSON round-trip", () => {
    const plan: InvocationPlan = planWire as InvocationPlan;
    const req: ExecuteRequest = {
      protocol_version: PROTOCOL_VERSION,
      id: 1,
      command: "execute",
      function: "Search",
      inputs: [],
      mocks: [],
      plan,
    };
    const decoded = JSON.parse(JSON.stringify(req)) as ExecuteRequest;
    expect(decoded).toEqual(req);
    expect(decoded.plan?.receiver_field_plans?.[0]?.path).toEqual(["Resolver", "SearchBackend"]);
  });

  it("PrepareRequest with plan survives JSON round-trip", () => {
    const req: PrepareRequest = {
      protocol_version: PROTOCOL_VERSION,
      id: 2,
      command: "prepare",
      file: "a.ts",
      function: "f",
      mocks: [],
      plan: planWire as InvocationPlan,
    };
    expect(JSON.parse(JSON.stringify(req))).toEqual(req);
  });

  it("requests without plan (or with plan: null) are unchanged", () => {
    const exec: ExecuteRequest = { protocol_version: PROTOCOL_VERSION, id: 3, command: "execute", function: "f", inputs: [1], mocks: [] };
    expect(JSON.parse(JSON.stringify(exec))).toEqual(exec);
    expect("plan" in JSON.parse(JSON.stringify(exec))).toBe(false);

    const prep: PrepareRequest = { protocol_version: PROTOCOL_VERSION, id: 4, command: "prepare", file: "a.ts", function: "f", mocks: [], plan: null };
    expect(JSON.parse(JSON.stringify(prep)).plan).toBeNull();
  });
});
