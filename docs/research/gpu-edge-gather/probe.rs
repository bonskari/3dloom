use std::collections::BTreeMap;
use loom_scene::{Scene,Modifier};
use loom_render::{MeshStyle,SubStyles,SurfaceStyle};
#[path="/home/b1s/dev/github/3dloom/crates/loom_render/src/gpu_edges.rs"]
mod gpu_edges;
use gpu_edges::*;
fn style(s:&Scene,id:loom_scene::NodeId)->MeshStyle{let n=s.node(id).unwrap();let d=s.display_of(n);let srgb=|c:u8|{let x=c as f32/255.;if x<=0.04045{x/12.92}else{((x+0.055)/1.055).powf(2.4)}};let color=s.display_color(n).map(srgb);let mut out=MeshStyle{color,wire_color:Some(color),diagonals:!d.edges_only,..Default::default()};match s.node_surfaces(id){Some(loom_scene::material::Surfaces::Single(m))=>out.wire=m.wire,Some(loom_scene::material::Surfaces::PerId(m))=>{let base=out.surface();out.sub=Some(std::sync::Arc::new(SubStyles{by_id:m.into_iter().map(|(id,m)|(id,SurfaceStyle{wire:m.wire,..base})).collect(),missing:SurfaceStyle{wire:false,..base}}));},None=>{}}out}
fn main(){
let path=std::env::args().nth(1).unwrap();let mut v:serde_json::Value=serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();let s:Scene=serde_json::from_value(v["scene"].take()).unwrap();
let ids:Vec<_>=s.nodes().filter(|n|n.modifiers.iter().any(|m|matches!(m.modifier,Modifier::Skin(_)))).map(|n|n.id).collect();assert_eq!(ids.len(),5);
let instance=wgpu::Instance::new(&Default::default());let adapter=pollster::block_on(instance.request_adapter(&Default::default())).unwrap();let(device,queue)=pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor{required_limits:adapter.limits(),..Default::default()},None)).unwrap();let program=GpuEdgeProgram::new(&device).unwrap();let mut caches=BTreeMap::new();let(mut comparisons,mut vertices,mut hits)=(0usize,0usize,0usize);
for frame in 0..317{
s.set_time(149.0*frame as f32/316.0);let mut expected=Vec::new();let mut ids_now=Vec::new();
for id in &ids{let mut mesh=s.eval_mesh(*id).value.mesh.clone();loom_mesh::edit_ops::transform_mesh(&mut mesh,s.object_matrix(*id)).unwrap();let style=style(&s,*id);assert!(!style.diagonals,"actual saved skin uses unsupported diagonal style");
let positions:Vec<[f32;4]>=mesh.vertices().iter().map(|v|[v.position.x,v.position.y,v.position.z,1.0]).collect();
if !caches.contains_key(id){let source=device.create_buffer(&wgpu::BufferDescriptor{label:None,size:positions.len()as u64*16,usage:wgpu::BufferUsages::STORAGE|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});let edges=GpuEdges::new(&device,&program,EdgeSource{buffer:&source,offset:0,vertex_count:positions.len()},&mesh,&style).unwrap();caches.insert(*id,(source,edges));}
let(source,edges)=caches.get_mut(id).unwrap();assert!(edges.matches_source(&EdgeSource{buffer:source,offset:0,vertex_count:positions.len()}));assert!(edges.matches_geometry(&mesh,&style).unwrap());if frame>0{hits+=1;}queue.write_buffer(source,0,bytemuck::cast_slice(&positions));edges.update_style(&queue,&style).unwrap();let color=style.wire_color.unwrap_or(style.color);let records:Vec<loom_render::grid::LineVertex>=mesh.unique_edges().into_iter().flat_map(|(a,b)|[loom_render::grid::LineVertex{position:a.to_array(),color},loom_render::grid::LineVertex{position:b.to_array(),color}]).collect();assert_eq!(edges.vertex_count()as usize,records.len());vertices+=records.len();expected.push(bytemuck::cast_slice::<_,u8>(&records).to_vec());ids_now.push(*id);
}
let bytes=expected.iter().map(|v|v.len()as u64).sum();let staging=device.create_buffer(&wgpu::BufferDescriptor{label:None,size:bytes,usage:wgpu::BufferUsages::COPY_DST|wgpu::BufferUsages::MAP_READ,mapped_at_creation:false});let mut encoder=device.create_command_encoder(&Default::default());let mut offset=0;for(id,data)in ids_now.iter().zip(&expected){let edges=&caches[id].1;edges.encode(&mut encoder);encoder.copy_buffer_to_buffer(edges.output_buffer(),0,&staging,offset,data.len()as u64);offset+=data.len()as u64;}
queue.submit([encoder.finish()]);let(tx,rx)=std::sync::mpsc::channel();staging.slice(..).map_async(wgpu::MapMode::Read,move|r|tx.send(r).unwrap());device.poll(wgpu::Maintain::Wait);rx.recv().unwrap().unwrap();{let data=staging.slice(..).get_mapped_range();let mut offset=0;for e in &expected{assert_eq!(&data[offset..offset+e.len()],e);offset+=e.len();comparisons+=1;}}staging.unmap();
}
eprintln!("GPU_EDGES_ACTUAL317 exact_streams={comparisons} vertices={vertices} descriptor_hits={hits}");}
