// Param-name generator for authToken parameters.
//
// When Shatter encounters a parameter named "authToken", it calls this
// generator to produce a realistic token string.
//
// Usage in .shatter/config.yaml:
//   defaults:
//     param_generators:
//       authToken: ./generators/token.ts

export function authToken(): string {
  return "tok_live_a1b2c3d4e5f6";
}
