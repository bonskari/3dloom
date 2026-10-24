//! Unwrap UVW commands: the Edit UVWs window's tools and the modifier's
//! rollout buttons, each one undo entry on the entry's `UnwrapUvw` data.

use crate::execute::commit_change;
use crate::state::{AppState, PickMode, SubObjectId, TvLevel, TvMode, UvTexture};
use crate::{Command as C, SelectOp, SelectionMode};
use glam::{Vec2, Vec3};
use loom_mesh::select_ops::SubLevel;
use loom_mesh::unwrap as uw;
use loom_mesh::uvmap::Projection;
use loom_mesh::{FaceId, HalfEdgeId, Mesh, VertexId};
use loom_scene::{Change, Modifier, NodeId, UnwrapUvw};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};

/// The entry being edited: node, stack index, its data, and the mesh below it.
struct Entry {
    id: NodeId,
    index: usize,
    data: UnwrapUvw,
    input: Mesh,
}

fn entry(state: &AppState) -> Result<Entry, String> {
    let (id, index) = state.unwrap_entry().ok_or("no Unwrap UVW on the selection (add the modifier first)")?;
    let node = state.scene.node(id).ok_or("missing node")?;
    let Modifier::UnwrapUVW(u) = &node.modifiers[index].modifier else { return Err("not an Unwrap UVW".into()) };
    let input = match index.checked_sub(1) {
        Some(prev) => state.scene.evaluated(id, Some(prev)).value.mesh.clone(),
        None => node.object.display_output(),
    };
    Ok(Entry { id, index, data: (**u).clone(), input })
}

/// The working mesh (input + edits) and the channel being edited.
fn working(e: &Entry) -> (Mesh, u32) {
    let ch = e.data.channel.get();
    (e.data.working_mesh(&e.input), ch)
}

/// Stores edited data back as one undo entry; the tv selection is kept by
/// position (edits renumber texture vertices).
fn store(state: &mut AppState, e: &Entry, data: UnwrapUvw, label: &str) -> Result<Value, String> {
    let node = state.scene.node(e.id).ok_or("missing node")?;
    let mut stack = node.modifiers.clone();
    stack[e.index].modifier = Modifier::UnwrapUVW(Box::new(data));
    commit_change(state, label, Change::SetStack { id: e.id, before: node.modifiers.clone(), after: stack })?;
    Ok(status(state))
}

/// Runs `f` on the working mesh, takes the result as the edits and keeps
/// the selection on the tvs that still sit where the selected ones were.
fn edit(state: &mut AppState, label: &str, f: impl FnOnce(&mut Mesh, u32, &mut UnwrapUvw) -> Result<(), String>) -> Result<Value, String> {
    let e = entry(state)?;
    let (mut mesh, ch) = working(&e);
    let mut data = e.data.clone();
    // Selection, pins, hidden and frozen sets follow the face corners (half-edge
    // ids are stable under UV edits; texture vertex ids are not).
    let corners_of = |m: &loom_mesh::half_edge::MapChannel, set: &BTreeSet<u32>| -> Vec<usize> { m.corners.iter().enumerate().filter(|(_, c)| set.contains(c)).map(|(i, _)| i).collect() };
    let before = mesh.map(ch).cloned().unwrap_or_default_channel();
    let sel_c = corners_of(&before, &data.tv_selection);
    let pin_c = corners_of(&before, &data.pins);
    let hid_c = corners_of(&before, &data.hidden_tvs);
    let fro_c = corners_of(&before, &data.frozen_tvs);
    let before_count = before.verts.len();
    f(&mut mesh, ch, &mut data)?;
    data.take_edits(&mesh);
    if !data.topology_matches(&e.input) {
        // The working mesh already carried the old edits onto this topology.
        data.init(&e.input);
    }
    let after = mesh.map(ch).cloned().unwrap_or_default_channel();
    if after.verts.len() != before_count || after != before {
        let tvs_of = |cs: &[usize]| -> BTreeSet<u32> { cs.iter().filter_map(|i| after.corners.get(*i).copied()).collect() };
        data.tv_selection = tvs_of(&sel_c);
        data.pins = tvs_of(&pin_c);
        data.hidden_tvs = tvs_of(&hid_c);
        data.frozen_tvs = tvs_of(&fro_c);
        data.tv_edge_selection = uw::edges_of_tvs(&mesh, ch, &data.tv_selection).into_iter().collect();
    }
    store(state, &e, data, label)
}

trait DefaultChannel {
    fn unwrap_or_default_channel(self) -> loom_mesh::half_edge::MapChannel;
}
impl DefaultChannel for Option<loom_mesh::half_edge::MapChannel> {
    fn unwrap_or_default_channel(self) -> loom_mesh::half_edge::MapChannel {
        self.unwrap_or(loom_mesh::half_edge::MapChannel { verts: vec![Vec3::ZERO], corners: Vec::new() })
    }
}

/// Changes selection / seams / pins without touching the coordinates.
fn edit_data(state: &mut AppState, label: &str, f: impl FnOnce(&Mesh, u32, &mut UnwrapUvw) -> Result<(), String>) -> Result<Value, String> {
    let e = entry(state)?;
    let (mesh, ch) = working(&e);
    let mut data = e.data.clone();
    f(&mesh, ch, &mut data)?;
    store(state, &e, data, label)
}

/// The faces the mapping tools work on: the modifier's face selection,
/// else the faces of the selected tvs, else every face.
fn target_faces(mesh: &Mesh, ch: u32, data: &UnwrapUvw) -> Vec<FaceId> {
    if !data.face_selection.is_empty() {
        return data.face_selection.iter().map(|f| FaceId(*f)).filter(|f| (f.0 as usize) < mesh.face_count()).collect();
    }
    let by_tv = uw::faces_of_tvs(mesh, ch, &data.tv_selection);
    if !by_tv.is_empty() {
        return by_tv;
    }
    (0..mesh.face_count() as u32).map(FaceId).collect()
}

/// The tvs the editing tools work on: the selection (nothing selected =
/// nothing happens), minus frozen tvs.
fn target_tvs(_mesh: &Mesh, _ch: u32, data: &UnwrapUvw) -> BTreeSet<u32> {
    data.tv_selection.iter().copied().filter(|t| !data.frozen_tvs.contains(t)).collect()
}

/// Selectable tvs: not hidden, not frozen.
fn selectable(data: &UnwrapUvw, t: u32) -> bool {
    !data.hidden_tvs.contains(&t) && !data.frozen_tvs.contains(&t)
}

fn soft(state: &AppState, mesh: &Mesh, ch: u32, data: &UnwrapUvw) -> Option<HashMap<u32, f32>> {
    let e = &state.uv_editor;
    e.soft_selection.then(|| if e.soft_edge_distance { uw::soft_weights_edges(mesh, ch, &data.tv_selection, e.soft_edge_range) } else { uw::soft_weights(mesh, ch, &data.tv_selection, e.soft_falloff) })
}

fn pivot(state: &AppState, mesh: &Mesh, ch: u32, data: &UnwrapUvw) -> Vec2 {
    uw::centre(mesh, ch, &data.tv_selection) + Vec2::from(state.uv_editor.pivot)
}

/// The Unwrap state for the status reply and `State`.
pub fn status(state: &AppState) -> Value {
    match entry(state) {
        Ok(e) => {
            let (mesh, ch) = working(&e);
            json!({
                "node": e.id.0,
                "index": e.index,
                "channel": ch,
                "tvs": mesh.map(ch).map(|m| m.verts.len()).unwrap_or(0),
                "tv_selection": e.data.tv_selection.iter().collect::<Vec<_>>(),
                "tv_edge_selection": e.data.tv_edge_selection.iter().map(|(a, b)| [a, b]).collect::<Vec<_>>(),
                "face_selection": e.data.face_selection.iter().collect::<Vec<_>>(),
                "seams": e.data.seams.iter().map(|(a, b)| [a, b]).collect::<Vec<_>>(),
                "pins": e.data.pins.iter().collect::<Vec<_>>(),
                "editor": serde_json::to_value(&state.uv_editor).unwrap_or(Value::Null),
            })
        }
        Err(_) => json!({ "editor": serde_json::to_value(&state.uv_editor).unwrap_or(Value::Null) }),
    }
}

fn apply_op(set: &mut BTreeSet<u32>, ids: impl IntoIterator<Item = u32>, op: SelectOp) {
    match op {
        SelectOp::Replace => {
            set.clear();
            set.extend(ids);
        }
        SelectOp::Add => set.extend(ids),
        SelectOp::Subtract => {
            for i in ids {
                set.remove(&i);
            }
        }
    }
}

/// Viewport geometry selection (edges / faces) of the entry's node.
fn viewport_edges(state: &AppState, id: NodeId) -> Vec<HalfEdgeId> {
    state.sub_selection.iter().filter(|(n, _)| *n == id).filter_map(|(_, s)| match s { SubObjectId::Edge(e) => Some(e), _ => None }).collect()
}

fn viewport_faces(state: &AppState, id: NodeId) -> Vec<FaceId> {
    state.sub_selection.iter().filter(|(n, _)| *n == id).filter_map(|(_, s)| match s { SubObjectId::Face(f) => Some(f), _ => None }).collect()
}

fn edge_pair(mesh: &Mesh, he: HalfEdgeId) -> Option<(u32, u32)> {
    let (a, b) = mesh.edge_vertices(he)?;
    Some((a.0.min(b.0), a.0.max(b.0)))
}

/// Geometry edges of the entry's input as (vertex pair) → canonical half-edge.
fn edge_index(mesh: &Mesh) -> HashMap<(u32, u32), HalfEdgeId> {
    let mut out = HashMap::new();
    for i in 0..mesh.half_edge_count() as u32 {
        if let Some(c) = mesh.canonical_edge_id(HalfEdgeId(i)) {
            if let Some(p) = edge_pair(mesh, c) {
                out.entry(p).or_insert(c);
            }
        }
    }
    out
}

fn map_params(data: &UnwrapUvw, kind: u32) -> (Projection, glam::Mat4, Vec3) {
    let projection = match kind {
        2 => Projection::Cylindrical { cap: false },
        3 => Projection::Spherical,
        4 => Projection::Box,
        _ => Projection::Planar,
    };
    (projection, data.map_gizmo.matrix(), data.map_size)
}

/// Sizes the map gizmo to the faces' bounds in its frame (Fit / Center).
fn fit_gizmo(mesh: &Mesh, faces: &[FaceId], data: &mut UnwrapUvw, align: Option<Vec3>, center_only: bool) {
    if let Some(n) = align {
        let rot = glam::Quat::from_rotation_arc(Vec3::Z, n.normalize_or_zero());
        data.map_gizmo.set_rotation_quat(rot);
    }
    let rot = data.map_gizmo.rotation_quat();
    let inv = glam::Mat4::from_quat(rot).inverse();
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for f in faces {
        for p in mesh.face_positions(*f).unwrap_or_default() {
            let q = inv.transform_point3(p);
            lo = lo.min(q);
            hi = hi.max(q);
        }
    }
    if lo.x.is_finite() {
        data.map_gizmo.position.set(rot * ((lo + hi) * 0.5));
        if !center_only {
            data.map_size = (hi - lo).max(Vec3::splat(1e-4));
        }
    }
}

fn selected_faces_normal(mesh: &Mesh, faces: &[FaceId]) -> Vec3 {
    let n: Vec3 = faces.iter().map(|f| mesh.face_normal(*f)).sum();
    if n.length_squared() < 1e-9 { Vec3::Z } else { n.normalize() }
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    let ed = |state: &mut AppState, f: &dyn Fn(&mut crate::state::UvEditor)| -> Result<Value, String> {
        f(&mut state.uv_editor);
        Ok(status(state))
    };
    match cmd {
        C::UnwrapUvwModifier => crate::execute::execute(state, C::AddModifier { name: "Unwrap UVW".into() }),
        C::EditUvws => {
            entry(state)?;
            state.uv_editor.open = true;
            Ok(status(state))
        }
        C::CloseUvws => ed(state, &|e| e.open = false),
        C::UnwrapOptions => {
            state.dialog = Some(crate::viewport::Dialog::UnwrapOptions);
            Ok(json!({}))
        }
        C::SetUnwrapOption { name, value } => {
            state.uv_editor.set(&name, value)?;
            Ok(status(state))
        }
        C::LoadDefaults => {
            let open = state.uv_editor.open;
            state.uv_editor = state.uv_defaults.clone();
            state.uv_editor.open = open;
            Ok(status(state))
        }
        C::SaveCurrentSettingsAsDefault => {
            state.uv_defaults = state.uv_editor.clone();
            state.uv_defaults.open = false;
            Ok(status(state))
        }
        C::TvVertexSubobjectMode => ed(state, &|e| e.level = TvLevel::Vertex),
        C::TvEdgeSubobjectMode => ed(state, &|e| e.level = TvLevel::Edge),
        C::TvPolygonSubobjectMode | C::PolygonMode | C::PolygonSelect => ed(state, &|e| e.level = TvLevel::Face),
        C::TvElementMode => ed(state, &|e| e.select_element = !e.select_element),
        C::TextureVertexMoveMode => ed(state, &|e| e.mode = TvMode::Move),
        C::MoveHorizontal => ed(state, &|e| e.mode = TvMode::MoveHorizontal),
        C::MoveVertical => ed(state, &|e| e.mode = TvMode::MoveVertical),
        C::TextureVertexRotateMode | C::Rotate => ed(state, &|e| e.mode = TvMode::Rotate),
        C::TextureVertexScaleMode => ed(state, &|e| e.mode = TvMode::Scale),
        C::ScaleHorizontal => ed(state, &|e| e.mode = TvMode::ScaleHorizontal),
        C::ScaleVertical => ed(state, &|e| e.mode = TvMode::ScaleVertical),
        C::FreeformMode => ed(state, &|e| e.mode = TvMode::Freeform),
        C::Pan => ed(state, &|e| e.mode = TvMode::Pan),
        C::FilterSelectedPolygons => ed(state, &|e| e.filter_selected_polygons = !e.filter_selected_polygons),
        C::IgnoreBackPolygons => edit_data(state, "Ignore Backfacing", |_, _, d| { d.ignore_backfacing.set(!d.ignore_backfacing.get()); Ok(()) }),
        C::GeomElementSelectMode => edit_data(state, "Select By Element", |_, _, d| { d.select_by_element.set(!d.select_by_element.get()); Ok(()) }),
        C::OpenEdgeMode => ed(state, &|e| e.open_edge_mode = !e.open_edge_mode),
        C::UvEdgeMode => ed(state, &|e| e.uv_edge_mode = !e.uv_edge_mode),
        C::SyncSelectionMode => ed(state, &|e| e.sync_selection = !e.sync_selection),
        C::LockSelectedVertices => ed(state, &|e| e.lock_selected = !e.lock_selected),
        C::AutoPinMovedVertices => ed(state, &|e| e.auto_pin_moved = !e.auto_pin_moved),
        C::AlwaysBringUpTheEditWindow => ed(state, &|e| e.always_bring_up = !e.always_bring_up),
        C::ShowSubobjectCounter => ed(state, &|e| e.show_counter = !e.show_counter),
        C::ShowSharedSubObjects => ed(state, &|e| e.show_shared = !e.show_shared),
        C::ShowVertexConnections => ed(state, &|e| e.show_vertex_connections = !e.show_vertex_connections),
        C::ShowHiddenEdges => ed(state, &|e| e.show_hidden_edges = !e.show_hidden_edges),
        C::DisplaySeams => ed(state, &|e| e.display_seams_in_editor = !e.display_seams_in_editor),
        C::ShowSeamsInViewport => edit_data(state, "Show Seams", |_, _, d| { d.show_seams.set(!d.show_seams.get()); Ok(()) }),
        C::PeltAlwaysShowSeams => edit_data(state, "Always Show Pelt Seam", |_, _, d| { d.always_show_pelt_seam.set(!d.always_show_pelt_seam.get()); Ok(()) }),
        C::PreventReflattening => edit_data(state, "Prevent Reflattening", |_, _, d| { d.prevent_reflattening.set(!d.prevent_reflattening.get()); Ok(()) }),
        C::ShowMap => ed(state, &|e| e.show_map = !e.show_map),
        C::BlendTilesToBackground => ed(state, &|e| e.blend_tiles = !e.blend_tiles),
        C::BrightnessAffectsCenterTile => ed(state, &|e| e.brightness_center_tile = !e.brightness_center_tile),
        C::GridVisible => ed(state, &|e| e.grid_visible = !e.grid_visible),
        C::Snap => ed(state, &|e| e.snap = !e.snap),
        C::SnapToGridVertexEdge => ed(state, &|e| {
            let (g, v, d) = (e.grid_snap, e.vertex_snap, e.edge_snap);
            (e.grid_snap, e.vertex_snap, e.edge_snap) = match (g, v, d) { (true, _, _) => (false, true, false), (_, true, _) => (false, false, true), _ => (true, false, false) };
        }),
        C::GridSnap => ed(state, &|e| e.grid_snap = !e.grid_snap),
        C::VertexSnap => ed(state, &|e| e.vertex_snap = !e.vertex_snap),
        C::EdgeSnap => ed(state, &|e| e.edge_snap = !e.edge_snap),
        C::FreeformSnap => ed(state, &|e| e.freeform_snap = !e.freeform_snap),
        C::AllowSelectionsInsideTranformGizmo => ed(state, &|e| e.allow_select_inside_gizmo = !e.allow_select_inside_gizmo),
        C::PaintSelectMode => ed(state, &|e| e.paint_select = !e.paint_select),
        C::PaintSelectIncrementCursorSize => ed(state, &|e| e.paint_size = (e.paint_size + 2.0).min(200.0)),
        C::PaintSelectDecrementCursorSize => ed(state, &|e| e.paint_size = (e.paint_size - 2.0).max(1.0)),
        C::PaintSelectTv { points, radius, op } => edit_data(state, "Paint Select", |mesh, ch, d| {
            let Some(m) = mesh.map(ch) else { return Ok(()) };
            let pts: Vec<Vec2> = points.iter().map(|p| Vec2::from(*p)).collect();
            let hit: Vec<u32> = m.verts.iter().enumerate()
                .filter(|(i, v)| selectable(d, *i as u32) && pts.iter().any(|p| Vec2::new(v.x, v.y).distance(*p) <= radius.max(0.0)))
                .map(|(i, _)| i as u32)
                .collect();
            apply_op(&mut d.tv_selection, hit, op);
            d.tv_edge_selection = uw::edges_of_tvs(mesh, ch, &d.tv_selection).into_iter().collect();
            Ok(())
        }),
        C::Sketch => ed(state, &|e| e.sketch = !e.sketch),
        C::SketchDialog => ed(state, &|e| e.sketch_dialog = true),
        C::SketchReverseVertexOrder => ed(state, &|e| e.sketch_order.reverse()),
        C::SketchTv { tvs, align, points } => {
            let order = state.uv_editor.sketch_order.clone();
            let r = edit(state, "Sketch", |mesh, ch, d| {
                let tvs: Vec<u32> = match tvs {
                    Some(t) => t,
                    None => {
                        // The picked order, then any selected tv it does not name.
                        let mut t: Vec<u32> = order.iter().copied().filter(|i| d.tv_selection.contains(i)).collect();
                        t.extend(d.tv_selection.iter().copied().filter(|i| !order.contains(i)));
                        t
                    }
                };
                let n = mesh.map(ch).map(|m| m.verts.len()).unwrap_or(0) as u32;
                let tvs: Vec<u32> = tvs.into_iter().filter(|t| *t < n && !d.frozen_tvs.contains(t)).collect();
                if tvs.is_empty() {
                    return Err("Sketch: select the vertices to line up first".into());
                }
                let pts: Vec<Vec2> = points.iter().map(|p| Vec2::from(*p)).collect();
                let at = uw::sketch_points(align, &pts, tvs.len());
                if let Some(m) = mesh.map_mut(ch) {
                    for (t, p) in tvs.iter().zip(at) {
                        let w = m.verts[*t as usize].z;
                        m.verts[*t as usize] = Vec3::new(p.x, p.y, w);
                    }
                }
                Ok(())
            });
            state.uv_editor.sketch = false;
            r
        }
        C::SplineMap => {
            entry(state)?;
            state.pick_mode = if state.pick_mode == Some(PickMode::UnwrapSplineMap) { None } else { Some(PickMode::UnwrapSplineMap) };
            Ok(json!({ "pick": state.pick_mode.map(|m| format!("{m:?}")) }))
        }
        C::UnwrapSplineMap { spline, mapping, normalize } => {
            let sid = NodeId(spline);
            let shape = state.scene.node(sid).and_then(|n| n.object.shape()).ok_or("Spline Map: pick a shape")?;
            let (uid, _) = state.unwrap_entry().ok_or("no Unwrap UVW on the selection")?;
            // The spline in this object's space.
            let m = state.scene.object_matrix(uid).inverse() * state.scene.object_matrix(sid);
            let path: Vec<Vec3> = shape.polylines().into_iter().next().unwrap_or_default().into_iter().map(|p| m.transform_point3(p)).collect();
            if path.len() < 2 {
                return Err("Spline Map: the spline needs two points".into());
            }
            state.pick_mode = None;
            edit(state, "Spline Map", |mesh, ch, d| {
                let faces = target_faces(mesh, ch, d);
                spline_map(mesh, ch, &faces, &path, mapping, normalize);
                Ok(())
            })
        }
        C::ShowEdgeDistortion => ed(state, &|e| e.show_edge_distortion = !e.show_edge_distortion),
        C::ShowLocalDistortion => ed(state, &|e| e.show_local_distortion = !e.show_local_distortion),
        C::DisplayQuickMapGizmo => ed(state, &|e| e.display_quick_map_gizmo = !e.display_quick_map_gizmo),
        C::UpdateMap => ed(state, &|e| e.map_version += 1),
        C::PickTexture { path } => {
            if !std::path::Path::new(&path).is_file() {
                return Err(format!("{path}: not a file"));
            }
            ed(state, &|e| { e.texture = UvTexture::Bitmap(path.clone()); e.show_map = true; })
        }
        C::RemoveTexture => ed(state, &|e| e.texture = UvTexture::None),
        C::CheckerPattern => ed(state, &|e| { e.texture = UvTexture::Checker; e.show_map = true; }),
        C::UvEditorPlane { plane } => ed(state, &|e| e.plane = plane.min(2)),
        C::UnwrapMapChannel { channel } => {
            let e = entry(state)?;
            let mut data = e.data.clone();
            if data.channel.get() != channel {
                data.channel.set(channel.min(99));
                data.edits = None;
                data.tv_selection.clear();
                data.tv_edge_selection.clear();
            }
            store(state, &e, data, "Map Channel")
        }
        C::SoftSelection => ed(state, &|e| e.soft_selection = !e.soft_selection),
        C::SoftSelectionEdgeDistance => ed(state, &|e| e.soft_edge_distance = !e.soft_edge_distance),
        C::SoftSelectionEdgeDistanceRange1 => ed(state, &|e| e.soft_edge_range = 1),
        C::SoftSelectionEdgeDistanceRange2 => ed(state, &|e| e.soft_edge_range = 2),
        C::SoftSelectionEdgeDistanceRange3 => ed(state, &|e| e.soft_edge_range = 3),
        C::SoftSelectionEdgeDistanceRange4 => ed(state, &|e| e.soft_edge_range = 4),
        C::SoftSelectionEdgeDistanceRange5 => ed(state, &|e| e.soft_edge_range = 5),
        C::SoftSelectionEdgeDistanceRange6 => ed(state, &|e| e.soft_edge_range = 6),
        C::SoftSelectionEdgeDistanceRange7 => ed(state, &|e| e.soft_edge_range = 7),
        C::SoftSelectionEdgeDistanceRange8 => ed(state, &|e| e.soft_edge_range = 8),
        C::PivotSnapToCenter | C::ResetPivotOnSelection => ed(state, &|e| e.pivot = [0.0, 0.0]),
        C::PivotSnapUpperLeft | C::PivotSnapUpperCenter | C::PivotSnapUpperRight | C::PivotSnapLeftCenter | C::PivotSnapRightCenter | C::PivotSnapLowerLeft | C::PivotSnapLowerCenter | C::PivotSnapLowerRight => {
            let e = entry(state)?;
            let (mesh, ch) = working(&e);
            let (lo, hi) = uw::bounds(&mesh, ch, Some(&e.data.tv_selection));
            let c = (lo + hi) * 0.5;
            let (x, y) = match cmd {
                C::PivotSnapUpperLeft => (lo.x, hi.y),
                C::PivotSnapUpperCenter => (c.x, hi.y),
                C::PivotSnapUpperRight => (hi.x, hi.y),
                C::PivotSnapLeftCenter => (lo.x, c.y),
                C::PivotSnapRightCenter => (hi.x, c.y),
                C::PivotSnapLowerLeft => (lo.x, lo.y),
                C::PivotSnapLowerCenter => (c.x, lo.y),
                _ => (hi.x, lo.y),
            };
            state.uv_editor.pivot = [x - c.x, y - c.y];
            Ok(status(state))
        }
        C::ZoomExtentsUv | C::ZoomExtentsSelectedUv => {
            let e = entry(state)?;
            let (mesh, ch) = working(&e);
            let sel = (matches!(cmd, C::ZoomExtentsSelectedUv) && !e.data.tv_selection.is_empty()).then_some(&e.data.tv_selection);
            let (lo, hi) = uw::bounds(&mesh, ch, sel);
            let extent = (hi - lo).max_element().max(1e-3);
            state.uv_editor.zoom = 0.9 / extent;
            let c = (lo + hi) * 0.5;
            state.uv_editor.pan = [c.x, c.y];
            Ok(status(state))
        }

        // --- selection ---
        C::SelectTv { ids, op } => {
            let by_element = state.uv_editor.select_element;
            let mut new_order = state.uv_editor.sketch_order.clone();
            let order = &mut new_order;
            let r = edit_data(state, "Select TV", |mesh, ch, d| {
            let n = mesh.map(ch).map(|m| m.verts.len()).unwrap_or(0) as u32;
            let mut ids: Vec<u32> = ids.into_iter().filter(|i| *i < n && selectable(d, *i)).collect();
            if d.select_by_element.get() || by_element {
                let set: BTreeSet<u32> = ids.iter().copied().collect();
                ids = uw::element(mesh, ch, &set).into_iter().collect();
            }
            // The order tvs are picked in (Sketch places them in it).
            match op {
                SelectOp::Replace => *order = ids.clone(),
                SelectOp::Add => order.extend(ids.iter().copied().filter(|i| !d.tv_selection.contains(i))),
                SelectOp::Subtract => order.retain(|i| !ids.contains(i)),
            }
            apply_op(&mut d.tv_selection, ids, op);
            d.tv_edge_selection = uw::edges_of_tvs(mesh, ch, &d.tv_selection).into_iter().collect();
            Ok(())
        });
            state.uv_editor.sketch_order = new_order;
            r
        }
        C::SelectTvEdges { edges, op } => {
            let open_mode = state.uv_editor.open_edge_mode;
            let seam_mode = state.uv_editor.uv_edge_mode;
            edit_data(state, "Select TV Edges", |mesh, ch, d| {
            let valid: BTreeSet<(u32, u32)> = uw::tv_edges(mesh, ch).into_iter().collect();
            let mut edges: Vec<(u32, u32)> = edges.into_iter().map(|[a, b]| (a.min(b), a.max(b))).filter(|e| valid.contains(e)).collect();
            if open_mode || seam_mode {
                // Grow along the connected run of open edges (or seam edges) the click landed on.
                let open: BTreeSet<(u32, u32)> = open_tv_edges(mesh, ch, seam_mode).into_iter().collect();
                let mut grown: BTreeSet<(u32, u32)> = BTreeSet::new();
                let mut queue: Vec<(u32, u32)> = edges.iter().copied().filter(|e| open.contains(e)).collect();
                while let Some(e) = queue.pop() {
                    if !grown.insert(e) {
                        continue;
                    }
                    for o in &open {
                        if !grown.contains(o) && (o.0 == e.0 || o.0 == e.1 || o.1 == e.0 || o.1 == e.1) {
                            queue.push(*o);
                        }
                    }
                }
                edges.extend(grown);
            }
            match op {
                SelectOp::Replace => d.tv_edge_selection = edges.iter().copied().collect(),
                SelectOp::Add => d.tv_edge_selection.extend(edges.iter().copied()),
                SelectOp::Subtract => {
                    for e in &edges {
                        d.tv_edge_selection.remove(e);
                    }
                }
            }
            d.tv_selection = d.tv_edge_selection.iter().flat_map(|(a, b)| [*a, *b]).collect();
            Ok(())
        })
        }
        C::SelectTvFaces { faces, op } => edit_data(state, "Select TV Faces", |mesh, ch, d| {
            let faces: Vec<u32> = faces.into_iter().filter(|f| (*f as usize) < mesh.face_count()).collect();
            apply_op(&mut d.face_selection, faces, op);
            let fs: Vec<FaceId> = d.face_selection.iter().map(|f| FaceId(*f)).collect();
            d.tv_selection = uw::tvs_of_faces(mesh, ch, &fs);
            d.tv_edge_selection = uw::edges_of_tvs(mesh, ch, &d.tv_selection).into_iter().collect();
            Ok(())
        }),
        C::SelectTvRegion { min, max, op } => edit_data(state, "Select TV Region", |mesh, ch, d| {
            let Some(m) = mesh.map(ch) else { return Ok(()) };
            let (lo, hi) = (Vec2::from(min).min(Vec2::from(max)), Vec2::from(min).max(Vec2::from(max)));
            let ids: Vec<u32> = m.verts.iter().enumerate().filter(|(i, v)| selectable(d, *i as u32) && v.x >= lo.x && v.x <= hi.x && v.y >= lo.y && v.y <= hi.y).map(|(i, _)| i as u32).collect();
            apply_op(&mut d.tv_selection, ids, op);
            d.tv_edge_selection = uw::edges_of_tvs(mesh, ch, &d.tv_selection).into_iter().collect();
            Ok(())
        }),
        C::SelectAllTv => edit_data(state, "Select All", |mesh, ch, d| {
            d.tv_selection = (0..mesh.map(ch).map(|m| m.verts.len()).unwrap_or(0) as u32).filter(|t| selectable(d, *t)).collect();
            d.tv_edge_selection = uw::edges_of_tvs(mesh, ch, &d.tv_selection).into_iter().collect();
            d.face_selection = (0..mesh.face_count() as u32).collect();
            Ok(())
        }),
        C::SelectNoneTv => edit_data(state, "Select None", |_, _, d| {
            d.tv_selection.clear();
            d.tv_edge_selection.clear();
            d.face_selection.clear();
            Ok(())
        }),
        C::SelectInvertTv => edit_data(state, "Select Invert", |mesh, ch, d| {
            let n = mesh.map(ch).map(|m| m.verts.len()).unwrap_or(0) as u32;
            d.tv_selection = (0..n).filter(|t| !d.tv_selection.contains(t) && selectable(d, *t)).collect();
            d.tv_edge_selection = uw::edges_of_tvs(mesh, ch, &d.tv_selection).into_iter().collect();
            Ok(())
        }),
        C::TextureVertexExpandSelection => edit_data(state, "Expand Selection", |mesh, ch, d| {
            d.tv_selection = uw::expand(mesh, ch, &d.tv_selection);
            d.tv_edge_selection = uw::edges_of_tvs(mesh, ch, &d.tv_selection).into_iter().collect();
            Ok(())
        }),
        C::TextureVertexContractSelection => edit_data(state, "Contract Selection", |mesh, ch, d| {
            d.tv_selection = uw::contract(mesh, ch, &d.tv_selection);
            d.tv_edge_selection = uw::edges_of_tvs(mesh, ch, &d.tv_selection).into_iter().collect();
            Ok(())
        }),
        C::EdgeToVertexSelect => edit_data(state, "Edge to Vertex", |_, _, d| {
            d.tv_selection = d.tv_edge_selection.iter().flat_map(|(a, b)| [*a, *b]).collect();
            Ok(())
        }),
        C::EdgeToPolygonSelect => edit_data(state, "Edge to Polygon", |mesh, ch, d| {
            let tvf = uw::tv_faces(mesh, ch);
            d.face_selection = tvf.iter().enumerate().filter(|(_, t)| (0..t.len()).any(|i| { let (a, b) = (t[i], t[(i + 1) % t.len()]); d.tv_edge_selection.contains(&(a.min(b), a.max(b))) })).map(|(f, _)| f as u32).collect();
            Ok(())
        }),
        C::PolygonToEdgeSelect => edit_data(state, "Polygon to Edge", |mesh, ch, d| {
            let fs: Vec<FaceId> = d.face_selection.iter().map(|f| FaceId(*f)).collect();
            let tvs = uw::tvs_of_faces(mesh, ch, &fs);
            d.tv_edge_selection = uw::edges_of_tvs(mesh, ch, &tvs).into_iter().collect();
            Ok(())
        }),
        C::PolygonToVertexSelect => edit_data(state, "Polygon to Vertex", |mesh, ch, d| {
            let fs: Vec<FaceId> = d.face_selection.iter().map(|f| FaceId(*f)).collect();
            d.tv_selection = uw::tvs_of_faces(mesh, ch, &fs);
            Ok(())
        }),
        C::GetSelectionFromPolygons | C::SyncViewportSelectionToTexture => {
            let e = entry(state)?;
            let faces: Vec<u32> = viewport_faces(state, e.id).iter().map(|f| f.0).collect();
            execute(state, C::SelectTvFaces { faces, op: SelectOp::Replace })
        }
        C::GetPolygonSelectionFromStack => {
            let e = entry(state)?;
            let below = match e.index.checked_sub(1) {
                Some(prev) => state.scene.evaluated(e.id, Some(prev)).value.selection.clone(),
                None => state.scene.node(e.id).and_then(|n| n.object.poly().and_then(|p| p.passes_up.clone())),
            };
            let faces: Vec<u32> = match below {
                Some((SubLevel::Face | SubLevel::Element, ids)) => ids,
                Some((level, ids)) => loom_mesh::mesh_mods::faces_of_selection(&e.input, level, &ids).into_iter().map(|f| f.0).collect(),
                None => Vec::new(),
            };
            execute(state, C::SelectTvFaces { faces, op: SelectOp::Replace })
        }
        C::SyncTextureSelectionToViewport => {
            let e = entry(state)?;
            state.sub_selection.retain_nodes(|n| n != e.id);
            for f in &e.data.face_selection {
                state.sub_selection.insert(e.id, SubObjectId::Face(FaceId(*f)));
            }
            state.selection_mode = SelectionMode::Face;
            Ok(status(state))
        }
        C::SelectByMaterialIdUnwrap | C::SelectBySmoothingGroupUnwrap => {
            let e = entry(state)?;
            let faces: Vec<u32> = (0..e.input.face_count())
                .filter(|f| {
                    let face = &e.input.faces()[*f];
                    if matches!(cmd, C::SelectByMaterialIdUnwrap) { face.material_id as u32 == e.data.select_matid.get() } else { face.smoothing_groups & (1 << (e.data.select_sg.get().clamp(1, 32) - 1)) != 0 }
                })
                .map(|f| f as u32)
                .collect();
            execute(state, C::SelectTvFaces { faces, op: SelectOp::Replace })
        }
        C::SelectInvertedPolygons | C::SelectOverlappedPolygons => {
            let e = entry(state)?;
            let (mesh, ch) = working(&e);
            let faces: Vec<u32> = if matches!(cmd, C::SelectInvertedPolygons) { uw::inverted_faces(&mesh, ch) } else { uw::overlapped_faces(&mesh, ch) }.into_iter().map(|f| f.0).collect();
            execute(state, C::SelectTvFaces { faces, op: SelectOp::Replace })
        }
        C::SelectPinnedVerts => edit_data(state, "Select Pinned", |_, _, d| { d.tv_selection = d.pins.clone(); Ok(()) }),
        C::TvLoopSelect | C::TvRingSelect | C::TvLoopGrowSelect | C::TvLoopShrinkSelect | C::TvRingGrowSelect | C::TvRingShrinkSelect => {
            let e = entry(state)?;
            let (mesh, ch) = working(&e);
            let index = edge_index(&mesh);
            // The selected tv edges as geometry edges, then loop / ring on the mesh.
            let geo_of = |a: u32, b: u32| -> Option<HalfEdgeId> {
                let m = mesh.map(ch)?;
                let (va, vb) = (m.corners.iter().position(|c| *c == a)?, m.corners.iter().position(|c| *c == b)?);
                let (va, vb) = (mesh.half_edges()[va].origin.0, mesh.half_edges()[vb].origin.0);
                index.get(&(va.min(vb), va.max(vb))).copied()
            };
            let seeds: Vec<HalfEdgeId> = e.data.tv_edge_selection.iter().filter_map(|(a, b)| geo_of(*a, *b)).collect();
            let shrink = matches!(cmd, C::TvLoopShrinkSelect | C::TvRingShrinkSelect);
            let grown: Vec<HalfEdgeId> = if matches!(cmd, C::TvLoopSelect | C::TvLoopGrowSelect | C::TvLoopShrinkSelect) { loom_mesh::select_ops::edge_loop(&mesh, &seeds) } else { loom_mesh::select_ops::edge_ring(&mesh, &seeds) };
            let grow_one = matches!(cmd, C::TvLoopGrowSelect | C::TvRingGrowSelect);
            let mut data = e.data.clone();
            let as_tv = |he: HalfEdgeId| -> Option<(u32, u32)> {
                let m = mesh.map(ch)?;
                let a = m.corners[he.0 as usize];
                let b = m.corners[mesh.half_edges()[he.0 as usize].next.0 as usize];
                Some((a.min(b), a.max(b)))
            };
            if shrink {
                // Drop the edges at the ends of each run.
                let ends: BTreeSet<(u32, u32)> = data.tv_edge_selection.iter().copied().filter(|(a, b)| {
                    let deg = |t: u32| data.tv_edge_selection.iter().filter(|(x, y)| *x == t || *y == t).count();
                    deg(*a) == 1 || deg(*b) == 1
                }).collect();
                for e in ends {
                    data.tv_edge_selection.remove(&e);
                }
            } else {
                let mut new: Vec<(u32, u32)> = grown.iter().filter_map(|h| as_tv(*h)).collect();
                if grow_one {
                    // One step: only edges touching the current selection's tvs.
                    let tvs: BTreeSet<u32> = data.tv_edge_selection.iter().flat_map(|(a, b)| [*a, *b]).collect();
                    new.retain(|(a, b)| tvs.contains(a) || tvs.contains(b));
                }
                data.tv_edge_selection.extend(new);
            }
            data.tv_selection = data.tv_edge_selection.iter().flat_map(|(a, b)| [*a, *b]).collect();
            store(state, &e, data, cmd.name())
        }
        C::GeomEdgeLoopSelection | C::GeomEdgeRingSelection => {
            let e = entry(state)?;
            let seeds = viewport_edges(state, e.id);
            let grown = if matches!(cmd, C::GeomEdgeLoopSelection) { loom_mesh::select_ops::edge_loop(&e.input, &seeds) } else { loom_mesh::select_ops::edge_ring(&e.input, &seeds) };
            for h in grown {
                if let Some(c) = e.input.canonical_edge_id(h) {
                    state.sub_selection.insert(e.id, SubObjectId::Edge(c));
                }
            }
            Ok(status(state))
        }
        C::GeomElement => {
            let e = entry(state)?;
            let faces = viewport_faces(state, e.id);
            for comp in uw::geometry_components(&e.input, &(0..e.input.face_count() as u32).map(FaceId).collect::<Vec<_>>()) {
                if comp.iter().any(|f| faces.contains(f)) {
                    for f in comp {
                        state.sub_selection.insert(e.id, SubObjectId::Face(f));
                    }
                }
            }
            Ok(status(state))
        }
        C::ExpandGeomPolygons | C::ContractGeomPolygons | C::PlanarThreshold => {
            let e = entry(state)?;
            let faces = viewport_faces(state, e.id);
            let set: BTreeSet<u32> = faces.iter().map(|f| f.0).collect();
            let next: BTreeSet<u32> = match cmd {
                C::ExpandGeomPolygons => {
                    let mut out = set.clone();
                    for f in &faces {
                        for he in e.input.face_corners(*f).unwrap_or_default() {
                            if let Some(o) = e.input.half_edges()[e.input.half_edges()[he.0 as usize].twin.0 as usize].face {
                                out.insert(o.0);
                            }
                        }
                    }
                    out
                }
                C::ContractGeomPolygons => set.iter().copied().filter(|f| e.input.face_corners(FaceId(*f)).unwrap_or_default().iter().all(|he| e.input.half_edges()[e.input.half_edges()[he.0 as usize].twin.0 as usize].face.is_none_or(|o| set.contains(&o.0)))).collect(),
                _ => {
                    // Planar Angle: grow to neighbours within the threshold of the seed normals.
                    let cos_t = e.data.planar_angle.get().to_radians().cos();
                    let mut out = set.clone();
                    let mut queue: Vec<FaceId> = faces.clone();
                    while let Some(f) = queue.pop() {
                        let n0 = e.input.face_normal(f);
                        for he in e.input.face_corners(f).unwrap_or_default() {
                            if let Some(o) = e.input.half_edges()[e.input.half_edges()[he.0 as usize].twin.0 as usize].face {
                                if !out.contains(&o.0) && e.input.face_normal(o).dot(n0) >= cos_t {
                                    out.insert(o.0);
                                    queue.push(o);
                                }
                            }
                        }
                    }
                    out
                }
            };
            state.sub_selection.retain_nodes(|n| n != e.id);
            for f in next {
                state.sub_selection.insert(e.id, SubObjectId::Face(FaceId(f)));
            }
            state.selection_mode = SelectionMode::Face;
            Ok(status(state))
        }
        C::OpenEdgeSelect | C::UvEdgeSelect => edit_data(state, cmd.name(), |mesh, ch, d| {
            d.tv_edge_selection = open_tv_edges(mesh, ch, matches!(cmd, C::UvEdgeSelect)).into_iter().collect();
            d.tv_selection = d.tv_edge_selection.iter().flat_map(|(a, b)| [*a, *b]).collect();
            Ok(())
        }),
        C::SelectGroup => {
            // Groups are corner (half-edge) lists: they survive renumbering.
            let groups = state.uv_editor.groups.clone();
            edit_data(state, "Select Group", |mesh, ch, d| {
                let Some(m) = mesh.map(ch) else { return Ok(()) };
                for g in &groups {
                    let tvs: BTreeSet<u32> = g.iter().filter_map(|c| m.corners.get(*c as usize).copied()).collect();
                    if tvs.iter().any(|t| d.tv_selection.contains(t)) {
                        d.tv_selection.extend(tvs);
                    }
                }
                Ok(())
            })
        }
        C::GroupSelected => {
            let e = entry(state)?;
            let (mesh, ch) = working(&e);
            let corners: Vec<u32> = mesh.map(ch).map(|m| m.corners.iter().enumerate().filter(|(_, t)| e.data.tv_selection.contains(t)).map(|(i, _)| i as u32).collect()).unwrap_or_default();
            if !corners.is_empty() {
                state.uv_editor.groups.push(corners);
            }
            Ok(status(state))
        }
        C::UngroupSelected => {
            let e = entry(state)?;
            let (mesh, ch) = working(&e);
            let sel = e.data.tv_selection.clone();
            if let Some(m) = mesh.map(ch) {
                state.uv_editor.groups.retain(|g| !g.iter().any(|c| m.corners.get(*c as usize).is_some_and(|t| sel.contains(t))));
            }
            Ok(status(state))
        }
        C::FreezeSelected => edit_data(state, "Freeze Selected", |_, _, d| { d.frozen_tvs.extend(d.tv_selection.iter().copied()); d.tv_selection.clear(); Ok(()) }),
        C::UnfreezeAllUnwrap => edit_data(state, "Unfreeze All", |_, _, d| { d.frozen_tvs.clear(); Ok(()) }),
        C::HideSelectedUnwrap => edit_data(state, "Hide Selected", |_, _, d| { d.hidden_tvs.extend(d.tv_selection.iter().copied()); d.tv_selection.clear(); Ok(()) }),
        C::UnhideAllUnwrap => edit_data(state, "Unhide All", |_, _, d| { d.hidden_tvs.clear(); Ok(()) }),
        C::PinSelected => edit_data(state, "Pin Selected", |_, _, d| { d.pins.extend(d.tv_selection.iter().copied()); Ok(()) }),
        C::UnpinSelected => edit_data(state, "Unpin Selected", |_, _, d| { for t in d.tv_selection.clone() { d.pins.remove(&t); } Ok(()) }),

        // --- transforms ---
        C::MoveTv { delta } => {
            if state.uv_editor.lock_selected {
                return Err("the selected vertices are locked".into());
            }
            let auto_pin = state.uv_editor.auto_pin_moved;
            let ed_ = state.uv_editor.clone();
            edit(state, "Move", |mesh, ch, d| {
                let w = ed_.soft_selection.then(|| if ed_.soft_edge_distance { uw::soft_weights_edges(mesh, ch, &d.tv_selection, ed_.soft_edge_range) } else { uw::soft_weights(mesh, ch, &d.tv_selection, ed_.soft_falloff) });
                let sel: BTreeSet<u32> = d.tv_selection.iter().copied().filter(|t| !d.frozen_tvs.contains(t)).collect();
                uw::move_tvs(mesh, ch, &sel, w.as_ref(), Vec2::from(delta));
                if auto_pin {
                    d.pins.extend(sel);
                }
                Ok(())
            })
        }
        C::RotateTv { .. } | C::RotatePlus90 | C::RotateMinus90 => {
            let degrees = match cmd { C::RotateTv { degrees } => degrees, C::RotatePlus90 => 90.0, _ => -90.0 };
            let e = entry(state)?;
            let (mesh, ch) = working(&e);
            let p = pivot(state, &mesh, ch, &e.data);
            let w = soft(state, &mesh, ch, &e.data);
            edit(state, "Rotate", |mesh, ch, d| {
                let sel: BTreeSet<u32> = d.tv_selection.iter().copied().filter(|t| !d.frozen_tvs.contains(t)).collect();
                uw::rotate_tvs(mesh, ch, &sel, w.as_ref(), degrees, p);
                Ok(())
            })
        }
        C::ScaleTv { factor } => {
            let e = entry(state)?;
            let (mesh, ch) = working(&e);
            let p = pivot(state, &mesh, ch, &e.data);
            let w = soft(state, &mesh, ch, &e.data);
            edit(state, "Scale", |mesh, ch, d| {
                let sel: BTreeSet<u32> = d.tv_selection.iter().copied().filter(|t| !d.frozen_tvs.contains(t)).collect();
                uw::scale_tvs(mesh, ch, &sel, w.as_ref(), Vec2::from(factor), p);
                Ok(())
            })
        }
        C::SetTvPosition { u, v, w } => edit(state, "Type-In", |mesh, ch, d| {
            let Some(m) = mesh.map_mut(ch) else { return Ok(()) };
            for t in &d.tv_selection {
                if let Some(p) = m.verts.get_mut(*t as usize) {
                    if let Some(u) = u { p.x = u; }
                    if let Some(v) = v { p.y = v; }
                    if let Some(w) = w { p.z = w; }
                }
            }
            Ok(())
        }),
        C::MirrorHorizontal | C::MirrorVertical => edit(state, cmd.name(), |mesh, ch, d| {
            let sel = target_tvs(mesh, ch, d);
            uw::mirror_tvs(mesh, ch, &sel, matches!(cmd, C::MirrorHorizontal));
            Ok(())
        }),
        C::FlipHorizontal | C::FlipVertical => edit(state, cmd.name(), |mesh, ch, d| {
            if d.tv_selection.is_empty() && d.face_selection.is_empty() {
                return Ok(());
            }
            let faces = target_faces(mesh, ch, d);
            uw::flip_faces(mesh, ch, &faces, matches!(cmd, C::FlipHorizontal));
            Ok(())
        }),
        C::BreakSelectedVertices => edit(state, "Break", |mesh, ch, d| {
            let sel = target_tvs(mesh, ch, d);
            uw::break_tvs(mesh, ch, &sel);
            Ok(())
        }),
        C::BreakBySubobject => {
            let level = state.uv_editor.level;
            edit(state, "Break", |mesh, ch, d| {
                match level {
                    TvLevel::Vertex => uw::break_tvs(mesh, ch, &target_tvs(mesh, ch, d)),
                    TvLevel::Edge => {
                        let edges: Vec<(u32, u32)> = d.tv_edge_selection.iter().copied().collect();
                        uw::break_edges(mesh, ch, &edges)
                    }
                    TvLevel::Face => uw::detach_faces(mesh, ch, &target_faces(mesh, ch, d)),
                }
                Ok(())
            })
        }
        C::DetachEdgeVerts => edit(state, "Detach Edge Verts", |mesh, ch, d| {
            if d.tv_selection.is_empty() && d.face_selection.is_empty() {
                return Ok(());
            }
            let faces = target_faces(mesh, ch, d);
            uw::detach_faces(mesh, ch, &faces);
            Ok(())
        }),
        C::TextureVertexWeldSelected => {
            let threshold = state.uv_editor.weld_threshold;
            edit(state, "Weld Selected", |mesh, ch, d| {
                let sel = target_tvs(mesh, ch, d);
                uw::weld_tvs(mesh, ch, &sel, threshold);
                Ok(())
            })
        }
        C::TextureVertexTargetWeld => {
            entry(state)?;
            state.pick_mode = Some(PickMode::TvTargetWeldSource);
            state.pick_source = None;
            Ok(status(state))
        }
        C::TargetWeldTv { from, to } => {
            state.pick_mode = None;
            edit(state, "Target Weld", |mesh, ch, _| uw::target_weld(mesh, ch, from, to))
        }
        C::Stitch | C::StitchAverage | C::StitchToSource | C::StitchToTarget => {
            let (bias, average) = match cmd {
                C::StitchAverage => (0.5, true),
                C::StitchToSource => (0.0, false),
                C::StitchToTarget => (1.0, false),
                _ => (state.uv_editor.stitch_bias, state.uv_editor.stitch_average),
            };
            edit(state, "Stitch", |mesh, ch, d| {
                let sel = target_tvs(mesh, ch, d);
                uw::stitch(mesh, ch, &sel, bias, average);
                Ok(())
            })
        }
        C::StitchDialog => { state.dialog = Some(crate::viewport::Dialog::StitchTool); Ok(json!({})) }
        C::RelaxDialog => { state.dialog = Some(crate::viewport::Dialog::RelaxTool); Ok(json!({})) }
        C::PackDialog => { state.dialog = Some(crate::viewport::Dialog::PackUvs); Ok(json!({})) }
        C::FlattenMapDialog => { state.dialog = Some(crate::viewport::Dialog::FlattenMapping); Ok(json!({})) }
        C::NormalMapDialog => { state.dialog = Some(crate::viewport::Dialog::NormalMapping); Ok(json!({})) }
        C::UnfoldMapDialog => { state.dialog = Some(crate::viewport::Dialog::UnfoldMapping); Ok(json!({})) }
        C::PeltDialog => { state.dialog = Some(crate::viewport::Dialog::PeltMap); Ok(json!({})) }
        C::OpenRenderTemplateDialog => { state.dialog = Some(crate::viewport::Dialog::RenderUvwTemplate); Ok(json!({})) }
        C::AlignHorizontallyToPivot | C::AlignVerticallyToPivot => {
            let e = entry(state)?;
            let (mesh, ch) = working(&e);
            let p = pivot(state, &mesh, ch, &e.data);
            let horizontal = matches!(cmd, C::AlignHorizontallyToPivot);
            edit(state, cmd.name(), |mesh, ch, d| {
                let sel = target_tvs(mesh, ch, d);
                uw::align(mesh, ch, &sel, horizontal, Some(if horizontal { p.y } else { p.x }));
                Ok(())
            })
        }
        C::AlignSelectionHorizontal | C::AlignSelectionVertical => edit(state, cmd.name(), |mesh, ch, d| {
            let sel = target_tvs(mesh, ch, d);
            uw::align(mesh, ch, &sel, matches!(cmd, C::AlignSelectionHorizontal), None);
            Ok(())
        }),
        C::AlignToEdge => edit(state, "Align to Edge", |mesh, ch, d| {
            let Some((a, b)) = d.tv_edge_selection.iter().next().copied() else { return Err("select an edge first".into()) };
            let Some(m) = mesh.map(ch) else { return Ok(()) };
            let (pa, pb) = (Vec2::new(m.verts[a as usize].x, m.verts[a as usize].y), Vec2::new(m.verts[b as usize].x, m.verts[b as usize].y));
            let angle = -(pb - pa).y.atan2((pb - pa).x).to_degrees();
            let sel = target_tvs(mesh, ch, d);
            uw::rotate_tvs(mesh, ch, &sel, None, angle, pa);
            Ok(())
        }),
        C::LinearAlign => edit(state, "Linear Align", |mesh, ch, d| { let sel = target_tvs(mesh, ch, d); uw::linear_align(mesh, ch, &sel); Ok(()) }),
        C::SpaceHorizontally | C::SpaceVertically => edit(state, cmd.name(), |mesh, ch, d| { let sel = target_tvs(mesh, ch, d); uw::space(mesh, ch, &sel, matches!(cmd, C::SpaceHorizontally)); Ok(()) }),
        C::StraightenEdges => edit(state, "Straighten Edges", |mesh, ch, d| { let edges: Vec<(u32, u32)> = d.tv_edge_selection.iter().copied().collect(); uw::straighten_edges(mesh, ch, &edges); Ok(()) }),
        C::RelaxUnwrap | C::RelaxOneClick | C::PeltDialogRunSimulation | C::PeltDialogRelaxSimulationLight | C::PeltDialogRelaxSimulationHeavy => {
            let ed_ = state.uv_editor.clone();
            let iterations = match cmd { C::RelaxOneClick => 1, C::PeltDialogRelaxSimulationLight => 5, C::PeltDialogRelaxSimulationHeavy => 50, C::PeltDialogRunSimulation => ed_.pelt_iterations, _ => ed_.relax_iterations };
            let by = match ed_.relax_by { 1 => uw::RelaxBy::EdgeAngles, 2 => uw::RelaxBy::Centers, _ => uw::RelaxBy::FaceAngles };
            edit(state, "Relax", |mesh, ch, d| {
                let sel: BTreeSet<u32> = target_tvs(mesh, ch, d).into_iter().filter(|t| !d.pins.contains(t) && !d.frozen_tvs.contains(t)).collect();
                uw::relax(mesh, ch, Some(&sel), iterations, ed_.relax_amount, ed_.relax_stretch, by, ed_.relax_keep_boundary);
                Ok(())
            })
        }
        C::Pack | C::PackButtons | C::PackNormalize | C::PackRegion | C::PackTogether => {
            let ed_ = state.uv_editor.clone();
            let method = if ed_.pack_method == 0 { uw::PackMethod::Linear } else { uw::PackMethod::Recursive };
            let normalize = match cmd { C::PackNormalize => true, C::PackTogether => false, _ => ed_.pack_normalize };
            edit(state, "Pack UVs", |mesh, ch, d| {
                let faces = if matches!(cmd, C::PackRegion | C::PackTogether) { target_faces(mesh, ch, d) } else { (0..mesh.face_count() as u32).map(FaceId).collect() };
                uw::pack(mesh, ch, &faces, method, ed_.pack_spacing, normalize, ed_.pack_rotate, ed_.pack_fill_holes);
                Ok(())
            })
        }
        C::Rescale | C::RescaleClusters => edit(state, "Rescale", |mesh, ch, d| {
            // Each cluster scaled so its UV area matches its 3D area (relative to the largest).
            let faces = target_faces(mesh, ch, d);
            let groups = uw::clusters(mesh, ch, &faces);
            let mut scale_max = 0.0f32;
            let mut per: Vec<(BTreeSet<u32>, f32)> = Vec::new();
            for g in &groups {
                let a3: f32 = g.iter().map(|f| { let p = mesh.face_positions(*f).unwrap_or_default(); let mut n = Vec3::ZERO; for i in 0..p.len() { n += p[i].cross(p[(i + 1) % p.len()]); } n.length() * 0.5 }).sum();
                let a2: f32 = g.iter().map(|f| uw::uv_area(mesh, ch, *f).abs()).sum();
                let s = if a2 > 1e-12 { (a3 / a2).sqrt() } else { 1.0 };
                scale_max = scale_max.max(s);
                per.push((uw::tvs_of_faces(mesh, ch, g), s));
            }
            for (tvs, s) in per {
                let c = uw::centre(mesh, ch, &tvs);
                uw::scale_tvs(mesh, ch, &tvs, None, Vec2::splat(s / scale_max.max(1e-6)), c);
            }
            Ok(())
        }),
        C::FlattenMap | C::FlattenByMaterialId | C::FlattenByMaterialIds | C::FlattenBySmoothingGroup => {
            let ed_ = state.uv_editor.clone();
            let by = match cmd { C::FlattenByMaterialId | C::FlattenByMaterialIds => uw::FlattenBy::MaterialId, C::FlattenBySmoothingGroup => uw::FlattenBy::SmoothingGroup, _ => match ed_.flatten_by { 1 => uw::FlattenBy::SmoothingGroup, 2 => uw::FlattenBy::MaterialId, _ => uw::FlattenBy::Angle } };
            edit(state, "Flatten Mapping", |mesh, ch, d| {
                if d.prevent_reflattening.get() && d.edits.is_some() {
                    return Err("Prevent Reflattening is on".into());
                }
                let faces = target_faces(mesh, ch, d);
                uw::flatten(mesh, ch, &faces, ed_.flatten_angle, ed_.flatten_spacing, ed_.flatten_normalize, ed_.flatten_rotate, ed_.flatten_fill_holes, by);
                Ok(())
            })
        }
        C::NormalMap => {
            let ed_ = state.uv_editor.clone();
            let kind = match ed_.normal_kind { 1 => uw::NormalKind::LeftRight, 2 => uw::NormalKind::TopBottom, 3 => uw::NormalKind::Box, 4 => uw::NormalKind::Diamond, _ => uw::NormalKind::BackFront };
            edit(state, "Normal Mapping", |mesh, ch, d| {
                let faces = target_faces(mesh, ch, d);
                uw::normal_map(mesh, ch, &faces, kind, ed_.normal_spacing, ed_.normal_normalize, ed_.normal_rotate, ed_.normal_align_width);
                Ok(())
            })
        }
        C::UnfoldMap | C::UnfoldEdge => {
            let ed_ = state.uv_editor.clone();
            let walk = if ed_.unfold_walk == 1 { uw::UnfoldWalk::Farthest } else { uw::UnfoldWalk::Closest };
            edit(state, "Unfold Mapping", |mesh, ch, d| {
                let faces = if matches!(cmd, C::UnfoldEdge) {
                    let tvf = uw::tv_faces(mesh, ch);
                    tvf.iter().enumerate().filter(|(_, t)| (0..t.len()).any(|i| { let (a, b) = (t[i], t[(i + 1) % t.len()]); d.tv_edge_selection.contains(&(a.min(b), a.max(b))) })).map(|(f, _)| FaceId(f as u32)).collect()
                } else {
                    target_faces(mesh, ch, d)
                };
                uw::unfold(mesh, ch, &faces, walk, ed_.unfold_normalize);
                Ok(())
            })
        }
        C::PeltMap | C::PeltDialogResetStretcher => {
            let iterations = if matches!(cmd, C::PeltDialogResetStretcher) { 0 } else { state.uv_editor.pelt_iterations };
            edit(state, "Pelt Map", |mesh, ch, d| {
                let faces = target_faces(mesh, ch, d);
                uw::pelt(mesh, ch, &faces, &d.seams, iterations, &d.pins);
                Ok(())
            })
        }
        C::PeltDialogSelectPeltUvs => edit_data(state, "Select Pelt UVs", |mesh, ch, d| {
            let faces = target_faces(mesh, ch, d);
            d.tv_selection = uw::tvs_of_faces(mesh, ch, &faces);
            Ok(())
        }),
        C::PeltEditSeams => ed(state, &|e| e.edit_seams = !e.edit_seams),
        C::PeltExpandSelectionToSeams => {
            let e = entry(state)?;
            let faces = viewport_faces(state, e.id);
            let set: BTreeSet<u32> = faces.iter().map(|f| f.0).collect();
            let mut out = set.clone();
            let mut queue = faces.clone();
            while let Some(f) = queue.pop() {
                for he in e.input.face_corners(f).unwrap_or_default() {
                    let h = &e.input.half_edges()[he.0 as usize];
                    let (a, b) = (h.origin.0, e.input.half_edges()[h.next.0 as usize].origin.0);
                    if e.data.seams.contains(&(a.min(b), a.max(b))) {
                        continue;
                    }
                    if let Some(o) = e.input.half_edges()[h.twin.0 as usize].face {
                        if out.insert(o.0) {
                            queue.push(o);
                        }
                    }
                }
            }
            state.sub_selection.retain_nodes(|n| n != e.id);
            for f in out {
                state.sub_selection.insert(e.id, SubObjectId::Face(FaceId(f)));
            }
            state.selection_mode = SelectionMode::Face;
            Ok(status(state))
        }
        C::PeltSeamToEdgeSelAdd | C::PeltSeamToEdgeSelReplace => {
            let e = entry(state)?;
            let index = edge_index(&e.input);
            if matches!(cmd, C::PeltSeamToEdgeSelReplace) {
                state.sub_selection.retain_nodes(|n| n != e.id);
            }
            for s in &e.data.seams {
                if let Some(h) = index.get(s) {
                    state.sub_selection.insert(e.id, SubObjectId::Edge(*h));
                }
            }
            state.selection_mode = SelectionMode::Edge;
            Ok(status(state))
        }
        C::EdgeSelToPeltSeamAdd | C::EdgeSelToPeltSeamReplace => {
            let e = entry(state)?;
            let pairs: Vec<(u32, u32)> = viewport_edges(state, e.id).iter().filter_map(|h| edge_pair(&e.input, *h)).collect();
            let replace = matches!(cmd, C::EdgeSelToPeltSeamReplace);
            edit_data(state, "Edge Sel To Seams", |_, _, d| {
                if replace {
                    d.seams.clear();
                }
                d.seams.extend(pairs);
                Ok(())
            })
        }
        C::ClearSeams => edit_data(state, "Clear Seams", |_, _, d| { d.seams.clear(); Ok(()) }),
        C::ToggleSeamEdge { node, edge } => {
            let e = entry(state)?;
            if NodeId(node) != e.id {
                return Err("pick an edge of the Unwrap object".into());
            }
            let pair = edge_pair(&e.input, HalfEdgeId(edge)).ok_or_else(|| format!("no edge {edge}"))?;
            edit_data(state, "Edit Seams", |_, _, d| {
                if !d.seams.remove(&pair) {
                    d.seams.insert(pair);
                }
                Ok(())
            })
        }
        C::PointToPointEdgeSelection => {
            entry(state)?;
            state.pick_mode = Some(PickMode::SeamPointA);
            state.seam_point = None;
            Ok(status(state))
        }
        C::SeamPoint { node, vertex } => {
            let e = entry(state)?;
            if NodeId(node) != e.id {
                return Err("pick a vertex of the Unwrap object".into());
            }
            if (vertex as usize) >= e.input.vertex_count() {
                return Err(format!("no vertex {vertex}"));
            }
            match state.seam_point.take() {
                None => {
                    state.seam_point = Some(vertex);
                    state.pick_mode = Some(PickMode::SeamPointB);
                    Ok(status(state))
                }
                Some(a) => {
                    let path = uw::seam_path(&e.input, VertexId(a), VertexId(vertex));
                    state.seam_point = Some(vertex);
                    state.pick_mode = Some(PickMode::SeamPointB);
                    edit_data(state, "Point To Point Seam", |_, _, d| { d.seams.extend(path); Ok(()) })
                }
            }
        }
        C::PlanarMapFaces | C::BoxMap | C::CylindricalMap | C::SphericalMap => {
            let kind = match cmd { C::BoxMap => 4, C::CylindricalMap => 2, C::SphericalMap => 3, _ => 0 };
            let normalize_pref = entry(state)?.data.normalize_map.get();
            edit(state, cmd.name(), |mesh, ch, d| {
                let faces = target_faces(mesh, ch, d);
                if d.map_size == Vec3::ONE && d.map_gizmo == loom_scene::Transform::default() {
                    fit_gizmo(mesh, &faces, d, None, false);
                }
                let (projection, gizmo, size) = map_params(d, kind);
                uw::map_with_gizmo(mesh, ch, &faces, projection, gizmo, size);
                if normalize_pref {
                    let tvs = uw::tvs_of_faces(mesh, ch, &faces);
                    let (lo, hi) = uw::bounds(mesh, ch, Some(&tvs));
                    let extent = (hi - lo).max_element().max(1e-6);
                    uw::transform_tvs(mesh, ch, &tvs, None, |q| (q - lo) / extent);
                }
                d.map_kind.set(kind);
                Ok(())
            })
        }
        C::QuickPlanarMap => edit(state, "Quick Planar Map", |mesh, ch, d| {
            let faces = target_faces(mesh, ch, d);
            uw::quick_planar(mesh, ch, &faces, None);
            Ok(())
        }),
        C::MappingAlignX | C::MappingAlignY | C::MappingAlignZ | C::MappingAlignNormal | C::MappingFit | C::MappingCenter | C::MappingReset => edit_data(state, cmd.name(), |mesh, ch, d| {
            let faces = target_faces(mesh, ch, d);
            match cmd {
                C::MappingAlignX => fit_gizmo(mesh, &faces, d, Some(Vec3::X), false),
                C::MappingAlignY => fit_gizmo(mesh, &faces, d, Some(Vec3::Y), false),
                C::MappingAlignZ | C::MappingReset => fit_gizmo(mesh, &faces, d, Some(Vec3::Z), false),
                C::MappingAlignNormal => { let n = selected_faces_normal(mesh, &faces); fit_gizmo(mesh, &faces, d, Some(n), false) }
                C::MappingCenter => fit_gizmo(mesh, &faces, d, None, true),
                _ => fit_gizmo(mesh, &faces, d, None, false),
            }
            Ok(())
        }),
        C::MappingAlignToView => {
            let vp = state.active();
            let forward = (vp.camera.eye_position() - vp.camera.target).normalize_or_zero();
            let e = entry(state)?;
            let local = state.scene.object_matrix(e.id).inverse().transform_vector3(forward).normalize_or_zero();
            edit_data(state, "Align To View", |mesh, ch, d| { let faces = target_faces(mesh, ch, d); fit_gizmo(mesh, &faces, d, Some(local), false); Ok(()) })
        }
        C::RenderUvwTemplate { path, width, height, mode } => {
            if width == 0 || height == 0 || width > 8192 || height > 8192 {
                return Err(format!("template size {width}x{height}: 1 to 8192 pixels"));
            }
            let e = entry(state)?;
            let (mesh, ch) = working(&e);
            let solid = mode.eq_ignore_ascii_case("solid");
            let img = render_template(&mesh, ch, width, height, solid);
            img.save(&path).map_err(|e| format!("{path}: {e}"))?;
            Ok(json!({ "path": path, "width": width, "height": height }))
        }
        C::SaveUvw { path } => {
            let Some(path) = path else {
                state.dialog = Some(crate::viewport::Dialog::SaveUvw);
                return Ok(json!({ "dialog": "SaveUvw" }));
            };
            let e = entry(state)?;
            let (mesh, ch) = working(&e);
            let m = mesh.map(ch).cloned().unwrap_or_default_channel();
            let doc = json!({ "loom_uvw": 1, "channel": ch, "faces": mesh.face_count(), "vertices": mesh.vertex_count(), "corners": m.corners.len(), "verts": m.verts.iter().map(|v| [v.x, v.y, v.z]).collect::<Vec<_>>(), "corners_tv": m.corners });
            std::fs::write(&path, serde_json::to_string(&doc).map_err(|e| e.to_string())?).map_err(|e| format!("{path}: {e}"))?;
            Ok(json!({ "path": path }))
        }
        C::LoadUvw { path } => {
            let Some(path) = path else {
                state.dialog = Some(crate::viewport::Dialog::LoadUvw);
                return Ok(json!({ "dialog": "LoadUvw" }));
            };
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
            let doc: Value = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
            let verts: Vec<Vec3> = doc["verts"].as_array().ok_or("not a .uvw file")?.iter().filter_map(|v| Some(Vec3::new(v[0].as_f64()? as f32, v[1].as_f64()? as f32, v[2].as_f64()? as f32))).collect();
            let corners: Vec<u32> = doc["corners_tv"].as_array().ok_or("not a .uvw file")?.iter().filter_map(|c| c.as_u64().map(|c| c as u32)).collect();
            let (faces, vertices) = (doc["faces"].as_u64(), doc["vertices"].as_u64());
            edit(state, "Load UVW", |mesh, ch, _| {
                if corners.len() != mesh.half_edge_count() || faces.is_some_and(|f| f as usize != mesh.face_count()) || vertices.is_some_and(|v| v as usize != mesh.vertex_count()) {
                    return Err(format!("{path}: made for a mesh with {} corners, {} faces and {} vertices; this one has {}, {} and {}", corners.len(), faces.unwrap_or(0), vertices.unwrap_or(0), mesh.half_edge_count(), mesh.face_count(), mesh.vertex_count()));
                }
                if corners.iter().any(|c| *c as usize >= verts.len()) {
                    return Err(format!("{path}: a corner points past the texture vertices"));
                }
                mesh.set_map(ch, loom_mesh::half_edge::MapChannel { verts: verts.clone(), corners: corners.clone() });
                Ok(())
            })
        }
        C::ResetUvws => {
            let e = entry(state)?;
            let mut data = e.data.clone();
            data.edits = None;
            data.tv_selection.clear();
            data.tv_edge_selection.clear();
            data.init(&e.input);
            store(state, &e, data, "Reset UVWs")
        }
        C::CopyTv => {
            let e = entry(state)?;
            let (mesh, ch) = working(&e);
            let faces = target_faces(&mesh, ch, &e.data);
            let mut buf = Vec::new();
            for f in &faces {
                for he in mesh.face_corners(*f).unwrap_or_default() {
                    let uv = mesh.corner_uv(ch, he).unwrap_or(Vec3::ZERO);
                    buf.push([uv.x, uv.y, uv.z]);
                }
            }
            state.uv_editor.clipboard = buf;
            Ok(status(state))
        }
        C::PasteTv | C::PasteInstance | C::PasteSymmetrical => {
            let clip = state.uv_editor.clipboard.clone();
            if clip.is_empty() {
                return Err("nothing copied".into());
            }
            let mirror = matches!(cmd, C::PasteSymmetrical);
            edit(state, "Paste", |mesh, ch, d| {
                let faces = target_faces(mesh, ch, d);
                let corners: Vec<HalfEdgeId> = faces.iter().flat_map(|f| mesh.face_corners(*f).unwrap_or_default()).collect();
                if corners.len() != clip.len() {
                    return Err(format!("the selection has {} corners, the copy {}", corners.len(), clip.len()));
                }
                let (lo, hi) = {
                    let mut lo = Vec2::splat(f32::INFINITY);
                    let mut hi = Vec2::splat(f32::NEG_INFINITY);
                    for c in &clip {
                        lo = lo.min(Vec2::new(c[0], c[1]));
                        hi = hi.max(Vec2::new(c[0], c[1]));
                    }
                    (lo, hi)
                };
                let vals: Vec<Vec3> = clip.iter().map(|c| if mirror { Vec3::new(lo.x + hi.x - c[0], c[1], c[2]) } else { Vec3::from(*c) }).collect();
                let by_corner: HashMap<u32, Vec3> = corners.iter().zip(vals).map(|(h, v)| (h.0, v)).collect();
                uw::assign_corner_uvs(mesh, ch, &faces, |he| by_corner.get(&he.0).copied().unwrap_or(Vec3::ZERO));
                Ok(())
            })
        }
        other => Err(format!("{} is not an Unwrap UVW command", other.name())),
    }
}

/// Render UVW Template: the texture edges (or filled faces) of the unit
/// square in a black image.
fn render_template(mesh: &Mesh, ch: u32, width: u32, height: u32, solid: bool) -> image::RgbaImage {
    let mut img = image::RgbaImage::from_pixel(width, height, image::Rgba([0, 0, 0, 255]));
    let Some(m) = mesh.map(ch) else { return img };
    let mut put = |x: i64, y: i64, c: [u8; 4]| {
        if x >= 0 && y >= 0 && (x as u32) < width && (y as u32) < height {
            img.put_pixel(x as u32, y as u32, image::Rgba(c));
        }
    };
    if solid {
        for f in 0..mesh.face_count() {
            let pts: Vec<(f32, f32)> = uw::face_tvs(mesh, ch, FaceId(f as u32)).iter().map(|t| { let v = m.verts[*t as usize]; (v.x * (width - 1) as f32, (1.0 - v.y) * (height - 1) as f32) }).collect();
            if pts.len() < 3 {
                continue;
            }
            let (ylo, yhi) = pts.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), p| (a.min(p.1), b.max(p.1)));
            for y in ylo.max(0.0) as i64..=(yhi.min(height as f32 - 1.0)) as i64 {
                let yf = y as f32 + 0.5;
                let mut xs: Vec<f32> = Vec::new();
                for i in 0..pts.len() {
                    let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
                    if (p.1 <= yf) != (q.1 <= yf) {
                        xs.push(p.0 + (yf - p.1) / (q.1 - p.1) * (q.0 - p.0));
                    }
                }
                xs.sort_by(|a, b| a.total_cmp(b));
                for pair in xs.chunks(2) {
                    if let [a, b] = pair {
                        for x in a.max(0.0) as i64..=(b.min(width as f32 - 1.0)) as i64 {
                            put(x, y, [90, 90, 160, 255]);
                        }
                    }
                }
            }
        }
    }
    let (w, h) = ((width - 1) as f32, (height - 1) as f32);
    for (a, b) in uw::tv_edges(mesh, ch) {
        let pa = m.verts[a as usize];
        let pb = m.verts[b as usize];
        let (ax, ay) = (pa.x * w, (1.0 - pa.y) * h);
        let (bx, by) = (pb.x * w, (1.0 - pb.y) * h);
        // Clip to the image (Liang–Barsky) before stepping: far-away tvs stay cheap.
        let Some((x0, y0, x1, y1)) = clip_segment(ax, ay, bx, by, w, h) else { continue };
        let steps = ((x1 - x0).abs().max((y1 - y0).abs()).ceil() as i64).clamp(1, 2 * (width + height) as i64);
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            put((x0 + (x1 - x0) * t).round() as i64, (y0 + (y1 - y0) * t).round() as i64, [255, 255, 255, 255]);
        }
    }
    img
}

/// Liang–Barsky clip of a segment to `0..=w` × `0..=h`.
fn clip_segment(x0: f32, y0: f32, x1: f32, y1: f32, w: f32, h: f32) -> Option<(f32, f32, f32, f32)> {
    if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
        return None;
    }
    let (dx, dy) = (x1 - x0, y1 - y0);
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [(-dx, x0), (dx, w - x0), (-dy, y0), (dy, h - y0)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 {
                    return None;
                }
                t0 = t0.max(r);
            } else {
                if r < t0 {
                    return None;
                }
                t1 = t1.min(r);
            }
        }
    }
    (t0 <= t1).then_some((x0 + dx * t0, y0 + dy * t0, x0 + dx * t1, y0 + dy * t1))
}

/// Open tv edges (used by one face), or only those whose geometry edge has
/// a face on both sides (UV seams).
fn open_tv_edges(mesh: &Mesh, ch: u32, seams_only: bool) -> Vec<(u32, u32)> {
    let mut count: HashMap<(u32, u32), u32> = HashMap::new();
    for t in uw::tv_faces(mesh, ch) {
        for i in 0..t.len() {
            let (a, b) = (t[i], t[(i + 1) % t.len()]);
            *count.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let open: Vec<(u32, u32)> = count.iter().filter(|(_, c)| **c == 1).map(|(e, _)| *e).collect();
    if !seams_only {
        return open;
    }
    let index = edge_index(mesh);
    let Some(m) = mesh.map(ch) else { return Vec::new() };
    open.into_iter()
        .filter(|(a, b)| {
            let (Some(ia), Some(ib)) = (m.corners.iter().position(|c| c == a), m.corners.iter().position(|c| c == b)) else { return false };
            let (va, vb) = (mesh.half_edges()[ia].origin.0, mesh.half_edges()[ib].origin.0);
            index.get(&(va.min(vb), va.max(vb))).is_some_and(|h| mesh.half_edges()[mesh.half_edges()[h.0 as usize].twin.0 as usize].face.is_some())
        })
        .collect()
}

/// Spline mapping: V is the arc length of the nearest point on `path`
/// (over its length when `normalize`), U the angle around it (Circular,
/// one turn = 1) or the signed distance across it over the widest (Planar).
fn spline_map(mesh: &mut Mesh, ch: u32, faces: &[FaceId], path: &[Vec3], mapping: u32, normalize: bool) {
    let mut acc = vec![0.0f32];
    for w in path.windows(2) {
        acc.push(acc.last().unwrap() + w[0].distance(w[1]));
    }
    let total = acc.last().copied().unwrap_or(0.0).max(1e-9);
    // Nearest point: (arc length, point, tangent).
    let nearest = |p: Vec3| -> (f32, Vec3, Vec3) {
        let mut best = (f32::INFINITY, 0.0, path[0], Vec3::Z);
        for (i, w) in path.windows(2).enumerate() {
            let ab = w[1] - w[0];
            let t = if ab.length_squared() < 1e-12 { 0.0 } else { ((p - w[0]).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) };
            let q = w[0] + ab * t;
            let d = q.distance(p);
            if d < best.0 {
                best = (d, acc[i] + ab.length() * t, q, ab.normalize_or_zero());
            }
        }
        (best.1, best.2, best.3)
    };
    let frame = |tan: Vec3| -> (Vec3, Vec3) {
        let mut up = Vec3::Z - tan * Vec3::Z.dot(tan);
        if up.length_squared() < 1e-6 {
            up = Vec3::Y - tan * Vec3::Y.dot(tan);
        }
        let up = up.normalize();
        (up.cross(tan), up)
    };
    let mut raw: HashMap<HalfEdgeId, Vec3> = HashMap::new();
    let mut widest = 1e-9f32;
    for f in faces {
        for he in mesh.face_corners(*f).unwrap_or_default() {
            let v = mesh.half_edges()[he.0 as usize].origin;
            let p = mesh.vertex_position(v).unwrap_or_default();
            let (s, q, tan) = nearest(p);
            let (side, up) = frame(tan);
            let d = p - q;
            let u = if mapping == 1 { d.dot(side) } else { d.dot(up).atan2(d.dot(side)) / std::f32::consts::TAU + 0.5 };
            widest = widest.max(d.dot(side).abs());
            raw.insert(he, Vec3::new(u, if normalize { s / total } else { s }, 0.0));
        }
    }
    if mapping == 1 {
        for uv in raw.values_mut() {
            uv.x = 0.5 + 0.5 * uv.x / widest;
        }
    } else {
        // A face that straddles the seam of the turn keeps its corners together.
        for f in faces {
            let cs = mesh.face_corners(*f).unwrap_or_default();
            let us: Vec<f32> = cs.iter().filter_map(|c| raw.get(c).map(|u| u.x)).collect();
            let (lo, hi) = us.iter().fold((f32::MAX, f32::MIN), |(a, b), u| (a.min(*u), b.max(*u)));
            if hi - lo > 0.5 {
                for c in cs {
                    if let Some(uv) = raw.get_mut(&c) {
                        if uv.x < 0.5 {
                            uv.x += 1.0;
                        }
                    }
                }
            }
        }
    }
    uw::assign_corner_uvs(mesh, ch, faces, |he| raw.get(&he).copied().unwrap_or_default());
}

/// What a click in Freeform mode grabs on the gizmo box `lo`..`hi` with its
/// pivot cross; `tol` in UV units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreeformHit {
    /// A corner (0 lower left, then counter-clockwise): scale.
    Scale { corner: u8 },
    /// An edge's middle: rotate.
    Rotate,
    /// The pivot cross: move the pivot.
    Pivot,
    /// Inside the box: move.
    Move,
    /// Nothing of the gizmo: an ordinary selection click.
    Outside,
}

pub fn freeform_hit(lo: Vec2, hi: Vec2, pivot: Vec2, p: Vec2, tol: f32, inside_selects: bool) -> FreeformHit {
    if p.distance(pivot) <= tol {
        return FreeformHit::Pivot;
    }
    let corners = [lo, Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y)];
    if let Some(i) = corners.iter().position(|c| c.distance(p) <= tol) {
        return FreeformHit::Scale { corner: i as u8 };
    }
    let mids = [(corners[0] + corners[1]) * 0.5, (corners[1] + corners[2]) * 0.5, (corners[2] + corners[3]) * 0.5, (corners[3] + corners[0]) * 0.5];
    if mids.iter().any(|m| m.distance(p) <= tol) {
        return FreeformHit::Rotate;
    }
    let inside = p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y;
    if inside && !inside_selects {
        FreeformHit::Move
    } else {
        FreeformHit::Outside
    }
}
