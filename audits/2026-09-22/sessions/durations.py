#!/usr/bin/env python3
"""Measure wall-clock duration of foreground tool calls (tool_use ts -> tool_result ts)
and of background Bash tasks (launch -> task-notification) by command family.

Usage: durations.py [since]
"""
import collections
import datetime as dt
import glob
import json
import os
import re
import statistics
import sys

BASE = os.path.expanduser('~/.claude/projects')
since = sys.argv[1] if len(sys.argv) > 1 else ''


def ts(s):
    return dt.datetime.fromisoformat(s.replace('Z', '+00:00'))


def family(c):
    c = c.strip()
    c = re.sub(r'^(cd [^&;\n]+(&&|;|\n)\s*)+', '', c)
    c = re.sub(r'^([A-Z_]+=\S+\s+)+', '', c)
    c = re.sub(r'^(nice -n \d+ |timeout \d+ |rtk )', '', c)
    if 'land.py' in c:
        return 'land.py'
    m = re.search(r'land-work-([a-z-]+)\.py', c)
    if m:
        return 'land-work-' + m.group(1)
    if re.match(r'(/usr/bin/)?git (-c \S+ )*push', c):
        return 'git push' + (' (no-verify)' if 'no-verify' in c or 'hooksPath' in c else '')
    if re.match(r'(/usr/bin/)?git (-c \S+ )*commit', c):
        return 'git commit' + (' (no-verify)' if 'no-verify' in c or 'hooksPath' in c else '')
    if re.match(r'bd (close|update|create|show|list|ready|dep|search)', c):
        return 'bd ' + c.split()[1]
    if re.match(r'\S*run-heavy task ', c) or re.match(r'task ', c):
        w = re.sub(r'^\S*run-heavy ', '', c).split()
        return 'task ' + (w[1] if len(w) > 1 else '')
    if re.search(r'cargo test', c):
        return 'cargo test'
    return None


fg = collections.defaultdict(list)
bg = collections.defaultdict(list)
timeouts = collections.Counter()
for d in os.listdir(BASE):
    if 'shatter' not in d:
        continue
    for f in glob.glob(os.path.join(BASE, d, '*.jsonl')):
        if os.path.getsize(f) < 5000:
            continue
        pend = {}
        bgpend = {}
        for line in open(f, errors='replace'):
            try:
                o = json.loads(line)
            except Exception:
                continue
            t = o.get('timestamp')
            if not t or t < since:
                continue
            if o.get('type') == 'assistant':
                for c in o['message'].get('content') or []:
                    if isinstance(c, dict) and c.get('type') == 'tool_use' and c.get('name') == 'Bash':
                        fam = family(c['input'].get('command') or '')
                        if fam:
                            pend[c['id']] = (fam, t, bool(c['input'].get('run_in_background')))
            elif o.get('type') == 'user':
                content = o['message'].get('content')
                if isinstance(content, list):
                    for c in content:
                        if isinstance(c, dict) and c.get('type') == 'tool_result' and c.get('tool_use_id') in pend:
                            fam, t0, isbg = pend.pop(c['tool_use_id'])
                            txt = c.get('content')
                            txt = txt if isinstance(txt, str) else ' '.join(x.get('text', '') for x in txt or [] if isinstance(x, dict))
                            if isbg:
                                m = re.search(r'background with ID: (\w+)', txt)
                                if m:
                                    bgpend[m.group(1)] = (fam, t0)
                            else:
                                fg[fam].append((ts(t) - ts(t0)).total_seconds())
                                if 'timed out' in txt[:300].lower():
                                    timeouts[fam] += 1
                elif isinstance(content, str) and '<task-notification' in content:
                    m = re.search(r'<task-id>(\w+)</task-id>', content)
                    if m and m.group(1) in bgpend:
                        fam, t0 = bgpend.pop(m.group(1))
                        bg[fam].append((ts(t) - ts(t0)).total_seconds())
            if o.get('type') == 'attachment':
                a = o.get('attachment', {})
                if a.get('type') == 'queued_command' and '<task-notification' in str(a.get('prompt')):
                    m = re.search(r'<task-id>(\w+)</task-id>', a['prompt'])
                    if m and m.group(1) in bgpend:
                        fam, t0 = bgpend.pop(m.group(1))
                        bg[fam].append((ts(t) - ts(t0)).total_seconds())


def show(title, dd):
    print(title)
    print(f"{'family':28s} {'n':>4s} {'median_s':>9s} {'p90_s':>8s} {'max_s':>8s}")
    for k, v in sorted(dd.items(), key=lambda kv: -len(kv[1])):
        v = sorted(v)
        print(f'{k:28s} {len(v):4d} {statistics.median(v):9.0f} {v[int(0.9 * (len(v) - 1))]:8.0f} {v[-1]:8.0f}  timeouts={timeouts.get(k, 0)}')


show(f'# foreground durations (since {since or "all"})', fg)
show('\n# background durations (launch -> notification)', bg)
