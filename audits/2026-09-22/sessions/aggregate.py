#!/usr/bin/env python3
"""Aggregate sessions.json into metrics tables. Prints markdown-ish text to stdout.

Usage: aggregate.py [--since YYYY-MM-DD] [--primary-only]
"""
import collections
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
S = json.load(open(os.path.join(HERE, 'sessions.json')))
since = None
if '--since' in sys.argv:
    since = sys.argv[sys.argv.index('--since') + 1]
if since:
    S = [s for s in S if (s['start'] or '') >= since]
if '--primary-only' in sys.argv:
    S = [s for s in S if s['dir'] == '-home-ketan-project-shatter']

PRIMARY = '/home/ketan/project/shatter'


def bash_cmds(s):
    return [t for t in s['tools'] if t['name'] == 'Bash' and isinstance(t['input'], dict)]


def cmd(t):
    return (t['input'].get('command') or '')


def lead(c):
    c = c.strip()
    c = re.sub(r'^(cd [^&;]+(&&|;)\s*)+', '', c)
    c = re.sub(r'^([A-Z_]+=\S+\s+)+', '', c)
    c = re.sub(r'^rtk\s+', '', c)
    c = re.sub(r'^\S*/run-heavy\s+', 'RUNHEAVY ', c)
    w = c.split()
    if not w:
        return ''
    first = os.path.basename(w[0])
    if first in ('git', 'bd', 'cargo', 'task', 'go', 'gh', 'npm', 'npx') and len(w) > 1:
        return first + ' ' + w[1]
    if first.startswith('python') and len(w) > 1:
        return 'python ' + os.path.basename(w[1])
    return first


def p(*a):
    print(*a)


p(f'# Aggregate ({len(S)} sessions' + (f', since {since}' if since else '') + ')\n')
starts = sorted(s['start'] for s in S if s['start'])
p('date span', starts[0][:10] if starts else '-', '->', starts[-1][:10] if starts else '-')
by_month = collections.Counter((s['start'] or '?')[:7] for s in S)
p('by month', dict(sorted(by_month.items())))
p('primary-dir sessions', sum(1 for s in S if s['dir'] == '-home-ketan-project-shatter'))
hs = [s for s in S if s['human']]
p('sessions with human msgs', len(hs), 'human msgs', sum(len(s['human']) for s in S))
p('interrupts', sum(s['interrupts'] for s in S), 'in', sum(1 for s in S if s['interrupts']), 'sessions')
p('teammate msgs', sum(s['teammate'] for s in S), 'task notifs', sum(s['task_notif'] for s in S))
p('compactions', sum(s['compactions'] for s in S), 'usage-limit notices', sum(s['usage_limit'] for s in S))
p('models', collections.Counter(m for s in S for m in s['models']).most_common(8))
p('permission modes', collections.Counter(m for s in S for m in s['permission_modes']))
slash = collections.Counter(x for s in S for x in s['slash'])
p('slash commands', slash.most_common(20))

tools = collections.Counter(t['name'] for s in S for t in s['tools'])
N = sum(tools.values())
p('\n## tools total', N)
p(tools.most_common(30))
nt = sorted(((len(s['tools']), s['id'], s['start'][:10] if s['start'] else '', s['dir'][-40:]) for s in S), reverse=True)
p('median tools', nt[len(nt) // 2][0], ' sessions >200 tools:', sum(1 for x in nt if x[0] > 200))
for x in nt[:20]:
    p('  ', x)

skills = collections.Counter((t['input'] or {}).get('skill') for s in S for t in s['tools'] if t['name'] == 'Skill')
p('\nskills', skills.most_common(40))
agents = collections.Counter((t['input'] or {}).get('subagent_type') for s in S for t in s['tools'] if t['name'] == 'Agent')
p('agent types', agents.most_common(10))

# Bash
B = [t for s in S for t in bash_cmds(s)]
berr = [t for t in B if t['err']]
p(f'\n## Bash {len(B)} calls, {len(berr)} errors ({100*len(berr)/max(1,len(B)):.1f}%)')
lc = collections.Counter(lead(cmd(t)) for t in B)
le = collections.Counter(lead(cmd(t)) for t in berr)
p('top prefixes (n, err, rate)')
for k, n in lc.most_common(45):
    p(f'  {k:35s} {n:5d} {le[k]:4d} {100*le[k]/n:5.1f}%')
p('highest failure-rate prefixes n>=8')
for k, n in sorted(lc.items(), key=lambda kv: -le[kv[0]] / kv[1]):
    if n >= 8 and le[k] / n >= 0.1:
        p(f'  {k:35s} {n:5d} {le[k]:4d} {100*le[k]/n:5.1f}%')

# text tools via bash
TEXT = ('grep', 'cat', 'sed', 'find', 'head', 'tail', 'ls', 'rg', 'awk', 'wc')
tt = collections.Counter()
tts = collections.Counter()
for s in S:
    for t in bash_cmds(s):
        c = cmd(t).strip()
        l = lead(c)
        if l in ('grep', 'cat', 'sed', 'find', 'rg', 'head') and '|' not in c.split('\n')[0][:0] :
            # count only when not part of a larger pipeline starting with a real command
            tt[l] += 1
            tts[s['id']] += 1
p('\ntext-tools-in-bash (lead grep/cat/sed/find/rg/head):', sum(tt.values()), dict(tt), 'sessions', len(tts), 'worst', tts.most_common(5))
dt = collections.Counter(t['name'] for s in S for t in s['tools'] if t['name'] in ('Read', 'Grep', 'Glob', 'Edit', 'Write'))
p('dedicated tools', dict(dt))
sed_i = sum(1 for t in B if re.search(r'\bsed\s+-i', cmd(t)))
heredoc_write = sum(1 for t in B if re.search(r'cat\s+>\s*\S+\s*<<', cmd(t)) or re.search(r'>\s*\S+\s*<<\s*[\'"]?EOF', cmd(t)))
p('sed -i edits', sed_i, 'heredoc file writes', heredoc_write)

# failure streaks
p('\n## failure streaks (>=3 consecutive errors, any tool)')
streaks = []
for s in S:
    run = []
    for t in s['tools']:
        if t['err']:
            run.append(t)
        else:
            if len(run) >= 3:
                streaks.append((len(run), s['id'], run[0]['ts'][:16] if run[0]['ts'] else '', [x['name'] + ':' + (cmd(x)[:60] if x['name'] == 'Bash' else '') for x in run[:4]]))
            run = []
    if len(run) >= 3:
        streaks.append((len(run), s['id'], run[0]['ts'][:16] if run[0]['ts'] else '', [x['name'] for x in run[:4]]))
streaks.sort(reverse=True)
p('count', len(streaks))
for x in streaks[:15]:
    p('  ', x)

# repeated identical failing commands
rep = collections.Counter()
for s in S:
    c = collections.Counter(cmd(t) for t in bash_cmds(s) if t['err'])
    for k, n in c.items():
        if n >= 3:
            rep[(s['id'], k[:120])] = n
p('\nidentical failing bash cmd >=3x in a session:', len(rep))
for k, n in rep.most_common(12):
    p('  ', n, k)
