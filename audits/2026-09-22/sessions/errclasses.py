import json,re,collections,sys
S=json.load(open('sessions.json'))
since=sys.argv[1] if len(sys.argv)>1 else ''
C=[('classifier_denied',r'auto mode classifier|auto mode cannot determine'),('blocked_sleep',r'Blocked: sleep'),('guard_block',r'require-worktree'),
   ('not_work_tree',r'must be run in a work tree|not_a_work_tree'),('no_such_file',r'No such file or directory'),('cargo_compile',r'error\[E\d+\]|could not compile'),
   ('test_failed',r'test result: FAILED|FAIL\b|panicked'),('rtk',r'^rtk:|rtk: '),('timeout',r'timed out|did not complete within'),('traceback',r'Traceback'),
   ('permission',r'Permission denied|permission denied'),('git_reject',r'\[rejected\]|non-fast-forward'),('pathspec',r'pathspec'),('user_reject',r"doesn't want to proceed"),('exit_only',r'^Exit code \d+\s*$')]
c=collections.Counter(); ss=collections.defaultdict(set); other=collections.Counter()
for s in S:
  if (s['start'] or '')<since: continue
  for t in s['tools']:
    if not t['err']: continue
    r=t['result'] or ''
    for k,p in C:
      if re.search(p,r,re.M): c[k]+=1; ss[k].add(s['id']); break
    else:
      c['other']+=1; other[re.sub(r'\d+','N',r[:90]).replace('\n',' ')]+=1
tot=sum(c.values()); print('errored tool calls',tot)
for k,v in c.most_common(): print(f'  {k:18s} {v:4d} sessions={len(ss[k])}')
print('other samples'); [print('   ',n,k) for k,n in other.most_common(12)]
