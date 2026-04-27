# str-hy9b Expedition Log

## Frozen Header

- Expedition: `str-hy9b`
- Base branch: `str-hy9b`
- Primary branch: `main`
- Base worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b`
- State file: `docs/expeditions/str-hy9b/state.json`

## Activity Log

### 2026-04-19 — Expedition bootstrapped from prior swarm workflow

Prior workflow (swarm/beads) completed 9 tasks directly to main before the
expedition was set up:
- A1: Outcome protocol types
- B1: Artifact workspace layout
- C1: Adopt go/packages loader
- D1: Legal-anchor resolver
- D2: Overlay manifest generator
- D5: Concolic instrumentation overlay
- D7: Internal-method spike fixture
- H3: Parity matrix update
- I1: Single-method interface stub

Stale worktrees left by the prior workflow (no commits on them, all behind main):
- str-hy9b-1-go-frontend-redesign-doc, str-hy9b-a2b2-outcome-gocache,
  str-hy9b-a3-markdown-renderer, str-hy9b-b3-workspace-gc,
  str-hy9b-c2-packages-analyzer, str-hy9b-d7-internal-method-spike

These must be removed before starting those tasks under the expedition model.


### 2026-04-19T21:07:05Z — Started task
- Branch: `str-hy9b-01-a2-outcome-plumbing`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-01-a2-outcome-plumbing`.
- Base head at branch creation: `f78ec486fce9ad5cea6a1193d015f2094c3266c5`.


### 2026-04-19T21:17:48Z — Closed task
- Branch: `str-hy9b-01-a2-outcome-plumbing`.
- Outcome: `kept`.
- Summary: Outcome plumbing in Go executor: failureOutcome + outcomeFromResult wired through handler; outcome_test.go covers all status paths; smoke passes
- Base branch rebased onto the primary branch.


### 2026-04-19T21:17:52Z — Started task
- Branch: `str-hy9b-02-a3-markdown-renderer`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-02-a3-markdown-renderer`.
- Base head at branch creation: `b5dc50c50167d36cd5e534ea84d048a273723262`.


### 2026-04-19T22:38:38Z — Closed task
- Branch: `str-hy9b-02-a3-markdown-renderer`.
- Outcome: `kept`.
- Summary: Markdown renderer drives outcomes: render_explore_outcomes() replaces md_fragments join; empty-discovery guard; 3 snapshots covering build_failed/unsupported/empty; smoke passes
- Base branch rebased onto the primary branch.


### 2026-04-19T22:38:43Z — Started task
- Branch: `str-hy9b-03-a4-empty-report-regression`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-03-a4-empty-report-regression`.
- Base head at branch creation: `a5b6d814a1d28c9e4d548da848b67c83d0804b35`.


### 2026-04-20T02:27:02Z — Closed task
- Branch: `str-hy9b-03-a4-empty-report-regression`.
- Outcome: `kept`.
- Summary: Empty-report regression test: smoke script asserts non-empty md, target name, and outcome status in {build_failed,unsupported,runtime_failed}; wired into npx task smoke
- Base branch rebased onto the primary branch.


### 2026-04-20T02:27:50Z — Started task
- Branch: `str-hy9b-04-b3-workspace-gc`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-04-b3-workspace-gc`.
- Base head at branch creation: `dcfe25adf53399037e116899b9ac7d558c1efce3`.


### 2026-04-20T14:22:03Z — Closed task
- Branch: `str-hy9b-04-b3-workspace-gc`.
- Outcome: `kept`.
- Summary: Workspace gc CLI: gc.go + run.go + workspace_cli.go; --dry-run lists candidates; size/age/keep-N caps enforced; property tests; CLI wired via Rust workspace command
- Base branch rebased onto the primary branch.


### 2026-04-20T14:22:18Z — Started task
- Branch: `str-hy9b-05-c2-packages-analyzer`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-05-c2-packages-analyzer`.
- Base head at branch creation: `47995e028158f10d88c526ee77a7f7f7615e473a`.


### 2026-04-20T14:55:16Z — Closed task
- Branch: `str-hy9b-05-c2-packages-analyzer`.
- Outcome: `kept`.
- Summary: Packages-based analyzer: go/packages loader replaces single-file types.Check path; multi-file package and internal-import acceptance criteria verified; lint cleanup across protocol pkg
- Base branch rebased onto the primary branch.


### 2026-04-20T14:55:33Z — Started task
- Branch: `str-hy9b-06-h4-conformance-tests`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-06-h4-conformance-tests`.
- Base head at branch creation: `57506a8b5342d2f519623b7a2ab7217f4a19fc6a`.


### 2026-04-20T15:09:53Z — Closed task
- Branch: `str-hy9b-06-h4-conformance-tests`.
- Outcome: `kept`.
- Summary: Conformance test additions: 8 new cases covering outcome/adapter_http_nethttp/invocation_plan/hint_config_v1 from H3; Go-only capabilities assert TS/Rust return clean errors; 39 checks pass
- Base branch rebased onto the primary branch.


### 2026-04-20T15:10:01Z — Started task
- Branch: `str-hy9b-07-i2-multi-method-interface-stub`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-07-i2-multi-method-interface-stub`.
- Base head at branch creation: `59007c2f170c76245ca5e32bdf96584910d21a98`.


### 2026-04-20T15:28:38Z — Closed task
- Branch: `str-hy9b-07-i2-multi-method-interface-stub`.
- Outcome: `kept`.
- Summary: Multi-method interface stub: cap lifted to 5 methods; three-method acceptance criterion test added; lint sweep of executor/visitor/mcdc (unused funcs removed, switches modernized, fmt.Fprintf bulk conversion)
- Base branch rebased onto the primary branch.


### 2026-04-20T18:27:11Z — Started task
- Branch: `str-hy9b-08-c3-discovered-target-schema`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-08-c3-discovered-target-schema`.
- Base head at branch creation: `0a6408859bdc394a3394e19c5a2b327581ba260d`.


### 2026-04-20T18:48:23Z — Closed task
- Branch: `str-hy9b-08-c3-discovered-target-schema`.
- Outcome: `kept`.
- Summary: DiscoveredTarget schema: TargetKind/ReceiverShape/DiscoveredTarget types + BuildDiscoveredTarget builder; stable IDs; tests cover all 4 acceptance criteria cases + JSON roundtrip; smoke passes
- Base branch rebased onto the primary branch.


### 2026-04-20T18:49:18Z — Started task
- Branch: `str-hy9b-09-c4-method-classification-gate`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-09-c4-method-classification-gate`.
- Base head at branch creation: `a60be177510e596092d655e98d5e3369c35c7853`.


### 2026-04-20T19:08:50Z — Closed task
- Branch: `str-hy9b-09-c4-method-classification-gate`.
- Outcome: `kept`.
- Summary: Method classification gate: analyzeForExecution detects method receivers and returns sentinel error; failureOutcome maps to OutcomeStatusUnsupported with method_not_supported error type; kind=function/method logged; test with *Service.Compute verifies unsupported outcome instead of build failure; smoke passes
- Base branch rebased onto the primary branch.


### 2026-04-20T19:09:06Z — Started task
- Branch: `str-hy9b-10-c5-constructor-catalog`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-10-c5-constructor-catalog`.
- Base head at branch creation: `ecb10150f98421e1a6a4f60d3e94b10dcfab9ec5`.


### 2026-04-20T23:32:28Z — Started task
- Branch: `str-hy9b-12-d4-launcher-loop-harness`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-12-d4-launcher-loop-harness`.
- Base head at branch creation: `6bf9444bd977f4c55238d330cefcb8bb19122903`.


### 2026-04-21T00:13:22Z — Closed task
- Branch: `str-hy9b-12-d4-launcher-loop-harness`.
- Outcome: `kept`.
- Summary: Per-target launcher loop harness: launcher/launcher.go generates self-contained main.go (stdlib + target pkg), BuildLauncher compiles with overlay and caches binary at workspace/binaries; LauncherSession tracks InvocationsDispatched; harness.Request extended with Plan field; integration test verifies 5 plans × 10 inputs = 50 invocations, 1 build, 1 binary invocation
- Base branch rebased onto the primary branch.


### 2026-04-21T00:13:44Z — Started task
- Branch: `str-hy9b-13-d6-build-orchestrator`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-13-d6-build-orchestrator`.
- Base head at branch creation: `24e6fe5c33de3200e8168e1bdb48ce1e5fb68079`.


### 2026-04-21T00:27:35Z — Closed task
- Branch: `str-hy9b-13-d6-build-orchestrator`.
- Outcome: `kept`.
- Summary: Build orchestrator and binary registry: build/builder.go Builder.Build() coordinates D3 wrapper generation + D4 launcher compilation, keyed by discovery hash; BinaryRegistry (in-memory+JSON persist) ensures two plans→one build; ParseBuildOutput extracts structured Diagnostics; build logs to workspace/runs/<runID>/; integration tests verify all 3 ACs
- Base branch rebased onto the primary branch.


### 2026-04-21T00:28:33Z — Started task
- Branch: `str-hy9b-14-e1-invocation-plan-schemas`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-14-e1-invocation-plan-schemas`.
- Base head at branch creation: `ea9abaee799b6a928f124c96c34fe140e6429ee3`.


### 2026-04-21T01:22:21Z — Closed task
- Branch: `str-hy9b-14-e1-invocation-plan-schemas`.
- Outcome: `kept`.
- Summary: Invocation plan schemas: protocol/invocation_plan.go adds InvocationRequirement, ValueRequirement, RuntimeRequirement, InvocationPlan, ValuePlan, UnsatisfiedRequirement as Go-only planner types; all 6 types have round-trip JSON tests and JSON constant spelling tests; parity matrix already marks invocation_plan as go: supported
- Base branch rebased onto the primary branch.


### 2026-04-21T01:54:53Z — Started task
- Branch: `str-hy9b-15-e2-target-classifier`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-15-e2-target-classifier`.
- Base head at branch creation: `dcad0860516377b0ad8ac63f35c3e4c20dccd105`.


### 2026-04-21T04:24:37Z — Closed task (reconciled)
- Branch: `str-hy9b-15-e2-target-classifier`.
- Outcome: `kept`.
- Summary: Target classifier: planner.Classify returns DirectFunction/Method/AdapterCandidate/Unsupported (reasons: generic_unconstrained/interface_receiver/cgo_dependency/test_only_visibility); DiscoveredTarget extended with HasTypeParams/HasCGoDep/IsTestFile + ReceiverShape.IsInterface from AST+types.Info; 7 classification paths + 2 priority-order tests
- Reconciliation: task code already merged into base by prior session (commit ae7bf6ef); close-task.py merge would conflict on stale task-branch state.json. State updated manually; base branch pending rebase onto main.


### 2026-04-22T01:19:18Z — Started task
- Branch: `str-hy9b-16-e3-receiver-planner`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-16-e3-receiver-planner`.
- Base head at branch creation: `008675e75bf01d7aa783288458b5ad08c7cf8a3a`.


### 2026-04-22T01:55:44Z — Closed task
- Branch: `str-hy9b-16-e3-receiver-planner`.
- Outcome: `kept`.
- Summary: Receiver planner: shatter-go/planner/receiver.go implements PlanReceivers with strategy order adapter>same-pkg-ctor>nearby-pkg-ctor>composite-literal>useful-zero-value>hint, capped at DefaultMaxReceiverPlans=3; 3 ACs + ordering + cap + interface/generic short-circuit + rapid invariants covered; smoke and standard tier pass
- Base branch rebased onto the primary branch.


### 2026-04-22T03:13:14Z — Started task
- Branch: `str-hy9b-17-e4-param-planner-primitives`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-17-e4-param-planner-primitives`.
- Base head at branch creation: `b7c5faffcb3678426d795d1cf550fc1d625c18af`.


### 2026-04-22T04:42:29Z — Closed task
- Branch: `str-hy9b-17-e4-param-planner-primitives`.
- Outcome: `kept`.
- Summary: Parameter planner primitives: shatter-go/planner/param.go implements PlanParam/PlanParams over primitive families string/int/float/bool/[]byte; unsupported types emit per-parameter complex_type UnsatisfiedRequirement without blocking siblings; capped at DefaultMaxParamValuePlans=4; both ACs covered plus hint-priority, mixed-support, rapid invariants; smoke passes
- Base branch rebased onto the primary branch.


### 2026-04-22T13:44:29Z — Started task
- Branch: `str-hy9b-18-e5-plan-ranking-and-budget`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-18-e5-plan-ranking-and-budget`.
- Base head at branch creation: `e3edf2e02fbc7f3706c1d738b4224cd0ef3cde87`.


### 2026-04-22T13:52:58Z — Closed task
- Branch: `str-hy9b-18-e5-plan-ranking-and-budget`.
- Outcome: `kept`.
- Summary: Plan ranking and budget: shatter-go/planner/compose.go implements deterministic beam search over receiver x ValuePlan matrix; ranked by (hintDepCount, receiverPriority, enumerationIndex); capped at DefaultMaxComposedPlansPerTarget=5; AC 2x3 cap check + ranking determinism + free function + method-with-no-receivers + hint-ranked-after-nonhint + MaxPlans override + zero-param + rapid invariant; smoke passes
- Base branch rebased onto the primary branch.


### 2026-04-22T14:20:18Z — Started task
- Branch: `str-hy9b-19-f1-composite-literal-synthesis`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-19-f1-composite-literal-synthesis`.
- Base head at branch creation: `c9af94281d15d12e6a0a21fa2655decce1e00046`.


### 2026-04-22T15:34:05Z — Closed task
- Branch: `str-hy9b-19-f1-composite-literal-synthesis`.
- Outcome: `kept`.
- Summary: Composite literal synthesis: shatter-go/planner/composite.go PlanComposite emits Go composite-literal for primitives + elided nested structs + pointer-as-nil; bounded recursion via MaxDepth; all 4 F1 ACs and rapid termination property covered; smoke passes
- Base branch rebased onto the primary branch.


### 2026-04-22T17:24:36Z — Started task
- Branch: `str-hy9b-20-f2-runtime-values-registry`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-20-f2-runtime-values-registry`.
- Base head at branch creation: `d30be6506d578d66c1934f00f3a8984a1f08e256`.


### 2026-04-22T20:16:30Z — Closed task
- Branch: `str-hy9b-20-f2-runtime-values-registry`.
- Outcome: `kept`.
- Summary: Runtime values registry: shatter-go/planner/runtime_values.go RuntimeValue{Expression,TypeHint,Imports,SideEffectClass} registry for context.Context/*bytes.Buffer/io.Reader/io.Writer/time.Time/http.Header; PlanParam consults registry before UnsatisfiedRequirement fallback; adds ValuePlanKindRuntimeValue (Go-only invocation_plan); AC func(ctx context.Context) emits context.Background(); all defaults + registry isolation + cap + unknown-opaque + sorted listing + rapid roundtrip covered; 941 tests + smoke pass
- Base branch rebased onto the primary branch.


### 2026-04-22T23:08:40Z — Started task
- Branch: `str-hy9b-21-f3-constructor-scoring`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-21-f3-constructor-scoring`.
- Base head at branch creation: `2faf6c8b73cc4393f9cbe2edd09f11573daee9ca`.


### 2026-04-22T23:21:30Z — Closed task
- Branch: `str-hy9b-21-f3-constructor-scoring`.
- Outcome: `kept`.
- Summary: Constructor scoring: shatter-go/planner/constructor.go additive weights (+3 same-pkg, +2 return-match, +1 zero-param, +1/satisfiable param, +1 New/Default, -1 error, -2 Must); RankConstructors sorts desc with alpha tie-break; reuses classifyParamFamily/runtimeValuePlans for satisfiability; AC two *Service ctors satisfiable scores higher + each rule + Must/idiomatic exclusivity + rank monotonicity + deterministic tie-break + rapid invariants; 91 planner tests + smoke pass
- Base branch rebased onto the primary branch.


### 2026-04-23T02:12:30Z — Started task
- Branch: `str-hy9b-22-f4-param-aggregate-types`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-22-f4-param-aggregate-types`.
- Base head at branch creation: `afd9beacb779970c087695b0a20765fc4e7231c9`.


### 2026-04-23T02:47:39Z — Closed task
- Branch: `str-hy9b-22-f4-param-aggregate-types`.
- Outcome: `kept`.
- Summary: Parameter planner aggregate types: shatter-go/planner/aggregate.go PlanAggregate dispatches on slice/map/struct TypeInfo, reuses synthesizeFieldValue/PlanComposite; emits runtime_value ValuePlans with Go source expression in Literal. Wired into PlanParam after primitive family, before runtime_values. AC1 []int/[]string/[]bool zero-length+one-element + AC2 map[string]int one-entry + AC3 pkg.Req composite + AC4 unsupported aggregates fall through without blocking siblings + rapid determinism + cap + byte-slice primitive-routing preserved + no-TypeName fallthrough; 965 go tests + smoke pass
- Base branch rebased onto the primary branch.


### 2026-04-23T03:03:12Z — Started task
- Branch: `str-hy9b-23-f5-param-error-chan-func`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-23-f5-param-error-chan-func`.
- Base head at branch creation: `92a3d07b19cd46b70467909419568b78a274f8e8`.


### 2026-04-23T03:12:36Z — Closed task
- Branch: `str-hy9b-23-f5-param-error-chan-func`.
- Outcome: `kept`.
- Summary: Parameter planner error/chan/func fallbacks: shatter-go/planner/fallback.go PlanFallback emits runtime_value ValuePlans for error (nil + fmt.Errorf), chan T (nil + make(chan T), directional variants nil only), and func types (nil only, non-nil literal deferred). Wired into PlanParam after runtime-value registry, before complex_type UnsatisfiedRequirement. AC1 error nil+Errorf + AC2 chan int/string + AC3 func nil + AC4 mixed-support unsupported-chan-without-TypeName + AC5 rapid primitive/fallback mutual exclusion + regression: context.Context registry preserved, maxPlans cap, directional chan nil-only; 983 shatter-go tests + smoke pass
- Base branch rebased onto the primary branch.


### 2026-04-23T03:41:50Z — Started task
- Branch: `str-hy9b-24-h2-planner-wire`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-24-h2-planner-wire`.
- Base head at branch creation: `deea7009958f8a9c57903dc40bf76cb10c240578`.


### 2026-04-23T04:38:41Z — Closed task
- Branch: `str-hy9b-24-h2-planner-wire`.
- Outcome: `kept`.
- Summary: H2 Wire Go planner into explorer+orchestrator: protocol wire fixes (invocation_requirements / invocation_plans rename for Go parity + Go-shape response fixture test); planner.PlanRequirements entry point in shatter-go (free functions only, methods get NoConstructor); handleGetInvocationPlan in Go handler w/ PlannerFunc injection hook; get_invocation_plan advertised in CommandCapabilities; shatter-core/src/planner_consumer.rs materializes Literal+Zero ValuePlans (Random/Symbolic fall through); CLI --planner=go drops hard-error, primes task_frontend analyze cache, feeds planner seeds via ObserveStageOptions.extra_seeds (single call site; both explorer user_seeds and concolic seed_inputs consume the same channel, no parallel-path divergence); parity-matrix + registry updated. Gates: 991 go tests, 5 planner_consumer + 3 invocation_plan protocol tests, 17 e2e_concolic, parity+conformance pass, CLI E2E against ClassifyNumber returns 3 planner seeds affecting exploration.
- Base branch rebased onto the primary branch.


### 2026-04-23T17:33:25Z — Started task
- Branch: `str-hy9b-25-j1-retirement-inventory`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-25-j1-retirement-inventory`.
- Base head at branch creation: `d6e926472955174246ebc12d192c5dd55bbd9dc3`.


### 2026-04-23T19:16:25Z — Closed task
- Branch: `str-hy9b-25-j1-retirement-inventory`.
- Outcome: `kept`.
- Summary: J1 Retirement gate inventory: docs/specs/2026-04-23-go-harness-retirement-checklist.md enumerates every shatter-go file/test/env var/fixture/doc to remove or rewrite when legacy foreign-module direct-call harness retires. Grouped by J2 (code removal), J3 (CI regression gate), J4 (walkthrough/docs); each item names replacement path (launcher/builder/registry/overlay/planner). Structural finding: D4 launcher + D6 builder packages exist but not wired into protocol/handler.go — J2 must rewrite handler.go:366,625-639 against build.Builder+BinaryRegistry+launcher.Session before deletions are safe. Gates: 991 shatter-go tests pass; root test-standard trips on pre-existing environmental failure in shatter-core project::tests::returns_none_for_isolated_file (reproduces on clean main, not introduced by this change).
- Base branch rebased onto the primary branch.


### 2026-04-23T20:12:04Z — Started task
- Branch: `str-hy9b-26-j2-remove-legacy-direct-call-harness`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-26-j2-remove-legacy-direct-call-harness`.
- Base head at branch creation: `b045556a51f1f4c2c3cd45c929ae6edde6b6dee8`.


### 2026-04-23T22:45:00Z — Progress update
- Branch: `str-hy9b-26-j2-remove-legacy-direct-call-harness`.
- Summary: Rewired the protocol free-function `prepare`/`execute` path off `instrument.PrepareHarness` / `ExecuteFunctionWithTiming` onto launcher-backed cached programs. Added a protocol-local prepared-launcher cache with dead-session recovery, synthetic-module workspace bootstrapping, launcher-response -> `instrument.ExecuteResult` mapping, and handler timing phases for analyze/build/run. Broke the `protocol` -> `build` cycle by moving wrapper-only target/constructor types into `shatter-go/wrapper/`, and extended the launcher/runtime generation so standalone temp-file targets build cleanly (`go 1.23.0` launcher go.mod and self-contained generated target runtime). Remaining J2 scope: adapter hooks still route through `instrument.ExecuteHTTPHandler` / `ExecuteGinHandler`, `instrument` legacy executor code still exists, and wrapper/legacy-instrument retirement is not finished.
- Verification:
  - `go test ./protocol -run TestPrepareWithValidFileReturnsSuccess -count=1 -v`
  - `go test ./protocol -run TestExecuteRunsFunctionAndReturnsBranchData -count=1 -v`
  - `go test ./protocol -run 'Test(PrepareIsIdempotent|PrepareAndExecuteSucceeds|PreparedHarnessDeadProcessRecovery|ExecuteAutoLookupPreparedHarness|ExecuteWithStalePrepareIdFallsThrough|ExecuteEmitsTimingWhenRequested|ExecuteEmitsBuildFailedOutcome|ExecuteMethodTargetEmitsUnsupportedOutcome|ShutdownCleansUpPreparedHarnesses)' -count=1`
  - `go test ./protocol -run 'Test(HandleExecute_AdapterDispatch|HTTPHandler_Execute_Integration|GinHandler_Execute_Integration)' -count=1`
  - `go test ./wrapper ./build -count=1`
  - `go test ./build -run TestBuilderInstrumentedLauncherEmitsRecorderData -tags=integration -count=1`


### 2026-04-24T02:24:00Z — Progress update
- Branch: `str-hy9b-26-j2-remove-legacy-direct-call-harness`.
- Summary: Migrated both adapter hooks off `instrument.ExecuteHTTPHandler` / `ExecuteGinHandler` onto a new launcher-backed `protocol/adapter_launcher.go` path, including direct launcher tests for net/http and Gin handlers. Hardened launcher module builds for adapter-owned entrypoints by seeding `go.sum` from the target module and building generated launcher modules with `-mod=mod`. Deleted the now-dead legacy adapter harness implementations in `instrument/http_harness.go` and `instrument/gin_harness.go`, and updated `shatter-go/CLAUDE.md` to describe the launcher-backed prepare/adapter contracts. Remaining J2 scope: the broader `instrument.PreparedHarness` / legacy executor retirement is still open because protocol tests and compatibility glue still reference that concrete type.
- Verification:
  - `go test ./instrument ./launcher ./build ./wrapper -count=1`
  - `go test ./protocol -run 'Test(ExecuteAdapterViaLauncher_(HTTPHandler|GinHandler)|HTTPHandler_Execute_Integration|HTTPHandler_Execute_POST|GinHandler_Execute_Integration|GinHandler_Execute_WithRouteParams|HandleExecute_AdapterDispatch|PrepareWithValidFileReturnsSuccess|PrepareIsIdempotent|PrepareAndExecuteSucceeds|ExecuteRunsFunctionAndReturnsBranchData|PreparedHarnessDeadProcessRecovery|ExecuteAutoLookupPreparedHarness|ExecuteWithStalePrepareIdFallsThrough|ExecuteEmitsTimingWhenRequested|ExecuteEmitsBuildFailedOutcome|ExecuteMethodTargetEmitsUnsupportedOutcome|ShutdownCleansUpPreparedHarnesses)' -count=1`
  - `go vet ./...`


### 2026-04-24T02:28:35Z — Progress update
- Branch: `str-hy9b-26-j2-remove-legacy-direct-call-harness`.
- Summary: Removed the last protocol-side dependency on `*instrument.PreparedHarness` by switching `protocol/handler_test.go` to a local fake `preparedExecution`, then deleted the `PreparedHarness.Invoke` compatibility shim from `instrument/api.go`. That narrows the remaining legacy prepared-harness surface to `instrument/executor.go` and its own tests/benchmarks; protocol/runtime code no longer imports or injects the concrete type.
- Verification:
  - `go test ./instrument -count=1`
  - `go test ./protocol -run 'Test(HandleExecute_AdapterDispatch|ExecuteAdapterViaLauncher_(HTTPHandler|GinHandler)|HTTPHandler_Execute_Integration|HTTPHandler_Execute_POST|GinHandler_Execute_Integration|GinHandler_Execute_WithRouteParams|PrepareWithValidFileReturnsSuccess|PrepareIsIdempotent|PrepareAndExecuteSucceeds|ExecuteRunsFunctionAndReturnsBranchData|PreparedHarnessDeadProcessRecovery|ExecuteAutoLookupPreparedHarness|ExecuteWithStalePrepareIdFallsThrough|ExecuteEmitsTimingWhenRequested|ExecuteEmitsBuildFailedOutcome|ExecuteMethodTargetEmitsUnsupportedOutcome|ShutdownCleansUpPreparedHarnesses|PruneOrphansRemovesStaleEntries|PruneOrphansKeepsValidEntries|PruneOrphansIsIdempotent|ShutdownPrunesOrphansBeforeCleanup|LookupPreparedHarnessPrunesInvalid)' -count=1`
  - `go vet ./...`


### 2026-04-24T02:35:51Z — Progress update
- Branch: `str-hy9b-26-j2-remove-legacy-direct-call-harness`.
- Summary: Deleted the legacy prepared-harness implementation from `instrument/executor.go` (`PreparedHarness`, `PrepareHarness`, `ExecuteWithPreparedHarness`, and related lifecycle helpers) and removed the associated benchmark/cleanup tests from `instrument/executor_test.go`. The direct and adapter runtime paths are now fully launcher-backed; the remaining J2 retirement scope is the broader legacy temp-dir executor/instrumentation path (`ExecuteFunction`, `InstrumentFileWithTiming`, related tests/env knobs) and the still-live `wrapper/` package used by `build.Builder`.
- Verification:
  - `go test ./instrument -count=1`
  - `go test ./protocol -run 'Test(HandleExecute_AdapterDispatch|ExecuteAdapterViaLauncher_(HTTPHandler|GinHandler)|HTTPHandler_Execute_Integration|HTTPHandler_Execute_POST|GinHandler_Execute_Integration|GinHandler_Execute_WithRouteParams|PrepareWithValidFileReturnsSuccess|PrepareIsIdempotent|PrepareAndExecuteSucceeds|ExecuteRunsFunctionAndReturnsBranchData|PreparedHarnessDeadProcessRecovery|ExecuteAutoLookupPreparedHarness|ExecuteWithStalePrepareIdFallsThrough|ExecuteEmitsTimingWhenRequested|ExecuteEmitsBuildFailedOutcome|ExecuteMethodTargetEmitsUnsupportedOutcome|ShutdownCleansUpPreparedHarnesses|PruneOrphansRemovesStaleEntries|PruneOrphansKeepsValidEntries|PruneOrphansIsIdempotent|ShutdownPrunesOrphansBeforeCleanup|LookupPreparedHarnessPrunesInvalid)' -count=1`
  - `go vet ./...`


### 2026-04-24T03:54:18Z — Progress update
- Branch: `str-hy9b-26-j2-remove-legacy-direct-call-harness`.
- Summary: Tightened the launcher-backed runtime by adding session-level execution timeouts (`launcher/session.go` + `protocol/prepared_launcher.go`) so launcher executes now classify timed-out subprocesses the same way the old executor did. Removed the last production call into the legacy global harness cache (`instrument.CloseAllHarnesses()` from `protocol/handler.go`). The task branch now has two pushed commits on top of the prior J2 work: `str-kzxt: remove legacy prepared harness path` and `str-kzxt: enforce launcher execution timeouts`. Remaining J2 scope is now clearly the larger retirement checklist: the old temp-dir executor/instrumentation entrypoints (`ExecuteFunction`, `InstrumentFileWithTiming`, related env knobs/tests) and the still-live `wrapper/` package used by `build.Builder`.
- Verification:
  - `go test ./instrument -count=1`
  - `go test ./protocol -run 'Test(ShutdownCleansUpPreparedHarnesses|HandleExecute_AdapterDispatch|ExecuteAdapterViaLauncher_(HTTPHandler|GinHandler)|HTTPHandler_Execute_Integration|HTTPHandler_Execute_POST|GinHandler_Execute_Integration|GinHandler_Execute_WithRouteParams|PrepareWithValidFileReturnsSuccess|PrepareIsIdempotent|PrepareAndExecuteSucceeds|ExecuteRunsFunctionAndReturnsBranchData|PreparedHarnessDeadProcessRecovery|ExecuteAutoLookupPreparedHarness|ExecuteWithStalePrepareIdFallsThrough|ExecuteEmitsTimingWhenRequested|ExecuteEmitsBuildFailedOutcome|ExecuteMethodTargetEmitsUnsupportedOutcome|ShutdownPrunesOrphansBeforeCleanup|LookupPreparedHarnessPrunesInvalid)' -count=1`
  - `go vet ./...`


### 2026-04-24T06:05:00Z — Progress update
- Branch: `str-hy9b-26-j2-remove-legacy-direct-call-harness`.
- Summary: Removed the embedded `instrument/harness_runtime_source.go` bridge and switched `EnsureHarnessRuntimeDir()` to resolve the checked-in `shatter-go/harness` module directly. This preserves the live launcher/adapter `shatter-harness` import path but deletes another legacy runtime duplication layer and its dedicated test file. Remaining J2 scope is still the larger retirement checklist: old temp-dir executor/instrumentation entrypoints (`ExecuteFunction`, `InstrumentFileWithTiming`, env-var cache/scratch handling, broad `instrument/*` legacy tests) plus the still-live `wrapper/` package used by `build.Builder`.
- Verification:
  - `go test ./instrument -run 'Test(EnsureHarnessRuntimeDirPointsAtCheckedInModule|GenerateLoopMockFileUsesAtomicCounters)' -count=1`
  - `go test ./build -run TestBuilderInstrumentedLauncherEmitsRecorderData -tags=integration -count=1`
  - `go test ./protocol -run TestPrepareAndExecuteSucceeds -count=1 -timeout 120s`
  - `go test ./protocol -run 'TestExecuteAdapterViaLauncher_(HTTPHandler|GinHandler)' -count=1 -timeout 120s`
  - `go vet ./instrument ./build ./protocol`


### 2026-04-24T06:40:00Z — Progress update
- Branch: `str-hy9b-26-j2-remove-legacy-direct-call-harness`.
- Summary: Pruned more production-dead legacy executor surface. Removed `CloseAllHarnesses()` from `instrument/executor.go` now that no non-test caller remains, moved cache cleanup into a test-local helper in `executor_test.go`, dropped the `overlay_test.go` legacy-tempdir parity assertion that compared overlay output against `InstrumentFile`, and deleted the fully unused `instrument/entropy.go` utility plus its test file. This reduces J2’s remaining work to live legacy behaviors rather than dead helpers and legacy-only test comparisons.
- Verification:
  - `go test ./instrument -run 'Test(PersistentHarnessWarmReuse|PersistentHarnessResultsDoNotAccumulate|PersistentHarnessCrashRecovery|ExecuteFunctionModuleBackedWithSiblingHelper|ExecuteFunctionModuleBackedWithIntraModuleImport|InstrumentPackageForOverlay_FileSelection|RegisterInstrumentedOverlay_WritesManifestEntries|InstrumentPackageForOverlay_PropertyValidGoWithBranchRecorders)' -count=1`
  - `go test ./build ./protocol -run '^$' -count=1`
  - `go vet ./instrument ./build ./protocol`


### 2026-04-24T07:20:00Z — Progress update
- Branch: `str-hy9b-26-j2-remove-legacy-direct-call-harness`.
- Summary: Removed the last production `protocol` callsite of `instrument.InstrumentFileWithTiming`. `handleInstrument()` now resolves a workspace, creates a workspace-backed generated output directory, and materializes the instrumented source/recorder/go.mod via the new `instrument.MaterializeInstrumentedDirectory()` helper. `prepared_launcher.go` now shares a smaller `ensureWorkspace()` helper so both instrument/prepare paths use the same workspace bootstrap. This makes `instrument.go` temp-dir entrypoints production-dead; they now survive only for direct legacy tests.
- Verification:
  - `go test ./protocol -run 'Test(InstrumentWithValidFileReturnsSuccess|ExecuteAfterInstrumentWithoutAnalyze|PrepareWithValidFileReturnsSuccess|PrepareAndExecuteSucceeds)' -count=1 -timeout 120s`
  - `go test ./instrument ./build ./protocol -run '^$' -count=1`
  - `go vet ./instrument ./build ./protocol`


### 2026-04-24T09:05:00Z — Progress update
- Branch: `str-hy9b-26-j2-remove-legacy-direct-call-harness`.
- Summary: Deleted `shatter-go/instrument/instrument.go` entirely. The remaining directory-materialization logic now lives in `instrument/materialize.go`, `executor.go` uses that internal helper directly, and `instrument_test.go` targets `MaterializeInstrumentedDirectory()` instead of the removed temp-dir convenience wrappers. This retires the old `InstrumentFile*` API surface completely; `protocol` and `instrument` now share the same directory materialization path.
- Verification:
  - `go test ./instrument -run 'Test(InstrumentSimpleIfElseCompiles|InstrumentAndRunClassify|InstrumentMainOnlyImportPreserved|InstrumentFileNotFound)' -count=1 -timeout 120s`
  - `go test ./build ./protocol -run '^$' -count=1`
  - `go vet ./instrument ./build ./protocol`
  - `go test ./...`
  - `go vet ./...`


### 2026-04-25T03:09:30Z — Closed task
- Branch: `str-hy9b-26-j2-remove-legacy-direct-call-harness`.
- Outcome: `kept`.
- Summary: J2: trim legacy direct-call harness from executor.go (1666 → 364 lines); migrate MC/DC integration tests to launcher.Session under shatter-go/build
- Base branch rebased onto the primary branch.


### 2026-04-25T03:34:34Z — Started task
- Branch: `str-hy9b-27-g3-hint-config-v1-expansion`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-27-g3-hint-config-v1-expansion`.
- Base head at branch creation: `f38ff4d6d370c99521b5890251ee7219ca22c012`.


### 2026-04-25T03:34:38Z — Started task
- Branch: `str-hy9b-28-h5-planner-driven-e2e`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-28-h5-planner-driven-e2e`.
- Base head at branch creation: `aef66b228ec8dc209c7973cd9d8d976a23fd077c`.


### 2026-04-25T17:26:23Z — Closed task
- Branch: `str-hy9b-27-g3-hint-config-v1-expansion`.
- Outcome: `kept`.
- Summary: G3: hint_config_v1 expansion beyond policy.allow — loader parses defaults/mocks/generators with unknown-key warns; planner consumes defaults via PerTargetHints.Defaults precedence and generators via runtime-values registry; mocks emitted as deterministic MockSpec artifacts (substitution at execute time deferred to str-8v66). 7 loader tests + 11 planner tests including 2 rapid property tests. Pre-existing cli:clippy failure unrelated (str-7syr).
- Base branch rebased onto the primary branch.


### 2026-04-26T18:48:08Z — Closed task
- Branch: `str-hy9b-28-h5-planner-driven-e2e`.
- Outcome: `kept`.
- Summary: H5: Planner-driven E2E for method-receiver dispatch. `shatter-core/tests/e2e_concolic.rs` now drives a real analyze → plan → execute roundtrip against `examples/go/service-method` via the Go frontend subprocess. Additive `Command::Execute.plan: Option<InvocationPlan>` and `ExecuteResult.outcome: Option<InvocationOutcome>` on the Rust side. Go launcher dispatches method targets through the wrapper's receiver-kind switch driven by `plan.receiver_kind`; method targets without a plan now emit `runtime_failed` with `short_reason` containing "unknown receiver kind", retiring the pre-H5 `unsupported`/`method_not_supported` capability rejection. Tracked divergence widened in `protocol/parity-matrix.yaml::ts-rust-execute-plan-not-implemented`. Followups str-yi9y (orchestrator/explorer Execute.plan wiring) and str-oegu (Prepare-path receiver_kind support) filed P2 blocked-by str-ekjh. Pre-existing cli:clippy failure unrelated (str-7syr); branch rebased onto str-hy9b before merge to incorporate G3 work, two real conflicts in `shatter-go/planner/plan.go` plus signature adapter in `main.go` resolved by combining both feature sets.


### 2026-04-26T19:58:01Z — Started task
- Branch: `str-hy9b-29-h2-execute-plan-wiring`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-29-h2-execute-plan-wiring`.
- Base head at branch creation: `5d9360e718c77027f14a0bd0f4569f810e464788`.


### 2026-04-26T21:39:48Z — Closed task
- Branch: `str-hy9b-29-h2-execute-plan-wiring`.
- Outcome: `kept`.
- Summary: str-yi9y + str-oegu: wire InvocationPlan through Execute and Prepare paths. default_execute_plan propagates from ObserveStageOptions into both explorer and concolic orchestrator ExploreConfig; all Command::Execute calls carry the plan so method targets dispatch via constructor. Command::Prepare gains optional plan field; computePrepareID keys on receiver_kind. New E2E test go_method_planner_driven_via_orchestrator (orchestrator path AC3). Two Rust round-trip tests for plan-bearing Prepare (AC4). CLAUDE.md Prepare Parity Contract updated (AC5).
- Base branch rebased onto the primary branch.


### 2026-04-26T21:54:29Z — Started task
- Branch: `str-hy9b-30-a5-outcome-parity-ts-rust`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-30-a5-outcome-parity-ts-rust`.
- Base head at branch creation: `643b982521c42f34e218157847125750ea6d53f9`.


### 2026-04-26T21:54:36Z — Started task
- Branch: `str-hy9b-31-j1-retirement-gate-script`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-31-j1-retirement-gate-script`.
- Base head at branch creation: `84ec90f7ad19f975273a08ba0ae11cd6bed5cea0`.


### 2026-04-27T01:28:33Z — Closed task
- Branch: `str-hy9b-30-a5-outcome-parity-ts-rust`.
- Outcome: `kept`.
- Summary: str-hy9b.A5: outcome parity for TS and Rust frontends. TS deriveOutcome and Rust derive_execute_outcome/error_outcome emit completed/runtime_failed/timed_out. New conformance cases execute_outcome_shape_ts/_rust; 23/23 checks pass. Outcome Emission Contract documented in shatter-ts/CLAUDE.md and shatter-rust/CLAUDE.md.
- Task merged into base branch.


### 2026-04-27T01:42:03Z — Closed task
- Branch: `str-hy9b-31-j1-retirement-gate-script`.
- Outcome: `kept`.
- Summary: str-hy9b.J1: scripts/go-frontend-retirement-gate.sh retirement gate script. Probes gauntlet (analyze ×3), D7 spike (explore), net/http G1 adapter (go test ./protocol/). All 5 fixtures PASS. RESULT: PASS.
- Task merged into base branch.


### 2026-04-27T01:42:58Z — Started task
- Branch: `str-hy9b-32-j4-retirement-validation-gates`.
- Worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-32-j4-retirement-validation-gates`.
- Base head at branch creation: `41bc91bd13152edf05a487919ecf3effaacfc1f6`.


### 2026-04-27T02:24:50Z — Closed task
- Branch: `str-hy9b-32-j4-retirement-validation-gates`.
- Outcome: `kept`.
- Summary: str-hy9b.J4: all retirement validation gates pass. Fixed multi-return Go wrapper codegen bug (WrapperTarget.ResultCount, blank-identifier emission, build.rs watches wrapper/). Parity 12/12, conformance 23/23, E2E 19/19, smoke, gauntlet 62/62, walkthrough all pass.
- Task merged into base branch.


## RESUME HERE
### 2026-04-26T21:45:00Z — Closed task
- Branch: `str-hy9b-29-h2-execute-plan-wiring`.
- Outcome: `kept`.
- Summary: str-yi9y + str-oegu: wire InvocationPlan through Execute and Prepare paths. All Command::Execute calls in explorer.rs and orchestrator.rs now use config.default_execute_plan (set from fetch_planner_extra_seeds's first plan via ObserveStageOptions.execute_plan). Command::Prepare gains optional plan field; computePrepareID/lookupPreparedHarness/handleExecute key on receiver_kind so plan-aware callers pre-build the right wrapper case. New E2E test go_method_planner_driven_via_orchestrator validates orchestrator path (str-yi9y AC3). Two Rust round-trip tests for plan-bearing Prepare (str-oegu AC4). shatter-go/CLAUDE.md Prepare Parity Contract updated (str-oegu AC5). 2886 Rust unit tests pass; Go prepare tests pass; pre-existing TS E2E and slow Go tests unaffected.
- Base branch rebased onto the primary branch.

## RESUME HERE
<!-- expedition-resume:start -->
- Expedition: `str-hy9b`
- Status: `ready_for_task`
- Base branch: `str-hy9b`
- Base worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b`
- Active task branch: `none`
- Active task worktree: `none`
- Last completed: `str-hy9b-32-j4-retirement-validation-gates (kept)`
- Next action: Create the next task branch from the expedition base branch.
<!-- expedition-resume:end -->
