#!/usr/bin/env python3
"""Top recurring 3/4-grams of normalized Bash command families (recurring manual procedures)."""
import json,re,collections,sys
S=json.load(open('sessions.json'))
since=sys.argv[1] if len(sys.argv)>1 else ''
def norm(c):
    c=c.strip(); c=re.sub(r'^(cd [^&;\n]+(&&|;|\n)\s*)+','',c); c=re.sub(r'^([A-Z_]+=\S+\s+)+','',c)
    c=re.sub(r'^(nice -n \d+ |timeout \d+ |rtk |/usr/bin/)','',c)
    m=re.search(r'(land-work-[a-z-]+|launch-work-[a-z-]+|swarm-[a-z-]+|closure-[a-z-]+|land)\.py',c)
    if m: return m.group(1)
    w=c.split()
    if not w: return ''
    if w[0] in ('git','bd','task','cargo','gh') and len(w)>1: return w[0]+' '+w[1]
    if re.match(r'sleep \d+',c): return 'sleep'
    return w[0].split('/')[-1]
for n in (3,4):
    g=collections.Counter(); ss=collections.defaultdict(set)
    for s in S:
        if (s['start'] or '')<since: continue
        seq=[norm(t['input'].get('command') or '') for t in s['tools'] if t['name']=='Bash']
        seq=[x for x in seq if x not in ('sleep','echo','true',':','cat','tail','ls','grep','sed','head','wc','pwd','cd','python3','for')]
        for i in range(len(seq)-n+1):
            k=tuple(seq[i:i+n])
            if len(set(k))<n: continue
            g[k]+=1; ss[k].add(s['id'])
    print(f'top {n}-grams')
    for k,v in g.most_common(12): print(' ',v,len(ss[k]),' -> '.join(k))
