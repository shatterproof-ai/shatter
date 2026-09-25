import json,re
S=json.load(open('sessions.json'))
rows=[]
for s in S:
  for t in s['tools']:
    r=t['result'] or ''
    m=re.search(r'create_preview: passed \(([\d.]+)s\)',r)
    if m: rows.append((t['ts'][:10],float(m.group(1))))
    for m in re.finditer(r"hook 'post-checkout' timed out after (\d+)s",r): rows.append((t['ts'][:10],'pc-timeout-'+m.group(1)))
import collections
d=collections.defaultdict(list)
for a,b in rows: d[a].append(b)
for k in sorted(d): print(k, [round(x) if isinstance(x,float) else x for x in d[k]][:12])
