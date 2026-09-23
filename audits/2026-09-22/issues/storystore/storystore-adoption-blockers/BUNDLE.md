# Bucket bundle: storystore-adoption-blockers

- **Audit:** Shatter audit 2026-09-22 (final drafts; nothing filed by agents)
- **Bucket:** storystore-adoption-blockers: what blocks storystore adoption in shatter (tracker migration, clap/cobra extractors, version bumps)
- **Repo / tracker:** storystore, bd in /home/ketan/project/storystore (prefix ss)
- **Parent epic:** "Epic: Audit 2026-09-22 findings (storystore)". File it after the v32->v53 schema migration.
- **Filing gate:** the storystore tracker is write-blocked (schema v32 -> v53, 21 pending migrations, remote-backed). The maintainer must run `BD_ALLOW_REMOTE_MIGRATE=1 bd migrate && bd dolt push` on ONE designated clone, with approval, before anything in this bucket (including the epic and the ss-yoa comment) is filed. The filer script must check `bd list` and stop with a clear message if the refusal is still printed.
- **Filing order:** 01 -> 02 -> 03 (needs 02's real id) -> 04.

## Maintainer decisions (2026-09-23), which override the report and old drafts

- **D1 Releases:** KEEP Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix. Fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64); do not drop them. Release work closes only with a green release-run URL.
- **D2 shatter diff:** RETIRE `shatter diff` and the unused Snapshot writer path; spec-diff is THE regression tool. Update SPEC/README/QUICKSTART. The `diff` name becomes free; str-81xiw decides whether to take it. Correct the shatter-agents plugin's `shatter diff --staged` docs.
- **D3 Concolic positioning:** MEASURE FIRST. P1 controlled default-vs-concolic benchmark; P1 fix concolic early termination; a follow-up decision issue re-decides "concolic-first" positioning. No doc softening now.
- **D4 Beads hook stall:** RETIRE the JSONL import in shatter; move tracker sync to a Dolt remote; first verify whether the stale JSONL import has overwritten newer DB state; AGENTS.md drops `bd sync`; str-qwua7.28 superseded; bento beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT fix or hook-bypass guidance. *(Applied here: storystore's new AGENTS.md uses the Dolt remote as sync and has no `bd sync` or JSONL import.)*
- **D5 Git identity:** the leaked [user] section was already removed. Drafts cover .mailmap, a git-state drift check, and fixture .git/config snapshotting (shatter only).
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. Agents file nothing.

## Drafts

| # | slug | kind | priority | title |
|---|------|------|----------|-------|
| 01 | tracker-migration-and-agents-md | new | P2 | Record the v32->v53 bd schema migration, set beads.role, and add AGENTS.md/CLAUDE.md with plan docs moved to docs/plans/ |
| 02 | clap-cobra-extractors | new | P2 | inventory: add Rust clap and Go cobra cli-command extractors; flag a detected language with no CLI extractor in the coverage findings |
| 03 | ss-yoa-reopen-note | reopen-note | P2 | Comment on closed ss-yoa: extractors are still TS/JS-only for cli-command; follow-up is clap-cobra-extractors |
| 04 | automatic-version-bump | new | P2 | Adopt automatic content-hash plugin version bumps with a staleness check; installed storystore cache is 128 commits behind |

<!-- file: 01-tracker-migration-and-agents-md.md -->
---
slug: tracker-migration-and-agents-md
kind: new
title: "Record the v32->v53 bd schema migration, set beads.role, and add AGENTS.md/CLAUDE.md with plan docs moved to docs/plans/"
priority: P2
type: chore
labels: [agents, beads, tracker, docs, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Maintainer has run the v32->v53 migration on ONE designated clone and pushed it. The filer must run `bd list` in /home/ketan/project/storystore first and STOP (non-zero exit, no partial filing) if its output contains 'refusing to auto-apply' or 'Writes are blocked', printing: 'storystore tracker is still on schema v32; run BD_ALLOW_REMOTE_MIGRATE=1 bd migrate && bd dolt push on the designated clone (maintainer approval required), then re-run the filer.' This applies to every draft in this bucket, including the ss-yoa comment."
---

# Record the v32->v53 bd schema migration, set beads.role, and add AGENTS.md/CLAUDE.md with plan docs moved to docs/plans/

## Problem

1. **Tracker was write-blocked.** On 2026-09-23, `bd list` in
   `/home/ketan/project/storystore` (bd 1.1.0) printed:

   ```
   Warning: refusing to auto-apply 21 pending schema migrations to a remote-backed database (v32 -> v53): migrating clones independently forks the schema (#4259)
     Read-only command: continuing on schema v32 without migrating.
     Writes are blocked until the schema is reconciled.
     ...
       • designated migrator (only ONE machine): BD_ALLOW_REMOTE_MIGRATE=1 bd migrate && bd dolt push
       • every other clone (another already migrated): bd bootstrap
   ```

   `bd config get beads.role` gives the same refusal. The audit also saw
   "beads.role not configured". The database is remote-backed
   (`bd dolt remote list` shows `origin git+ssh://git@github.com/ketang/storystore.git`),
   so migrating more than one clone would fork the schema. That is why the
   migration needs the maintainer's approval and exactly one migrator.
   **This issue can only be filed after that migration.** The filer script
   checks for it (see `filer_precondition`). The migration itself happens
   before filing. This issue records it, verifies the other clones, and
   covers the remaining setup.

2. **No agent conventions.** The repo root has `README.md`, `spec.md`,
   `INSTALL.md` and four top-level plan files
   (`2026-05-01-storystore-plan-1-foundation.md`, `-plan-2-fidelity.md`,
   `-plan-3-edits-and-impact.md`, `-target-design.md`). It has no `AGENTS.md`
   or `CLAUDE.md`, so an agent working here gets no build, version-bump,
   test or landing rules. bugshot's convention is a `CLAUDE.md` that contains
   only `@AGENTS.md`.

3. **Tracker sync guidance must follow shatter decision D4.** storystore
   commits `.beads/issues.jsonl` (54 lines, last committed a89e7e2 on
   2026-06-16, while `bd count` = 56, so it is already stale) and commits
   `.beads/hooks/{post-checkout,post-merge,pre-commit,pre-push,prepare-commit-msg}`.
   Those hooks are **not** installed in this clone: `core.hooksPath` is
   unset and `.git/hooks` has only samples. bd 1.1.0 describes the JSONL as
   "an export, not cross-machine sync or source of truth". The shatter audit
   decided (D4, 2026-09-23) that the Dolt remote is the sync channel and the
   JSONL import is retired. storystore's new AGENTS.md must not tell agents
   to `bd sync` or import the JSONL.

## Evidence (re-verified 2026-09-23 against storystore HEAD cca768d)

- `bd list` / `bd config get beads.role`: the migration refusal quoted above.
- `ls /home/ketan/project/storystore`: no AGENTS.md or CLAUDE.md. The four
  `2026-05-01-storystore-*.md` files are at the root.
- The plan file names are referenced in `tests/test_published_bundle.py:28-31`
  and `.gitattributes:12-15` (`export-ignore` entries with root-anchored
  paths). Moving the files requires updating both.
- `README.md` "Test" section: `python3 -m pytest tests/ -x -q`. Build:
  `scripts/build-plugin` (and `--shared-only`).
- Source finding: plugins-09 (`audits/2026-09-22/findings.json` in the shatter
  audit worktree). Old draft: `drafts/other-first-party/31-ss-migration-agents-md.md`.

## Acceptance criteria

- [ ] The close reason names the designated migrator clone and the time
      `BD_ALLOW_REMOTE_MIGRATE=1 bd migrate && bd dolt push` was run. It
      includes `bd list` output from that clone with no "refusing to
      auto-apply" line.
- [ ] Every other storystore clone the maintainer uses has run `bd bootstrap`
      (not `bd migrate`). The close reason lists those clones, or says "no
      other clones".
- [ ] `bd config get beads.role` prints a value, and `bd create --title test
      ... && bd delete <id>` (or an equivalent write) succeeds. Paste the output.
- [ ] `AGENTS.md` exists at the repo root and covers: building
      (`scripts/build-plugin`, `--shared-only`); the version-bump rule (link
      to the automatic-version-bump issue, or the rule it lands); the test
      command; branch/worktree and landing; and tracker use through
      bento:beads-issue-flow, with the Dolt remote as sync (`bd dolt push` /
      `bd dolt pull`). It contains no `bd sync` and no instruction to import
      or hand-edit `.beads/issues.jsonl`.
- [ ] `CLAUDE.md` exists and contains `@AGENTS.md`.
- [ ] The four plan files are in `docs/plans/`, moved with `git mv`.
      `.gitattributes` export-ignore entries and
      `tests/test_published_bundle.py` are updated. `python3 -m pytest tests/ -x -q`
      passes (paste the summary line).

## Suggested approach

1. (Before filing; maintainer) Choose one clone, run
   `BD_ALLOW_REMOTE_MIGRATE=1 bd migrate && bd dolt push`, then run
   `bd bootstrap` in any other clone. Set `beads.role` as bd suggests.
2. File this bucket.
3. On a feature branch/worktree: write AGENTS.md (use bugshot's AGENTS.md
   as a model), add `CLAUDE.md` = `@AGENTS.md`, `git mv` the plan docs, fix
   the `.gitattributes` and test references, and run the test suite.
4. Whether to stop committing `.beads/issues.jsonl` (and remove the
   uninstalled `.beads/hooks/`) follows the D4 outcome in shatter and the
   matching bento beads-issue-flow guidance. Note which option was chosen in
   AGENTS.md. Do not install the JSONL-importing post-checkout hook.

## Out of scope

- Any `BEADS_HOOK_TIMEOUT` or hook-bypass guidance (excluded by D4).
- The clap/cobra extractors and the version bump (separate issues in this
  bucket).
- Adopting storystore in shatter (str-qwua7.52).

## Priority

P2: this blocks every other storystore write and every agent session in the
repo, but it does not break shipped behavior.

## Type / Labels

chore; agents, beads, tracker, docs, audit-2026-09-22

## Parent epic

Epic: Audit 2026-09-22 findings (storystore)

## Dependencies

- blocked_by: none. The migration is a filing precondition, not a tracker
  dependency.
- Blocks, in practice: every other draft in this bucket, since none can be
  filed until the migration is done.

---

<!-- file: 02-clap-cobra-extractors.md -->
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

---

<!-- file: 03-ss-yoa-reopen-note.md -->
---
slug: ss-yoa-reopen-note
kind: reopen-note
title: "Comment on closed ss-yoa: extractors are still TS/JS-only for cli-command; follow-up is clap-cobra-extractors"
priority: P2
type: task
labels: [inventory, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: [clap-cobra-extractors]
existing_id: ss-yoa
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Same as tracker-migration-and-agents-md (comments are writes). File clap-cobra-extractors first so its real id can replace the placeholder."
---

# Comment on closed ss-yoa: extractors are still TS/JS-only for cli-command; follow-up is clap-cobra-extractors

**Target:** `ss-yoa` (CLOSED, P2, feature, "Support skill/markdown repos: a
skill: surface prefix or doc-directory extractor (current extractors are
TS/JS-only)". Close reason: "4083924b... landed on main (skill: prefix +
skill-dir extractor + audit resolution + generator ref guard + markdown-only
round-trip test)").

**Action:** add the comment below with `bd comments add ss-yoa ...`. **Do
not reopen ss-yoa.** Its stated scope (skill/markdown repos) was delivered.
The remaining gap is tracked in the new issue. The filer replaces
`<clap-cobra-extractors>` with the real id.

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
`shared/inventory.py:140` (`_CLI_COMMAND_RE`). Shatter's ~25 top-level clap
subcommands (`shatter-cli/src/args.rs:1165`) are not found, so
`stories-coverage` reports no uncovered CLI surfaces on shatter.

Follow-up: <clap-cobra-extractors> adds Rust clap and Go cobra extraction
and puts a detected-but-unextracted language into the coverage findings. This
issue stays closed.

---

<!-- file: 04-automatic-version-bump.md -->
---
slug: automatic-version-bump
kind: new
title: "Adopt automatic content-hash plugin version bumps with a staleness check; installed storystore cache is 128 commits behind"
priority: P2
type: chore
labels: [release, packaging, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: [tracker-migration-and-agents-md]
existing_id: ""
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Same as tracker-migration-and-agents-md: stop if `bd list` still reports the v32->v53 migration refusal."
---

# Adopt automatic content-hash plugin version bumps with a staleness check; installed storystore cache is 128 commits behind

## Problem

storystore's plugin version has stayed at 0.1.1 since 2026-05-25. The bento
marketplace lists storystore with no pinned version (github source), so
Claude Code keys its plugin cache on the manifest version. If the version
does not change, consumers keep the old cache. Every consumer is on a build
from 2026-05-10, 128 commits behind `main`. That build lacks
`shared/impact_trigger.py` and has older copies of six shared scripts,
including `inventory.py`, the file the clap/cobra extractor issue changes.
Nothing bumps the version automatically, and nothing fails when shipped
content changes without a bump.

## Evidence (re-verified 2026-09-23; storystore HEAD cca768d)

- `cat plugin-version.json` → `{"version": "0.1.1"}`.
  `git log --oneline -1 -- plugin-version.json` → `147af27 Bump plugin version to 0.1.1`.
  `git log --oneline 147af27..HEAD | wc -l` → 65.
- `~/.claude/plugins/installed_plugins.json`: `storystore@bento` version
  0.1.1, `gitCommitSha` ca16aef4d418..., installPath
  `~/.claude/plugins/cache/bento/storystore/0.1.1`.
  `git rev-list --count ca16aef4..HEAD` → 128.
- The cache's `shared/` has audit.py, coverage.py, drift_todo.py,
  edit_section.py, impact_check.py, inventory.py, list_candidates.py,
  lock_check.py, storystore_lib.py and write_story.py, but no
  `impact_trigger.py`. The audit's `diff -rq` found that audit.py,
  coverage.py, drift_todo.py, impact_check.py, inventory.py and
  list_candidates.py differ from the repo.
- `scripts/build-plugin` reads `VERSION_FILE = ROOT / "plugin-version.json"`
  and never changes it.
- **storystore has no `.github/workflows/`**, so there is no CI to fail. The
  test suite (`python3 -m pytest tests/ -x -q`, per README "Test") is the
  only automated gate.
- Precedent: shatter-agents `scripts/build-plugins:187-210`
  (`compute_plugin_hash` sha256 over the built plugin dir;
  `bump_version_if_changed` bumps patch when `content_hash` changes, stored
  in `catalog/plugin-versions.json`) plus `scripts/check-plugins-clean`, run
  by `.github/workflows/ci.yml` job `build-clean` (shatter-agents issue sa-8xg).
- Source finding: plugins-07. Old draft: `drafts/other-first-party/33-ss-version-bump.md`.

## Acceptance criteria

- [ ] `scripts/build-plugin` computes a content hash over the shipped
      payload (`shared/` and `skills/`, or the built plugin outputs), stores
      it next to the version (e.g. `plugin-version.json` gains
      `content_hash`), and bumps the patch version when the hash changes.
      Running it twice with no content change leaves the version unchanged.
      Tests in `tests/test_build_plugin.py` cover both cases.
- [ ] A staleness check (e.g. `scripts/check-plugin-clean`, plus a pytest
      test that runs it) fails when `shared/` or `skills/` differs from the
      recorded hash, i.e. content changed without running build-plugin. Show
      it failing on a deliberately edited shared file and passing after
      rebuild. If a GitHub Actions workflow is added, it runs this check and
      pytest on push to `main`, and the close reason includes a green run
      URL. If not, AGENTS.md names the check as a landing gate.
- [ ] The version is bumped now (0.1.2 or later). After the plugin is
      updated, a fresh install in `~/.claude/plugins/cache/bento/storystore/<new version>/shared/`
      contains `impact_trigger.py`. Paste the `ls`.
- [ ] The rule ("never hand-edit the version; build-plugin bumps it") is
      written in AGENTS.md (created by tracker-migration-and-agents-md).

## Suggested approach

Port `compute_plugin_hash` / `bump_version_if_changed` from shatter-agents
`scripts/build-plugins`, adapted to storystore's single-plugin
`plugin-version.json`. Hash in sorted path order, excluding `__pycache__`.
Write the new version into `.claude-plugin/plugin.json` and
`.codex-plugin/plugin.json` through the existing manifest writers.

## Out of scope

- A general installed-vs-source drift doctor for all plugins (finding
  plugins-04; tracked elsewhere).
- Changing the bento marketplace entry to pin versions.

## Priority

P2: consumers run a four-month-old build, but nothing is broken for them
beyond missing fixes.

## Type / Labels

chore; release, packaging, audit-2026-09-22

## Parent epic

Epic: Audit 2026-09-22 findings (storystore)

## Dependencies

- blocked_by: tracker-migration-and-agents-md (the tracker must accept
  writes; the version rule goes into its AGENTS.md).
- Related: clap-cobra-extractors. Its fix only reaches consumers once this
  issue's bump mechanism exists.
