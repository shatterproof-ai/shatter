# Revisions: shatter-int-width-signedness

Revision 1 created the epic at the maintainer's request (2026-09-24). Revision 2 applied the
first-round Codex review, `crosscheck/shatter-int-width-signedness.codex-r1.md`, which fixed the
hierarchy and step numbering, narrowed the core issue's widths, and added the alias release marker.

## Revision 3

This revision applies `crosscheck/shatter-int-width-signedness.codex.md` (round 2). Code facts were
re-verified against the audit worktree at 16794cef. New baselines were recorded on 2026-09-24 with
fresh debug builds of `shatter`, `shatter-rust` and `shatter-go` from that tree. The fixtures are
the ones now given inline in the drafts.

| Codex finding | Change |
|---|---|
| MAJOR: the epic bundles independent work and an unrelated reporting fix | `15-rust-input-deserialize-classification` loses `parent_slug`; its `parent_epic` is back to "Epic: Audit 2026-09-22 findings". The epic lists it only under "Related, not a child". "Done when" covers only `int-unsigned64-clamp`, `core-int-range-i128` and `go-int-width-sign-emission`. The dry run now files 15 under `DRY:epic:shatter`. |
| MAJOR: the i128 issue has conflicting range and value requirements | `02` has a single **Contract** section: the `IntRange { min: i128, max: i128 }` carrier; exact ranges for every width of 64 bits or less (`u64` = `[0, u64::MAX]`, `i64` full); 128-bit types clamped to their 64-bit counterparts and documented. It lists the paths that widen to i128: generate, mutate, shrink, shape check, boundary seeding, the Z3 param range assertion, `extract_concrete_values`, `ConcreteValue::Int` and `concrete_to_json`. It lists the paths that stay i64: `ConstValue::Int` and `LiteralValue::Int`. It states the consequence that a `u64::MAX` source literal is reached through seeds, not Z3. The contradictory "model extraction returns i128" versus "`ConstValue` stays i64" wording is gone. |
| MAJOR: the first child's tests do not prove width bounds | `01` has a per-row expected-bounds table (`u8`..`u32` exact; `u64`/`usize`/`u128` → `[0, i64::MAX]`; `i64`/`i128`/`isize` and unspecified → full i64). It names 7 verified entry points with line numbers: generation `:128`/`:460`/`:478`, shape check `:225`, mutation `:1955`/`:2107`, shrink `:3576`/`:3596`, literal seeding `:4020`, boundary seeding `boundary_dict.rs:54`/`:109` and its callers, and the solver `solver.rs:172` with its 3 callers. Table-driven tests assert every generated or solved value is within bounds. **New finding while verifying:** boundary seeding never consults `int_range`, so `u8` gets `-1`/`-2` today. `01`'s title and scope now include it. |
| MAJOR: acceptance depends on unavailable or untracked artifacts | The external `shatter-examples` fixture and the untracked audit transcript are no longer referenced. New repo-local fixtures are given inline: `examples/rust/int-width/{Cargo.toml,src/lib.rs}` (01, extended by 02) and `examples/go/int-width/{go.mod,widths.go}` (03). These use the existing `repo_examples_rust_dir()`/`repo_examples_go_dir()` helpers, so no `SHATTER_EXAMPLES_DIR` is needed. The drafts also give inline artifact bounds checkers (`scripts/check_int_width_bounds.py`, `scripts/check_go_int_width_bounds.py`), exact build, explore and check commands (copying the fixture to a temp dir so explore does not write into the repo), and the expected observable. Recorded baselines: Rust 41/360 executed inputs out of bounds; Go 80/360; `above_i64_max` never reaches `upper-half`. The Rust and Go E2E commands are given per test. |
| MAJOR: E2E criteria conflate generation correctness with error classification | Every integer-epic proof now asserts on input values (`raw_results` inputs, or checker output on artifacts), never on "deserialization failed" rows or report text. `15` has its own independent proof: an explicit `["en", -1]` seed, harness-, report- and E2E-level tests, and the nine `executor.rs` sites that hard-code `runtime_error`. That proof keeps working after the generator fix. |
| MINOR: the Go alias criterion assumes release coordination outside the issue | `03` no longer has a release-dependent close condition. It closes on its own criteria and must only file the follow-up. The new `04-go-uint-alias-removal.md` (P3, `parent_slug: int-width-signedness-epic`, `blocked_by: [go-int-width-sign-emission]`) removes the aliases once one `continuous-*` release containing 03's merge commit is at least 30 days old. It has an exact `gh release list` plus `git merge-base --is-ancestor` verification script (the tag format and retention come from `release.yml` and `cleanup-continuous-releases.yml`), the full list of alias sites, a grep-clean criterion and a rejection test. `03` also now names every alias consumer: the planner, the handler capability, export, the registry, the handshake golden and the generated bindings. |
| MINOR: duplication and status not verifiable | The epic has a tracker table with ids checked by `bd show` on 2026-09-24: str-ddxe (closed), str-cfsa (closed), str-ieuc (closed), str-79nvf (closed), str-4yc9w (open, started), str-qwua7.14 (closed). It records the duplicate searches (`int_range`, `int_width`, `int8`, `i128`, `go_uint`, `boundary_dict`, `input_error`: no hits; `unsigned`/`usize`: only the closed predecessors). Each child repeats the relevant ids. |

Other changes in this revision:

- `03`'s title now says the aliases are demoted, not retired.
- The epic's wording about who creates the audit parent epic was changed so that it no longer
  triggers a manual-follow-up warning in the dry run.
- The `15` section of `shatter-frontend-rust/BUNDLE.md` was refreshed to match the revised draft.
- `DRY_RUN=1 bash file-all.sh` reports "dry run clean". The 5 warnings are the pre-existing ones
  from other buckets. The epic and its four children file under `DRY:int-width-signedness-epic`,
  with dependencies 01 ← 02 ← 03 ← 04.

## Revision 5

This revision applies `crosscheck/shatter-int-width-signedness.codex.md` (the round-4 review,
2026-09-24). Code facts were re-checked against the audit checkout; no non-audit file differs
between 16794cef and its HEAD, so the 16794cef line numbers still hold.

| Codex finding | Change |
|---|---|
| MAJOR: the epic's child relationships contradict each other | `00` now says its children are exactly the drafts with `parent_slug: int-width-signedness-epic` (01-04). `go-uint-alias-removal` is described everywhere as a follow-up parented to the audit epic; the "each names this epic" wording that included it is gone, and its own intro no longer calls it a step of the epic. "Done when" lists all four children and says the epic closes without the follow-up. |
| MAJOR: the Go issue bundles a migration across several surfaces | The old `03-go-int-width-sign-emission` is split in two, each with its own owner and proof. New `03-core-go-alias-int-range` (shatter-core, P3, S, blocked by `core-int-range-i128`): the core treats incoming `go_uint`/`go_byte` TypeInfo as `Int { 64, false }`/`Int { 8, false }` on the `int_range` path, with no Go change; proof is a golden compatibility test on today's `shatter-go` handshake and analyze payloads plus a bounds proptest and a boundary unit test. `04-go-int-width-sign-emission` (shatter-go, P2, M upper end, now blocked by `core-go-alias-int-range`): analyzer emission for every Go integer kind plus planner, handler, handshake, registry, parity-matrix, conformance, SPEC §8 deprecation row, fixture, checker, E2E, and default and `--concolic` reruns; no core source changes. |
| MINOR: a superseded Rust draft remains in the audit materials | `00` gains a "Superseded material" section: the `drafts/` tree is superseded (SUPERSEDED banners on each `drafts/*/INDEX.md`; nothing there is filed), and `drafts/shatter-code/82-rust-usize-negative-inputs.md` is replaced by `int-unsigned64-clamp`. `01`'s References say the same. |

Other changes in this revision:

- Files renumbered: `03-go-int-width-sign-emission.md` → `04-…`, `04-go-uint-alias-removal.md` →
  `05-…`. Slugs are unchanged. `05` stays `blocked_by: [go-int-width-sign-emission]` and now also
  removes the core normalization and replaces its golden test with a rejection test.
- Slug references updated: `01` (out of scope now names `core-go-alias-int-range` for the Go
  aliases) and `02` (the `go_uint` note now points at `core-go-alias-int-range`). No other draft
  outside this bucket references the Go slugs. `DECISIONS-2026-09-23.md` D11 is a historical
  record and was left as written.
- Facts verified for the split, 2026-09-24: today's `shatter-go` (audit checkout
  `shatter-go/bin/shatter-go`, protocol `0.1.0`) emits `go_uint` for `uint64`, `go_byte` for
  `byte`, `array` of `go_byte` for `[]byte` and bare `int` for `int8`, and its handshake declares
  `complex_type:go_byte`. No request schema carries TypeInfo, so core-side normalization is
  invisible to the Go planner. The Go `TypeInfo` struct (`shatter-go/protocol/types.go:329`) has
  no width/sign fields yet. `typeInfoFromAST` has no `uintptr` case. `export.rs:513`/`:589` key on
  a `__complex_type: "go_byte"` value tag that `concrete_to_json` never emits, so neither split
  issue has to change export; generated bindings are regenerated only by `go-uint-alias-removal`.
- `04`'s baseline no longer promises the 16794cef count on its branch base: after the three core
  children land the out-of-bounds count changes but stays non-zero.
- `BUNDLE.md` regenerated as revision 5 (00-05 plus the related
  `shatter-frontend-rust/15-rust-input-deserialize-classification.md`).
