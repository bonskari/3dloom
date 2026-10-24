//! Phase 9 mapping helpers: Camera Map (+ WSM), MapScaler (+ WSM), UVW
//! Mapping Add / Clear; Unwrap UVW Sketch, Paint Select, Spline Map, Edge
//! Snap and the Freeform gizmo.

use glam::{Vec2, Vec3};
use loom_commands::{execute, AppState, Command, PrimitiveKind, SelectOp};
use loom_mesh::Mesh;
use loom_scene::NodeId;

fn prim(s: &mut AppState, kind: PrimitiveKind, position: [f32; 3], size: [f32; 3]) -> NodeId {
    execute(s, Command::CreatePrimitive { kind, position, size }).unwrap();
    *s.selection.iter().next().unwrap()
}

fn add(s: &mut AppState, name: &str) {
    execute(s, Command::AddModifier { name: name.into() }).unwrap();
}

fn set(s: &mut AppState, index: u32, name: &str, value: f64) {
    execute(s, Command::SetModifierParameter { index, name: name.into(), value }).unwrap();
}

/// The channel's coordinate at every vertex (first corner found).
fn uv_at_vertices(m: &Mesh, ch: u32) -> Vec<Vec3> {
    let map = m.map(ch).expect("the channel exists");
    let mut out = vec![Vec3::NAN; m.vertex_count()];
    for (i, he) in m.half_edges().iter().enumerate() {
        if he.face.is_some() {
            out[he.origin.0 as usize] = map.verts[map.corners[i] as usize];
        }
    }
    out
}

/// The channel's coordinate at every face corner.
fn corner_uvs(m: &Mesh, ch: u32) -> Vec<Vec3> {
    let map = m.map(ch).expect("the channel exists");
    m.half_edges().iter().enumerate().filter(|(_, h)| h.face.is_some()).map(|(i, _)| map.verts[map.corners[i] as usize]).collect()
}

fn camera_at(s: &mut AppState, pos: [f32; 3]) -> NodeId {
    let r = execute(s, Command::CreateCamera { target: false }).unwrap();
    let id = NodeId(r["id"].as_u64().or_else(|| r["node"].as_u64()).expect("camera id"));
    execute(s, Command::SetTransform { id: id.0, position: Some(pos), rotation_deg: Some([0.0; 3]), scale: None }).unwrap();
    id
}

#[test]
fn camera_map_projects_from_the_camera() {
    let mut s = AppState::new();
    let cam = camera_at(&mut s, [0.0, 0.0, 100.0]);
    let fov = match &s.scene.node(cam).unwrap().object { loom_scene::ObjectKind::Camera { fov, .. } => fov.get(), _ => unreachable!() };
    let p = prim(&mut s, PrimitiveKind::Plane, [0.0; 3], [100.0, 100.0, 0.0]);
    add(&mut s, "Camera Map");
    execute(&mut s, Command::PickModifierNode { index: 0, node: cam.0 }).unwrap();
    let m = s.scene.eval_mesh(p).value.mesh.clone();
    let uv = uv_at_vertices(&m, 1);
    let half_h = (fov.to_radians() * 0.5).tan() * 100.0;
    let half_w = half_h * 4.0 / 3.0;
    for (v, t) in m.vertices().iter().zip(&uv) {
        let want = Vec2::new(0.5 + 0.5 * v.position.x / half_w, 0.5 + 0.5 * v.position.y / half_h);
        assert!(Vec2::new(t.x, t.y).distance(want) < 1e-3, "{} -> {t} want {want}", v.position);
    }
    // The object-space version keeps the mapping when the camera moves on; the WSM follows it.
    execute(&mut s, Command::SetTransform { id: cam.0, position: Some([30.0, 0.0, 100.0]), rotation_deg: None, scale: None }).unwrap();
    assert_eq!(uv_at_vertices(&s.scene.eval_mesh(p).value.mesh, 1), uv);
    let mut t = AppState::new();
    let cam2 = camera_at(&mut t, [0.0, 0.0, 100.0]);
    let p2 = prim(&mut t, PrimitiveKind::Plane, [0.0; 3], [100.0, 100.0, 0.0]);
    add(&mut t, "Camera Map (WSM)");
    execute(&mut t, Command::PickModifierNode { index: 0, node: cam2.0 }).unwrap();
    let before = uv_at_vertices(&t.scene.eval_mesh(p2).value.mesh, 1);
    execute(&mut t, Command::SetTransform { id: cam2.0, position: Some([30.0, 0.0, 100.0]), rotation_deg: None, scale: None }).unwrap();
    let after = uv_at_vertices(&t.scene.eval_mesh(p2).value.mesh, 1);
    assert!((before[0].x - after[0].x - 0.5 * 30.0 / half_w).abs() < 1e-3, "{} {}", before[0], after[0]);
}

#[test]
fn map_scaler_keeps_texel_size() {
    // The WSM keeps the map's world size when the object is scaled.
    let mut s = AppState::new();
    let b = prim(&mut s, PrimitiveKind::Box, [0.0; 3], [20.0; 3]);
    add(&mut s, "MapScaler (WSM)");
    set(&mut s, 0, "Scale", 10.0);
    // Wrap Texture on: the sides share one U running round the box (8 = 80 / 10).
    let wrapped = corner_uvs(&s.scene.eval_mesh(b).value.mesh, 1);
    let top = wrapped.iter().map(|t| t.x).fold(f32::MIN, f32::max);
    assert!((top - 8.0).abs() < 1e-3, "round the 80-unit perimeter: {top}");
    set(&mut s, 0, "Wrap Texture", 0.0);
    let span = |s: &AppState| {
        let uv = corner_uvs(&s.scene.eval_mesh(b).value.mesh, 1);
        let us: Vec<f32> = uv.iter().map(|t| t.x).collect();
        us.iter().cloned().fold(f32::MIN, f32::max) - us.iter().cloned().fold(f32::MAX, f32::min)
    };
    assert!((span(&s) - 2.0).abs() < 1e-3, "20 units at 10 per repeat: {}", span(&s));
    execute(&mut s, Command::SetTransform { id: b.0, position: None, rotation_deg: None, scale: Some([2.0; 3]) }).unwrap();
    assert!((span(&s) - 4.0).abs() < 1e-3, "40 world units: {}", span(&s));
    set(&mut s, 0, "U Offset", 0.25);
    let uv = corner_uvs(&s.scene.eval_mesh(b).value.mesh, 1);
    let lo = uv.iter().map(|t| t.x).fold(f32::MAX, f32::min);
    assert!((lo - (-2.0 + 0.25)).abs() < 1e-3, "U Offset shifts: {lo}");
    // The object-space version measures in the object's own space.
    let mut t = AppState::new();
    let b2 = prim(&mut t, PrimitiveKind::Box, [0.0; 3], [20.0; 3]);
    add(&mut t, "MapScaler");
    set(&mut t, 0, "Scale", 10.0);
    set(&mut t, 0, "Wrap Texture", 0.0);
    execute(&mut t, Command::SetTransform { id: b2.0, position: None, rotation_deg: None, scale: Some([2.0; 3]) }).unwrap();
    let uv = corner_uvs(&t.scene.eval_mesh(b2).value.mesh, 1);
    let us: Vec<f32> = uv.iter().map(|t| t.x).collect();
    let span2 = us.iter().cloned().fold(f32::MIN, f32::max) - us.iter().cloned().fold(f32::MAX, f32::min);
    assert!((span2 - 2.0).abs() < 1e-3, "scaled with the object: {span2}");
}

#[test]
fn uvw_mapping_add_and_clear() {
    let mut s = AppState::new();
    let b = prim(&mut s, PrimitiveKind::Box, [0.0; 3], [10.0; 3]);
    add(&mut s, "UVW Mapping Add");
    let m = s.scene.eval_mesh(b).value.mesh.clone();
    let chans: Vec<u32> = m.texture_maps().map(|(c, _)| *c).collect();
    assert_eq!(chans, [1, 2], "a channel after the last one");
    add(&mut s, "UVW Mapping Clear");
    set(&mut s, 1, "Map Channel", 1.0);
    let m = s.scene.eval_mesh(b).value.mesh.clone();
    let chans: Vec<u32> = m.texture_maps().map(|(c, _)| *c).collect();
    assert_eq!(chans, [2], "channel 1 cleared, 2 kept");
}

fn unwrap_plane(s: &mut AppState) -> NodeId {
    let p = prim(s, PrimitiveKind::Plane, [0.0; 3], [10.0, 10.0, 0.0]);
    add(s, "Unwrap UVW");
    p
}

fn tv_pos(s: &AppState, id: NodeId, t: u32) -> Vec2 {
    let n = s.scene.node(id).unwrap();
    let loom_scene::Modifier::UnwrapUVW(u) = &n.modifiers[0].modifier else { unreachable!() };
    let input = n.object.display_output();
    let m = u.working_mesh(&input);
    let v = m.map(1).unwrap().verts[t as usize];
    Vec2::new(v.x, v.y)
}

#[test]
fn unwrap_sketch_line_and_circle() {
    let mut s = AppState::new();
    let p = unwrap_plane(&mut s);
    let tvs = vec![0, 1, 2];
    execute(&mut s, Command::SketchTv { tvs: Some(tvs.clone()), align: 1, points: vec![[0.0, 0.0], [1.0, 0.0]] }).unwrap();
    for (i, t) in tvs.iter().enumerate() {
        let want = Vec2::new(i as f32 * 0.5, 0.0);
        assert!(tv_pos(&s, p, *t).distance(want) < 1e-4, "line: {} {want}", tv_pos(&s, p, *t));
    }
    // Circle: centre and a point on the rim.
    execute(&mut s, Command::SketchTv { tvs: Some(vec![0, 1, 2, 3]), align: 3, points: vec![[0.5, 0.5], [0.9, 0.5]] }).unwrap();
    for t in 0..4 {
        assert!((tv_pos(&s, p, t).distance(Vec2::splat(0.5)) - 0.4).abs() < 1e-4);
    }
    // Pick order: the order vertices were picked, reversed by Sketch Reverse Vertex Order.
    execute(&mut s, Command::SelectTv { ids: vec![2], op: SelectOp::Replace }).unwrap();
    execute(&mut s, Command::SelectTv { ids: vec![0], op: SelectOp::Add }).unwrap();
    execute(&mut s, Command::SketchReverseVertexOrder).unwrap();
    execute(&mut s, Command::SketchTv { tvs: None, align: 1, points: vec![[0.0, 1.0], [1.0, 1.0]] }).unwrap();
    assert!(tv_pos(&s, p, 0).distance(Vec2::new(0.0, 1.0)) < 1e-4, "reversed: 0 first");
    assert!(tv_pos(&s, p, 2).distance(Vec2::new(1.0, 1.0)) < 1e-4);
}

#[test]
fn unwrap_paint_select_and_cursor_size() {
    let mut s = AppState::new();
    let p = unwrap_plane(&mut s);
    execute(&mut s, Command::PaintSelectMode).unwrap();
    assert!(s.uv_editor.paint_select);
    let size = s.uv_editor.paint_size;
    execute(&mut s, Command::PaintSelectIncrementCursorSize).unwrap();
    assert!(s.uv_editor.paint_size > size);
    execute(&mut s, Command::PaintSelectDecrementCursorSize).unwrap();
    assert_eq!(s.uv_editor.paint_size, size);
    let at = tv_pos(&s, p, 0);
    execute(&mut s, Command::PaintSelectTv { points: vec![at.to_array()], radius: 0.01, op: SelectOp::Add }).unwrap();
    let n = s.scene.node(p).unwrap();
    let loom_scene::Modifier::UnwrapUVW(u) = &n.modifiers[0].modifier else { unreachable!() };
    assert!(u.tv_selection.contains(&0) && u.tv_selection.len() == 1, "{:?}", u.tv_selection);
}

#[test]
fn spline_map_follows_the_spline() {
    let mut s = AppState::new();
    let r = execute(&mut s, Command::LineStart { point: [0.0, 0.0, 0.0] }).unwrap();
    let line = r["id"].as_u64().unwrap();
    execute(&mut s, Command::LineAddKnot { node: line, point: [0.0, 0.0, 40.0], smooth: false }).unwrap();
    execute(&mut s, Command::LineEnd { node: line }).unwrap();
    let c = prim(&mut s, PrimitiveKind::Cylinder, [0.0; 3], [10.0, 10.0, 40.0]);
    execute(&mut s, Command::SetParameter { id: c.0, name: "Height Segments".into(), value: 4.0 }).unwrap();
    add(&mut s, "Unwrap UVW");
    execute(&mut s, Command::UnwrapSplineMap { spline: line, mapping: 0, normalize: true }).unwrap();
    let n = s.scene.node(c).unwrap();
    let loom_scene::Modifier::UnwrapUVW(u) = &n.modifiers[0].modifier else { unreachable!() };
    let m = u.working_mesh(&n.object.display_output());
    let uv = uv_at_vertices(&m, 1);
    for (v, t) in m.vertices().iter().zip(&uv) {
        if v.position.x.hypot(v.position.y) > 4.0 {
            assert!((t.y - v.position.z / 40.0).abs() < 1e-3, "V runs along the spline: {} {t}", v.position);
        }
    }
}

#[test]
fn edge_snap_and_freeform_gizmo() {
    // Edge Snap: a point near a texture edge lands on it.
    let mut s = AppState::new();
    let p = unwrap_plane(&mut s);
    let n = s.scene.node(p).unwrap();
    let loom_scene::Modifier::UnwrapUVW(u) = &n.modifiers[0].modifier else { unreachable!() };
    let m = u.working_mesh(&n.object.display_output());
    let (a, b) = loom_mesh::unwrap::tv_edges(&m, 1)[0];
    let (pa, pb) = (tv_pos(&s, p, a), tv_pos(&s, p, b));
    let mid = (pa + pb) * 0.5;
    let off = (pb - pa).perp().normalize() * 0.01;
    let snapped = loom_mesh::unwrap::snap_to_edge(&m, 1, &Default::default(), mid + off, 0.02).expect("near an edge");
    assert!(snapped.distance(mid) < 1e-4, "{snapped} {mid}");
    assert!(loom_mesh::unwrap::snap_to_edge(&m, 1, &Default::default(), mid + off * 10.0, 0.02).is_none(), "too far from any edge");
    // Freeform gizmo: corners scale, edge middles rotate, the cross is the pivot, inside moves.
    use loom_commands::FreeformHit;
    let (lo, hi, pivot) = (Vec2::ZERO, Vec2::ONE, Vec2::splat(0.5));
    let hit = |p: Vec2, inside_selects: bool| loom_commands::freeform_hit(lo, hi, pivot, p, 0.02, inside_selects);
    assert_eq!(hit(Vec2::new(1.0, 1.0), false), FreeformHit::Scale { corner: 2 });
    assert_eq!(hit(Vec2::new(0.5, 1.0), false), FreeformHit::Rotate);
    assert_eq!(hit(Vec2::new(0.5, 0.5), false), FreeformHit::Pivot);
    assert_eq!(hit(Vec2::new(0.3, 0.7), false), FreeformHit::Move);
    assert_eq!(hit(Vec2::new(2.0, 2.0), false), FreeformHit::Outside);
    // Allow Selections Inside Gizmo: a click inside selects instead of moving.
    assert_eq!(hit(Vec2::new(0.3, 0.7), true), FreeformHit::Outside);
    execute(&mut s, Command::AllowSelectionsInsideTranformGizmo).unwrap();
    assert!(s.uv_editor.allow_select_inside_gizmo);
}
