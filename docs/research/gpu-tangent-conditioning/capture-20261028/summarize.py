import json,pathlib,struct,re
root=pathlib.Path('/tmp/loom-conditioning-capture');a=json.load(open(root/'analysis.json'));f=[v for v in a if v['gpu_f64']>=2e-5];c=[v for v in a if v['gpu_f64']<2e-5]
rows=bits=0
for p in root.glob('*-*-*.json'):
 d=json.load(open(p))
 for m in d.get('members',[]):
  rows+=1
  for x,y in zip(m['derivatives'],m['baseline_derivatives']):bits+=sum(struct.pack('f',i)!=struct.pack('f',j)for i,j in zip(x,y))
old=[]
for line in pathlib.Path('/home/b1s/dev/github/3dloom/docs/research/gpu-tangent-conditioning/raw102.log').read_text().splitlines():
 m=re.search(r'node=(\S+) time=(\S+) part=(\d+) vertex=(\d+)',line);old.append((m[1],round(float(m[2])*316/149),int(m[4])))
assert set(old)=={(r['node'],r['frame'],r['vertex'])for r in f}
s=dict(captures=len(a),unique_group_instances=len(set((r['node'],r['frame'],r['group'])for r in a)),baseline_failures=len(f),passing_controls=len(c),exact_historical_failure_locations=True,captured_member_rows=rows,derivative_record_bit_mismatches=bits,membership_mismatches=sum(r['membership_mismatches']for r in a),max_reference_reconstruction_error=max(r['reference_check']for r in a),max_target_instrumentation_delta=max(r['target_instrumentation_delta']for r in a),min_failed_baseline_indicator=min(r['baseline_conditioning_indicator']for r in f),max_control_baseline_indicator=max(r['baseline_conditioning_indicator']for r in c),indicator_true_positive=sum(r['baseline_conditioning_indicator']>=2e-5 for r in f),indicator_false_positive=sum(r['baseline_conditioning_indicator']>=2e-5 for r in c),baseline_derivative_f64_rest_failures=sum(r['baseline_derivative_f64_rest']>=2e-5 for r in f),max_control_error=max(r['gpu_f64']for r in c))
(root/'summary.json').write_text(json.dumps(s,indent=2));print(json.dumps(s,indent=2))
