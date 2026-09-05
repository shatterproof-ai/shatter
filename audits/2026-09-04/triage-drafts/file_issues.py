#!/usr/bin/env python3
"""File reviewed issue drafts.

  file_issues.py bd <repo-dir> <parent-or-'-'> <draft.md>... [--include-triage]
  file_issues.py gh <owner/repo> <draft.md>... [--include-triage]

Behaviour (bd mode):
  * type: epic            -> created with --waits-for-gate all-children; its id is remembered
  * child naming          -> p2-20a-*.md / p2-31c1-*.md are filed under the epic draft whose stem
                             is the prefix (p2-20-*, p2-31c-*) when that epic is in the same batch
  * existing: <id>        -> body is appended to that issue with `bd note`, no new issue
  * triage drafts         -> skipped unless --include-triage (body starts with "Triage:")
  * labels                -> filtered to labels already used by open issues in that tracker
  * dependency edges      -> DEPS below, added after all creates, verified with `bd dep list`
Prints one line per draft: <draft> -> <id|url|note|SKIP|FAILED>.
"""
import re, subprocess, sys, json, collections

PROV = "\n\nFiled from the shatter audit 2026-09-04 (shatter repo, branch audit-2026-09-04, audits/2026-09-04.md)."

# blocker -> blocked, by draft stem prefix or existing tracker id
DEPS = [
    ("p2-25d", "p2-25b"), ("p2-25d", "p2-25c"),
    ("str-qwua7.12", "p2-22"),          # SPEC exit codes before CLI error type
    ("a2-38b", "a2-38c"), ("a2-38b", "p2-31c3"),
    ("p2-31a", "p2-31b"),
]

def parse(path):
    s = open(path).read()
    m = re.match(r'---\n(.*?)\n---\n# (.*?)\n(.*)$', s, re.S)
    meta = dict((k.strip(), v.strip()) for k, v in (l.split(':', 1) for l in m.group(1).splitlines() if ':' in l))
    title, body = m.group(2).strip(), m.group(3).strip()
    am = re.search(r'## Acceptance checks\n(.*?)(?=\n## |\Z)', body, re.S)
    acc = am.group(1).strip() if am else ''
    triage = body.lstrip().lower().startswith('triage') or meta.get('status', '').startswith('triage')
    return meta, title, body + PROV, acc, triage

def run(cmd, cwd=None):
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=300)
    return p.returncode, p.stdout + p.stderr

def stem(path):
    name = path.split('/')[-1]
    return name.split('-')[0] if name[0] in 'pa' else name.rsplit('.md', 1)[0]

def prefix_key(path):
    # 'p2-20a-foo.md' -> 'p2-20a' ; 'p2-31c1-x.md' -> 'p2-31c1'
    return '-'.join(path.split('/')[-1].split('-')[:2])

def epic_parent_key(key):
    # 'p2-20a' -> 'p2-20' ; 'p2-31c1' -> 'p2-31c' ; 'p1-05a' -> 'p1-05'
    m = re.match(r'^(p\d-\d{2}[a-z]?)(\d|[a-z])$', key)
    return m.group(1) if m else None

def tracker_labels(repo):
    rc, out = run(['bd', 'list', '--status=open', '--json'], cwd=repo)
    labels = set()
    try:
        for i in json.loads(out):
            labels.update(i.get('labels') or [])
    except Exception:
        pass
    return labels

def file_bd(repo, default_parent, drafts, include_triage):
    allowed = tracker_labels(repo)
    ids = {}
    order = sorted(drafts, key=lambda d: (0 if parse(d)[0].get('type') == 'epic' else 1, d))
    for d in order:
        meta, title, body, acc, triage = parse(d)
        key = prefix_key(d)
        name = d.split('/')[-1]
        if meta.get('existing', 'none') not in ('none', ''):
            rc, out = run(['bd', 'note', meta['existing'], body], cwd=repo)
            print(f"{name} -> {'note on ' + meta['existing'] if rc == 0 else 'FAILED note: ' + out.strip()[-200:]}", flush=True)
            continue
        if triage and not include_triage:
            print(f"{name} -> SKIP triage (needs maintainer decision)", flush=True)
            continue
        parent = default_parent
        pk = epic_parent_key(key)
        if pk and pk in ids:
            parent = ids[pk]
        labels = [l.strip() for l in meta.get('labels', '').split(',') if l.strip() and l.strip() in allowed]
        cmd = ['bd', 'create', title, '-t', meta['type'], '-p', meta['priority'], '-d', body]
        if labels:
            cmd += ['-l', ','.join(labels)]
        if parent != '-':
            cmd += ['--parent', parent]
        if meta['type'] == 'epic':
            cmd += ['--waits-for-gate', 'all-children']
        elif acc:
            cmd += ['--acceptance', acc]
        rc, out = run(cmd, cwd=repo)
        m = re.search(r'Created issue: (\S+)', out)
        if rc != 0 or not m:
            # retry without acceptance
            cmd2 = [c for c in cmd if c not in ('--acceptance', acc)]
            rc, out = run(cmd2, cwd=repo)
            m = re.search(r'Created issue: (\S+)', out)
        if rc == 0 and m:
            ids[key] = m.group(1)
            print(f"{name} -> {m.group(1)}", flush=True)
        else:
            print(f"{name} -> FAILED: {out.strip().splitlines()[-1] if out.strip() else rc}", flush=True)
    for blocker, blocked in DEPS:
        b = ids.get(blocker, blocker if blocker.startswith(('str-', 'bento-')) else None)
        t = ids.get(blocked)
        if b and t:
            rc, out = run(['bd', 'dep', b, '--blocks', t], cwd=repo)
            rc2, out2 = run(['bd', 'dep', 'list', t], cwd=repo)
            print(f"dep {b} --blocks {t} -> {'ok' if rc == 0 and b in out2 else 'CHECK: ' + out.strip()[-120:]}", flush=True)

def file_gh(repo, drafts, include_triage):
    rc, out = run(['gh', 'label', 'list', '-R', repo, '--json', 'name'])
    try:
        allowed = {l['name'] for l in json.loads(out)}
    except Exception:
        allowed = set()
    for d in drafts:
        meta, title, body, acc, triage = parse(d)
        name = d.split('/')[-1]
        if triage and not include_triage:
            print(f"{name} -> SKIP triage (needs maintainer decision)", flush=True)
            continue
        labels = [l.strip() for l in meta.get('labels', '').split(',') if l.strip() in allowed]
        cmd = ['gh', 'issue', 'create', '-R', repo, '--title', title, '--body', body]
        if labels:
            cmd += ['--label', ','.join(labels)]
        rc, out = run(cmd)
        print(f"{name} -> {out.strip().splitlines()[-1] if out.strip() else 'rc=' + str(rc)}", flush=True)

args = [a for a in sys.argv[1:] if not a.endswith('.review.md')]
include_triage = '--include-triage' in args
args = [a for a in args if a != '--include-triage']
mode = args[0]
if mode == 'bd':
    file_bd(args[1], args[2], args[3:], include_triage)
elif mode == 'gh':
    file_gh(args[1], args[2:], include_triage)
