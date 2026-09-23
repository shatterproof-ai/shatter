# Revision: bugshot-tracker-and-payload (2026-09-23)

This revision applies the Codex cross-check in
`crosscheck/bugshot-tracker-and-payload.codex.md`, which is the primary review,
and the earlier same-runtime review in
`crosscheck/bugshot-tracker-and-payload.md`, which is secondary.

Codex could not query the live tracker. This revision checked the claims below
against the live `bd` databases (bugshot and shatter) and against
`/home/ketan/project/bugshot` at HEAD `e622d73`:

- `bd show bgs-3tq`: open, P3.
- `bd show bgs-3cz`: closed, P2, 0 comments.
- `bd show str-qwua7.53`: open, P2.
- `bd config get external_projects`: not set.
- `bd list --help`: default `--limit 50`.
- `bd dep list --help`: default direction is `down`; `--direction=up` lists dependents.
- `bd dep list bugshot-6zc --direction=up` and `bd dep list bgs-6zc --direction=up` return equivalent `blocks` edges.
- `bd search bundle` and `bd search marketplace` (bugshot) and `bd search bugshot` (bento) find no open issue that already owns the publication fix.

## Codex findings

| # | Sev | Finding (one line) | Action | Files |
|---|---|---|---|---|
| 1 | MAJOR | 01: proof script only counts prefixes, doesn't check statuses/edges, and omits `--limit 0` | applied. `--limit 0` is now used everywhere. The close proof is an executable verifier with 5 assertions and a non-zero exit. It must also be shown failing against the pre-change snapshot. | 01 |
| 2 | MAJOR | 01: dep migration ignores incoming edges, edge types and dedup against existing twin edges | applied. The inventory saves both `--direction` down and up. Each `bugshot-*` endpoint maps to its `bgs-*` twin, the edge type is kept, and edges the twin already has are skipped. Cross-prefix edges are forbidden. Evidence records that the twin edges sampled so far already exist. | 01 |
| 3 | MAJOR | 03: investigation expands into cross-repo implementation | applied (split). 03 now covers investigation and a decision only: it names the follow-ups and their owning repos, and it reconciles bgs-3cz. The implementation moved to the new 06, `publish-staged-plugin-bundle`, which is blocked by 03. The bento marketplace change must be filed by 06 as a linked bento issue and is not implemented in bugshot. bgs-3cz stays closed, and 06 is the single owner of the fix. | 03, 06, 04 |
| 4 | MAJOR | 03: reproduction uninstalls the live cache before establishing history and forces a historical answer | applied. AC 1 preserves `installed_plugins.json` and a cache metadata snapshot, and forbids touching the live install. The fresh install must run in a throwaway HOME or CLAUDE_CONFIG_DIR. A separate AC allows "historical cause undetermined" when it lists what was checked. | 03 |
| 5 | MAJOR | 03: regression coverage could duplicate the existing `test_bundle_dir_stages_slim_plugin_payload` | applied. The existing test is cited (verified at tests/test_build_plugin.py:294). 06 requires a smoke test of the published install route: no excluded paths, a size budget, and a functional check of the installed plugin (four SKILL.md files plus an entry point that exits 0). The test must go red on the current route and green on the new one, and a fresh marketplace install must pass. | 03, 06 |
| 6 | MINOR | 02: "bd cannot express cross-repo deps" is false (`external:<project>:<capability>`) | applied. The comment now says that direct issue-ID dependencies do not work across databases, that `external_projects` is not set here (verified live), and that a dependency edge would not propagate priority anyway. | 02 |
| 7 | MINOR | 03: `docs/specs` wrongly grouped as unwanted payload | applied. The drafts now name `docs/plans/` as unwanted and state that `docs/specs/` ships intentionally (verified in `BUNDLE_PATHS` in `scripts/build-plugin` and in INSTALL.md:72). The same correction was carried into the 04 comment text. | 03, 04, 06 |
| 8 | MINOR | 05: doc check matches basenames, so it can pass falsely | applied. The check now uses exact `skills/<name>/SKILL.md` paths with fixed-string matching and a non-zero exit, as a pytest. It must be shown failing before the change. Verified that AGENTS.md:34-35 already mentions `vizdiff`. | 05 |

No Codex finding is disputed.

## Secondary (same-runtime) review findings also applied

- MAJOR, 01: re-pointing would create cross-prefix or duplicate edges. Handled together with Codex finding 2.
- MINOR, 01: twin state is already mostly equal. The AC is narrowed to a field diff and allows "no mirror-only state".
- MINOR, 01: the root-cause pointer was weak. It now points at `.beads/issues.jsonl` history (present at 616affd) instead of `config.yaml`, which only has its bd-init commit.
- MINOR, 03: the reinstall would destroy evidence. Handled together with Codex finding 4.
- MINOR, 03: "installer copied a local working tree" was missing as a hypothesis. Added as (c).
- MINOR, 05: the vizdiff copy inventory was incomplete. The full list of core-module copies was added, and the sync rule now says "rebuild and commit".
- MINOR, 05: the mechanical check was trivially satisfied. Handled together with Codex finding 8.

## Splits and conversions

- **Split:** `installed-cache-bloat-investigation` (03) now covers
  investigation and a decision. The new
  `06-publish-staged-plugin-bundle.md` (slug `publish-staged-plugin-bundle`,
  `blocked_by: [installed-cache-bloat-investigation]`) holds the
  implementation. If 03 decides "no change", 06 closes as not needed.
- No slugs were removed or converted. All existing slugs are unchanged.
- Filing order: 03 before 04 (for the id substitution) and 03 before 06 (for
  the dependency edge).
- 06 must file a bento issue for the marketplace `source` change at
  implementation time, if the chosen route needs one. No bento draft exists in
  this bucket.

## Maintainer decisions check

D1 to D5 do not affect this bucket. D6 is respected: nothing was filed. No
draft contains a timeout environment variable or hook-bypass guidance (D4).
