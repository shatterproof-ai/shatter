---
slug: clap-cobra-extractors
kind: new
title: "inventory: extract Rust clap derive subcommands (top-level and nested) as cli-command surfaces and update the language-coverage contract in spec.md"
priority: P2
type: feature
labels: [inventory, coverage, adoption, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Same as tracker-migration-and-agents-md: stop if `bd list` still reports the v32->v53 migration refusal."
---

# inventory: extract Rust clap derive subcommands (top-level and nested) as cli-command surfaces and update the language-coverage contract in spec.md

> Slug note: the slug is kept from the pre-revision draft because shatter
> drafts (shatter-docs 08/10) reference it as "the issue that makes shatter's
> clap subcommands visible". After the 2026-09-23 revision it covers **only
> clap derive extraction plus the contract change**. Builder-style clap is
> **clap-builder-extractor**, Go cobra is **go-cobra-extractor**, and the
> coverage-gap finding is **coverage-unextracted-language-finding**.

## Problem

storystore's inventory only finds CLI commands written with commander.js.
On the shatter repo (Rust CLI built with clap derive, plus Go and TS
frontends), it finds **zero** `cli-command` surfaces. `cli-command` is one of
`stories-coverage`'s default surface kinds (`shared/coverage.py:69`,
`DEFAULT_SURFACE_KINDS`), so on shatter it reports no uncovered CLI surfaces
while ~25 top-level subcommands and four nested groups have no coverage.
This blocks shatter's storystore adoption (str-qwua7.52, open). Closed ss-yoa
noted "current extractors are TS/JS-only", but its fix (4083924b) only added
the `skill:` surface prefix and a skill-dir extractor.

Adding default Rust extraction also changes storystore's published contract:
`spec.md:475` (packaged copy `shared/spec.md:694`) lists as a V1 non-goal
"No language coverage beyond TypeScript without `--thorough`", and
`spec.md:385-392` describes the Language Coverage header in terms of a flat
detected/extracted language list. Both must change with the code.

## Evidence (re-verified 2026-09-23; storystore HEAD cca768d, shatter audit worktree)

```
$ cd /home/ketan/project/storystore
$ python3 shared/inventory.py --repo-root /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 \
    | python3 -c '...Counter(kind)...'
Counter({'test': 2664, 'heading': 73, 'skill': 15, 'bin': 1})
{'detected': ['go', 'javascript', 'rust', 'typescript'], 'extracted': ['javascript', 'typescript']}
```

- `shared/inventory.py:61`: `EXTRACTED_LANGUAGES = frozenset({"typescript", "javascript"})`,
  used at `:181` to compute `extracted`.
- `shared/inventory.py:140`: the only CLI regex,
  `_CLI_COMMAND_RE = re.compile(r"""\.command\(\s*['"]([^'"\s]+)['"]""")`,
  used in `_extract_ts_surfaces` (`:228`, emit at `:236`).
- `shared/inventory.py:336` `build_inventory` dispatches by file name or
  suffix (`SKILL.md`, `package.json`, TS suffixes, heading docs). It has no
  branch for `.rs`.
- Shatter's clap surface (all in `shatter-cli/src/args.rs`):
  `#[derive(Subcommand, Debug)]` at :1165 on `enum CliCommand` (:1172),
  reached from the root struct's `#[command(subcommand)] command: CliCommand`
  (:199-200). Variants include `Explore(Box<ExploreArgs>)`, `Scan(Box<ScanArgs>)`,
  `SpecDiff` with `#[command(name = "spec-diff")]` (:1454), `BuildFrontend`,
  `ListTargets`. Nested groups are **struct-like variants** whose body holds
  `#[command(subcommand)] action: XAction` (`Telemetry` :1692-1694,
  `Cache` :1698-1700, `Workspace` :1704-1706, `Nondeterminism` :1710-1712),
  with the target enums `#[derive(Debug, Clone, Subcommand)]` at
  :1830/:1853/:1866/:1883 in the same file.
- Story refs use `cli: <name>`; `shared/audit.py:90-91` parses the prefix,
  and `shared/audit.py:120-121,483-484` and `shared/impact_check.py:84` key
  `cli-command` surfaces by `name`. Whatever name format nested commands get
  must resolve in audit and impact-check as well as coverage.
- Source finding: plugins-05, verifier-corrected P1 -> P2 (a missing
  extractor in a tool shatter has not adopted yet, not a regression). Old
  draft: `drafts/other-first-party/32-ss-clap-cobra-extractors.md`.

## Extraction boundary (decided here, so the criteria are testable)

- Files: every `.rs` file under the scanned roots, skipping `target/` and
  `vendor/` via the existing skip set.
- Resolution scope for nested subcommand enums: **one crate**, defined as
  the `.rs` files under the nearest ancestor directory that has a
  `Cargo.toml`. A `#[command(subcommand)] field: T` resolves to the
  `Subcommand`-deriving `enum T` in that crate. If `T` is not found, or is
  defined more than once in the crate, emit only the parent command and
  record the unresolved name in the inventory output (not a silent drop).
- Top-level commands: variants of a `Subcommand`-deriving enum that is not
  itself the target of another enum's `#[command(subcommand)]` field.
- No macro expansion, no cross-crate resolution, no builder API (see
  clap-builder-extractor).

## Acceptance criteria

- [ ] For each case below there is a fixture under `tests/fixtures/` and a
      test in `tests/test_storystore_inventory.py`. The close reason pastes
      the failing run of the new tests at the pre-change commit and the
      passing run after:
      - `#[derive(Subcommand)]`, and `#[derive(Debug, Clone, Subcommand)]`
        (Subcommand not first), tuple, unit and struct-like variants;
        PascalCase -> kebab-case as clap does (`ListTargets` -> `list-targets`).
      - `#[command(name = "...")]` override on a variant (`SpecDiff` ->
        `spec-diff`).
      - A struct-like variant carrying `#[command(subcommand)] action: X`
        where `enum X` is in the **same** file, and a second fixture where
        it is in a **different** file of the same crate.
      - A `#[command(subcommand)]` target that is unresolved, and one that is
        defined twice in the crate: parent is emitted, the unresolved name is
        reported, no nested names are invented.
      - Doc comments, `#[arg(...)]` fields and `#[command(about = ...)]`
        lines inside variants do not produce surfaces.
- [ ] Nested commands use one documented name format, `parent child`
      (e.g. `cache clear`). A story ref `cli: cache clear` resolves in
      `stories-audit` and `stories-impact-check`, and a missing nested command
      produces `surface-missing`; tests show both.
- [ ] The inventory's language metadata reports extraction **per surface
      kind** (or an equivalent documented replacement for the flat
      `extracted` list), so `rust` counts as extracted for `cli-command` only.
      A test asserts that `rust` is not reported as extracted for
      `http-route`.
- [ ] `spec.md` and the packaged `shared/spec.md` are updated in the same
      change: the V1 non-goal at `spec.md:475` / `shared/spec.md:694` is
      rewritten to name Rust clap derive as built-in coverage; the Language
      Coverage Header section (`spec.md:385-392` and its `shared/spec.md`
      counterpart) documents the per-kind metadata; the nested `cli:` name
      convention and the crate resolution boundary above are documented.
      Close-time proof: `grep -n "beyond TypeScript" spec.md shared/spec.md`
      prints nothing or only the rewritten wording (paste it).
- [ ] Proof on shatter: `python3 shared/inventory.py --repo-root <shatter checkout>`
      lists `cli-command` surfaces including `explore`, `scan`, `spec-diff`,
      `list-targets`, `build-frontend`, and at least one nested name under
      each of `cache`, `telemetry`, `workspace`, `nondeterminism`, and reports
      zero unresolved subcommand targets. Paste the kind counter and the
      sorted cli-command names in the close reason, with the shatter commit
      they were taken from.
- [ ] `python3 -m pytest tests/ -x -q` passes; paste the summary line.

## Suggested approach

Regex/line-based extraction, like the TS extractor: track
`#[derive(...Subcommand...)]` -> `enum X {` blocks with a brace-depth
counter, take variant idents at enum-body depth 1, apply an immediately
preceding `#[command(name = "...")]`, and look for `#[command(subcommand)]`
fields at depth 2 inside struct-like variants. Build an enum index per crate
first, then emit names.

## Out of scope

- clap builder `Command::new(...)` (clap-builder-extractor).
- Go cobra (go-cobra-extractor). Shatter has no cobra dependency.
- Reporting a detected language with no CLI extractor as a coverage finding
  (coverage-unextracted-language-finding).
- Rust `#[test]` test-surface extraction, Rust http-route extraction,
  Python argparse/click.
- Writing shatter's `docs/stories` (str-qwua7.52).

## Priority

P2 (verifier-corrected from P1): this blocks shatter adoption, but nothing
shipped regressed.

## Type / Labels

feature; inventory, coverage, adoption, audit-2026-09-22

## Parent epic

Epic: Audit 2026-09-22 findings (storystore)

## Dependencies

- blocked_by: none (the tracker migration is a filing precondition, not an
  implementation dependency).
- Blocks: clap-builder-extractor, go-cobra-extractor,
  coverage-unextracted-language-finding (they reuse the nested-name format
  and per-kind language metadata defined here).
- Related: shatter str-qwua7.52 (adopt storystore, OPEN) benefits from this.
  The filer should add a cross-repo note there with this issue's id.
- Referenced by: ss-yoa-reopen-note.
- Reaches consumers only through a version bump (automatic-version-bump).
