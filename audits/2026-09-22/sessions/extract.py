#!/usr/bin/env python3
"""Extract per-session facts from Claude Code JSONL transcripts for the shatter project.

Output: sessions.json (list of per-session dicts) next to this script.
Scope: every ~/.claude/projects/*shatter* dir, files >= 5 KB.
"""
import glob
import json
import os
import re
import sys

BASE = os.path.expanduser('~/.claude/projects')
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'sessions.json')


def text_of(content):
    if isinstance(content, str):
        return content
    out = []
    if isinstance(content, list):
        for c in content:
            if isinstance(c, dict):
                if c.get('type') == 'text':
                    out.append(c.get('text', ''))
                elif c.get('type') == 'tool_result':
                    out.append(text_of(c.get('content')))
    return '\n'.join(out)


def classify_user_text(t):
    s = t.lstrip()
    if s.startswith('<task-notification'):
        return 'task_notification'
    if s.startswith('<teammate-message'):
        return 'teammate'
    if s.startswith('[Request interrupted by user'):
        return 'interrupt'
    if s.startswith('<command-name>') or s.startswith('<command-message>'):
        return 'slash_command'
    if s.startswith('<local-command'):
        return 'local_command'
    if s.startswith('Caveat:'):
        return 'caveat'
    if s.startswith('This session is being continued') or 'conversation that ran out of context' in s[:300]:
        return 'compaction_summary'
    if s.startswith('<system-reminder>') and s.rstrip().endswith('</system-reminder>'):
        return 'system_reminder'
    if 'Workflow harness' in s[:400]:
        return 'workflow'
    if s.startswith('Base directory for this skill'):
        return 'skill_body'
    return 'human'


def parse(path):
    sess = {
        'file': path, 'dir': os.path.basename(os.path.dirname(path)),
        'id': os.path.basename(path)[:8], 'size': os.path.getsize(path),
        'start': None, 'end': None, 'cwds': set(), 'branches': set(), 'models': set(),
        'human': [], 'interrupts': 0, 'slash': [], 'teammate': 0, 'task_notif': 0,
        'tools': [], 'hook_errors': [], 'hook_cancelled': [], 'stop_hook_prevented': 0,
        'compactions': 0, 'usage_limit': 0, 'api_errors': 0, 'sidechain_msgs': 0,
        'permission_modes': set(), 'team': set(), 'agent_names': set(),
        'assistant_text': [], 'nested_memory': set(),
    }
    pending = {}
    for line in open(path, errors='replace'):
        try:
            o = json.loads(line)
        except Exception:
            continue
        ts = o.get('timestamp')
        if ts:
            sess['start'] = ts if not sess['start'] or ts < sess['start'] else sess['start']
            sess['end'] = ts if not sess['end'] or ts > sess['end'] else sess['end']
        if o.get('cwd'):
            sess['cwds'].add(o['cwd'])
        if o.get('gitBranch'):
            sess['branches'].add(o['gitBranch'])
        if o.get('teamName'):
            sess['team'].add(o['teamName'])
        if o.get('agentName'):
            sess['agent_names'].add(o['agentName'])
        if o.get('isSidechain'):
            sess['sidechain_msgs'] += 1
        t = o.get('type')
        if t == 'permission-mode':
            sess['permission_modes'].add(o.get('permissionMode'))
        if t == 'system':
            st = o.get('subtype')
            if st == 'compact_boundary':
                sess['compactions'] += 1
            if st == 'informational' and 'Usage limit' in str(o.get('content')):
                sess['usage_limit'] += 1
            if st == 'stop_hook_summary':
                if o.get('preventedContinuation'):
                    sess['stop_hook_prevented'] += 1
                for e in o.get('hookErrors') or []:
                    sess['hook_errors'].append({'event': 'Stop', 'err': str(e)[:300], 'ts': ts})
            if st == 'api_error' or (st and 'error' in st):
                sess['api_errors'] += 1
        if t == 'attachment':
            a = o.get('attachment', {})
            at = a.get('type')
            if at == 'hook_non_blocking_error':
                sess['hook_errors'].append({'event': a.get('hookName'), 'err': (a.get('stderr') or '')[:300],
                                            'cmd': (a.get('command') or '')[:200], 'ts': ts})
            elif at == 'hook_cancelled':
                sess['hook_cancelled'].append({'event': a.get('hookName'), 'cmd': (a.get('command') or '')[:200],
                                               'ms': a.get('durationMs'), 'ts': ts})
            elif at == 'model':
                sess['models'].add(a.get('identity', {}).get('modelId'))
            elif at == 'nested_memory':
                sess['nested_memory'].add(a.get('path'))
        if t == 'assistant':
            m = o.get('message', {})
            if m.get('model'):
                sess['models'].add(m['model'])
            for c in m.get('content') or []:
                if not isinstance(c, dict):
                    continue
                if c.get('type') == 'tool_use':
                    rec = {'id': c.get('id'), 'name': c.get('name'), 'input': c.get('input'), 'ts': ts,
                           'side': bool(o.get('isSidechain')), 'cwd': o.get('cwd'), 'branch': o.get('gitBranch'),
                           'err': None, 'result': None}
                    pending[c.get('id')] = rec
                    sess['tools'].append(rec)
                elif c.get('type') == 'text' and c.get('text'):
                    sess['assistant_text'].append(c['text'][:2000])
            if o.get('isApiErrorMessage') or o.get('error'):
                sess['api_errors'] += 1
        if t == 'user':
            m = o.get('message', {})
            content = m.get('content')
            if isinstance(content, list):
                for c in content:
                    if isinstance(c, dict) and c.get('type') == 'tool_result':
                        rec = pending.get(c.get('tool_use_id'))
                        if rec is not None:
                            rec['err'] = bool(c.get('is_error'))
                            rec['result'] = text_of(c.get('content'))[:1500]
            if o.get('isMeta') or o.get('isSidechain'):
                continue
            txt = text_of(content) if not (isinstance(content, list) and any(
                isinstance(c, dict) and c.get('type') == 'tool_result' for c in content)) else ''
            if not txt:
                continue
            k = classify_user_text(txt)
            if k == 'human':
                sess['human'].append({'ts': ts, 'text': txt[:1200], 'src': o.get('promptSource')})
            elif k == 'interrupt':
                sess['interrupts'] += 1
            elif k == 'slash_command':
                mm = re.search(r'<command-name>(.*?)</command-name>', txt)
                sess['slash'].append(mm.group(1) if mm else '?')
            elif k == 'teammate':
                sess['teammate'] += 1
            elif k == 'task_notification':
                sess['task_notif'] += 1
    for k in ('cwds', 'branches', 'models', 'permission_modes', 'team', 'agent_names', 'nested_memory'):
        sess[k] = sorted(x for x in sess[k] if x)
    return sess


def main():
    files = []
    for d in os.listdir(BASE):
        if 'shatter' not in d:
            continue
        for f in glob.glob(os.path.join(BASE, d, '*.jsonl')):
            if os.path.getsize(f) >= 5000:
                files.append(f)
    out = []
    for f in sorted(files):
        try:
            out.append(parse(f))
        except Exception as e:  # keep going; report
            print('ERR', f, e, file=sys.stderr)
    json.dump(out, open(OUT, 'w'))
    print(len(out), 'sessions ->', OUT)


if __name__ == '__main__':
    main()
