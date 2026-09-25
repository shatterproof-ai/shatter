# Revision: storystore-adoption-blockers (2026-09-23)

Source review: `issues/crosscheck/storystore-adoption-blockers.codex.md` (Codex).
The secondary same-runtime review `issues/crosscheck/storystore-adoption-blockers.md`
does not exist. Live tracker checks run with bd from `/home/ketan/project/storystore`
and `/home/ketan/project/shatter`: `bd list` still prints the v32->v53
refusal and the `beads.role not configured` warning (reads work); `bd show ss-yoa` = CLOSED
with the quoted close reason; `bd list --all` has no open clap/cobra/version-bump/AGENTS
issue (no duplicates); shatter `str-qwua7.52` = OPEN.

| # | Severity | Codex finding (one line) | Action | Files changed |
|---|---|---|---|---|
| 1 | MAJOR | 04: the hash covers only `shared/`+`skills/`, but the published bundle also ships scripts, examples, docs, root docs, manifests | applied: hash scope is now every non-`export-ignore` tracked file (same set as `tests/test_published_bundle.py`) minus the three version-carrying files. A parametrized test must show the hash changes for shared/skills/scripts/examples/docs/README and a new file, and does not change for tests/ or .beads/ | 04 |
| 2 | MAJOR | 04: comparing sources with a recorded hash does not prove the committed generated outputs and manifests match | applied: `check-plugin-clean` rebuilds into a temp copy and fails on a missing or different generated file, a manifest version that differs from `plugin-version.json`, or a hash mismatch. Each failure mode needs a red test and then green after a rebuild | 04 |
| 3 | MAJOR | 02: bundles clap derive, clap builder, fixture-only cobra and the coverage-gap finding; "single-file scope" contradicts cross-file enum resolution | applied: split into 02 (clap derive plus nested plus contract), new 06 clap-builder-extractor (P3), new 07 go-cobra-extractor (P3) and new 08 coverage-unextracted-language-finding (P2). 02 now has an "Extraction boundary" section: files are scanned one at a time and nested enums resolve within one crate (nearest `Cargo.toml`). Unresolved or duplicate targets are reported, not guessed. Both same-file and cross-file fixtures are required | 02, 06, 07, 08, 03 |
| 4 | MAJOR | 02: acceptance omits updating the published contract (`spec.md:475` "No language coverage beyond TypeScript without --thorough") | applied: re-verified `spec.md:475`, `shared/spec.md:694` and the Language Coverage Header at `spec.md:385-392`. 02 now requires updating both spec.md copies (non-goal, per-kind metadata, nested `cli:` naming, resolution boundary), with a grep as close proof. 06, 07 and 08 each require their own spec.md updates | 02, 06, 07, 08 |
| 5 | MAJOR | 01/02: the filing prerequisite (the migration) is carried as an implementation dependency; 02 waits for unrelated plan moves | applied: removed `blocked_by: [tracker-migration-and-agents-md]` from 02 and 04. 01 is split: 01 keeps its slug and is now only AGENTS.md/CLAUDE.md plus the plan-doc move; new 05 tracker-migration-verification holds the migration record, other-clone `bd bootstrap`, `beads.role` and a write round-trip. The migration remains a `filer_precondition` only. The AGENTS.md version-rule line in 04 is conditional on AGENTS.md existing, so there is no hard dependency | 01, 02, 04, 05 |
| 6 | MINOR | 04: the draft says build-plugin never changes the version, but `--bump` exists and is documented | applied: the Problem section now describes `--bump` (`scripts/build-plugin:231-235,259-261`, README.md:53, INSTALL.md:118-121) as manual. A new criterion defines what `--bump` and `--shared-only` do after the change and requires README/INSTALL updates | 04 |
| 7 | MINOR | 04: "every consumer" is not supported; the evidence covers one installation | applied: the claim is limited to the maintainer's inspected Claude Code install (`installed_plugins.json`: 0.1.1, ca16aef4 committed 2026-05-10, lastUpdated 2026-05-26). The stale cache (observed) is separated from the version-key cause (inferred). Title and priority text are reworded | 04 |

## Additional fixes found while re-verifying (not in Codex review)

- 01: the plan-doc move must add `/docs/plans/ export-ignore`. `docs/` ships in
  the archive today (`docs/adr/`, `docs/contributing/`), so a bare `git mv`
  would leak the plans to consumers. A red/green proof of the leak test is now required.
- 01/05: `beads.role` is set with `git config beads.role` (per bd's own hint),
  not `bd config`. The acceptance criteria in 05 now use `git config`.
- 01: the D4 compliance proof is a grep of AGENTS.md/CLAUDE.md for `bd sync`,
  `bd import`, `BEADS_HOOK_TIMEOUT` and `--no-verify`, and it must return empty.
- 02: `coverage.py:69` is a frozenset, so the old "cli-command first" wording
  was wrong and is now "one of the default kinds". Nested groups are struct-like
  variants (`Cache { #[command(subcommand)] action: CacheAction }`,
  `args.rs:1698-1700`). The nested enums derive `(Debug, Clone, Subcommand)`
  with Subcommand not first. Both cases need fixtures.
- 03: the comment now lists all three follow-ups. Its `blocked_by` is marked as
  filing-order-only, for placeholder substitution, and is not a tracker dependency on the closed ss-yoa.

## Split / convert notes

- 01 `tracker-migration-and-agents-md`: slug kept, scope narrowed to AGENTS.md/CLAUDE.md plus
  the plan-doc move. The migration and clone verification moved to **05 `tracker-migration-verification`** (new).
- 02 `clap-cobra-extractors`: slug kept because shatter-docs 08/10 reference it as the
  issue that makes shatter's clap subcommands visible, and it still is. Scope is now clap derive only.
  The rest was split into **06 `clap-builder-extractor`**, **07 `go-cobra-extractor`** and
  **08 `coverage-unextracted-language-finding`** (all new, blocked_by 02).
- No drafts were removed or converted to note-to-existing.
