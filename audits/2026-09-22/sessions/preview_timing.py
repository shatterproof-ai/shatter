"""Find create_preview / bootstrap durations and hook messages mentioning post-checkout."""
import json,re,collections
S=json.load(open('sessions.json'))
c=collections.Counter(); ex=[]
for s in S:
  for t in s['tools']:
    r=t['result'] or ''
    for m in re.finditer(r"hook 'post-checkout' timed out after (\d+)s",r):
      c[(s['start'][:10]>='2026-09-04', m.group(1))]+=1
    m=re.search(r'create_preview: passed \(([\d.]+)s\)',r)
    if m: ex.append(float(m.group(1)))
print('post-checkout timeouts (recent?,secs):',c)
ex.sort(); print('create_preview secs n=',len(ex),'median',ex[len(ex)//2] if ex else None,'max',ex[-1] if ex else None)
