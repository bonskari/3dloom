//! Shared prepared geometry for direct manipulation of existing LoomRig limbs.

use glam::{Mat4, Vec3};
use loom_scene::{NodeId, ObjectKind, Scene};

// Canonical topology is shared by every control and never rebuilt for a pose.
static EDGES: [[usize; 2]; 12] = [[0,1],[1,2],[2,3],[3,0],[4,5],[5,6],[6,7],[7,4],[0,4],[1,5],[2,6],[3,7]];
static TRIANGLES: [[usize; 3]; 12] = [[0,2,1],[0,3,2],[4,5,6],[4,6,7],[0,1,5],[0,5,4],[1,2,6],[1,6,5],[2,3,7],[2,7,6],[3,0,4],[3,4,7]];
const VERTICES: [Vec3; 8] = [
    Vec3::new(-1., -1., 0.08), Vec3::new(1., -1., 0.08),
    Vec3::new(1., 1., 0.08), Vec3::new(-1., 1., 0.08),
    Vec3::new(-0.6, -0.6, 0.92), Vec3::new(0.6, -0.6, 0.92),
    Vec3::new(0.6, 0.6, 0.92), Vec3::new(-0.6, 0.6, 0.92),
];

pub(crate) struct ControlProxy {
    pub control: NodeId,
    pub vertices: [Vec3; 8],
    surface_pick: bool,
}
impl ControlProxy {
    pub fn segments(&self) -> impl Iterator<Item = (Vec3, Vec3)> + '_ {
        EDGES.iter().map(|[a, b]| (self.vertices[*a], self.vertices[*b]))
    }

    /// Pick the projected cage interior with the same camera-plane clipping as its outline.
    pub fn screen_depth(&self, point: eframe::egui::Pos2, view_proj: Mat4, rect: eframe::egui::Rect) -> Option<f32> {
        if !self.surface_pick { return None; }
        let clip = self.vertices.map(|v| view_proj * v.extend(1.));
        let cross = |u: eframe::egui::Vec2, v: eframe::egui::Vec2| u.x * v.y - u.y * v.x;
        let mut best: Option<f32> = None;
        for indices in TRIANGLES {
            let (polygon, count) = clip_face(indices.map(|i| clip[i]));
            for i in 1..count.saturating_sub(1) {
                let c = [polygon[0], polygon[i], polygon[i + 1]];
                let [p, q, r] = c.map(|v| crate::camera_overlay::project_clip(v, rect));
                let area = cross(q - p, r - p);
                if !area.is_finite() || area.abs() < 1e-6 { continue; }
                let u = cross(q - point, r - point) / area;
                let v = cross(r - point, p - point) / area;
                let w = 1. - u - v;
                if u < -1e-5 || v < -1e-5 || w < -1e-5 { continue; }
                let depth = u * c[0].z / c[0].w + v * c[1].z / c[1].w + w * c[2].z / c[2].w;
                if best.is_none_or(|old| depth < old) { best = Some(depth); }
            }
        }
        best
    }

    pub fn in_region(&self, region: &crate::region::Region, view_proj: Mat4, rect: eframe::egui::Rect, window: bool) -> bool {
        let mut segments = self.segments().filter_map(|(a, b)| crate::camera_overlay::segment(view_proj, rect, a, b));
        if window { return segments.all(|[a, b]| region.contains(a) && region.contains(b)); }
        if segments.any(|[a, b]| region.intersects_segment(a, b)) { return true; }
        let sample = match region {
            crate::region::Region::Rect(r) => Some(r.center()),
            crate::region::Region::Circle { center, .. } => Some(*center),
            crate::region::Region::Polygon(points) if points.len() >= 3 => points.first().copied(),
            crate::region::Region::Paint { path, .. } => path.first().copied(),
            _ => None,
        };
        sample.is_some_and(|p| self.screen_depth(p, view_proj, rect).is_some())
    }
}

// One camera-plane cut produces at most four vertices from a triangle.
fn clip_face(points: [glam::Vec4; 3]) -> ([glam::Vec4; 4], usize) {
    let mut out = [glam::Vec4::ZERO; 4];
    if points.iter().any(|p| !p.is_finite()) { return (out, 0); }
    let plane = crate::camera_overlay::MIN_PROJECT_W;
    let mut count = 0;
    let mut previous = points[2];
    for current in points {
        let inside = current.w >= plane;
        if inside != (previous.w >= plane) {
            out[count] = previous + (current - previous) * ((plane - previous.w) / (current.w - previous.w));
            count += 1;
        }
        if inside { out[count] = current; count += 1; }
        previous = current;
    }
    (out, count)
}

/// Shared display eligibility; frozen controls remain drawn but are not picked.
pub(crate) fn visible_node<'a>(app: &'a crate::LoomApp, proxy: &ControlProxy) -> Option<&'a loom_scene::Node> {
    app.state.scene.node(proxy.control).filter(|node| app.state.is_visible(node)
        && !(node.parent.is_some() && node.display.display_links && node.display.link_replaces_object))
}

fn box_offsets(x: Vec3, y: Vec3, z: Vec3) -> [Vec3; 8] {
    [-x-y-z,x-y-z,x+y-z,-x+y-z,-x-y+z,x-y+z,x+y+z,-x+y+z]
}

fn push_cage(scene: &Scene, out: &mut Vec<ControlProxy>, control: NodeId, origin: Vec3, offsets: [Vec3;8], surface_pick: bool) {
    let Some(node)=scene.node(control) else {return};
    let ObjectKind::Point {size,..}=&node.object else {return};
    let scale=size.get().abs()/4.;
    if !scale.is_finite() || scale==0. {return;}
    let vertices=offsets.map(|offset|origin+offset*scale);
    if vertices.iter().all(|point|point.is_finite()) {out.push(ControlProxy {control,vertices,surface_pick});}
}

/// Prepare once with the evaluated world pose. Existing rig/node data is read-only.
pub(crate) fn prepare(scene: &Scene, out: &mut Vec<ControlProxy>) {
    let _profile = loom_render::profile::Scope::new("ui.loomrig_prepare");
    out.clear();
    for rig in &scene.loom_rigs {
        let mut max_span=0f32;
        for limb in &rig.limbs {
            let joints=[limb.mapping.start,limb.mapping.middle,limb.mapping.end].map(|bone|scene.world_matrix(bone));
            let span=joints.windows(2).map(|pair|pair[0].w_axis.truncate().distance(pair[1].w_axis.truncate())).sum();
            max_span=max_span.max(span);
            for (index,(start, end)) in [(limb.mapping.start, limb.mapping.middle), (limb.mapping.middle, limb.mapping.end)].into_iter().enumerate() {
                let Some(binding) = rig.bindings.iter().find(|b| b.bone == start) else { continue };
                let Some(node) = scene.node(binding.control) else { continue };
                let ObjectKind::Point { size, .. } = &node.object else { continue };
                let width = size.get().abs() / 4.;
                if width == 0. || !width.is_finite() || scene.node(end).is_none() { continue; }
                let frame = scene.world_matrix(binding.control);
                let origin = frame.transform_point3(Vec3::ZERO);
                let target = joints[index+1].w_axis.truncate();
                let length = origin.distance(target);
                if !length.is_finite() || length <= f32::EPSILON { continue; }
                let axis = (target - origin) / length;
                let mut side = [Vec3::Y, Vec3::Z, Vec3::X].into_iter().find_map(|v| {
                    let v = frame.transform_vector3(v);
                    let p = v - axis * v.dot(axis);
                    (p.is_finite() && p.length_squared() > 1e-12).then_some(p)
                }).unwrap_or_else(|| axis.cross(if axis.z.abs() < 0.9 { Vec3::Z } else { Vec3::X }));
                side = side.normalize_or_zero();
                let up = axis.cross(side);
                let radius = length * 0.08 * width;
                let vertices = VERTICES.map(|v| origin + axis * (length * v.z) + side * (radius * v.x) + up * (radius * v.y));
                if vertices.iter().all(|v| v.is_finite()) { out.push(ControlProxy { control: binding.control, vertices, surface_pick:true }); }
            }
            let Some(binding)=rig.bindings.iter().find(|binding|binding.bone==limb.mapping.end) else {continue};
            let Ok(contact_weight)=limb.contacts.sample_blend(scene.time()) else {continue};
            let ik=contact_weight.unwrap_or_else(||limb.blend.get())>0.;
            let control=if ik {limb.goal} else {binding.control};
            let frame=scene.world_matrix(control);
            let end_frame=joints[2];
            let lower=end_frame.w_axis.truncate().distance(joints[1].w_axis.truncate());
            if !lower.is_finite() || lower<=f32::EPSILON {continue;}
            let mut direction=Vec3::ZERO;
            let mut child_length=0f32;
            let mut children=0;
            for child in scene.nodes().filter(|node|node.parent==Some(limb.mapping.end) && node.object.is_bone()) {
                let offset=child.transform.position.get();
                if offset.is_finite() && offset.length_squared()>1e-8 {
                    direction+=offset.normalize();children+=1;
                    child_length=child_length.max(end_frame.transform_vector3(offset).length());
                }
            }
            let direction=direction.try_normalize().unwrap_or(Vec3::X);
            let side=[Vec3::Z,Vec3::Y,Vec3::X].into_iter().find_map(|up|up.cross(direction).try_normalize()).unwrap();
            let up=direction.cross(side);
            let axis=|local| {let world=frame.transform_vector3(local);if ik {world} else {world.normalize_or_zero()}};
            let (forward,side,up)=(axis(direction),axis(side),axis(up));
            let length=(lower*0.45).max(child_length*1.2);
            let width=lower*if children>1 {0.2} else {0.14};
            let thick=lower*0.045;
            let near=-forward*length*0.1;
            let far=forward*length;
            let offsets=[near-side*width*0.65-up*thick,near+side*width*0.65-up*thick,
                near+side*width*0.65+up*thick,near-side*width*0.65+up*thick,
                far-side*width-up*thick*0.65,far+side*width-up*thick*0.65,
                far+side*width+up*thick*0.65,far-side*width+up*thick*0.65];
            push_cage(scene,out,control,frame.w_axis.truncate(),offsets,true);
        }
        if let Some(binding)=rig.bindings.iter().find(|binding|binding.bone==rig.mapping.pelvis) {
            if !out.iter().any(|proxy|proxy.control==binding.control) {
                let frame=scene.world_matrix(binding.control);
                let origin=frame.w_axis.truncate();
                let next=rig.mapping.spine.first().copied().unwrap_or(rig.mapping.head);
                let delta=scene.world_matrix(next).w_axis.truncate()-origin;
                if let Some(up)=delta.try_normalize() {
                    let mut closest:Option<(f32,Vec3)>=None;
                    for limb in &rig.limbs {
                        let offset=scene.world_matrix(limb.mapping.start).w_axis.truncate()-origin;
                        let side=offset-up*offset.dot(up);
                        let distance=side.length();
                        if distance>1e-4 && closest.is_none_or(|old|offset.length()<old.0) {closest=Some((offset.length(),side));}
                    }
                    let side=closest.map(|(_,side)|side).unwrap_or_else(|| {
                        [frame.x_axis.truncate(),frame.y_axis.truncate(),frame.z_axis.truncate()].into_iter()
                            .find_map(|axis|(axis-up*axis.dot(up)).try_normalize()).unwrap_or(Vec3::ZERO)*delta.length()*0.5
                    });
                    let width=side.length()*1.4;
                    let lateral=side.normalize_or_zero();
                    push_cage(scene,out,binding.control,origin,box_offsets(lateral*width,up.cross(lateral)*width*0.65,up*delta.length()*0.5),true);
                }
            }
        }
        if max_span>f32::EPSILON {
            let frame=scene.world_matrix(rig.master);
            let x=frame.x_axis.truncate().normalize_or_zero();
            let y=frame.y_axis.truncate().normalize_or_zero();
            let z=frame.z_axis.truncate().normalize_or_zero();
            push_cage(scene,out,rig.master,frame.w_axis.truncate(),box_offsets(x*max_span*0.4,y*max_span*0.28,z*max_span*0.016),false);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use glam::Vec3;
    use loom_commands::{AppState, Command};
    use loom_scene::{Node, ObjectKind};
    use loom_scene::loomrig::{HumanoidMapping, LimbMapping};

    pub(crate) fn rig() -> (AppState, HumanoidMapping) {
        let mut state = AppState::new();
        let root = state.scene.add_node(Node::new("root", ObjectKind::bone(1.)));
        let mut joints = Vec::new();
        for i in 0..3 {
            let mut node = Node::new("joint", ObjectKind::bone(10.));
            node.parent = Some(if i == 0 { root } else { joints[i - 1] });
            if i > 0 { node.transform.position.set(Vec3::X * 10.); }
            joints.push(state.scene.add_node(node));
        }
        let mapping = HumanoidMapping { root, pelvis: joints[0], spine: vec![], head: joints[2],
            limbs: vec![LimbMapping { name: "Arm".into(), start: joints[0], middle: joints[1], end: joints[2] }] };
        loom_commands::execute(&mut state, Command::CreateLoomRig { root: root.0, mapping: Some(mapping.clone()) }).unwrap();
        (state, mapping)
    }

    pub(crate) fn body_rig() -> (AppState, HumanoidMapping) {
        fn bone(s: &mut AppState, parent: Option<NodeId>, position: Vec3) -> NodeId {
            let mut node=Node::new("arbitrary joint",ObjectKind::bone(10.));
            node.parent=parent;node.transform.position.set(position);s.scene.add_node(node)
        }
        let mut s=AppState::new();
        let root=bone(&mut s,None,Vec3::ZERO);
        let pelvis=bone(&mut s,Some(root),Vec3::Z*100.);
        let spine=bone(&mut s,Some(pelvis),Vec3::Z*30.);
        let head=bone(&mut s,Some(spine),Vec3::Z*50.);
        let mut limbs=Vec::new();
        for arm in [true,false] { for side in [-1.,1.] {
            let start=bone(&mut s,Some(if arm {spine} else {pelvis}),if arm {Vec3::new(side*30.,0.,25.)} else {Vec3::new(side*10.,0.,-5.)});
            let step=if arm {Vec3::X*side*20.} else {Vec3::Z*-40.};
            let middle=bone(&mut s,Some(start),step);
            let end=bone(&mut s,Some(middle),step);
            if arm {
                for spread in [-3.,0.,3.] {bone(&mut s,Some(end),Vec3::new(side*8.,spread,0.));}
            } else {bone(&mut s,Some(end),Vec3::Y*20.);}
            limbs.push(LimbMapping {name:format!("chain {}",limbs.len()),start,middle,end});
        }}
        let mapping=HumanoidMapping {root,pelvis,spine:vec![spine],head,limbs};
        loom_commands::execute(&mut s,Command::CreateLoomRig {root:root.0,mapping:Some(mapping.clone())}).unwrap();
        (s,mapping)
    }

    #[test]
    fn body_and_end_shapes_use_existing_distinct_controls_without_scene_edits() {
        let (s,mapping)=body_rig();let rig=&s.scene.loom_rigs[0];
        let original=serde_json::to_value(&s.scene).unwrap();
        let mut proxies=Vec::new();prepare(&s.scene,&mut proxies);
        let ids:std::collections::BTreeSet<_>=proxies.iter().map(|proxy|proxy.control).collect();
        assert_eq!(proxies.len(),14,"eight limb spans, four ends, pelvis and master");
        assert_eq!(ids.len(),proxies.len(),"each visible controller has one pick proxy");
        assert!(ids.contains(&rig.master));
        for bone in std::iter::once(mapping.pelvis).chain(mapping.limbs.iter().map(|limb|limb.end)) {
            let control=rig.bindings.iter().find(|binding|binding.bone==bone).unwrap().control;
            assert!(ids.contains(&control),"missing direct end/body control {control:?}");
        }
        assert_eq!(serde_json::to_value(&s.scene).unwrap(),original);
    }

    #[test]
    fn end_shape_routes_by_effective_contact_weight_and_follows_endpoint_orientation() {
        let (mut s,mapping)=body_rig();let limb=s.scene.loom_rigs[0].limbs[1].clone();
        let end=s.scene.loom_rigs[0].bindings.iter().find(|binding|binding.bone==limb.mapping.end).unwrap().control;
        let mut proxies=Vec::new();prepare(&s.scene,&mut proxies);
        let original=proxies.iter().find(|proxy|proxy.control==end).expect("FK End shape").vertices;
        loom_commands::execute(&mut s,Command::SetTransform {id:end.0,position:None,rotation_deg:Some([0.,0.,90.]),scale:None}).unwrap();
        prepare(&s.scene,&mut proxies);
        let changed=proxies.iter().find(|proxy|proxy.control==end).unwrap();
        let pivot=s.scene.world_matrix(limb.mapping.end).w_axis.truncate();
        for (a,b) in original.into_iter().zip(changed.vertices) {
            assert!(b.abs_diff_eq(pivot+glam::Quat::from_rotation_z(90f32.to_radians())*(a-pivot),0.001));
        }
        for blend in [0.35,1.,0.] {
            loom_commands::execute(&mut s,Command::SetLoomRigLimb {rig:1,limb:limb.mapping.name.clone(),solver:"limb".into(),blend}).unwrap();
            prepare(&s.scene,&mut proxies);
            assert!(proxies.iter().any(|proxy|proxy.control==if blend>0. {limb.goal} else {end}));
        }
        loom_commands::execute(&mut s,Command::SetLoomRigContactPreset {rig:1,limb:limb.mapping.name.clone(),time:0.,preset:"planted".into()}).unwrap();
        prepare(&s.scene,&mut proxies);
        assert!(proxies.iter().any(|proxy|proxy.control==limb.goal));
        assert!(!proxies.iter().any(|proxy|proxy.control==end));
        loom_commands::execute(&mut s,Command::SetLoomRigContactPreset {rig:1,limb:limb.mapping.name.clone(),time:0.,preset:"free".into()}).unwrap();
        prepare(&s.scene,&mut proxies);
        assert!(proxies.iter().any(|proxy|proxy.control==end));
        assert!(!proxies.iter().any(|proxy|proxy.control==limb.goal));
        assert_eq!(mapping.limbs.len(),4);
    }

    #[test]
    fn master_outline_does_not_pick_its_interior_and_shapes_remain_finite_under_mirrors() {
        let (mut s,_)=body_rig();let master=s.scene.loom_rigs[0].master;
        let mut proxies=Vec::new();prepare(&s.scene,&mut proxies);
        let proxy=proxies.iter().find(|proxy|proxy.control==master).expect("master outline");
        let rect=eframe::egui::Rect::from_min_size(eframe::egui::Pos2::ZERO,eframe::egui::vec2(600.,400.));
        let view=Mat4::look_at_rh(Vec3::Z*300.,Vec3::ZERO,Vec3::Y);
        let projection=Mat4::orthographic_rh(-100.,100.,-100.,100.,0.1,1000.);
        assert!(proxy.screen_depth(rect.center(),projection*view,rect).is_none(),"master floor outline must not steal interior foot selections");
        s.scene.node_mut(s.scene.loom_rigs[0].mapping.root).unwrap().transform.scale.set(Vec3::new(-2.,3.,0.5));s.scene.bump_version();
        let original=serde_json::to_value(&s.scene).unwrap();prepare(&s.scene,&mut proxies);
        assert_eq!(proxies.len(),14);
        assert!(proxies.iter().flat_map(|proxy|proxy.vertices).all(|point|point.is_finite()));
        assert_eq!(serde_json::to_value(&s.scene).unwrap(),original);
    }

    #[test]
    fn master_size_is_independent_of_limb_helper_sizes() {
        let (mut s,_)=body_rig();let rig=s.scene.loom_rigs[0].clone();
        for limb in &rig.limbs {for bone in [limb.mapping.start,limb.mapping.middle] {
            let control=rig.bindings.iter().find(|binding|binding.bone==bone).unwrap().control;
            if let ObjectKind::Point {size,..}=&mut s.scene.node_mut(control).unwrap().object {size.set(0.);}
        }}
        s.scene.bump_version();let mut proxies=Vec::new();prepare(&s.scene,&mut proxies);
        assert!(proxies.iter().any(|proxy|proxy.control==rig.master),"changing limb glyph size must not remove the master outline");
    }

    #[test]
    #[ignore = "requires LOOM_RAY_CONTACT_SOURCE production scene"]
    fn production_ray_end_body_shapes_preserve_source_and_follow_fractional_poses() {
        use std::hash::Hasher;
        struct Fingerprint(std::collections::hash_map::DefaultHasher);
        impl std::io::Write for Fingerprint {
            fn write(&mut self,bytes:&[u8])->std::io::Result<usize> {self.0.write(bytes);Ok(bytes.len())}
            fn flush(&mut self)->std::io::Result<()> {Ok(())}
        }
        fn source(scene:&Scene)->u64 {
            let mut hash=Fingerprint(Default::default());
            for node in scene.nodes() {serde_json::to_writer(&mut hash,node).unwrap();}
            serde_json::to_writer(&mut hash,&scene.materials).unwrap();hash.0.finish()
        }
        let mut s=AppState::new();
        loom_commands::execute(&mut s,Command::OpenFile {path:Some(std::env::var("LOOM_RAY_CONTACT_SOURCE").unwrap())}).unwrap();
        let original=source(&s.scene);let metadata=s.scene.loom_rigs.clone();
        let rig=&metadata[0];let mut proxies=Vec::new();let mut previous=None;
        for time in [39.,39.5,80.,0.] {
            s.scene.set_time(time);prepare(&s.scene,&mut proxies);
            assert_eq!(proxies.len(),14,"all four limbs, four ends, pelvis and master at {time}");
            let ids:std::collections::BTreeSet<_>=proxies.iter().map(|proxy|proxy.control).collect();assert_eq!(ids.len(),14);
            assert!(proxies.iter().flat_map(|proxy|proxy.vertices).all(|point|point.is_finite()));
            for limb in &rig.limbs {
                let end=rig.bindings.iter().find(|binding|binding.bone==limb.mapping.end).unwrap().control;
                assert!(ids.contains(&end));
            }
            let geometry:Vec<_>=proxies.iter().map(|proxy|proxy.vertices).collect();
            if let Some(before)=previous {assert_ne!(geometry,before,"end/body geometry must follow the evaluated fractional source pose");}
            previous=Some(geometry);
            assert_eq!(s.scene.loom_rigs,metadata);
        }
        assert_eq!(source(&s.scene),original,"overlay preparation changed source nodes, Skin weights or materials");
        assert!(!s.undo.can_undo());
    }

    #[test]
    fn prepared_limb_outlines_draw_on_top_in_both_shading_modes() {
        let (state, _) = rig();
        let mut app = crate::LoomApp::headless();
        app.state = state;
        app.state.mesh_dirty = true;
        app.sync_after_command();
        assert_eq!(app.rig_controls.len(), 4);
        let rect = eframe::egui::Rect::from_min_size(eframe::egui::Pos2::ZERO, eframe::egui::vec2(600., 400.));
        let expected: Vec<_> = app.rig_controls.iter().flat_map(|p| p.segments()).collect();
        for realistic in [false, true] {
            let (_, top) = crate::helper_overlay::lines(&app, glam::Mat4::IDENTITY, rect, realistic);
            for &(a, b) in &expected { assert!(top.iter().any(|&(x, y, _)| x == a && y == b), "visible limb outline must draw above the skin in either shading mode"); }
        }
    }

    #[test]
    fn limb_proxies_follow_current_fk_pose_without_editing_scene() {
        let (mut state, mapping) = rig();
        let control = state.scene.loom_rigs[0].bindings.iter().find(|b| b.bone == mapping.pelvis).unwrap().control;
        state.scene.node_mut(mapping.root).unwrap().transform.scale.set(Vec3::new(-2., 3., 0.5));
        state.scene.node_mut(control).unwrap().transform.rotation.set(Vec3::new(20., 0., 35.));
        let before = serde_json::to_value(&state.scene).unwrap();
        let mut proxies = Vec::new();
        prepare(&state.scene, &mut proxies);
        assert_eq!(proxies.len(), 4);
        for (proxy, (start, end)) in proxies.iter().zip([(mapping.pelvis, mapping.limbs[0].middle), (mapping.limbs[0].middle, mapping.head)]) {
            let expected_control = state.scene.loom_rigs[0].bindings.iter().find(|b| b.bone == start).unwrap().control;
            assert_eq!(proxy.control, expected_control);
            let a = state.scene.world_matrix(start).transform_point3(Vec3::ZERO);
            let b = state.scene.world_matrix(end).transform_point3(Vec3::ZERO);
            let near = proxy.vertices[..4].iter().copied().sum::<Vec3>() / 4.;
            let far = proxy.vertices[4..].iter().copied().sum::<Vec3>() / 4.;
            assert!(near.abs_diff_eq(a.lerp(b, 0.08), 1e-4));
            assert!(far.abs_diff_eq(a.lerp(b, 0.92), 1e-4));
            assert_eq!(proxy.segments().count(), 12);
            assert!(proxy.vertices.iter().all(|v| v.is_finite()));
        }
        assert_eq!(serde_json::to_value(&state.scene).unwrap(), before);
    }

    #[test]
    fn limb_proxy_storage_is_reused_and_scene_replacement_clears_it() {
        let (mut state, mapping) = rig();
        let mut proxies = Vec::new();
        prepare(&state.scene, &mut proxies);
        let allocation = proxies.as_ptr();
        let original = proxies[0].vertices;
        state.scene.node_mut(mapping.root).unwrap().transform.position.set(Vec3::new(4., 9., 2.));
        prepare(&state.scene, &mut proxies);
        assert_eq!(proxies.as_ptr(), allocation);
        assert_ne!(proxies[0].vertices, original);
        let reads = loom_scene::anim::animated_reads();
        for _ in 0..4 { assert_eq!(proxies.iter().flat_map(|p| p.segments()).count(), 48); }
        assert_eq!(loom_scene::anim::animated_reads(), reads);
        prepare(&loom_scene::Scene::new(), &mut proxies);
        assert!(proxies.is_empty());
    }

    #[test]
    fn collapsed_limb_span_does_not_generate_nonfinite_proxy_geometry() {
        let (mut state, mapping) = rig();
        state.scene.node_mut(mapping.limbs[0].middle).unwrap().transform.position.set(Vec3::ZERO);
        let mut proxies = Vec::new();
        prepare(&state.scene, &mut proxies);
        assert_eq!(proxies.len(), 4);
        assert!(proxies[0].vertices.iter().all(|v| v.is_finite()));
    }
}
