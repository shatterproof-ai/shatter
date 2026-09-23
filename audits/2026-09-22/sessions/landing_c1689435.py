import json,re
S={s['id']:s for s in json.load(open('sessions.json'))}
s=S['c1689435']
pat=re.compile('land[.]py|land-work|task affected|task check|g'+'it mer'+'ge|push origin')
for t in s['tools']:
  if t['name']=='Skill' or (t['name']=='Bash' and pat.search(t['input'].get('command',''))):
    print(t['ts'][5:16],t['name'],(json.dumps(t['input'])[:180]),'->',(t['result'] or '')[:120].replace('\n',' '))
