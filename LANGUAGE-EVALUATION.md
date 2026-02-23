# Language Evaluation for Shatter's Core Engine

This document evaluates candidate languages for the language-agnostic core of Shatter v2
(the orchestrator, constraint solver interface, invariant detector, and report generator).

## Quick Elimination: Languages That Don't Make the Cut

**C** — No property-based testing ecosystem to speak of (`theft` is minimal). Manual memory management for tree-structured constraint data is error-prone and slow to develop. No high-level JSON handling. Subprocess management requires significant boilerplate. The only advantage is raw speed and native Z3 access, but Rust and C++ offer both without the downsides.

**C++** — Better than C (has `z3++.h` native API, `nlohmann/json`, `RapidCheck`), but the CLI/distribution story is poor, subprocess management is clunky even with Boost.Process, and development velocity is low for the orchestration/IPC-heavy parts of this system. Semi-maintained PBT library.

**Java** — JVM startup of 100-500ms hurts CLI UX. 12-16 bytes overhead per object inflates memory for large execution record sets. Distribution requires JRE or GraalVM native-image (adds complexity). That said, it has excellent concurrency (Virtual Threads), the best multi-solver SMT library (JavaSMT), and solid PBT (jqwik). Loses to Kotlin on ergonomics for the same runtime.

**Python** — The GIL is a fundamental problem for parallel constraint solving and concurrent test execution. 30-100x slower than C for computation. 56+ bytes per object means ~1-2 GB for 100K execution records vs ~50 MB in Rust. Distribution is fragile. Hypothesis is excellent but can't compensate for the runtime limitations. Best Z3 bindings of any language, but that advantage is narrow.

**Kotlin** — Essentially "better Java" — same JVM startup penalty, same JNI overhead for Z3, same distribution problem. The coroutines model is elegant, and KSMT gives access to Z3+CVC5+Bitwuzla through one API. But it doesn't offer enough over the top three to justify the JVM baggage for a CLI tool.

## Notable Omission: OCaml

OCaml deserves mention as a strong candidate the original list didn't include. Its algebraic data types and pattern matching are nearly purpose-built for representing symbolic expression trees and constraint manipulation. The original Z3 was prototyped in OCaml. `semgrep`'s core engine is OCaml. Native compilation produces fast binaries (~2-4x C) with ~5ms startup. However, the ecosystem for CLI tooling, JSON handling, and cross-platform distribution is significantly smaller than Rust or Go, and the hiring pool is narrow. **Strong for the symbolic engine specifically, weaker as a general-purpose tool platform.**

## Other Languages Considered and Dismissed

- **F# (.NET)** — Similar ADT/pattern matching benefits as OCaml. Access to .NET ecosystem. But .NET runtime dependency makes binaries large even with AOT. No compelling advantage over Rust or OCaml for this workload.
- **Zig** — C-level performance, excellent C interop (trivial Z3 integration). But not yet at 1.0, immature ecosystem, no algebraic data types. Premature for a production tool.
- **Nim** — Python-like syntax with C-level performance. But very small community, limited library support, bus factor concerns.
- **Swift** — Good performance, strong type system with enums/pattern matching. But Linux support is secondary and distribution outside macOS is awkward.

---

## Top Three Candidates vs TypeScript

### 1. Rust

| Dimension | Rust | TypeScript (Node.js) |
|-----------|------|---------------------|
| **Computation speed** | 1.0-1.2x C | 3-10x slower than C |
| **Memory per 100K records** | ~50 MB | ~500-800 MB |
| **Z3 integration** | `z3` crate (C FFI, thin wrapper, actively maintained) | `z3-solver` npm (WASM, ~2x overhead vs native) |
| **Parallelism** | `rayon` data parallelism + `tokio` async; shared memory between threads | worker_threads require full serialization of shared data |
| **JSON perf** | `serde_json` at 600-800 MB/s | V8 `JSON.parse` at 200-400 MB/s |
| **CLI startup** | 1-5ms | 30-80ms (+ TS compiler load) |
| **Distribution** | Single static binary, <10 MB, trivial cross-compilation | Requires Node.js or 50-80MB bundled binary |
| **Cross-lang AST** | tree-sitter (native!) + SWC for TS/JS parsing at native speed | TS Compiler API (only for TS); tree-sitter bindings exist |
| **PBT / value gen** | `proptest` — mature, `#[derive(Arbitrary)]`, good shrinking | `fast-check` — mature, composable arbitraries |
| **Dev velocity** | Moderate — ownership model has a learning curve | High — familiar, dynamic, fast iteration |
| **Type system for SymExpr** | Enums with data are zero-cost, exhaustive pattern matching | Discriminated unions work but are runtime-checked |

**Rust's case:** It is the strongest technical choice by a wide margin. The `SymExpr` type from the plan maps directly to a Rust enum with zero-cost pattern matching. SWC means the Rust core could parse and transform TypeScript **without subprocess overhead**, collapsing the TypeScript frontend into a library call. Z3 runs at native speed via C FFI. The 10-20x memory advantage over Node.js means dramatically larger explorations. `rayon::par_iter()` makes parallel invariant detection trivial with shared memory.

**Rust's weakness:** Development velocity. The ownership model adds friction, especially for the tree-structured, graph-heavy data (constraint trees, call graphs, execution records with shared references). A team familiar with Rust would be fine; a team that isn't will lose weeks to the borrow checker.

### 2. Go

| Dimension | Go | TypeScript (Node.js) |
|-----------|-----|---------------------|
| **Computation speed** | 2-4x slower than C | 3-10x slower than C |
| **Memory per 100K records** | ~150 MB | ~500-800 MB |
| **Z3 integration** | `aclements/go-z3` (CGo, stale, ~200ns/call overhead) or subprocess | `z3-solver` npm (WASM, actively maintained) |
| **Parallelism** | Goroutines + channels; trivial, lightweight (~4KB/goroutine) | worker_threads with serialization overhead |
| **JSON perf** | `encoding/json` at 50-100 MB/s (or `sonic` at 300-500 MB/s) | V8 `JSON.parse` at 200-400 MB/s |
| **CLI startup** | 1-5ms | 30-80ms |
| **Distribution** | Single static binary, trivial cross-compilation (`GOOS/GOARCH`) | Requires Node.js or large bundled binary |
| **Cross-lang AST** | Native `go/ssa`+`go/types` for Go; tree-sitter bindings for others | TS Compiler API for TS; nothing native for Go |
| **PBT / value gen** | `rapid` — good, actively maintained, generics-aware | `fast-check` — mature, composable |
| **Dev velocity** | High — simple language, fast compilation | High — familiar, dynamic |
| **Type system for SymExpr** | Interfaces + type switches; not as clean as Rust enums or TS unions | Discriminated unions work well |

**Go's case:** The pragmatic choice. Excellent concurrency primitives, instant startup, single-binary distribution, and the Go frontend for analyzing Go code could share types and libraries with the core (no IPC needed for Go-on-Go analysis). Development velocity is high. The language is simple enough that contributions from a wider team are feasible.

**Go's weakness:** Z3 integration is the Achilles' heel. The bindings are stale and CGo adds real overhead (~200ns per Z3 API call, which adds up when building large constraint trees). The stdlib `encoding/json` is surprisingly slow (Node.js's `JSON.parse` is actually faster). The type system lacks sum types, making `SymExpr` representation less elegant (interface + type switch, no exhaustiveness checking). Go is 2-4x slower than Rust for the computation-heavy parts.

### 3. Kotlin (with GraalVM native-image)

| Dimension | Kotlin (native-image) | TypeScript (Node.js) |
|-----------|----------------------|---------------------|
| **Computation speed** | 1.5-3x slower than C (JIT), ~2-5x as native | 3-10x slower than C |
| **Memory per 100K records** | ~300 MB (JVM) | ~500-800 MB |
| **Z3 integration** | KSMT — best multi-solver API (Z3+CVC5+Bitwuzla+Yices2) | `z3-solver` npm (Z3 only via WASM) |
| **Parallelism** | Coroutines + `Dispatchers.Default`; structured concurrency | worker_threads with serialization |
| **JSON perf** | `kotlinx.serialization` at 150-300 MB/s | V8 `JSON.parse` at 200-400 MB/s |
| **CLI startup** | 10-30ms (native-image) or 100-500ms (JVM) | 30-80ms |
| **Distribution** | Native-image: single binary ~30-50MB; JVM: requires JRE | Requires Node.js or large bundled binary |
| **Cross-lang AST** | ANTLR (grammar-based, many languages); tree-sitter (JNI) | TS Compiler API |
| **PBT / value gen** | Kotest property testing or jqwik (via JVM) | `fast-check` |
| **Dev velocity** | High — modern language, good tooling | High |
| **Type system for SymExpr** | Sealed classes — exhaustive `when` matching, very clean | Discriminated unions |

**Kotlin's case:** The dark horse. KSMT gives the best SMT solver story of any language — one API, four solvers, actively maintained by JetBrains Research. Sealed classes model `SymExpr` almost as well as Rust enums, with exhaustive `when` matching. Coroutines provide elegant structured concurrency. The JVM's JIT compilation means hot loops (invariant detection over large specimen sets) eventually reach near-native speed. GraalVM native-image solves the startup problem.

**Kotlin's weakness:** GraalVM native-image adds build complexity and doesn't support all JVM features (e.g., some reflection patterns). JNI overhead for Z3 is small but nonzero. Memory overhead is 2-6x worse than Rust. The distribution story, while improved by native-image, is still heavier than Go or Rust. The ecosystem for developer CLI tools is smaller — most Kotlin tooling targets Android or server-side JVM.

---

## Comparative Summary

| | Rust | Go | Kotlin | TypeScript |
|---|---|---|---|---|
| **Best at** | Performance, memory, Z3 native, cross-lang AST (SWC+tree-sitter), distribution | Concurrency, simplicity, Go analysis (`go/ssa`), distribution, dev velocity | Multi-solver SMT (KSMT), sealed classes, structured concurrency, JIT warmup | Existing codebase, TS Compiler API, Z3 WASM just works, fast iteration |
| **Worst at** | Learning curve, dev velocity | Z3 integration (CGo), type system (no sum types), JSON perf | JVM startup (fixable), memory, distribution weight | Computation speed, memory (10-20x Rust), parallelism (serialization), distribution |
| **Risk** | Team needs Rust expertise | Z3 bindings are stale; may need subprocess workaround | GraalVM native-image complexity; smaller CLI ecosystem | Won't scale to large codebases (memory/perf ceiling) |

---

## Full Scorecard (All Languages)

| Dimension | Python | Go | Rust | C | C++ | Java | Kotlin | Node.js (TS) |
|-----------|--------|-----|------|---|-----|------|--------|-------------|
| Raw computation | Very poor | Good | Excellent | Excellent | Excellent | Good | Good | Moderate |
| Subprocess/IPC | Poor | Excellent | Excellent | Good | Good | Poor (JVM startup) | Poor (JVM) | Moderate |
| Memory efficiency | Very poor | Good | Excellent | Excellent | Excellent | Moderate | Moderate | Poor |
| Concurrency | Poor (GIL) | Excellent | Excellent | Good (manual) | Good (manual) | Good | Good | Moderate |
| Startup time | Poor | Excellent | Excellent | Excellent | Excellent | Poor | Poor | Moderate |
| JSON perf | Poor-Good | Moderate | Excellent | Excellent | Excellent | Good | Good | Good |
| CLI ecosystem | Good | Excellent | Excellent | Poor | Poor | Moderate | Moderate | Good |
| Dev velocity | Excellent | Excellent | Moderate | Poor | Poor | Good | Good | Excellent |
| Distribution | Poor | Excellent | Excellent | Moderate | Moderate | Poor | Poor | Moderate |
| Z3 integration | Good (pip) | Moderate (CGo) | Good (C FFI) | Native | Native | Moderate (JNI) | Moderate (JNI) | Good (WASM) |
| PBT libraries | Excellent (Hypothesis) | Good (rapid) | Good (proptest) | Poor (theft) | Poor (RapidCheck) | Good (jqwik) | Good (Kotest) | Good (fast-check) |
| Cross-lang AST | Good (tree-sitter) | Good (native Go + tree-sitter) | Excellent (tree-sitter + SWC) | Good (tree-sitter native) | Good (tree-sitter + libclang) | Good (ANTLR) | Good (ANTLR) | TS only (Compiler API) |

---

## Recommendation

**If the goal is a production-grade tool that scales:** Rust. The 10-20x memory advantage and native Z3 + SWC mean it can handle large codebases that would exhaust Node.js's heap. The investment in Rust expertise pays off in a tool that's fast, small, and distributable as a single binary.

**If the goal is fastest path to a working prototype:** Stay with TypeScript for now, with the architecture designed so the core can be rewritten later. The JSON-over-stdio protocol between core and frontends means the core is swappable without changing the frontends.

**If the team knows Go well:** Go is a reasonable middle ground — 3-5x better than TypeScript on memory/speed, excellent distribution, but plan to use Z3 via subprocess (SMT-LIB2 text protocol) rather than fighting CGo bindings.

### Hybrid Architecture (Recommended Long-Term)

Invert the current plan (TypeScript core calling Go subprocess) to:

1. **Core engine in Rust**: orchestrator, constraint management, invariant detection, CLI, Z3 integration (via native C API bindings)
2. **TypeScript frontend as a subprocess**: analyzer, instrumentor, executor — communicates with the Rust core via the JSON stdio protocol
3. **Go frontend as a subprocess**: same protocol

Benefits of this inversion:
- The engine's hot loops run at native speed
- Memory-efficient storage of execution records means larger explorations
- CLI startup is instant (~2ms)
- Distribution is a single binary (the Rust core) plus language frontends installed as needed
- Adding new language frontends (Java, Rust-frontend, Python-frontend) is symmetric — none has a privileged position

Cost:
- Z3 integration via C API bindings is more work than `z3-solver` npm (but more reliable and faster)
- The team needs Rust expertise
- TypeScript frontend loses the "zero-cost" IPC advantage and must use the JSON protocol
