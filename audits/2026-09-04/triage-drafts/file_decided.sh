#!/usr/bin/env bash
# File the drafts whose maintainer decisions were recorded on 2026-09-06.
# Run once:  bash <this file>
set -u
D="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG="$D/file_decided.log"; : > "$LOG"
run(){ echo "== $*" | tee -a "$LOG"; python3 "$D/file_issues.py" "$@" 2>&1 | tee -a "$LOG"; }
# shatter: decided items become children of the audit epic; p2-20d under the CLI-args epic
run bd /home/ketan/project/shatter str-qwua7.20 "$D"/p2-20d-demote-tuning-flags.md
run bd /home/ketan/project/shatter str-qwua7 \
  "$D"/p1-13-exclude-lifecycle-exports-from-discovery.md \
  "$D"/p2-26b-unify-behavior-terminology.md \
  "$D"/p2-27b-implicit-init-policy.md \
  "$D"/p2-28a-rust-test-emitter-or-downgrade.md \
  "$D"/p2-28b-rust-frontend-distribution-docs.md \
  "$D"/p2-29-invariant-confidence.md \
  "$D"/p2-36a-tracker-content-sweep.md \
  "$D"/a2-40a-storystore-decision.md \
  "$D"/a2-40b-bugshot-decision.md \
  "$D"/a2-41a-codex-support-decision.md \
  "$D"/a2-43-land-wrapper.md
# bento: driver script and git-mutation hook under the bento audit epic
run bd /home/ketan/project/bento bento-rdtn "$D"/bento-64-land-work-single-driver.md "$D"/bento-65-require-worktree-hook-git-mutations.md
# dotfiles: allowlist-is-not-policy doc + sonnet default
run gh ketang/dotfiles "$D"/dotfiles-59b-triage-git-allowlist-and-default-model.md
echo "== done; log: $LOG" | tee -a "$LOG"
