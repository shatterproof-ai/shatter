---
slug: qwua7-39-json-stdout-first-run
kind: note-to-existing
title: "Note on str-qwua7.39 (raise P2 -> P1): first-run `scan --format json` stdout is not JSON; widen to every JSON stdout command; implicit init can print an empty path"
priority: P1
type: note
labels: [audit-2026-09-22]
parent_epic: ""
blocked_by: []
existing_id: str-qwua7.39
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.39 (raise P2 -> P1): first-run `scan --format json` stdout is not JSON; widen to every JSON stdout command; implicit init can print an empty path

## Tracker action

- Add the comment below to **str-qwua7.39** (`bd comments add str-qwua7.39 …`).
- Raise its priority from P2 to **P1** (`bd update str-qwua7.39 --priority 1`). Reason: cli-ux-04 was verified at P1, and report §14 item 22 calls for it.
- Do not create a new issue. This note also covers audit finding docs-15, which duplicates the open str-qwua7.58/.39. No separate note goes on str-qwua7.58.

## Comment text

> Audit 2026-09-22 (findings cli-ux-04, verified P1; frontend-go-12, init-path part; docs-15) adds evidence and widens scope. Raising to P1.
>
> **1. This breaks JSON contracts, not only the look of explore.** In a fresh directory, `shatter scan . --format json` writes the following to **stdout** before the `{`:
>
> ```
>   Created  .shatter/
>   Created  .shatter/config.yaml  (detected language: unknown)
>   Created  .gitignore …
> Initialized Shatter project at …
> ```
>
> `json.load` then fails with `Expecting value: line 1 column 3` (`audits/2026-09-22/cli-ux-transcripts/scan-json.out`, on branch `audit-2026-09-22`). `shatter explore c.ts:classifyNumber > report.md` has the same problem: the report file starts with the init lines (docs-15).
>
> **2. Code facts (re-checked on `56c86168`).** `shatter-cli/src/commands/init.rs:82`, `:91` and `:143` use `println!`. Implicit init goes through `maybe_implicit_init` (`shatter-cli/src/main.rs:53`, which itself prints `No .shatter/ found — initializing project` to stderr) into `run_implicit_init` (`init.rs:42`). Today it is called only from explore (`main.rs:318`) and scan (`main.rs:648`), so fixing `run_init_impl` covers both. An earlier audit draft said list-targets and spec-diff also trigger it. That is not true at this commit. They still need fresh-directory JSON contract tests (below) so that a future implicit-init call site cannot regress them.
>
> **3. Language detection says `unknown`** for pure-Go and pure-Rust directories as well as bare TS files (`cli-ux-transcripts/go-explore.out`, `rust-explore.out`). `detect_language` (`init.rs:150`) looks only for `package.json`/`go.mod`/`Cargo.toml` in the resolved directory. Detect from the target file extension(s) first.
>
> **4. The printed path can be empty.** `init.rs:143` prints `resolved_dir.display()` from the directory passed in, unchanged. For a bare filename target the parent is `Some("")`, so implicit init has printed `Initialized Shatter project at ` with nothing after it (frontend-go-12). Print the canonicalized absolute path.
>
> **Extra acceptance criteria for this issue:**
>
> - [ ] Implicit-init status lines go to stderr at info level. (Explicit `shatter init` may keep stdout. Decide and document in SPEC §2.8, as the issue already says.)
> - [ ] The language is detected from the target files, with directory markers as the tiebreak. A pure-Go or pure-Rust target never prints `unknown`.
> - [ ] The printed project path is absolute and never empty.
> - [ ] `shatter-cli/tests/json_stdout_contract.rs` gains fresh-directory (not yet initialized) cases, each asserting that the whole of stdout parses as JSON: `scan --format json` (the implicit-init case), and as guards for the other JSON-on-stdout commands, `list-targets --format json`, `spec-diff --json`, `compare --json` and `revalidate --output-format json`. `specify --json` (needs an observation file from `shatter observe`, a pipeline-stage command) and `discover-deps --json` (Linux-only strace diagnostic) are intentionally not covered here; say so in the test file. At close, show the `scan --format json` case failing on current `main` and passing after the fix. The guard cases already pass.
> - [ ] A fresh-directory `explore <file>:<fn>` (markdown on stdout) case asserts that stdout contains none of the init status lines (`Created`, `Initialized Shatter project`). This covers docs-15 and does not depend on str-qwua7.11.
> - [ ] Not in this issue: a whole-stdout-is-JSON case for explore `--spec-json`. That belongs to str-qwua7.11 (open, P1), which makes `--spec-json` stdout-exclusive; its own acceptance should add the fresh-directory case. Snapshot `shatter diff --json` is being removed (retire-snapshot-diff, D2), so there is no case for it.
> - [ ] `task affected` passes, and its `Gates selected` output is recorded.
>
> Related: str-qwua7.58 (keep implicit init; document it; `--no-init`), str-qwua7.11 (`--spec-json` stdout exclusivity; both must hold for stdout to be clean).
