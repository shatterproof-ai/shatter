#!/usr/bin/env bash
# Count feature code vs test code lines across the project.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Dedicated test files (*_test.go, *.test.ts)
test_file_lines=$(find shatter-core/src shatter-cli/src shatter-ts/src shatter-go \
  -name '*_test.go' -o -name '*.test.ts' | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')
test_file_lines=${test_file_lines:-0}

# Rust inline #[cfg(test)] modules
inline_test_lines=0
while IFS= read -r f; do
  n=$(awk '/#\[cfg\(test\)\]/{found=1} found{n++} END{print n+0}' "$f")
  inline_test_lines=$((inline_test_lines + n))
done < <(grep -rl '#\[cfg(test)\]' shatter-core/src/ shatter-cli/src/ 2>/dev/null)

# All source lines
total=$(find shatter-core/src shatter-cli/src shatter-ts/src shatter-go \
  -name '*.rs' -o -name '*.ts' -o -name '*.go' \
  | grep -v node_modules | grep -v __fixtures__ | grep -v testdata \
  | xargs wc -l | tail -1 | awk '{print $1}')

test_total=$((test_file_lines + inline_test_lines))
feature=$((total - test_total))
ratio=$((test_total * 100 / total))

printf "Feature code:  %'d lines\n" "$feature"
printf "Test code:     %'d lines  (dedicated: %'d, inline: %'d)\n" "$test_total" "$test_file_lines" "$inline_test_lines"
printf "Total:         %'d lines\n" "$total"
printf "Test ratio:    %d%%\n" "$ratio"
