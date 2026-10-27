//! Test-only upstream final-bucket investigation; production unchanged.
use super::*;
thread_local! { static CORRECT: std::cell::Cell<bool> = const { std::cell::Cell::new(true) }; }
pub(super) fn correct_final_buckets() -> bool { CORRECT.with(std::cell::Cell::get) }

#[test]
fn final_buckets_pair_every_manifold_opposite_edge() {
    let base = [[3, 4, 0], [4, 3, 1], [3, 5, 1], [5, 3, 2]];
    let mut failures = 0;
    for a in 0..4 { for b in 0..4 { for c in 0..4 { for d in 0..4 {
        let order = [a,b,c,d];
        if (0..4).any(|i| (i+1..4).any(|j| order[i] == order[j])) { continue; }
        let ids: Vec<i32> = order.iter().flat_map(|i| base[*i]).collect();
        let mut expected = vec![[-1;3];4];
        for f in 0..4 { for e in 0..3 {
            for g in 0..4 { for j in 0..3 {
                if f != g && ids[f*3+e] == ids[g*3+(j+1)%3] && ids[f*3+(e+1)%3] == ids[g*3+j] { expected[f][e] = g as i32; }
            } }
        } }
        for corrected in [false,true] {
            CORRECT.with(|v| v.set(corrected));
            let mut triangles = vec![STriInfo::zero();4];
            for tri in &mut triangles { tri.FaceNeighbors = [-1;3]; }
            let mut edges = vec![SEdge::zero();12];
            unsafe { BuildNeighborsFast(triangles.as_mut_ptr(), edges.as_mut_ptr(), ids.as_ptr(), 4, None); }
            let actual: Vec<_> = triangles.iter().map(|t| t.FaceNeighbors).collect();
            if corrected { assert_eq!(actual, expected, "order={order:?}"); }
            else { failures += usize::from(actual != expected); }
        }
    } } } }
    CORRECT.with(|v| v.set(true));
    assert!(failures > 0, "original must exhibit omission");
    eprintln!("NEIGHBOR original_incorrect_permutations={failures}/24 corrected_incorrect=0/24");
}

#[test]
#[ignore = "actual corrected-neighbor tangent probe requires LOOM_PERF_SCENE"]
fn actual_ray_corrected_neighbors_and_edge_incidence() {
    use crate::convert::mesh_data_by_id;
    use super::super::{capture, generate_parallel};
    let path = std::env::var("LOOM_PERF_SCENE").expect("real scene");
    let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let scene: loom_scene::Scene = serde_json::from_value(value["scene"].clone()).unwrap();
    let ids: Vec<_> = scene.nodes().filter(|n| n.name == "Body001" || n.name == "Face001").map(|n| (n.id,n.name.clone())).collect();
    assert_eq!(ids.len(),2);
    let pool = bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();
    let mut totals = [(0usize,0usize,0usize,0usize,0.0f32);2];
    let (mut reference_ms,mut cached_ms)=(0.0,0.0);
    let mut caches=[super::super::NeighborCache::default(),super::super::NeighborCache::default()];
    capture::enable();
    for frame in (0..=157).map(|f| f as f32).chain([39.25]) {
        scene.set_time(frame);
        for (i,(id,_)) in ids.iter().enumerate() {
            let mut mesh = scene.eval_mesh(*id).value.mesh.clone();
            loom_mesh::edit_ops::transform_mesh(&mut mesh,scene.object_matrix(*id)).unwrap();
            let data = mesh_data_by_id(&mesh).into_iter().find(|(id,_)| *id == 0).unwrap().1;
            CORRECT.with(|v| v.set(false));
            let original = generate_parallel(&pool,&data).unwrap();
            let snapshot = capture::take();
            let mut edges = std::collections::HashMap::<(i32,i32),usize>::new();
            for (tri,(_,flags)) in snapshot.indices.chunks_exact(3).zip(&snapshot.flags) {
                if flags & 1 != 0 { continue; }
                for e in 0..3 { let (a,b) = (tri[e],tri[(e+1)%3]); *edges.entry((a.min(b),a.max(b))).or_default() += 1; }
            }
            let ambiguous = edges.values().filter(|n| **n > 2).count();
            totals[i].2 += ambiguous;
            totals[i].3 = totals[i].3.max(ambiguous);
            CORRECT.with(|v| v.set(true));
            let timer=std::time::Instant::now();
            let corrected = generate_parallel(&pool,&data).unwrap();
            reference_ms+=timer.elapsed().as_secs_f64()*1000.0;
            let _ = capture::take();
            let timer=std::time::Instant::now();
            let cached=super::super::generate_cached(&pool,&data,&mut caches[i]).unwrap();
            cached_ms+=timer.elapsed().as_secs_f64()*1000.0;
            let _ = capture::take();
            assert_eq!(corrected.iter().map(|v|v.map(f32::to_bits)).collect::<Vec<_>>(),cached.iter().map(|v|v.map(f32::to_bits)).collect::<Vec<_>>(),"corrected cache frame={frame}");
            let mut changed = 0;
            for (a,b) in original.iter().zip(&corrected) {
                if a.map(f32::to_bits) != b.map(f32::to_bits) { changed += 1; }
                for c in 0..4 { totals[i].4 = totals[i].4.max((a[c]-b[c]).abs()); }
            }
            if changed > 0 { eprintln!("CHANGED name={} frame={frame} corners={changed}",ids[i].1); }
            totals[i].0 += usize::from(changed > 0);
            totals[i].1 += changed;
        }
    }
    CORRECT.with(|v| v.set(true));
    eprintln!("CACHE hits={} misses={} corrected_ms_per_pose={:.3} cached_ms_per_pose={:.3}",caches.iter().map(|c|c.hits).sum::<usize>(),caches.iter().map(|c|c.misses).sum::<usize>(),reference_ms/159.0,cached_ms/159.0);
    for ((_,name),(poses,corners,ambiguous,max_ambiguous,max_delta)) in ids.iter().zip(totals) {
        eprintln!("CORRECTED name={name} poses=159 changed_poses={poses} changed_corners={corners} ambiguous_edges_sum={ambiguous} ambiguous_edges_max={max_ambiguous} max_component_delta={max_delta}");
    }
}

#[test]
#[ignore = "actual textured GPU comparison requires LOOM_PERF_SCENE and LOOM_PROBE_FRAME"]
fn actual_ray_corrected_neighbor_pixels() {
    use crate::{BevyRenderer,SceneObject,ViewCamera,ObjectMaterial};
    use crate::convert::mesh_data_by_id;
    use super::super::generate_parallel;
    use bevy::asset::Assets;
    use bevy::render::mesh::{Mesh,Mesh3d};
    let path = std::env::var("LOOM_PERF_SCENE").expect("real scene");
    let frame: f32 = std::env::var("LOOM_PROBE_FRAME").expect("affected frame").parse().unwrap();
    let directory = std::env::var("LOOM_PROBE_OUTPUT").expect("output directory");
    std::fs::create_dir_all(&directory).unwrap();
    let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let scene: loom_scene::Scene = serde_json::from_value(value["scene"].clone()).unwrap();
    scene.set_time(frame);
    let id = scene.nodes().find(|n| n.name == "Face001").unwrap().id;
    let surfaces = scene.node_surfaces(id).expect("actual Face materials");
    let mut mesh = scene.eval_mesh(id).value.mesh.clone();
    loom_mesh::edit_ops::transform_mesh(&mut mesh,scene.object_matrix(id)).unwrap();
    let data = mesh_data_by_id(&mesh).into_iter().find(|(id,_)| *id==0).unwrap().1;
    let pool = bevy_tasks::TaskPoolBuilder::new().num_threads(4).build();
    CORRECT.with(|v|v.set(false));
    let original = generate_parallel(&pool,&data).unwrap();
    CORRECT.with(|v|v.set(true));
    let corrected = generate_parallel(&pool,&data).unwrap();
    CORRECT.with(|v|v.set(true));
    assert_ne!(original,corrected,"must compare an affected actual pose");
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("GPU adapter");
    assert!(crate::supported(&adapter));
    let (device,queue) = pollster::block_on(adapter.request_device(&crate::device_descriptor(&adapter),None)).unwrap();
    let mut renderer = BevyRenderer::new(adapter,device,queue);
    renderer.sync(&[SceneObject{id:1,mesh:&mesh,transform:glam::Mat4::IDENTITY,wire_color:[255;3],surfaces:Some(surfaces),renderable:true,flags:Default::default()}]);
    let object = &renderer.objects[&1];
    let part = object.part_ids.iter().position(|id|*id==Some(0)).unwrap();
    let (entity,material) = &object.parts[part];
    let entity=*entity;
    let ObjectMaterial::Standard(material) = material else { panic!("regular actual StandardMaterial required") };
    let mat = renderer.app.world().resource::<Assets<bevy::pbr::StandardMaterial>>().get(material).unwrap();
    assert!(mat.normal_map_texture.is_some(),"actual Face normal texture must load");
    assert!(mat.base_color_texture.is_some(),"actual Face diffuse texture must load");
    let texture=renderer.app.world().resource::<Assets<bevy::image::Image>>().get(mat.normal_map_texture.as_ref().unwrap()).unwrap();
    let bytes=texture.data.as_ref().expect("loaded normal texture data");
    let colors:std::collections::HashSet<_>=bytes.chunks_exact(4).map(|p|[p[0],p[1],p[2]]).collect();
    assert!(colors.len()>8,"actual normal texture must vary");
    eprintln!("NORMAL_TEXTURE width={} height={} unique_rgb={}",texture.texture_descriptor.size.width,texture.texture_descriptor.size.height,colors.len());
    let lo = data.positions.iter().copied().map(glam::Vec3::from).fold(glam::Vec3::splat(f32::INFINITY),glam::Vec3::min);
    let hi = data.positions.iter().copied().map(glam::Vec3::from).fold(glam::Vec3::splat(f32::NEG_INFINITY),glam::Vec3::max);
    let center = (lo+hi)*0.5;
    let radius = (hi-lo).max_element()*1.5;
    for (angle,direction) in [("front",glam::Vec3::new(0.0,1.0,0.05)),("left",glam::Vec3::new(-0.7,1.0,0.05)),("right",glam::Vec3::new(0.7,1.0,0.05)),("back",glam::Vec3::new(0.0,-1.0,0.05))] {
        let light=renderer.light.unwrap();
        *renderer.app.world_mut().get_mut::<bevy::transform::components::Transform>(light).unwrap()=bevy::transform::components::Transform::from_translation(bevy::math::Vec3::from(direction+glam::Vec3::Z)).looking_at(bevy::math::Vec3::ZERO,bevy::math::Vec3::Z);
        let camera = ViewCamera{view:glam::Mat4::look_at_rh(center+direction.normalize()*radius,center,glam::Vec3::Z),projection:glam::Mat4::perspective_rh(35f32.to_radians(),1.0,0.01,10000.0)};
        let mut images = Vec::new();
        for (name,tangents) in [("original",&original),("corrected",&corrected)] {
            let mut posed = data.to_bevy();
            posed.insert_attribute(Mesh::ATTRIBUTE_TANGENT,tangents.clone());
            let next=renderer.app.world_mut().resource_mut::<Assets<Mesh>>().add(posed);
            renderer.app.world_mut().entity_mut(entity).insert(Mesh3d(next));
            let image = renderer.render_image(768,768,&camera).unwrap();
            image.save(format!("{directory}/face-{frame}-{angle}-{name}.png")).unwrap();
            images.push(image);
        }
        if angle == "front" {
            let mut control=data.to_bevy();
            let values: Vec<_>=original.iter().map(|t|[t[1],-t[0],t[2],-t[3]]).collect();
            control.insert_attribute(Mesh::ATTRIBUTE_TANGENT,values);
            let next=renderer.app.world_mut().resource_mut::<Assets<Mesh>>().add(control);
            renderer.app.world_mut().entity_mut(entity).insert(Mesh3d(next));
            let image=renderer.render_image(768,768,&camera).unwrap();
            let changed=image.pixels().zip(images[0].pixels()).filter(|(a,b)|a!=b).count();
            assert!(changed>100,"tangent mutation must affect the real textured render: {changed}");
            eprintln!("PIXEL_CONTROL frame={frame} changed={changed}");
        }
        let mut changed=0usize; let mut max=0u8; let mut sum=0u64;
        for (a,b) in images[0].pixels().zip(images[1].pixels()) {
            changed += usize::from(a!=b);
            for c in 0..4 {let delta=a[c].abs_diff(b[c]);max=max.max(delta);sum+=u64::from(delta);}
        }
        eprintln!("PIXELS frame={frame} angle={angle} changed={changed}/589824 max_channel_delta={max} mean_channel_delta={:.8}",sum as f64/(768.0*768.0*4.0));
    }
}
