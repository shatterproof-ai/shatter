# Bucket bundle: storystore-adoption-blockers

- **Audit:** Shatter audit 2026-09-22 (final drafts, revised 2026-09-23 after the Codex cross-check; see REVISION.md; nothing filed by agents)
- **Bucket:** storystore-adoption-blockers: what blocks storystore adoption in shatter (repo setup and tracker migration, clap extraction and its follow-ups, version bumps)
- **Repo / tracker:** storystore, bd in /home/ketan/project/storystore (prefix ss)
- **Parent epic:** "Epic: Audit 2026-09-22 findings (storystore)". File it after the v32->v53 schema migration.
- **Filing gate:** the storystore tracker is write-blocked (schema v32 -> v53, 21 pending migrations, remote-backed). The maintainer must run `BD_ALLOW_REMOTE_MIGRATE=1 bd migrate && bd dolt push` on ONE designated clone, with approval, before anything in this bucket (including the epic and the ss-yoa comment) is filed. The filer script must check `bd list` and stop with a clear message if the refusal is still printed. The migration is a filing precondition only; no draft carries it as a tracker dependency.
- **Filing order:** 05, 01, 04, 02 (independent) -> 06, 07, 08 (blocked_by 02; need its real id) -> 03 (comment; needs the real ids of 02, 07, 08).

## Maintainer decisions (2026-09-23), which override the report and old drafts

- **D1 Releases:** KEEP Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix. Fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64); do not drop them. Release work closes only with a green release-run URL.
- **D2 shatter diff:** RETIRE `shatter diff` and the unused Snapshot writer path; spec-diff is THE regression tool. Update SPEC/README/QUICKSTART. The `diff` name becomes free; str-81xiw decides whether to take it. Correct the shatter-agents plugin's `shatter diff --staged` docs.
- **D3 Concolic positioning:** MEASURE FIRST. P1 controlled default-vs-concolic benchmark; P1 fix concolic early termination; a follow-up decision issue re-decides "concolic-first" positioning. No doc softening now.
- **D4 Beads hook stall:** RETIRE the JSONL import in shatter; move tracker sync to a Dolt remote; first verify whether the stale JSONL import has overwritten newer DB state; AGENTS.md drops `bd sync`; str-qwua7.28 superseded; bento beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT fix or hook-bypass guidance. *(Applied here: storystore's new AGENTS.md uses the Dolt remote as sync and has no `bd sync` or JSONL import.)*
- **D5 Git identity:** the leaked [user] section was already removed. Drafts cover .mailmap, a git-state drift check, and fixture .git/config snapshotting (shatter only).
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. Agents file nothing.

## Drafts

| # | slug | kind | priority | blocked_by | title |
|---|------|------|----------|------------|-------|
| 01 | tracker-migration-and-agents-md | new | P2 | [] | Add AGENTS.md/CLAUDE.md (Dolt-remote tracker sync, no bd sync) and move the four root plan docs to docs/plans/ with export-ignore kept |
| 02 | clap-cobra-extractors | new | P2 | [] | inventory: extract Rust clap derive subcommands (top-level and nested) as cli-command surfaces and update the language-coverage contract in spec.md |
| 03 | ss-yoa-reopen-note | reopen-note | P2 | [clap-cobra-extractors, go-cobra-extractor, coverage-unextracted-language-finding] | Comment on closed ss-yoa: extractors are still TS/JS-only for cli-command; follow-ups are clap-cobra-extractors, go-cobra-extractor and coverage-unextracted-language-finding |
| 04 | automatic-version-bump | new | P2 | [] | build-plugin: bump the patch version automatically from a hash of the full published payload, and add a check that fails on stale generated outputs, mismatched manifest versions, or an unrecorded payload change |
| 05 | tracker-migration-verification | new | P2 | [] | Record the v32->v53 bd schema migration, bootstrap every other storystore clone, and set beads.role |
| 06 | clap-builder-extractor | new | P3 | [clap-cobra-extractors] | inventory: extract clap builder-style Command::new(...).subcommand(...) commands as cli-command surfaces |
| 07 | go-cobra-extractor | new | P3 | [clap-cobra-extractors] | inventory: extract Go cobra commands (&cobra.Command{Use: ...} plus AddCommand nesting) as cli-command surfaces |
| 08 | coverage-unextracted-language-finding | new | P2 | [clap-cobra-extractors] | stories-coverage: report a detected language with no cli-command extractor as a counted finding (text and JSON), including under --thorough |

<!-- file: 01-tracker-migration-and-agents-md.md -->
---
slug: tracker-migration-and-agents-md
kind: new
title: "Add AGENTS.md/CLAUDE.md (Dolt-remote tracker sync, no bd sync) and move the four root plan docs to docs/plans/ with export-ignore kept"
priority: P2
type: chore
labels: [agents, beads, docs, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Maintainer has run the v32->v53 migration on ONE designated clone and pushed it. The filer must run `bd list` in /home/ketan/project/storystore first and STOP (non-zero exit, no partial filing) if its output contains 'refusing to auto-apply' or 'Writes are blocked', printing: 'storystore tracker is still on schema v32; run BD_ALLOW_REMOTE_MIGRATE=1 bd migrate && bd dolt push on the designated clone (maintainer approval required), then re-run the filer.' This applies to every draft in this bucket, including the ss-yoa comment. The post-migration verification (other clones, beads.role) is tracked separately in tracker-migration-verification."
---

# Add AGENTS.md/CLAUDE.md (Dolt-remote tracker sync, no bd sync) and move the four root plan docs to docs/plans/ with export-ignore kept

> Slug note: this slug is kept from the pre-revision draft for stability.
> The migration record and clone verification that it used to carry now
> live in **tracker-migration-verification**. This issue is only the
> repository setup.

## Problem

1. **No agent conventions.** The repo root has `README.md`, `spec.md`,
   `INSTALL.md` and four top-level plan files
   (`2026-05-01-storystore-plan-1-foundation.md`, `-plan-2-fidelity.md`,
   `-plan-3-edits-and-impact.md`, `-target-design.md`). It has no `AGENTS.md`
   or `CLAUDE.md`, so an agent working here gets no build, version-bump,
   test or landing rules. bugshot's convention is a `CLAUDE.md` that contains
   only `@AGENTS.md`.

2. **Tracker sync guidance must follow shatter decision D4.** storystore
   commits `.beads/issues.jsonl` (54 lines, last committed a89e7e2 on
   2026-06-16, while `bd count` = 56, so it is already stale) and commits
   `.beads/hooks/{post-checkout,post-merge,pre-commit,pre-push,prepare-commit-msg}`.
   Those hooks are **not** installed in this clone: `core.hooksPath` is
   unset and `.git/hooks` has only samples. bd 1.1.0 describes the JSONL as
   "an export, not cross-machine sync or source of truth". The shatter audit
   decided (D4, 2026-09-23) that the Dolt remote is the sync channel and the
   JSONL import is retired. storystore's new AGENTS.md must not tell agents
   to `bd sync` or import the JSONL. The database is remote-backed
   (`bd dolt remote list` shows `origin git+ssh://git@github.com/ketang/storystore.git`).

3. **Moving the plan docs is not a plain `git mv`.** The four plan files are
   excluded from the published plugin archive by root-anchored
   `export-ignore` lines in `.gitattributes:12-15`, and
   `tests/test_published_bundle.py:27-34` (`EXCLUDED_PREFIXES`) asserts they
   are absent. `docs/` itself **is** shipped (the archive today contains
   `docs/adr/` and `docs/contributing/`), so after a move to `docs/plans/`
   the old anchored lines stop matching and the plans would leak into the
   consumer bundle unless `/docs/plans/` is export-ignored.

## Evidence (re-verified 2026-09-23 against storystore HEAD cca768d)

- `ls /home/ketan/project/storystore`: no AGENTS.md or CLAUDE.md. The four
  `2026-05-01-storystore-*.md` files are at the root.
- `.gitattributes:12-15`: `/2026-05-01-storystore-*.md export-ignore`
  (four lines). `git archive --worktree-attributes HEAD | tar t` includes
  `docs/adr/*` and `docs/contributing/*`.
- `README.md` "Test" section: `python3 -m pytest tests/ -x -q`. Build:
  `scripts/build-plugin` (`--bump`, `--shared-only`; README.md:53-55,
  INSTALL.md:118-127).
- Source finding: plugins-09 (`audits/2026-09-22/findings.json` in the shatter
  audit worktree). Old draft: `drafts/other-first-party/31-ss-migration-agents-md.md`.

## Acceptance criteria

- [ ] `AGENTS.md` exists at the repo root and covers: building
      (`scripts/build-plugin`, `--shared-only`); the version rule (link to
      the automatic-version-bump issue until it lands; the rule text itself
      is added by that issue); the test command; branch/worktree and
      landing; and tracker use through bento:beads-issue-flow, with the Dolt
      remote as sync (`bd dolt push` / `bd dolt pull`).
- [ ] `AGENTS.md` states that `.beads/issues.jsonl` is an export, not a sync
      channel, and must not be imported or hand-edited. Close-time proof:
      `grep -nE 'bd sync|bd import|BEADS_HOOK_TIMEOUT|--no-verify' AGENTS.md CLAUDE.md`
      prints nothing (paste the empty result and exit status 1).
- [ ] `CLAUDE.md` exists and its only content is `@AGENTS.md`.
- [ ] The four plan files are in `docs/plans/`, moved with `git mv`
      (`git log --follow` on one of them shows the pre-move history).
- [ ] `.gitattributes` export-ignores `/docs/plans/` and no longer carries
      the four stale root-anchored lines. `EXCLUDED_PREFIXES` in
      `tests/test_published_bundle.py` lists `docs/plans/` instead of the
      four root names.
- [ ] Red->green proof that the leak check is live: with the
      `/docs/plans/ export-ignore` line temporarily removed,
      `python3 -m pytest tests/test_published_bundle.py -q` fails on
      `docs/plans/`; with it restored, it passes. Paste both summary lines.
- [ ] `python3 -m pytest tests/ -x -q` passes; paste the summary line.
- [ ] The close reason records the chosen handling of the committed
      `.beads/issues.jsonl` and the uninstalled `.beads/hooks/` (keep as an
      export, or remove), consistent with D4. Whatever is chosen, no hook
      that imports the JSONL on checkout/merge is installed.

## Suggested approach

On a feature branch/worktree: write AGENTS.md (use bugshot's AGENTS.md as a
model), add `CLAUDE.md` = `@AGENTS.md`, `git mv` the plan docs, replace the
four `.gitattributes` lines with `/docs/plans/ export-ignore`, update the
test's `EXCLUDED_PREFIXES`, and run the suite.

## Out of scope

- The schema migration itself, other-clone `bd bootstrap`, and `beads.role`
  (tracker-migration-verification).
- Any `BEADS_HOOK_TIMEOUT` or hook-bypass guidance (excluded by D4).
- The clap extractor and the version bump (separate issues in this bucket).
- Adopting storystore in shatter (str-qwua7.52).

## Priority

P2: every agent session in the repo lacks conventions, but shipped behavior
is not broken.

## Type / Labels

chore; agents, beads, docs, audit-2026-09-22

## Parent epic

Epic: Audit 2026-09-22 findings (storystore)

## Dependencies

- blocked_by: none. The migration is a filing precondition (already done
  when this exists), not an implementation dependency.
- Related: automatic-version-bump adds the version rule to this AGENTS.md
  if AGENTS.md exists when it lands.

---

<!-- file: 02-clap-cobra-extractors.md -->
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

---

<!-- file: 03-ss-yoa-reopen-note.md -->
---
slug: ss-yoa-reopen-note
kind: reopen-note
title: "Comment on closed ss-yoa: extractors are still TS/JS-only for cli-command; follow-ups are clap-cobra-extractors, go-cobra-extractor and coverage-unextracted-language-finding"
priority: P2
type: task
labels: [inventory, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: [clap-cobra-extractors, go-cobra-extractor, coverage-unextracted-language-finding]
existing_id: ss-yoa
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Same as tracker-migration-and-agents-md (comments are writes). File clap-cobra-extractors, go-cobra-extractor and coverage-unextracted-language-finding first so their real ids replace the placeholders. blocked_by here is a filing-order dependency for placeholder substitution only; do not add it as a tracker dependency on the closed ss-yoa."
---

# Comment on closed ss-yoa: extractors are still TS/JS-only for cli-command; follow-ups are clap-cobra-extractors, go-cobra-extractor and coverage-unextracted-language-finding

**Target:** `ss-yoa` (CLOSED, P2, feature, "Support skill/markdown repos: a
skill: surface prefix or doc-directory extractor (current extractors are
TS/JS-only)". Close reason: "4083924b42050d1d7cd882a5966b56b778e25d09 landed
on main (skill: prefix + skill-dir extractor + audit resolution + generator
ref guard + markdown-only round-trip test)"). Re-verified with
`bd show ss-yoa` on 2026-09-23.

**Action:** add the comment below with `bd comments add ss-yoa ...`. **Do
not reopen ss-yoa.** Its stated scope (skill/markdown repos) was delivered.
The remaining gap is tracked in the new issues. The filer replaces each
`<slug>` placeholder with the real id.

## Comment text

**Audit 2026-09-22 note (shatter audit, finding plugins-05)**

The skill/markdown work in this issue landed and works: 15 `skill` surfaces
are found on the shatter repo. The broader problem named in this issue's
title, "current extractors are TS/JS-only", is still there for CLI
surfaces. On 2026-09-23 at storystore HEAD cca768d:

```
python3 shared/inventory.py --repo-root <shatter checkout>
-> kinds {test: 2664, heading: 73, skill: 15, bin: 1}; no cli-command
-> languages {detected: [go, javascript, rust, typescript], extracted: [javascript, typescript]}
```

The only CLI extractor is the commander.js regex at
`shared/inventory.py:140` (`_CLI_COMMAND_RE`). Shatter's clap derive
subcommands (`shatter-cli/src/args.rs:1165`) are not found, so
`stories-coverage` reports no uncovered CLI surfaces on shatter.

Follow-ups (this issue stays closed):
- <clap-cobra-extractors>: Rust clap derive extraction, nested commands,
  per-kind language metadata, spec.md contract update.
- <go-cobra-extractor>: Go cobra extraction.
- <coverage-unextracted-language-finding>: report a detected language with
  no CLI extractor as a counted coverage finding.

---

<!-- file: 04-automatic-version-bump.md -->
---
slug: automatic-version-bump
kind: new
title: "build-plugin: bump the patch version automatically from a hash of the full published payload, and add a check that fails on stale generated outputs, mismatched manifest versions, or an unrecorded payload change"
priority: P2
type: chore
labels: [release, packaging, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Same as tracker-migration-and-agents-md: stop if `bd list` still reports the v32->v53 migration refusal."
---

# build-plugin: bump the patch version automatically from a hash of the full published payload, and add a check that fails on stale generated outputs, mismatched manifest versions, or an unrecorded payload change

## Problem

storystore's plugin version has stayed at 0.1.1 since 2026-05-25 while
shipped content kept changing. The bento marketplace lists storystore with
no pinned version (github source), and Claude Code keys its plugin cache
directory on the manifest version (`~/.claude/plugins/cache/bento/storystore/<version>/`).
On the maintainer's Claude Code installation inspected by the audit, the
installed storystore is a 2026-05-10 build (commit ca16aef4), 128 commits
behind `main`, missing `shared/impact_trigger.py` and carrying older copies
of six shared scripts, including `inventory.py`. Whether other
installations are equally stale was not inspected; the unchanged version
string is the likely reason `/plugin update` does not refresh it, but that
causal link is inferred, not demonstrated.

A version bump exists but is manual: `scripts/build-plugin --bump`
(`scripts/build-plugin:231-235,259-261`, documented in README.md:53 and
INSTALL.md:118-121) increments the patch version when asked. Nothing bumps
it when the payload changes, and nothing fails when shipped content,
generated outputs, or manifest versions drift apart.

## Evidence (re-verified 2026-09-23; storystore HEAD cca768d)

- `cat plugin-version.json` -> `{"version": "0.1.1"}`.
  `git log --oneline -1 -- plugin-version.json` -> `147af27 Bump plugin version to 0.1.1`.
  `git log --oneline 147af27..HEAD | wc -l` -> 65.
- `~/.claude/plugins/installed_plugins.json`: `storystore@bento` version
  0.1.1, `gitCommitSha` ca16aef4 (committed 2026-05-10), `lastUpdated`
  2026-05-26, installPath `~/.claude/plugins/cache/bento/storystore/0.1.1`.
  `git rev-list --count ca16aef4..HEAD` -> 128.
- The cache's `shared/` has no `impact_trigger.py`; the audit's `diff -rq`
  found audit.py, coverage.py, drift_todo.py, impact_check.py, inventory.py
  and list_candidates.py differ from the repo.
- **The published payload is more than `shared/` and `skills/`.**
  `tests/test_published_bundle.py:36-53` makes the `git archive` contents
  the consumer contract and requires `spec.md`, `README.md`, `INSTALL.md`,
  `plugin-version.json`, both `plugin.json` manifests, and the `skills/`,
  `shared/`, `scripts/`, `.claude/skills/`, `examples/` trees; `docs/adr/`
  and `docs/contributing/` also ship today. Exclusions come from
  `.gitattributes` `export-ignore` (plan docs, `tests/`, `.beads/`).
- Generated outputs: `scripts/build-plugin` writes `.claude-plugin/plugin.json`,
  `.codex-plugin/plugin.json`, `.claude/skills/<name>.md`, and
  `.codex-plugin/skills/<name>/SKILL.md` plus materialized shared scripts
  under `.codex-plugin/skills/<name>/{scripts,references}/` (all tracked);
  `skills/*/scripts/` and `skills/*/references/` are gitignored build
  copies. `--shared-only` (`:250-256`) materializes shared scripts without
  touching manifests or version.
- **storystore has no `.github/workflows/`**; `python3 -m pytest tests/ -x -q`
  is the only automated gate.
- Precedent: shatter-agents `scripts/build-plugins:187-210`
  (`compute_plugin_hash`, `bump_version_if_changed`, hash stored in
  `catalog/plugin-versions.json`) plus `scripts/check-plugins-clean`, run by
  `.github/workflows/ci.yml` job `build-clean` (shatter-agents sa-8xg).
- Source finding: plugins-07. Old draft: `drafts/other-first-party/33-ss-version-bump.md`.

## Acceptance criteria

- [ ] **Hash scope = the published payload.** `scripts/build-plugin`
      computes a content hash over every file the published archive ships
      (tracked files not marked `export-ignore`, i.e. the same set
      `tests/test_published_bundle.py` checks), in sorted path order, over
      path + bytes, excluding only the three version-carrying files
      (`plugin-version.json`, `.claude-plugin/plugin.json`,
      `.codex-plugin/plugin.json`). A parametrized test in
      `tests/test_build_plugin.py` shows the hash changes when a file under
      each of `shared/`, `skills/`, `scripts/`, `examples/`, `docs/` and a
      root doc (`README.md`) changes, and when a new shipped file is added;
      and does **not** change when a file under `tests/` or `.beads/`
      changes.
- [ ] **Automatic bump.** A full `scripts/build-plugin` run records the
      hash next to the version (e.g. `plugin-version.json` gains
      `content_hash`) and bumps the patch version when the hash differs from
      the recorded one. Tests: a second run with no content change leaves
      version and hash unchanged; a content change bumps exactly one patch
      level.
- [ ] **Flags defined.** `--bump` either is removed or becomes an explicit
      force-bump that still records the hash; `--shared-only` never changes
      the version and leaves the check below failing until a full build
      runs. Both behaviours have tests, and README.md ("Build") and
      INSTALL.md (:118-127) describe the new behaviour; no doc still says
      the version is bumped only by `--bump`.
- [ ] **Staleness check that cannot pass on broken outputs.** A
      `scripts/check-plugin-clean` (run by a pytest test) regenerates all
      build outputs into a temporary copy of the tracked tree and fails if
      any of these hold: a generated file listed above is missing or
      differs from the committed one; the version in either `plugin.json`
      differs from `plugin-version.json`; the recorded `content_hash`
      differs from the recomputed hash. Tests use a temp fixture repo and
      show each of the three failure modes red (one test per mode, e.g. a
      hand-edited `shared/inventory.py` with no rebuild, a deleted
      `.codex-plugin/skills/*/SKILL.md`, a hand-edited manifest version),
      and green after a full rebuild. Paste the red and green runs.
- [ ] **Gate wiring.** Either a GitHub Actions workflow runs the check and
      pytest on push/PR to `main` (close reason includes a green run URL),
      or AGENTS.md (if present; tracker-migration-and-agents-md creates it)
      and README.md name the check as a landing gate.
- [ ] **Bump now.** The version is bumped (0.1.2 or later) by the new
      mechanism and pushed. After `/plugin update` (or reinstall) on the
      maintainer's Claude Code installation,
      `~/.claude/plugins/cache/bento/storystore/<new version>/shared/`
      contains `impact_trigger.py`. Paste the `ls` and the new
      `installed_plugins.json` entry.
- [ ] The rule "never hand-edit the version; build-plugin bumps it" is
      written in AGENTS.md if it exists when this lands, and in README.md
      either way.

## Suggested approach

Port `compute_plugin_hash` / `bump_version_if_changed` from shatter-agents
`scripts/build-plugins`, adapted to storystore's single-plugin
`plugin-version.json` and enumerating files with `git ls-files` filtered by
`git check-attr export-ignore`. Write the new version into both manifests
through the existing `claude_manifest` / `codex_manifest` writers. For the
check, copy the tracked tree to a temp dir, run the build there, and diff.

## Out of scope

- A general installed-vs-source drift doctor for all plugins (finding
  plugins-04; tracked elsewhere).
- Changing the bento marketplace entry to pin versions.

## Priority

P2: at least one consumer runs a four-month-old build, but nothing is broken
beyond missing fixes.

## Type / Labels

chore; release, packaging, audit-2026-09-22

## Parent epic

Epic: Audit 2026-09-22 findings (storystore)

## Dependencies

- blocked_by: none (the migration is a filing precondition; the AGENTS.md
  line is conditional on AGENTS.md existing).
- Related: tracker-migration-and-agents-md (AGENTS.md links here for the
  version rule). clap-cobra-extractors and its follow-ups reach consumers
  only through a bump this issue automates.

---

<!-- file: 05-tracker-migration-verification.md -->
---
slug: tracker-migration-verification
kind: new
title: "Record the v32->v53 bd schema migration, bootstrap every other storystore clone, and set beads.role"
priority: P2
type: chore
labels: [beads, tracker, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Same as tracker-migration-and-agents-md: stop if `bd list` still reports the v32->v53 migration refusal. The migration itself is done by the maintainer before filing; this issue records it and finishes the per-clone follow-up."
---

# Record the v32->v53 bd schema migration, bootstrap every other storystore clone, and set beads.role

## Problem

On 2026-09-23, `bd list` in `/home/ketan/project/storystore` (bd 1.1.0)
printed:

```
Warning: refusing to auto-apply 21 pending schema migrations to a remote-backed database (v32 -> v53): migrating clones independently forks the schema (#4259)
  Read-only command: continuing on schema v32 without migrating.
  Writes are blocked until the schema is reconciled.
  ...
    • designated migrator (only ONE machine): BD_ALLOW_REMOTE_MIGRATE=1 bd migrate && bd dolt push
    • every other clone (another already migrated): bd bootstrap
warning: beads.role not configured (GH#2950).
  Fix: git config beads.role maintainer
  Or:  git config beads.role contributor
```

The database is remote-backed (`bd dolt remote list` shows
`origin git+ssh://git@github.com/ketang/storystore.git`), so migrating more
than one clone would fork the schema. The maintainer runs the migration on
one clone before this bucket is filed (see `filer_precondition`). What is
left after filing: a durable record of which clone migrated, `bd bootstrap`
on every other clone, and `beads.role` set in each clone.

## Evidence (re-verified 2026-09-23 against storystore HEAD cca768d)

- `bd list` / `bd show ss-yoa`: the refusal and `beads.role` warning quoted
  above, on every read.
- Source finding: plugins-09. Split out of tracker-migration-and-agents-md
  during the 2026-09-23 cross-check revision.

## Acceptance criteria

- [ ] The close reason names the designated migrator clone (host + path)
      and when `BD_ALLOW_REMOTE_MIGRATE=1 bd migrate && bd dolt push` ran,
      with `bd list 2>&1 | head -3` output from that clone that has no
      "refusing to auto-apply" line.
- [ ] Every other storystore clone the maintainer uses ran `bd bootstrap`
      (not `bd migrate`). The close reason lists each clone with its own
      `bd list 2>&1 | head -3` output free of the refusal, or says "no other
      clones".
- [ ] In each listed clone, `git config beads.role` prints `maintainer` or
      `contributor`, and `bd list` no longer prints "beads.role not
      configured". Paste the output.
- [ ] A write round-trip succeeds on a non-migrator clone after
      `bd dolt pull`: create a throwaway issue, `bd dolt push`, see it from
      the migrator clone after `bd dolt pull`, then delete it. Paste the ids
      and commands.

## Out of scope

- AGENTS.md / CLAUDE.md and the plan-doc move
  (tracker-migration-and-agents-md).
- Any `BEADS_HOOK_TIMEOUT` or hook-bypass guidance (excluded by D4).

## Priority

P2: a second, unbootstrapped clone can fork the schema or keep writes
blocked for whoever uses it.

## Type / Labels

chore; beads, tracker, audit-2026-09-22

## Parent epic

Epic: Audit 2026-09-22 findings (storystore)

## Dependencies

- blocked_by: none.

---

<!-- file: 06-clap-builder-extractor.md -->
---
slug: clap-builder-extractor
kind: new
title: "inventory: extract clap builder-style Command::new(...).subcommand(...) commands as cli-command surfaces"
priority: P3
type: feature
labels: [inventory, coverage, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: [clap-cobra-extractors]
existing_id: ""
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Same as tracker-migration-and-agents-md: stop if `bd list` still reports the v32->v53 migration refusal. File clap-cobra-extractors first so its real id replaces the blocked_by slug."
---

# inventory: extract clap builder-style Command::new(...).subcommand(...) commands as cli-command surfaces

## Problem

clap-cobra-extractors covers clap's derive API only. Rust CLIs that use the
builder API (`Command::new("app").subcommand(Command::new("sync"))`) still
produce no `cli-command` surfaces. Shatter does not use the builder API
(its CLI is derive-only, `shatter-cli/src/args.rs`), so this is for other
consumers and is not an adoption blocker for shatter. Split out of the
pre-revision clap-cobra-extractors draft during the 2026-09-23 cross-check
revision, which found the bundled scope unbounded.

## Evidence

- `shared/inventory.py:140` (storystore HEAD cca768d): the only CLI regex
  is commander.js `.command('name')`.
- Source finding: plugins-05.

## Acceptance criteria

- [ ] Fixtures under `tests/fixtures/` and tests in
      `tests/test_storystore_inventory.py` cover: `Command::new("x")` used as
      the argument of `.subcommand(...)` (emits `x` under its parent, using
      the `parent child` format defined by clap-cobra-extractors), the root
      `Command::new("app")` (not emitted as a command), `.subcommands([...])`
      with several entries, and a `Command::new` not attached to any
      `.subcommand` (not emitted). The new tests fail at the pre-change
      commit and pass after; paste both runs.
- [ ] A file that mixes derive and builder styles yields the union with no
      duplicate names (test).
- [ ] `spec.md` and `shared/spec.md` mention builder support next to the
      derive support documented by clap-cobra-extractors.
- [ ] `python3 -m pytest tests/ -x -q` passes; paste the summary line.

## Out of scope

- Derive API (clap-cobra-extractors). Macro-generated commands.

## Priority

P3: no current consumer is blocked.

## Type / Labels

feature; inventory, coverage, audit-2026-09-22

## Parent epic

Epic: Audit 2026-09-22 findings (storystore)

## Dependencies

- blocked_by: clap-cobra-extractors (reuses its nested-name format, Rust
  file dispatch and per-kind language metadata).

---

<!-- file: 07-go-cobra-extractor.md -->
---
slug: go-cobra-extractor
kind: new
title: "inventory: extract Go cobra commands (&cobra.Command{Use: ...} plus AddCommand nesting) as cli-command surfaces"
priority: P3
type: feature
labels: [inventory, coverage, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: [clap-cobra-extractors]
existing_id: ""
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Same as tracker-migration-and-agents-md: stop if `bd list` still reports the v32->v53 migration refusal. File clap-cobra-extractors first so its real id replaces the blocked_by slug."
---

# inventory: extract Go cobra commands (&cobra.Command{Use: ...} plus AddCommand nesting) as cli-command surfaces

## Problem

storystore detects Go (`go.mod`) but has no Go CLI extractor, so Go CLIs
built with cobra produce no `cli-command` surfaces. Shatter has no cobra
dependency (no `spf13/cobra` in any `go.mod` in the shatter tree), so this
is for other consumers and is tested with fixtures only. Split out of the
pre-revision clap-cobra-extractors draft during the 2026-09-23 cross-check
revision.

## Evidence

- `shared/inventory.py:61` (storystore HEAD cca768d):
  `EXTRACTED_LANGUAGES = frozenset({"typescript", "javascript"})`;
  `build_inventory` (`:336`) has no `.go` branch.
- Source finding: plugins-05.

## Acceptance criteria

- [ ] Fixtures under `tests/fixtures/` and tests in
      `tests/test_storystore_inventory.py` cover: `&cobra.Command{Use: "name [args]"}`
      (name = first word of `Use`); a root command (the one passed to
      `Execute()` or never `AddCommand`-ed) not emitted as a command;
      `parent.AddCommand(child)` nesting emitted as `parent child` (format
      from clap-cobra-extractors) when both are in the same package
      directory; a `Use` held in a const or built dynamically is skipped and
      reported, not guessed. The new tests fail at the pre-change commit and
      pass after; paste both runs.
- [ ] Go files under `vendor/` and `_test.go` files are skipped (test).
- [ ] The per-kind language metadata reports `go` as extracted for
      `cli-command` only when this extractor exists (test).
- [ ] `spec.md` and `shared/spec.md` document cobra support and its limits.
- [ ] `python3 -m pytest tests/ -x -q` passes; paste the summary line.

## Out of scope

- stdlib `flag`, urfave/cli, kong. Cross-package `AddCommand` resolution.

## Priority

P3: no current consumer is blocked.

## Type / Labels

feature; inventory, coverage, audit-2026-09-22

## Parent epic

Epic: Audit 2026-09-22 findings (storystore)

## Dependencies

- blocked_by: clap-cobra-extractors (nested-name format and per-kind
  language metadata).

---

<!-- file: 08-coverage-unextracted-language-finding.md -->
---
slug: coverage-unextracted-language-finding
kind: new
title: "stories-coverage: report a detected language with no cli-command extractor as a counted finding (text and JSON), including under --thorough"
priority: P2
type: feature
labels: [coverage, adoption, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: [clap-cobra-extractors]
existing_id: ""
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Same as tracker-migration-and-agents-md: stop if `bd list` still reports the v32->v53 migration refusal. File clap-cobra-extractors first so its real id replaces the blocked_by slug."
---

# stories-coverage: report a detected language with no cli-command extractor as a counted finding (text and JSON), including under --thorough

## Problem

When a repo's CLI is written in a language storystore cannot extract,
`stories-coverage` reports zero uncovered CLI surfaces, which reads as "fully
covered". The only signal today is advisory header text
(`shared/coverage.py:685-690`: "Note: go, rust detected but not covered by
built-in extractors. Re-run with --thorough ..."). That note is not a
finding: it is suppressed under `--thorough` (`if uncovered and not
thorough`), the "## Findings (N)" count ignores it, and it is absent from
JSON output, so a script checking findings sees an empty CLI result. On
shatter today this hides every clap subcommand; after clap-cobra-extractors
it would still hide any Go CLI surface in a repo without cobra support.
Split out of the pre-revision clap-cobra-extractors draft during the
2026-09-23 cross-check revision.

## Evidence (storystore HEAD cca768d)

- `shared/coverage.py:683-690`: the Language Coverage block and the
  `not thorough`-gated note.
- `shared/coverage.py:69`: `DEFAULT_SURFACE_KINDS` includes `cli-command`.
- `spec.md:385-392`: the header "suggests `--thorough`"; no finding kind is
  specified for the gap.
- Source finding: plugins-05.

## Acceptance criteria

- [ ] When `cli-command` is among the requested surface kinds and a detected
      language has no `cli-command` extractor (per the per-kind metadata
      from clap-cobra-extractors), the report emits a finding of a new,
      documented kind (e.g. `surface-kind-unextracted`, naming the language
      and surface kind). It is counted in "## Findings (N)" and present in
      the JSON output.
- [ ] The finding is still emitted under `--thorough` unless the agent
      supplied inferred surfaces of that kind for that language.
- [ ] No finding when `cli-command` is not requested, or when every detected
      language has a `cli-command` extractor.
- [ ] Tests in `tests/test_storystore_coverage.py` cover each of the three cases above
      with fixtures, for both text and JSON output. They fail at the
      pre-change commit and pass after; paste both runs.
- [ ] The new finding kind is documented in `spec.md` and `shared/spec.md`
      next to the other coverage findings, and in the stories-coverage
      SKILL.md if it lists finding kinds.
- [ ] `python3 -m pytest tests/ -x -q` passes; paste the summary line.

## Out of scope

- Writing any new extractor.
- The same treatment for `http-route` / `schema` / `copy` (file separately
  if wanted; the mechanism should not preclude it).

## Priority

P2: a silent "nothing uncovered" on an unextractable CLI misleads adopters.

## Type / Labels

feature; coverage, adoption, audit-2026-09-22

## Parent epic

Epic: Audit 2026-09-22 findings (storystore)

## Dependencies

- blocked_by: clap-cobra-extractors (defines the per-kind language
  metadata this finding keys on).
