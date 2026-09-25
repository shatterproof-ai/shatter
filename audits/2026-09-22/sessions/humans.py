#!/usr/bin/env python3
"""Print genuine human messages (excluding automated review prompts / teammate relays)."""
import json,sys
S=json.load(open('sessions.json'))
since=sys.argv[1] if len(sys.argv)>1 else ''
AUTO=('Review this change for security','You are an independent, skeptical reviewer','Another Claude session sent a message','Diagnostic request:','<teammate-message','[Workflow harness')
for s in sorted(S,key=lambda s:s['start'] or ''):
    if (s['start'] or '')<since: continue
    for h in s['human']:
        t=h['text'].strip()
        if t.startswith(AUTO): continue
        print(f"{s['id']} {h['ts'][:16]} | {t[:400]!r}")
