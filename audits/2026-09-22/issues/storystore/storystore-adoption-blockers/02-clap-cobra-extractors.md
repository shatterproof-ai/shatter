---
slug: clap-cobra-extractors
kind: new
title: "inventory: add Rust clap and Go cobra cli-command extractors; flag a detected language with no CLI extractor in the coverage findings"
priority: P2
type: feature
labels: [inventory, coverage, adoption, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: [tracker-migration-and-agents-md]
existing_id: ""
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Same as tracker-migration-and-agents-md: stop if `bd list` still reports the v32->v53 migration refusal."
---

# inventory: add Rust clap and Go cobra cli-command extractors; flag a detected language with no CLI extractor in the coverage findings

## Problem

storystore's inventory only finds CLI commands written with commander.js.
On the shatter repo (Rust CLI built with clap, plus Go and TS frontends), it
finds **zero** `cli-command` surfaces. `stories-coverage` lists `cli-command`
first among its default surface kinds (`shared/coverage.py:69`), so on
shatter it would report no uncovered CLI surfaces while ~25 top-level
subcommands have no coverage. This blocks shatter's storystore adoption
(str-qwua7.52). Closed ss-yoa noted "current extractors are TS/JS-only", but
its fix (4083924b) only added the `skill:` surface prefix and a skill-dir
extractor.

The coverage report does print a language note today
(`shared/coverage.py:683-690`: "Note: go, rust detected but not covered by
built-in extractors. Re-run with --thorough ..."). That note is advisory
text in the header. It is not a finding, it is suppressed under
`--thorough`, and the "## Findings (N)" count and JSON output do not
reflect it. A reader or a script checking findings still sees an
empty CLI result.

## Evidence (re-verified 2026-09-23; storystore HEAD cca768d, shatter audit worktree)

```
$ cd /home/ketan/project/storystore
$ python3 shared/inventory.py --repo-root /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 \
    | python3 -c '...Counter(kind)...'
Counter({'test': 2664, 'heading': 73, 'skill': 15, 'bin': 1})
{'detected': ['go', 'javascript', 'rust', 'typescript'], 'extracted': ['javascript', 'typescript']}
```

- `shared/inventory.py:61`: `EXTRACTED_LANGUAGES = frozenset({"typescript", "javascript"})`.
- `shared/inventory.py:140`: the only CLI regex,
  `_CLI_COMMAND_RE = re.compile(r"""\.command\(\s*['"]([^'"\s]+)['"]""")`,
  used in `_extract_ts_surfaces` (`:228`, match loop at `:235-236`).
- `shared/inventory.py:336-381` `build_inventory` dispatches by file name or
  suffix: `SKILL.md`, `package.json`, TS suffixes, heading docs. It has no
  branch for `.rs` or `.go`.
- Shatter's clap surface: `shatter-cli/src/args.rs:1165` `#[derive(Subcommand, Debug)]`
  with top-level variants such as `Explore(Box<ExploreArgs>)` (:1174),
  `Analyze`, `Scan(Box<ScanArgs>)` (:1303), `SpecDiff` with
  `#[command(name = "spec-diff")]` (:1454-1455), `BuildFrontend`
  (`build-frontend`, :1494), `DiscoverDeps` (:1514), `Init` (:1684),
  `ListTargets(ListTargetsArgs)` (`list-targets`, :1766), `Doctor`.
  Nested groups use `#[command(subcommand)] action: CacheAction` (e.g.
  `Cache` :1698, `Telemetry` :1692, `Workspace` :1704,
  `Nondeterminism` :1710), with nested enums at :1830/:1853/:1866/:1883.
- Shatter has no cobra dependency (no `spf13/cobra` in any go.mod), so the
  cobra extractor is for other consumers. Test it with fixtures only.
- `shared/audit.py:91,120,483` and `shared/impact_check.py:84` key
  `cli-command` refs by `name`. Whatever name format nested commands get
  must resolve in audit and impact-check as well as coverage.
- Source finding: plugins-05, verifier-corrected P1 -> P2 (a missing
  extractor in a tool shatter has not adopted yet, not a regression). Old
  draft: `drafts/other-first-party/32-ss-clap-cobra-extractors.md`.

## Acceptance criteria

- [ ] Rust clap extraction emits `cli-command` surfaces for: variants of
      `#[derive(... Subcommand ...)]` enums, kebab-cased (`ListTargets` ->
      `list-targets`); `#[command(name = "...")]` overrides; and builder
      `Command::new("...")` subcommands. Each case has a fixture under
      `tests/fixtures/` and a test in `tests/test_storystore_inventory.py`.
      The tests fail before the change and pass after; record both runs.
- [ ] Nested subcommands (a variant carrying `#[command(subcommand)]`)
      get one documented name format, e.g. `cache clear`. `stories-audit`
      and `stories-impact-check` resolve a story ref using that format, and
      a test shows it.
- [ ] Go cobra extraction (`&cobra.Command{Use: "name ..."}`, first word of
      `Use`) has a fixture and a test. stdlib `flag` is optional.
- [ ] `EXTRACTED_LANGUAGES` (or its replacement) reports `rust`/`go` as
      extracted only for the surface kinds actually covered.
- [ ] When `cli-command` is among the requested surface kinds and a detected
      language has no CLI extractor, the coverage report includes that gap in
      its findings (or in a warning section that is counted in the summary
      and present in JSON output), including under `--thorough`. A test
      covers it.
- [ ] Proof on shatter: running `python3 shared/inventory.py --repo-root <shatter checkout>`
      lists `cli-command` surfaces including `explore`, `scan`, `spec-diff`,
      `list-targets`, `build-frontend`, and a nested one (e.g. under
      `cache`). Paste the kind counter and the cli-command names in the close
      reason.
- [ ] `python3 -m pytest tests/ -x -q` passes; paste the summary line.

## Suggested approach

- Use regex/line-based extraction, like the TS extractor. Track
  `#[derive(...Subcommand...)]` → `enum X {` blocks, take variant idents at
  enum-body depth 1, apply an immediately preceding `#[command(name = "...")]`
  override, and convert to kebab case the way clap does.
- For nesting, map variants that carry `#[command(subcommand)] action: Y` to
  enum `Y` and emit `parent child` names. Keep a single-file scope; clap
  enums split across files can be resolved by enum name within the crate
  directory.
- Add `.rs` and `.go` branches to `build_inventory`. Skip `target/` and
  `vendor/` using the existing skip set.

## Out of scope

- Rust `#[test]` / Go `TestXxx` test-surface extraction and http-route
  extraction for Rust/Go frameworks. File separately if adoption needs them.
- Writing shatter's `docs/stories` (str-qwua7.52).
- Python argparse/click extractors.

## Priority

P2 (verifier-corrected from P1): this blocks shatter adoption, but nothing
shipped regressed.

## Type / Labels

feature; inventory, coverage, adoption, audit-2026-09-22

## Parent epic

Epic: Audit 2026-09-22 findings (storystore)

## Dependencies

- blocked_by: tracker-migration-and-agents-md (the tracker must accept writes).
- Related: shatter str-qwua7.52 (adopt storystore) depends on this. The
  filer should add a cross-repo note there with this issue's id.
- Referenced by: ss-yoa-reopen-note.
