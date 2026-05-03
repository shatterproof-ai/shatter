# no-target-categories

One file per variant of `shatter_core::protocol::NoTargetReason`. The gate
runs `shatter scan` over this directory and asserts the per-file
`no_target_reason` matches the expected token.

| File | Expected `NoTargetReason` |
| --- | --- |
| `declaration_only.d.ts` | `declaration_only` |
| `jsx_component_only.tsx` | `jsx_component_only` |
| `test_or_spec.test.ts` | `test_or_spec` |
| `test_file_test.go` | `test_file` |
| `test_module_lib.rs` | `test_module` |
| `build.rs` | `build_script` |
| `generated_models_gen.go` | `generated` |
| `generated_schema.graphql.ts` | `generated_schema` |
| `parser_failure.ts` | `parser_failure` |
| `receiver_method_gap.go` | `receiver_method_gap` |
| `policy_excluded.ts` | `policy_excluded` (gate passes `--exclude '**/policy_excluded.ts'`) |

`Unclassified` is intentionally not represented; that variant is the
fallback when no classifier matches and is exercised implicitly by any
edge-case file outside this directory.

These files are intentionally siblings (no `go.mod`, no `package.json`) so
the Go and Rust files only exercise the discovery / classification path.
