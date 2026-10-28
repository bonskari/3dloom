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
}
impl ControlProxy {
    pub fn segments(&self) -> impl Iterator<Item = (Vec3, Vec3)> + '_ {
        EDGES.iter().map(|[a, b]| (self.vertices[*a], self.vertices[*b]))
    }

    /// Pick the projected cage interior with the same camera-plane clipping as its outline.
    pub fn screen_depth(&self, point: eframe::egui::Pos2, view_proj: Mat4, rect: eframe::egui::Rect) -> Option<f32> {
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

/// Prepare once with the evaluated world pose. Existing rig/node data is read-only.
pub(crate) fn prepare(scene: &Scene, out: &mut Vec<ControlProxy>) {
    out.clear();
    for rig in &scene.loom_rigs {
        for limb in &rig.limbs {
            for (start, end) in [(limb.mapping.start, limb.mapping.middle), (limb.mapping.middle, limb.mapping.end)] {
                let Some(binding) = rig.bindings.iter().find(|b| b.bone == start) else { continue };
                let Some(node) = scene.node(binding.control) else { continue };
                let ObjectKind::Point { size, .. } = &node.object else { continue };
                let width = size.get().abs() / 4.;
                if width == 0. || !width.is_finite() || scene.node(end).is_none() { continue; }
                let frame = scene.world_matrix(binding.control);
                let origin = frame.transform_point3(Vec3::ZERO);
                let target = scene.world_matrix(end).transform_point3(Vec3::ZERO);
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
                if vertices.iter().all(|v| v.is_finite()) { out.push(ControlProxy { control: binding.control, vertices }); }
            }
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

    #[test]
    fn prepared_limb_outlines_draw_on_top_in_both_shading_modes() {
        let (state, _) = rig();
        let mut app = crate::LoomApp::headless();
        app.state = state;
        app.state.mesh_dirty = true;
        app.sync_after_command();
        assert_eq!(app.rig_controls.len(), 2);
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
        assert_eq!(proxies.len(), 2);
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
        for _ in 0..4 { assert_eq!(proxies.iter().flat_map(|p| p.segments()).count(), 24); }
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
        assert_eq!(proxies.len(), 1);
        assert!(proxies[0].vertices.iter().all(|v| v.is_finite()));
    }
}
