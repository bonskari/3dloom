"""Read a saved scene; report live element payload using observed x86_64 layouts."""
import hashlib,json,pathlib,sys
path=pathlib.Path(sys.argv[1]);value=json.loads(path.read_text())
nodes=value['scene']['nodes'];nodes=nodes.values() if isinstance(nodes,dict) else nodes
sizes={'vertices':52,'half_edges':24,'faces':12,'map_vertices':12,'map_corners':4,'invisible':1}
rows=[]
for node in nodes:
    if not any(isinstance(slot.get('modifier'),dict) and 'Skin' in slot['modifier'] for slot in node.get('modifiers',[])):continue
    mesh=next(iter(node['object'].values()))['mesh']
    count={'vertices':len(mesh['vertices']),'half_edges':len(mesh['half_edges']),'faces':len(mesh['faces']),'map_vertices':sum(len(m['verts']) for m in mesh.get('maps',{}).values()),'map_corners':sum(len(m['corners']) for m in mesh.get('maps',{}).values()),'invisible':len(mesh.get('invisible',[]))}
    rows.append({'name':node['name'],'counts':count,'payload_bytes':{k:n*sizes[k] for k,n in count.items()}})
assert len(rows)==5
counts={k:sum(row['counts'][k] for row in rows) for k in sizes}
result={'capture_sha256':hashlib.file_digest(path.open('rb'),'sha256').hexdigest(),'layout_sizes':sizes,'nodes':rows,'total_counts':counts,'total_payload_bytes':{k:counts[k]*sizes[k] for k in sizes}}
print(json.dumps(result,indent=2))
