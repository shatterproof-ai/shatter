# Symbolic Stub Returns — Research Note

- **Issue**: str-hy9b.I3
- **Spec reference**: docs/specs/2026-04-17-go-frontend-redesign-v2.md §I3 (referenced
  by the issue tracker; the rendered spec was retired with the closed expedition
  but the design intent survives in the parent epic str-hy9b and in the I1/I2
  closed-task descriptions).
- **Status**: research-only deliverable — no code changes.
- **Built on**: str-hy9b.I1 (single-method interface stub), str-hy9b.I2
  (multi-method interface stub).

## 1. Concept

I1 and I2 give the planner a way to satisfy an interface-typed parameter that
has no discoverable concrete implementation: synthesise a struct in the
generated wrapper, attach the interface's methods, and return zero values from
each. This unblocks compilation and exercises the target with `nil`/`""`/`0`
return values.

The I3 question is whether those stub return values can be *symbolic inputs*
the Z3 solver drives, rather than fixed zeros. Concretely, instead of:

```go
type stubClock struct{}
func (stubClock) Now() time.Time { return time.Time{} }
```

the wrapper would declare a stub whose method bodies read from
solver-controlled inputs threaded into the harness, e.g.:

```go
type stubClock struct{ in *shatterStubInputs }
func (s stubClock) Now() time.Time { return s.in.clock_now_0 }
```

`shatterStubInputs` is populated by the launcher from the same input vector
that drives the function-under-test's parameters. From the planner's
perspective each stub return becomes an additional `ParamInfo` slot with a
`ValuePlan`; from the explorer/orchestrator perspective each stub return is an
additional symbolic variable the SymExpr instrumentor and the Z3 solver can
constrain and refine.

The payoff: an interface stub stops being a one-shot zero-value placeholder
and becomes a real *exploration surface*. Branches inside the
function-under-test that depend on `clock.Now()` or `cache.Get(k)` become
solvable rather than stuck on a fixed return.

## 2. Viability assessment

### 2.1 Technical feasibility

Feasible but non-trivial. The core mechanism — "treat a callee-supplied value
as a symbolic input the solver can drive" — already exists in the project:
function parameters are exactly that. The novel pieces are (a) cataloguing
extra symbolic slots beyond the function's declared parameters and (b)
plumbing them through the wrapper, instrumentor, executor, and solver as
first-class inputs.

Per-call uniqueness is the first design choice. `Clock.Now()` may be invoked
N times during one execution; the symbolic model is faithful only if each
call can return a different solver-controlled value. The minimum viable
implementation gives every (stub-method, call-site) pair a single fresh
symbolic variable per *execution*, and a richer implementation pre-allocates
a small ring (call-index 0..K-1) that wraps after K calls. Both are
representable as `Param { name: "stub_<iface>_<method>_<i>" }` in the existing
`SymExpr::Param` shape.

### 2.2 Integration points

| Layer | What changes | Difficulty |
|---|---|---|
| Go planner (`shatter-go/planner`) | Stub generation (currently in I1/I2 codepath) extends `argument_plans` with auxiliary `stub_return` slots; each gets a `ValuePlan` with a fresh `ParamIndex` namespace and `TypeHint` from the method's return type. | M |
| Go wrapper (`shatter-go/wrapper`) | Stub struct gains a private `*stubInputs` field; method bodies read indexed values from it. Wrapper template grows a `stubInputs` declaration and a constructor that takes input bytes/typed values. | M |
| Go protocol (`shatter-go/protocol/types.go`) | `InvocationPlan` already carries `ValuePlan` slots; adding stub returns is additive. May want a discriminator on `ValuePlan.Kind` — e.g. `runtime_value_stub_return` — so the wrapper knows where to route the value. | S |
| Go executor / launcher | Marshalling the input vector now also fills `stubInputs`. Launcher code path that constructs the wrapper invocation passes the populated struct. | M |
| Go instrumentor (`shatter-go/instrument`) | Stub method bodies become legal SSA-shaped code; they should be analysed like any other Go function so reads of `stubInputs.X` flow into `exprToSymExpr` and become `Param` nodes. The known limitation that Go does not synthesise `ite` nodes (parity contract `ite-symexpr-production-partial`) carries through but is not blocking. | M |
| Rust core SymExpr / solver | No new node types. New parameter names appear in `SymExpr::Param`; the solver's existing per-name sort declaration handles them. The harness round-trip (param → solver → next iteration) needs a path that maps solver-assigned values for `stub_*` names back into the next execution's input vector. | M |
| Rust orchestrator (`shatter-core/src/orchestrator.rs`) and explorer (`explorer.rs`) | Need to know that the input vector has more slots than the target's declared params. Cleanest split: planner advertises the full slot list (declared params + stub auxiliaries), orchestrator/explorer treat slots opaquely and let the wrapper interpret. | M |
| CLI | None for v1. Possibly a `--stub-symbolic-returns=off\|zero\|symbolic` flag later. | — |

### 2.3 Cost / value

Cost: roughly one E-tier slice of work (planner + wrapper + executor) plus a
smaller follow-up to surface the new param names through SymExpr cleanly. Not
a multi-week refactor; the existing planner-driven plumbing carries most of
the load.

Value: directly increases branch coverage on idiomatic Go code that uses
small abstraction interfaces (clocks, caches, key-value stores, feature
flags, loggers). These are exactly the places where the current zero-return
stub stalls exploration. This is also the prerequisite for any future
"symbolic mock" story (e.g. runtime stubbing of standard-library
interfaces).

## 3. Risks and open questions

1. **Per-call freshness.** Single-fresh-per-execution is the simple model and
   is sound; it under-approximates "the stub may return different values at
   different call sites." K-call ringbuffer is a strict improvement; full
   uninterpreted-function modelling (each call gets a fresh Skolem variable
   constrained by argument equality) is the principled answer but materially
   harder. Recommendation: ship single-fresh-per-execution for v1, design the
   protocol so K-ring and UF can be added without breaking changes.
2. **Argument-dependent returns.** A real `cache.Get(key)` ought to satisfy
   `Get(k) == Get(k)` for the same `k`. The stub model ignores arguments by
   default. UF modelling fixes this but expands the SymExpr surface. Worth
   flagging; not worth blocking on.
3. **Type coverage.** Method returns of pointer/interface/struct/error types
   are harder than primitives. v1 should restrict to the param-planner
   primitive types (the `runtime_values` set) plus `error`, and cleanly emit
   `UnsatisfiedRequirement{Kind: ComplexType}` otherwise — analogous to
   `PlanFallback`'s shape today.
4. **Variadic and pointer-return methods.** Methods returning multiple values
   (common: `(T, error)`) need pairwise symbolic slots. Planner already
   handles parallel `argument_plans`; same shape applies.
5. **Method-set escape.** If the target *stores* the stub somewhere durable
   (e.g. registers it as a global), the symbolic input model still works but
   the per-execution freshness contract may surprise users. Document.
6. **Cross-frontend parity.** This is intentionally Go-only (interface stubs
   are a Go-frontend feature), and the parity matrix already isolates Go's
   planner capability. No conformance-test regression expected; new
   `ValuePlan.Kind` would be additive on the wire and TS/Rust would ignore
   it like they ignore `Execute.plan` today
   (`ts-rust-execute-plan-not-implemented`).
7. **E2E gate exposure.** Symbolic stub returns directly touch
   instrumentor + orchestrator + solver — exactly the surface
   `cargo test --test e2e_concolic` exists to guard. Implementation must
   ship with at least one E2E case (interface-stub function with a
   return-value-dependent branch that the solver must drive).

## 4. Recommendation

Proceed. The mechanism is well-scoped, the integration points are familiar,
and the value (concolic exploration through Go interface boundaries) is
directly aligned with the project's reason for existing.

Suggested phasing:

- **Phase 1** — single-fresh-per-execution, primitives only, single-method
  interfaces (extends I1).
- **Phase 2** — multi-method interfaces (extends I2), `error` return type,
  multi-return signatures.
- **Phase 3** — K-call ring, complex/struct return types, optional
  UF-style argument-dependent returns.

Phase 3 should not block a usable v1; phases 1+2 alone cover the bulk of
real-world idiomatic interface usage.

## 5. Proposed follow-up issues

If/when implementation is greenlit, file these as children of `str-hy9b`
(the parent expedition) — or as a new mini-epic if the work crosses an
expedition boundary:

| Title | One-liner |
|---|---|
| Symbolic stub returns: protocol slot for stub-return ValuePlan | Add `ValuePlanKind` discriminator (or equivalent) so wrapper routes a slot to a stub method's return rather than a function parameter. Additive to `protocol/parity-matrix.yaml`. |
| Symbolic stub returns: planner emits stub-return slots | Extend the I1/I2 stub generator in `shatter-go/planner` to allocate one symbolic slot per stub method (single-fresh-per-execution v1). Restrict to primitive return types. |
| Symbolic stub returns: wrapper threads stubInputs into stub bodies | Wrapper template grows a `stubInputs` struct; stub method bodies read indexed values; launcher constructs and passes it. |
| Symbolic stub returns: instrumentor SymExpr coverage of stub reads | Verify `exprToSymExpr` resolves `stubInputs.field` reads as `SymExpr::Param`. Add property tests under the formal-methods policy. |
| Symbolic stub returns: orchestrator/explorer slot-count handling | Orchestrator and explorer carry the planner's full slot list; round-trip solver-assigned values for `stub_*` names back into the next execution's input vector. |
| Symbolic stub returns: E2E pipeline test | `shatter-core/tests/e2e_concolic.rs` case — interface-stub-driven branch resolved by Z3. Required gate (per `CLAUDE.md`). |
| Symbolic stub returns: error-typed return support | Cover `(T, error)` and bare `error` returns. Coordinates with `PlanFallback`'s existing error model. |
| Symbolic stub returns: parity matrix + scope-limits doc | Update `protocol/parity-matrix.yaml` and `docs/go-frontend-scope-limits.md` to reflect Go-only capability and the explicit non-goals (no UF, no K-ring in v1). |
| Symbolic stub returns: K-call ringbuffer (Phase 3) | Replace single-fresh-per-execution with a K-call ring so multi-call stub methods can return distinct values per call. Deferred. |
| Symbolic stub returns: argument-dependent returns via UF (Phase 3) | Optional Skolem/UF model where `Get(k) == Get(k)`. Deferred; revisit only after Phase 1+2 ship. |

These are research-recommended titles; the team-lead should decide whether to
create beads issues now or wait until prioritisation. Per the team-lead's
brief, "file them with `bd create` linked to str-hy9b as parent if research
recommends proceeding" — see §6.

## 6. Filing follow-up issues

The recommendation in §4 is to proceed. However, str-hy9b is closed and the
remaining open children (F4, F5, C6, I3, J3) are deferred follow-ups rather
than new work targets. Filing 10 new tickets under a closed epic would
muddy the tracker. Recommendation: **do not file the issues from this
research note directly**; instead, surface this note to the team-lead and
let them decide whether to (a) reopen str-hy9b, (b) create a new
mini-epic for symbolic stub returns, or (c) hold the work until a future
planning cycle.

If the team-lead later asks for the issues to be filed, this note's §5
table is the source.
