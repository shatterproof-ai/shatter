# NOTE TO APPEND to str-qwua7.39: first-run `scan --format json` stdout is not JSON; widen scope and raise to P1

- Priority: P1 (recommend raising str-qwua7.39 from P2)
- Type: (existing issue)
- Labels: (existing)
- Tracker action: note to append to str-qwua7.39 (`bd comments add`), plus a priority bump
- Related: str-qwua7.58
- Source findings: audit 2026-09-22 cli-ux-04 (confirmed), frontend-go-12 (init-path part)

<!-- body -->
Audit 2026-09-22 adds evidence and scope to this issue:

1. **This breaks JSON contracts, not only the look of explore.** In a fresh directory, `shatter scan . --format json` writes `  Created  .shatter/`, `  Created  .shatter/config.yaml  (detected language: unknown)`, `  Created  .gitignore …` and `Initialized Shatter project at …` to stdout before the `{`. `json.load` then fails with `Expecting value: line 1 column 3`. `shatter explore … > report.md` has the same problem: the report file starts with the init lines.
2. **Code facts:** `shatter-cli/src/commands/init.rs:82`, `:91` and `:143` use `println!`. The call is shared through `run_implicit_init` in main.rs, so one fix covers explore, scan, list-targets, spec-diff and diff.
3. **Language detection says `unknown`** for pure-Go and pure-Rust directories as well as TS files. Detect it from the target file extension(s).
4. **The path can be empty:** implicit init has printed `Initialized Shatter project at ` with an empty path (init.rs:143 prints `resolved_dir.display()` from the passed directory unchanged, so a bare filename target gives `Some("")`). Print the absolute path.

Extra acceptance criteria:
- `shatter-cli/tests/json_stdout_contract.rs` gains fresh-directory, not-yet-initialized cases for `scan --format json`, `list-targets --format json`, `spec-diff --json` and `diff --json`, each asserting that stdout parses as JSON.
- The init lines go to stderr, the language is detected from the targets, and the printed path is absolute.
