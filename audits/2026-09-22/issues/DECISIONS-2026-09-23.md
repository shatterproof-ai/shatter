# Maintainer decisions on revised drafts (2026-09-23)

Applied to the drafts after the Codex-driven revision. D1-D6 are recorded in the report's
"Maintainer decisions (2026-09-23)" section.

- **Accepted reviser positions (no objection):** the concolic stop-reason loss is in the CLI
  `ExploreResultAccumulator`, not `orchestrator.rs:2557`; str-0z1im is closed, so there is no
  overlap; `validator-reopen-note` stays a comment-only reopen-note.
- **D7 revalidate ExpectedDrift:** fails the exit code by default. A flag overrides it (for example
  `--allow-drift`), and so does a config equivalent. Drift is still reported in its own count.
- **D8 sandbox backend:** confirmed fail-closed. With only `SHATTER_SANDBOX_BACKEND` set, TS and Rust targets are refused with a message that the backend is Go-only. An unknown backend value is an error.
- **D9 coverage headline:** **branch** coverage is the named headline for explore, scan and run. Line coverage is a named secondary line. `coverage-headline-metric-unification` is therefore blocked by `branch-metric-counts-sites`. run and scan share one default iteration budget.
- **D10 require flag:** `--require-frontend <lang>` (repeatable; doctor, scan and run; exit 2). A note on str-qwua7.40 retires `--require-rust`.
- **D11 integer width/signedness:** the single `usize` draft became epic `int-width-signedness-epic` (bucket `shatter/shatter-int-width-signedness`) with children in dependency order: `int-unsigned64-clamp` → `core-int-range-i128` → `go-int-width-sign-emission`, plus the existing `rust-input-deserialize-classification`, which is independent. The filer gained `parent_slug` for nested epics; a child of a held parent is held too.
- **D12 git-guard priority inversion:** raise bento-l01v and bento-i76i to P1. Their note drafts carry `set_priority: P1`, and the filer now runs `bd update <id> --priority P1` after posting each note.
- **D13 two engine gaps (item 12):** new P2 bug `list-targets-selects-shatter-cache` (bucket shatter-cli-flags-and-help, draft 20); `unknown-config-keys-warn` (draft 07) extended so an unparseable config is exit 2 everywhere and doctor parses it.
- **D14 autoUpdate:** landed directly in dotfiles (8764d01e: shatterproof autoUpdate + gitignore claude/skills/synced; 89468c43: live typesafe plugin + bento guard hook recorded in the pontoon overlay) and rendered live. typesafe-ai stays manual. dotfiles draft 04-first-party-plugin-autoupdate is now done: drop it from filing.
- **D11 update (2026-09-24):** after Codex rounds 2–5 the integer epic has four children: `int-unsigned64-clamp` → `core-int-range-i128` → `core-go-alias-int-range` → `go-int-width-sign-emission`, all P2 so no P2 waits behind a P3. `go-uint-alias-removal` (P3) is a follow-up under the audit epic. `rust-input-deserialize-classification` is related, not a child, and is blocked by str-4yc9w, which owns the classification choice.
- **D13 correction:** the "unparseable config is accepted" half was a false finding (explore already exits 2 with the parse error, and list-targets never reads config). Draft 07 is restored unchanged. The real gap is a new P3 bug, `doctor-parses-config` (draft 21). Draft 20 is narrowed to `.shatter/` exclusion in TargetManifest.
- **D15 (2026-09-24/25):** the filed issues were brought into line with D7 (str-49drv.14: drift fails revalidate by default, override flag), D9 (str-49drv.65: branch-coverage headline, shared run/scan budget, blocked by str-49drv.177) and D10 (str-49drv.61, plus comments on str-qwua7.13 and str-qwua7.40: `--require-frontend <lang>`). The 2026-09-04 audit report was recovered and landed (0edc5daa). storystore's tracker was migrated on pontoon (v32 -> v53, pushed) and its 8 drafts filed under epic ss-drt; any other clone must run `bd bootstrap`. str-wurp gained the audit acceptance criteria in its description, and str-qwua7.21.5 was filed (LLM oracle user guide).
