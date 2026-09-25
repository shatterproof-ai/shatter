import json,re,collections
S=json.load(open('sessions.json'))
r=collections.Counter(); w=collections.Counter(); cite=collections.Counter()
for s in S:
  if (s['start'] or '')<'2026-09-04': continue
  for t in s['tools']:
    fp=(t['input'] or {}).get('file_path','') if isinstance(t['input'],dict) else ''
    if '/memory/' in fp:
      (r if t['name']=='Read' else w)[fp.split('/')[-1]]+=1
  for x in s['assistant_text']:
    for m in re.findall(r'(project_[a-z_]+|feedback_[a-z_]+)',x): cite[m]+=1
print('reads',r.most_common(12)); print('writes',w.most_common(12)); print('cited in text',cite.most_common(12))
