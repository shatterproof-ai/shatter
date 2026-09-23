# Cross-check review

- **Reviewer:** claude (DEGRADED same-runtime fallback)
- **Artifact type:** issue
- **Mode:** degraded

> **DEGRADED REVIEW.** The counterpart runtime was unavailable, so this review came from an independent agent of the *same* runtime. It shares the original author's model and blind spots; weight it accordingly.

## Findings

# Cross-check review (DEGRADED: same-runtime fallback) - bundle dotfiles-global-guidance

Codex counterpart review failed identity validation (exit 4), so this is an independent same-runtime Claude review, read-only. Claims were checked against dotfiles `main` @ 81f35e1, the live `~/.claude` state, the Shatter memory directory, and `gh -R ketang/dotfiles issue list --state all`.

## Verified claims (spot checks that held)
- `codex/AGENTS.md:16` is a plain-text pointer with no `@`. `wc -w` returns 1146. Line 23 says "registered from `~/project/bento`", but `readlink -f ~/.claude/hooks/bento/require-worktree.sh` resolves into `plugins/cache/bento/bento/2.3.84`. (01)
- `claude/settings.json` lines 285, 334, 362, 373 and 384 use `$DOTFILES`. The `env` block sets only AGENT_TEAMS. `bashrc:3-4` is the non-interactive guard. No `$DOTFILES` appears in `claude/hosts/*/settings.json`. (08)
- In `claude/hosts/pontoon/settings.json`, `shatterproof` has no autoUpdate and `bento` has `autoUpdate: true`. `settings-sync.sh` has `status`. (04)
- `failing-checks.md` has 26 lines, `planning.md` 9, `questions.md` 7 and `fail-closed-defaults.md` 6. (03, 09, 10, 07)
- Open #18, #20, #21 and #23 exist with the described scope. #4-#8 are closed. #11 landed as `88e9cb5`. `agents-sync.sh` prints `render vs snapshot:`.

## Findings

### MAJOR - memory-lifecycle-rule: says the motivating memory files were "corrected on 2026-09-23", but two of the three are not
The draft says all three example problems are cleaned up and asks for no action. That is false. `project_shatter_gate_cache_and_bare_primary.md` (lines 18-25) and its `MEMORY.md` index line still say the primary checkout has `core.bare = true` and tell agents not to run git there. `git -C /home/ketan/project/shatter config --show-origin core.bare` returns `file:.git/config false`. `project_audit_2026_07_10_gate_state.md` still exists and is still missing from `MEMORY.md` (0 matches). Only the `--no-verify` memory was rewritten, and it is now marked HISTORICAL. Fix: either correct the files before filing, or change the "already cleaned up" section to list what is still stale.

### MAJOR - tool-precedence-vs-harness-mode: would reverse the decision in closed dotfiles#10 without mentioning it
Closed #10 ("Reconcile subagent, tools-vs-Bash, and RTK guidance into one paragraph", closed 2026-09-07) set an acceptance check: the dedicated-tools rule "wins over harness auto-mode prompts that prefer Bash for reads, because it is user instruction". The current Tool-Specific Notes text implements that precedence. This draft proposes the opposite: "plain shell is acceptable for one-off reads". It does not cite #10, and it does not say whether it supersedes #10 on purpose. Fix: cite #10 and state explicitly that it reverses that decision, which needs the maintainer's sign-off. Otherwise, reframe the draft as enforcing the #10 rule.

### MAJOR - background-wait-rule-and-hook: the proof-at-close runner cannot run the proposed test
The acceptance criterion requires `claude/tests/test_wait_guard.py`, "run by `claude/tests/run.sh`". But `run.sh` only discovers and sources `test_*.sh` files (see its header and usage). The existing Python tests (`test_rtk_prefilter.py` and others, with `conftest.py`) are pytest files that `run.sh` does not run, and no CI workflow runs `claude/tests` (only `tmc-tests.yml` runs pytest, for `tmc/tests/`). As written, the criterion cannot be met. Fix: say "run with `python3 -m pytest claude/tests/test_wait_guard.py`", or add the step of extending `run.sh` or CI to run the pytest files. The same gap applies to the `claude/tests/` proofs in `global-guidance-actually-loads`, `hooks-dotfiles-env-unset` and `rtk-head-range-compound`: none of them names the command that runs the test.

### MINOR - global-guidance-actually-loads: the causal claim is weak for P1
The P1 priority rests on an inferred link ("the verifier notes that the causal link is inferred"). The load-rate numbers are solid. Consider stating the P1 rationale in terms of the load rate alone.

### MINOR - first-party-plugin-autoupdate: "any other non-Anthropic first-party entry" is ambiguous
It is not clear whether `typesafe-ai` counts as first-party. Name the exact marketplaces that should get autoUpdate, so a fresh agent does not have to guess.

### MINOR - rtk-head-range-compound: the tokenizer is hedged
"using the prefilter's existing tokenizer, if it has one". `rtk_prefilter.py` has regex-based detection and no shlex tokenizer. Say that, so the implementer knows a top-level splitter must be written and must respect quotes.

### MINOR - blocked-escalation: part of the evidence is unverified
The `5617a8ed` timestamps and the count of 14 classifier denials are marked as not re-checked. The acceptance criteria do not depend on them, so this is fine. The flag could be dropped before filing.

### MINOR - hooks-dotfiles-env-unset: the `env -i` test may be too strict
Running the Stop hook command under `env -i` with stdin from `/dev/null` also exercises `jq` and the background python paths. The "no exit 127 / no `not found`" assertion is right. Clarify that other non-zero exits, for example from empty stdin, are acceptable.

## Verdict
Most drafts are ready to file after small edits (01, 03, 04, 05, 07, 08, 09, 10). Three need fixes first:
1. **memory-lifecycle-rule:** correct the false "already cleaned up" claim. The `core.bare` memory and the unindexed 07_10 file are still stale.
2. **tool-precedence-vs-harness-mode:** cite closed #10 and decide explicitly whether to reverse its precedence rule.
3. **background-wait-rule-and-hook** (and the sibling test proofs): name a test command that actually runs `.py` tests, because `claude/tests/run.sh` only runs `test_*.sh`.
