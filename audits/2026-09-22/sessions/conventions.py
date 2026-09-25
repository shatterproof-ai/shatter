#!/usr/bin/env python3
"""Detect convention violations / recurring friction in sessions.json.

Usage: conventions.py [since]
"""
import collections
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
S = json.load(open(os.path.join(HERE, 'sessions.json')))
since = sys.argv[1] if len(sys.argv) > 1 else ''
S = [s for s in S if (s['start'] or '') >= since]
PRIMARY = '/home/ketan/project/shatter'

PATTERNS = {
    'no_verify': r'--no-verify',
    'hooksPath_null': r'core\.hooksPath=/dev/null',
    'force_push': r'push\s+(-f\b|--force(?!-with-lease))',
    'force_with_lease': r'--force-with-lease',
    'reset_hard': r'reset\s+--hard',
    'git_stash': r'\bgit (-C \S+ )?stash\b',
    'bare_cargo_test': r'(^|[;&|]\s*|run-heavy\s+)cargo test',
    'task_cmd': r'(^|[;&|]\s*|run-heavy\s+)task (test|check|affected|e2e|smoke|walkthrough|gauntlet|parity|conformance)',
    'task_force': r'task \S+ .*--force|task --force',
    'run_heavy': r'run-heavy',
    'bd_any': r'(^|[;&|(]\s*)bd ',
    'usr_bin_git': r'/usr/bin/git',
    'usr_bin_grep': r'/usr/bin/grep',
    'rtk_prefix': r'(^|[;&|]\s*)rtk ',
    'gh_pr': r'gh pr ',
    'cd_primary': r'cd /home/ketan/project/shatter(\s|$|;|&)',
    'git_C_primary': r'git -C /home/ketan/project/shatter\s',
    'land_py': r'land\.py',
    'land_scripts': r'land-work-[a-z-]+\.py',
    'launch_scripts': r'launch-work[a-z-]*\.py',
    'swarm_scripts': r'swarm-[a-z-]+\.py',
    'closure_scripts': r'closure[a-z-]*\.py',
    'cargo_fmt': r'cargo fmt(?!.*--check)',
    'sleep_busy': r'^sleep\s+[0-9]+\s*(;\s*echo \w+)?\s*$',
    'dolt': r'\bdolt\b',
    'bd_export': r'bd export',
    'kill': r'(^|[;&|]\s*)(pkill|kill)\b',
}
RESULT_PATTERNS = {
    'must_run_in_work_tree': r'must be run in a work tree',
    'blocked_sleep': r'Blocked: sleep',
    'hook_blocked_generic': r'(PreToolUse|hook).{0,80}(block|denied|deny|refus)',
    'bento_main_block': r'(editing|edit).{0,40}\bmain\b.{0,80}(block|not allowed|refus)',
    'timeout_2m': r'Command timed out',
    'bd_timeout': r'(bd|beads|dolt).{0,80}(timed out|timeout|deadline)',
    'dolt_err': r'(dolt|Dolt).{0,60}(error|Error|failed)',
    'lease': r'lease',
    'task_up_to_date': r'is up to date',
    'rtk_err': r'rtk:',
    'permission_denied': r'Permission to use .* (has been denied|was denied)|permission denied',
    'user_rejected': r"The user doesn't want to proceed|user rejected|User rejected",
    'auto_mode_block': r'auto mode|Auto mode',
    'unknown_flag': r'unknown flag',
    'no_such_file': r'No such file or directory',
    'cwd_reset': r'Shell cwd was reset',
    'detached_head': r'detached HEAD',
    'merge_conflict': r'CONFLICT',
    'non_fast_forward': r'non-fast-forward|\[rejected\]|fetch first',
    'clippy': r'warning: .*clippy|error: .*clippy',
    'stale_cache_hint': r'cached|checksum',
}

cnt = collections.Counter()
sess = collections.defaultdict(set)
ex = {}
for s in S:
    for t in s['tools']:
        if t['name'] != 'Bash' or not isinstance(t['input'], dict):
            continue
        c = t['input'].get('command') or ''
        for k, rx in PATTERNS.items():
            if re.search(rx, c, re.M):
                cnt['cmd:' + k] += 1
                sess['cmd:' + k].add(s['id'])
                ex.setdefault('cmd:' + k, (s['id'], (t['ts'] or '')[:16], c[:220]))
    for t in s['tools']:
        r = t['result'] or ''
        for k, rx in RESULT_PATTERNS.items():
            if re.search(rx, r):
                cnt['res:' + k] += 1
                sess['res:' + k].add(s['id'])
                ex.setdefault('res:' + k, (s['id'], (t['ts'] or '')[:16], t['name'], json.dumps(t['input'])[:150], r[:300]))

print(f'# conventions ({len(S)} sessions since {since or "all"})')
for k in sorted(cnt, key=lambda k: -cnt[k]):
    print(f'{k:32s} {cnt[k]:5d} in {len(sess[k]):3d} sessions  e.g. {ex[k]!r}'[:700])

# Edits into primary checkout (not memory, not /tmp)
print('\n## Edit/Write into primary checkout paths')
pe = collections.Counter()
for s in S:
    for t in s['tools']:
        if t['name'] in ('Edit', 'Write', 'NotebookEdit') and isinstance(t['input'], dict):
            fp = t['input'].get('file_path') or ''
            if fp.startswith(PRIMARY + '/'):
                pe[(s['id'], fp, bool(t['err']), (t['result'] or '')[:160])] += 1
for k, n in pe.most_common(30):
    print(n, k)

# Hook-denied tool calls
print('\n## tool results mentioning hooks')
hc = collections.Counter()
hx = {}
for s in S:
    for t in s['tools']:
        r = t['result'] or ''
        m = re.search(r'([A-Za-z]*[Hh]ook[^\n]{0,160})', r)
        if m and t['err']:
            key = re.sub(r'[0-9a-f]{6,}|\d+', 'N', m.group(1))[:110]
            hc[key] += 1
            hx.setdefault(key, (s['id'], t['name'], json.dumps(t['input'])[:160]))
for k, n in hc.most_common(25):
    print(n, k, hx[k])

# Hook infrastructure errors
print('\n## hook non-blocking errors / cancellations')
he = collections.Counter()
for s in S:
    for h in s['hook_errors']:
        he[(h.get('event'), re.sub(r'\d+', 'N', h.get('err', ''))[:140])] += 1
    for h in s['hook_cancelled']:
        he[('CANCELLED ' + str(h.get('event')), h.get('cmd', '')[:120])] += 1
for k, n in he.most_common(20):
    print(n, k)
