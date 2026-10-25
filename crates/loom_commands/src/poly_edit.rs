//! Editable Poly > Edit Vertices / Edges / Borders operations: the
//! rollout buttons, their settings windows' values, the pick and drag modes
//! and Repeat Last Operation.

use crate::execute::{edit_meshes, selected_edges, selected_faces, selected_vertices, selection_json};
use crate::state::{AppState, PickMode, PolyDrag};
use crate::viewport::{Dialog, PolySettingsKind};
use crate::{Command as C, SelectionMode, SubObjectId};
use loom_mesh::geom_ops as go;
use loom_mesh::poly_ops as po;
use loom_mesh::topo_ops as t;
use loom_mesh::{FaceId, HalfEdgeId, Mesh, VertexId};
use loom_scene::{NodeId, ObjectKind};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    if matches!(
        cmd,
        C::Remove | C::CleanRemove | C::Break | C::SplitEdges | C::Weld { .. } | C::Chamfer { .. } | C::ConnectEdges { .. } | C::ConnectVertices | C::Connect | C::Cap | C::Bridge { .. } | C::Extrude { .. } | C::Turn | C::RemoveIsolatedVertices
            | C::Inset { .. } | C::Bevel { .. } | C::Outline { .. } | C::HingeFromEdge { .. } | C::FlipNormals | C::MakePlanar | C::MakePlanarAxis { .. } | C::ViewAlign | C::AlignToGrid
            | C::Slice | C::MSmooth { .. } | C::TessellateFaces { .. } | C::Relax { .. } | C::MeshSmooth | C::Tessellate
    ) {
        state.last_poly_op = Some(cmd.clone());
    }
    let ps = state.poly_settings;
    match cmd {
        C::Remove => match state.selection_mode {
            SelectionMode::Vertex => {
                let verts = selected_vertices(state);
                edit_meshes(state, "Remove", verts, |mesh, _, ids| t::remove_vertices(mesh, &ids).map(|_| ()))?;
                state.sub_selection.clear();
                Ok(json!({}))
            }
            SelectionMode::Edge | SelectionMode::Border => crate::execute(state, C::DissolveEdges),
            _ => Err("Remove works at Vertex and Edge level".into()),
        },
        C::Break => {
            let verts = selected_vertices(state);
            edit_meshes(state, "Break", verts, |mesh, _, ids| t::break_vertices(mesh, &ids).map(|_| ()))?;
            state.sub_selection.clear();
            Ok(json!({}))
        }
        C::SplitEdges => {
            let edges = selected_edges(state);
            edit_meshes(state, "Split Edges", edges, |mesh, _, ids| t::split_edges(mesh, &ids).map(|_| ()))?;
            state.sub_selection.clear();
            Ok(json!({}))
        }
        C::Weld { threshold } => {
            state.poly_settings.weld_threshold = threshold.max(0.0);
            let verts = selected_vertices(state);
            let mut welded = 0;
            edit_meshes(state, "Weld", verts, |mesh, _, ids| {
                welded += t::weld_vertices(mesh, &ids, threshold)?;
                Ok(())
            })?;
            state.sub_selection.clear();
            Ok(json!({ "welded": welded }))
        }
        C::WeldSettings => open_settings(state, PolySettingsKind::Weld),
        C::TargetWeldMode => toggle_pick(state, PickMode::TargetWeldSource, &[PickMode::TargetWeldSource, PickMode::TargetWeldTarget]),
        C::Chamfer { amount, segments, open } => {
            state.poly_settings.chamfer_amount = amount.max(0.0);
            state.poly_settings.chamfer_segments = segments.max(1);
            state.poly_settings.chamfer_open = open;
            let mut new_faces: Vec<(NodeId, Vec<FaceId>)> = Vec::new();
            match state.selection_mode {
                SelectionMode::Vertex => {
                    let verts = selected_vertices(state);
                    edit_meshes(state, "Chamfer", verts, |mesh, node, ids| {
                        new_faces.push((node, t::chamfer_vertices(mesh, &ids, amount, open)?));
                        Ok(())
                    })?;
                }
                SelectionMode::Edge | SelectionMode::Border => {
                    let edges = selected_edges(state);
                    edit_meshes(state, "Chamfer", edges, |mesh, node, ids| {
                        new_faces.push((node, t::chamfer_edges(mesh, &ids, amount, segments.max(1), open)?));
                        Ok(())
                    })?;
                }
                _ => return Err("Chamfer works at Vertex and Edge level".into()),
            }
            if state.selection_mode == SelectionMode::Vertex {
                // Vertex level: the new polygons' vertices stay selected.
                state.sub_selection.clear();
                for (n, fs) in new_faces {
                    let Some(node) = state.scene.node(n) else { continue };
                    let mesh = node.cage_mesh().into_owned();
                    for f in fs {
                        for v in mesh.face_vertex_ids(f).unwrap_or_default() {
                            state.sub_selection.insert(n, SubObjectId::Vertex(v));
                        }
                    }
                }
            } else {
                select_strip_edges(state, new_faces);
            }
            Ok(selection_json(state))
        }
        C::ChamferMode => toggle_drag(state, PolyDrag::Chamfer),
        C::ChamferSettings => open_settings(state, PolySettingsKind::Chamfer),
        C::Connect => match state.selection_mode {
            SelectionMode::Vertex => execute(state, C::ConnectVertices),
            SelectionMode::Edge | SelectionMode::Border => execute(state, C::ConnectEdges { segments: ps.connect_segments, pinch: ps.connect_pinch, slide: ps.connect_slide }),
            _ => Err("Connect works at Vertex and Edge level".into()),
        },
        C::ConnectVertices => {
            let verts = selected_vertices(state);
            let mut made = 0;
            edit_meshes(state, "Connect", verts, |mesh, _, ids| {
                made += t::connect_vertices(mesh, &ids)?;
                Ok(())
            })?;
            state.sub_selection.clear();
            Ok(json!({ "connected": made }))
        }
        C::ConnectEdges { segments, pinch, slide } => {
            state.poly_settings.connect_segments = segments.max(1);
            state.poly_settings.connect_pinch = pinch.clamp(-100.0, 100.0);
            state.poly_settings.connect_slide = slide.clamp(-100.0, 100.0);
            let edges = selected_edges(state);
            let mut new_edges: Vec<(NodeId, Vec<(glam::Vec3, glam::Vec3)>)> = Vec::new();
            edit_meshes(state, "Connect", edges, |mesh, node, ids| {
                new_edges.push((node, t::connect_edges(mesh, &ids, segments.max(1), pinch, slide)?));
                Ok(())
            })?;
            state.sub_selection.clear();
            state.selection_mode = SelectionMode::Edge;
            for (n, segs) in new_edges {
                let Some(node) = state.scene.node(n) else { continue };
                let mesh = node.cage_mesh().into_owned();
                for (a, b) in segs {
                    if let Some(e) = edge_at(&mesh, a, b) {
                        state.sub_selection.insert(n, SubObjectId::Edge(e));
                    }
                }
            }
            Ok(selection_json(state))
        }
        C::ConnectEdgeSettings => open_settings(state, PolySettingsKind::ConnectEdges),
        C::RemoveIsolatedVertices => {
            let polys: Vec<(NodeId, Vec<()>)> = selected_polys(state).into_iter().map(|n| (n, vec![])).collect();
            let mut removed = 0;
            edit_meshes(state, "Remove Isolated Vertices", polys, |mesh, _, _| {
                removed += t::remove_isolated_vertices(mesh)?;
                Ok(())
            })?;
            state.sub_selection.clear();
            Ok(json!({ "removed": removed }))
        }
        C::RemoveUnusedMapVertices => Ok(json!({ "removed": 0 })),
        C::InsertVertexMode => toggle_pick(state, PickMode::InsertVertex, &[PickMode::InsertVertex]),
        C::InsertVertexOnEdge { node, edge, t } => {
            let id = NodeId(node);
            let mut made = None;
            edit_meshes(state, "Insert Vertex", vec![(id, vec![HalfEdgeId(edge)])], |mesh, _, ids| {
                made = Some(t::insert_vertex_on_edge(mesh, ids[0], t.clamp(0.0, 1.0))?);
                Ok(())
            })?;
            Ok(json!({ "vertex": made.map(|v| v.0) }))
        }
        C::InsertVertexInFace { node, face, point } => {
            let id = NodeId(node);
            let local = state.scene.object_matrix(id).inverse().transform_point3(glam::Vec3::from(point));
            let mut made = None;
            edit_meshes(state, "Insert Vertex", vec![(id, vec![FaceId(face)])], |mesh, _, ids| {
                made = Some(t::insert_vertex_in_face(mesh, ids[0], local)?);
                Ok(())
            })?;
            Ok(json!({ "vertex": made.map(|v| v.0) }))
        }
        C::CreateVertex { node, point } => {
            let id = NodeId(node);
            let local = state.scene.object_matrix(id).inverse().transform_point3(glam::Vec3::from(point));
            let mut made = 0;
            edit_meshes(state, "Create Vertex", vec![(id, vec![()])], |mesh, _, _| {
                let mut im = loom_mesh::indexed::IndexedMesh::from_mesh(mesh);
                made = im.add_vertex(local, None);
                im.keep.insert(made);
                *mesh = im.build()?;
                Ok(())
            })?;
            state.sub_selection.clear();
            state.sub_selection.insert(id, SubObjectId::Vertex(VertexId(made)));
            Ok(selection_json(state))
        }
        C::Cap => {
            let edges = selected_edges(state);
            let mut made = 0;
            edit_meshes(state, "Cap", edges, |mesh, _, ids| {
                made += t::cap_borders(mesh, &ids)?.len();
                Ok(())
            })?;
            state.sub_selection.clear();
            Ok(json!({ "capped": made }))
        }
        C::Bridge { segments, smooth, bridge_adjacent, twist1, twist2 } => {
            state.poly_settings.bridge_segments = segments.max(1);
            state.poly_settings.bridge_smooth = smooth;
            state.poly_settings.bridge_adjacent = bridge_adjacent;
            state.poly_settings.bridge_twist1 = twist1;
            state.poly_settings.bridge_twist2 = twist2;
            let mut new_faces: Vec<(NodeId, Vec<FaceId>)> = Vec::new();
            let edge_level = state.selection_mode == SelectionMode::Edge;
            let specific = edge_level && state.poly_settings.bridge_specific;
            // Edge level keeps the bridged edges selected (by position, ids change).
            let mut keep_sel: Vec<(NodeId, Vec<(glam::Vec3, glam::Vec3)>)> = Vec::new();
            match state.selection_mode {
                SelectionMode::Border | SelectionMode::Edge => {
                    let edges = if specific {
                        match state.poly_settings.bridge_edges {
                            [Some((n1, e1)), Some((n2, e2))] if n1 == n2 => vec![(n1, vec![HalfEdgeId(e1), HalfEdgeId(e2)])],
                            _ => return Err("Bridge Specific Edges: pick Edge 1 and Edge 2 on one object first".into()),
                        }
                    } else {
                        selected_edges(state)
                    };
                    edit_meshes(state, "Bridge", edges, |mesh, node, ids| {
                        if edge_level {
                            // Runs of selected border edges, paired in increasing order of id.
                            let runs = t::border_runs(mesh, &ids);
                            if runs.iter().any(|r| !r.1) {
                                if runs.iter().any(|r| r.1) {
                                    return Err("Bridge: a whole border cannot be bridged to part of a border".into());
                                }
                                if runs.len() < 2 {
                                    return Err(format!("Bridge needs two border edge selections ({} found)", runs.len()));
                                }
                                let ends: Vec<(glam::Vec3, glam::Vec3)> = ids
                                    .iter()
                                    .filter_map(|e| mesh.edge_vertices(*e))
                                    .filter_map(|(a, b)| Some((mesh.vertex_position(a)?, mesh.vertex_position(b)?)))
                                    .collect();
                                let pairs: Vec<(Vec<HalfEdgeId>, Vec<HalfEdgeId>)> = runs.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].0.clone(), c[1].0.clone())).collect();
                                new_faces.push((node, t::bridge_edge_runs(mesh, &pairs, segments.max(1))?));
                                keep_sel.push((node, ends));
                                return Ok(());
                            }
                        }
                        let loops = border_loops(mesh, &ids);
                        if loops.len() != 2 {
                            return Err(format!("Bridge needs exactly two borders selected ({} found)", loops.len()));
                        }
                        new_faces.push((node, t::bridge(mesh, &loops[0], &loops[1], None, segments.max(1), twist1 - twist2)?));
                        Ok(())
                    })?;
                }
                SelectionMode::Face | SelectionMode::Element => {
                    let faces = selected_faces(state);
                    edit_meshes(state, "Bridge", faces, |mesh, node, ids| {
                        if ids.len() != 2 {
                            return Err(format!("Bridge needs exactly two polygons selected ({} found)", ids.len()));
                        }
                        new_faces.push((node, t::bridge(mesh, &[], &[], Some((ids[0], ids[1])), segments.max(1), twist1 - twist2)?));
                        Ok(())
                    })?;
                }
                _ => return Err("Bridge works at Border and Polygon level".into()),
            }
            state.sub_selection.clear();
            if matches!(state.selection_mode, SelectionMode::Face | SelectionMode::Element) {
                for (n, fs) in new_faces {
                    for f in fs {
                        state.sub_selection.insert(n, SubObjectId::Face(f));
                    }
                }
            } else if !specific {
                for (n, ends) in keep_sel {
                    let Some(node) = state.scene.node(n) else { continue };
                    let mesh = node.cage_mesh().into_owned();
                    for (a, b) in ends {
                        if let Some(e) = edge_at(&mesh, a, b) {
                            state.sub_selection.insert(n, SubObjectId::Edge(e));
                        }
                    }
                }
            }
            Ok(selection_json(state))
        }
        C::BridgeSettings => open_settings(state, PolySettingsKind::Bridge),
        C::BridgeSpecificEdges { on } => {
            state.poly_settings.bridge_specific = on;
            Ok(json!({ "specific": on }))
        }
        C::PickBridgeEdge { which } => {
            state.poly_settings.bridge_specific = true;
            state.pick_mode = Some(if which == 2 { PickMode::BridgeEdge2 } else { PickMode::BridgeEdge1 });
            Ok(json!({ "pick": format!("{:?}", state.pick_mode) }))
        }
        C::CleanRemove => {
            if !state.selection_mode.is_edge_level() {
                return Err("Clean Remove (Ctrl+Remove) works at Edge level".into());
            }
            let edges = selected_edges(state);
            let mut removed = 0;
            edit_meshes(state, "Remove", edges, |mesh, _, ids| {
                removed += t::clean_remove_edges(mesh, &ids)?;
                Ok(())
            })?;
            state.sub_selection.clear();
            Ok(json!({ "vertices_removed": removed }))
        }
        C::SwiftLoopMode => toggle_pick(state, PickMode::SwiftLoop, &[PickMode::SwiftLoop]),
        C::SwiftLoop { node, edge, t } => {
            let id = NodeId(node);
            edit_meshes(state, "Swift Loop", vec![(id, vec![HalfEdgeId(edge)])], |mesh, _, ids| t::swift_loop(mesh, ids[0], t).map(|_| ()))?;
            state.sub_selection.clear();
            Ok(selection_json(state))
        }
        C::LoopMode => {
            state.loop_mode = !state.loop_mode;
            Ok(json!({ "loop_mode": state.loop_mode }))
        }
        C::SelectByHalf { axis, invert } => select_by_half(state, axis, invert),
        C::Extrude { height, width, kind } => {
            state.poly_settings.extrude_height = height;
            state.poly_settings.extrude_width = width.max(0.0);
            state.poly_settings.extrude_kind = kind;
            let mut new_sel: Vec<(NodeId, SubObjectId)> = Vec::new();
            match state.selection_mode {
                SelectionMode::Vertex => {
                    let verts = selected_vertices(state);
                    edit_meshes(state, "Extrude", verts, |mesh, _, ids| t::extrude_vertices(mesh, &ids, height, width).map(|_| ()))?;
                }
                SelectionMode::Edge | SelectionMode::Border => {
                    let edges = selected_edges(state);
                    edit_meshes(state, "Extrude", edges, |mesh, _, ids| t::extrude_edges(mesh, &ids, height, width).map(|_| ()))?;
                }
                SelectionMode::Face | SelectionMode::Element => {
                    let faces = selected_faces(state);
                    edit_meshes(state, "Extrude", faces, |mesh, node, ids| {
                        for cap in po::extrude_faces(mesh, &ids, height, mesh_kind(kind))? {
                            new_sel.push((node, SubObjectId::Face(cap)));
                        }
                        Ok(())
                    })?;
                }
                SelectionMode::Object => return Err("Extrude needs a sub-object selection".into()),
            }
            state.sub_selection.clear();
            for (n, id) in new_sel {
                state.sub_selection.insert(n, id);
            }
            Ok(selection_json(state))
        }
        C::ExtrudeMode => toggle_drag(state, PolyDrag::Extrude),
        C::ExtrudeSettings => open_settings(state, PolySettingsKind::Extrude),
        C::Turn => {
            let edges = selected_edges(state);
            edit_meshes(state, "Turn", edges, |mesh, _, ids| {
                for e in ids {
                    t::turn_edge(mesh, e)?;
                }
                Ok(())
            })?;
            Ok(selection_json(state))
        }
        C::EditTriangulationMode => toggle_pick(state, PickMode::EditTriangulationA, &[PickMode::EditTriangulationA, PickMode::EditTriangulationB]),
        C::Retriangulate => Ok(json!({})),
        C::Create => match state.selection_mode {
            SelectionMode::Vertex => toggle_pick(state, PickMode::CreateVertex, &[PickMode::CreateVertex]),
            SelectionMode::Face | SelectionMode::Element => {
                state.create_verts.clear();
                toggle_pick(state, PickMode::CreatePolygon, &[PickMode::CreatePolygon])
            }
            _ => Err("Create works at Vertex and Polygon level".into()),
        },
        C::Inset { amount, by_polygon } => {
            state.poly_settings.inset_amount = amount;
            state.poly_settings.inset_by_polygon = by_polygon;
            face_op(state, "Inset", |mesh, ids| po::inset(mesh, ids, amount, by_polygon))
        }
        C::Bevel { height, outline, kind } => {
            state.poly_settings.bevel_height = height;
            state.poly_settings.bevel_outline = outline;
            state.poly_settings.bevel_kind = kind;
            face_op(state, "Bevel", |mesh, ids| po::bevel(mesh, ids, height, outline, mesh_kind(kind)))
        }
        C::Outline { amount } => {
            state.poly_settings.outline_amount = amount;
            face_op(state, "Outline", |mesh, ids| po::outline(mesh, ids, amount).map(|_| ids.to_vec()))
        }
        C::HingeFromEdge { angle, segments, edge } => {
            state.poly_settings.hinge_angle = angle;
            state.poly_settings.hinge_segments = segments.max(1);
            let faces = selected_faces(state);
            let Some((node, _)) = faces.first() else { return Err("select polygons to hinge".into()) };
            let hinge = match edge {
                Some(e) => HalfEdgeId(e),
                None => match state.poly_settings.hinge_edge {
                    Some((n, e)) if n == *node => HalfEdgeId(e),
                    _ => return Err("pick a hinge edge first (Pick Hinge)".into()),
                },
            };
            face_op(state, "Hinge From Edge", |mesh, ids| po::hinge_from_edge(mesh, ids, hinge, angle, segments.max(1)))
        }
        C::PickHinge => {
            state.pick_mode = Some(PickMode::HingeEdge);
            Ok(json!({ "pick": "HingeEdge" }))
        }
        C::InsetMode => toggle_drag(state, PolyDrag::Inset),
        C::InsetSettings => open_settings(state, PolySettingsKind::Inset),
        C::BevelMode => toggle_drag(state, PolyDrag::Bevel),
        C::BevelSettings => open_settings(state, PolySettingsKind::Bevel),
        C::OutlineMode => toggle_drag(state, PolyDrag::Outline),
        C::OutlineSettings => open_settings(state, PolySettingsKind::Outline),
        C::HingeFromEdgeMode => toggle_drag(state, PolyDrag::Hinge),
        C::HingeFromEdgeSettings => open_settings(state, PolySettingsKind::Hinge),
        C::ExtrudeAlongSplineMode => toggle_pick(state, PickMode::ExtrudeAlongSpline, &[PickMode::ExtrudeAlongSpline]),
        C::ExtrudeAlongSplineSettings => open_settings(state, PolySettingsKind::ExtrudeAlongSpline),
        C::FlipNormals => {
            let faces = selected_faces(state);
            if faces.is_empty() {
                return Err("select polygons to flip".into());
            }
            edit_meshes(state, "Flip Normals", faces, |mesh, _, ids| po::flip_faces(mesh, &ids))?;
            Ok(selection_json(state))
        }
        C::MakePlanar => planar(state, None),
        C::MakePlanarAxis { axis } => planar(state, Some(match axis { crate::Axis::X => 0, crate::Axis::Y => 1, crate::Axis::Z => 2 })),
        C::ViewAlign => {
            let frame = crate::transform::frame_for_system(state, state.active_viewport, None, crate::CoordSystem::Screen);
            align(state, "View Align", frame.axes.z_axis)
        }
        C::AlignToGrid => align(state, "Grid Align", glam::Vec3::Z),
        C::Attach => toggle_pick(state, PickMode::AttachObject, &[PickMode::AttachObject]),
        C::AttachList => {
            state.dialog_purpose = crate::state::DialogPurpose::Attach;
            state.dialog = Some(Dialog::SelectFromScene);
            Ok(json!({ "open": true }))
        }
        C::AttachNode { id } => attach_node(state, NodeId(id)),
        C::Detach => {
            if selected_faces(state).is_empty() && selected_vertices(state).is_empty() {
                return Err("select sub-objects to detach".into());
            }
            state.dialog = Some(Dialog::Detach);
            Ok(json!({ "open": true }))
        }
        C::DetachSelection { name, to_element, as_clone } => detach_selection(state, name, to_element, as_clone),
        C::SlicePlaneMode => {
            state.slice_plane_mode = !state.slice_plane_mode;
            if state.slice_plane_mode {
                reset_slice_plane(state);
            }
            Ok(json!({ "slice_plane_mode": state.slice_plane_mode }))
        }
        C::ResetSlicePlane => {
            reset_slice_plane(state);
            Ok(json!({ "slice_plane": state.slice_plane.origin.to_array() }))
        }
        C::SetSlicePlane { position, rotation_deg, size } => {
            if let Some(p) = position {
                state.slice_plane.origin = glam::Vec3::from(p);
            }
            if let Some(r) = rotation_deg {
                state.slice_plane.rotation = glam::Quat::from_euler(glam::EulerRot::XYZ, r[0].to_radians(), r[1].to_radians(), r[2].to_radians());
            }
            if let Some(sz) = size {
                state.slice_plane.size = sz.max(0.01);
            }
            Ok(json!({ "slice_plane": state.slice_plane.origin.to_array() }))
        }
        C::SliceSplit { on } => {
            state.slice_split = on;
            Ok(json!({ "split": on }))
        }
        C::Slice => {
            let sp = state.slice_plane;
            slice_with(state, "Slice", sp.origin, sp.normal())
        }
        C::QuickSliceMode => toggle_pick(state, PickMode::QuickSliceA, &[PickMode::QuickSliceA, PickMode::QuickSliceB]),
        C::QuickSlice { a, b } => {
            let (a, b) = (glam::Vec3::from(a), glam::Vec3::from(b));
            let view = crate::transform::frame_for_system(state, state.active_viewport, None, crate::CoordSystem::Screen).axes.z_axis;
            let n = (b - a).cross(view);
            if n.length_squared() < 1e-10 {
                return Err("QuickSlice needs two different points".into());
            }
            slice_with(state, "QuickSlice", a, n.normalize())
        }
        C::Cut => toggle_pick(state, PickMode::CutA, &[PickMode::CutA, PickMode::CutB]),
        C::PickFacePoint { node, face, point } => pick_face_point(state, NodeId(node), face, glam::Vec3::from(point)),
        C::MeshSmooth => execute(state, C::MSmooth { smoothness: ps.msmooth_smoothness, separate_by_smoothing: ps.msmooth_separate_by_smoothing, separate_by_materials: ps.msmooth_separate_by_materials }),
        C::MeshSmoothSettings => open_settings(state, PolySettingsKind::MSmooth),
        C::MSmooth { smoothness, separate_by_smoothing, separate_by_materials } => {
            state.poly_settings.msmooth_smoothness = smoothness.clamp(0.0, 1.0);
            state.poly_settings.msmooth_separate_by_smoothing = separate_by_smoothing;
            state.poly_settings.msmooth_separate_by_materials = separate_by_materials;
            // At a sub-object level only the selected polygons (those of the selected
            // vertices / edges) subdivide; at the top level the whole object.
            let sub = state.selection_mode != SelectionMode::Object && !state.sub_selection.is_empty();
            let targets: Vec<(NodeId, Vec<FaceId>)> = if sub { faces_of_selection(state) } else { selected_polys(state).into_iter().map(|n| (n, Vec::new())).collect() };
            if targets.is_empty() {
                return Err("select an Editable Poly".into());
            }
            edit_meshes(state, "MeshSmooth", targets, |mesh, _, ids| {
                let subset = (!ids.is_empty()).then_some(ids.as_slice());
                *mesh = go::msmooth_faces(mesh, subset, smoothness, separate_by_smoothing, separate_by_materials)?;
                Ok(())
            })?;
            state.sub_selection.clear();
            Ok(selection_json(state))
        }
        C::Tessellate => execute(state, C::TessellateFaces { kind: ps.tess_kind, tension: ps.tess_tension }),
        C::TessellateSettings => open_settings(state, PolySettingsKind::Tessellate),
        C::TessellateFaces { kind, tension } => {
            state.poly_settings.tess_kind = kind;
            state.poly_settings.tess_tension = tension;
            let k = match kind { crate::state::TessKind::Edge => go::TessKind::Edge, crate::state::TessKind::Face => go::TessKind::Face };
            let faces = faces_of_selection(state);
            if faces.is_empty() {
                return Err("select polygons to tessellate".into());
            }
            edit_meshes(state, "Tessellate", faces, |mesh, _, ids| go::tessellate(mesh, &ids, k, tension))?;
            state.sub_selection.clear();
            Ok(selection_json(state))
        }
        C::Relax { amount, iterations, hold_boundary, hold_outer } => {
            state.poly_settings.relax_amount = amount.clamp(0.0, 1.0);
            state.poly_settings.relax_iterations = iterations.max(1);
            state.poly_settings.relax_hold_boundary = hold_boundary;
            state.poly_settings.relax_hold_outer = hold_outer;
            let verts = if state.selection_mode == SelectionMode::Object {
                selected_polys(state).into_iter().filter_map(|n| {
                    let m = state.scene.node(n)?.cage_mesh().into_owned();
                    Some((n, (0..m.vertex_count() as u32).map(VertexId).collect::<Vec<_>>()))
                }).collect()
            } else {
                selected_vertices(state)
            };
            if verts.is_empty() {
                return Err("select something to relax".into());
            }
            edit_meshes(state, "Relax", verts, |mesh, _, ids| go::relax(mesh, &ids, amount, iterations, hold_boundary, hold_outer).map(|_| ()))?;
            Ok(selection_json(state))
        }
        C::RelaxSettings => open_settings(state, PolySettingsKind::Relax),
        C::ConstrainToNone => set_constraint(state, crate::state::PolyConstraint::None),
        C::ConstrainToEdges => set_constraint(state, crate::state::PolyConstraint::Edges),
        C::ConstrainToFaces => set_constraint(state, crate::state::PolyConstraint::Faces),
        C::ConstrainToNormals => set_constraint(state, crate::state::PolyConstraint::Normals),
        C::FullInteractivity { on } => {
            state.full_interactivity = on;
            Ok(json!({ "full_interactivity": on }))
        }
        C::PreserveUVs { on } => {
            state.preserve_uvs = on;
            Ok(json!({ "preserve_uvs": on }))
        }
        C::DeleteIsolatedVertices => execute(state, C::RemoveIsolatedVertices),
        C::PickSub { node, id } => pick_sub(state, NodeId(node), id),
        C::RepeatLastOperation => {
            let Some(op) = state.last_poly_op.clone() else { return Err("nothing to repeat".into()) };
            crate::execute(state, op)
        }
        other => crate::poly_props::execute(state, other),
    }
}

fn selected_polys(state: &AppState) -> Vec<NodeId> {
    state.selection.iter().copied().filter(|i| state.scene.node(*i).is_some_and(|n| n.edit_poly().is_some())).collect()
}

fn open_settings(state: &mut AppState, kind: PolySettingsKind) -> Result<Value, String> {
    let d = Dialog::PolySettings(kind);
    state.dialog = if state.dialog == Some(d) { None } else { Some(d) };
    Ok(json!({ "open": state.dialog.is_some() }))
}

fn toggle_drag(state: &mut AppState, mode: PolyDrag) -> Result<Value, String> {
    state.poly_drag = if state.poly_drag == Some(mode) { None } else { Some(mode) };
    state.pick_mode = None;
    Ok(json!({ "drag": state.poly_drag.map(|m| format!("{m:?}")) }))
}

fn toggle_pick(state: &mut AppState, first: PickMode, family: &[PickMode]) -> Result<Value, String> {
    state.pick_source = None;
    state.pick_mode = if state.pick_mode.is_some_and(|m| family.contains(&m)) { None } else { Some(first) };
    state.poly_drag = None;
    Ok(json!({ "pick": state.pick_mode.map(|m| format!("{m:?}")) }))
}

fn pick_sub(state: &mut AppState, node: NodeId, id: u32) -> Result<Value, String> {
    state.scene.node(node).ok_or("missing node")?;
    match state.pick_mode {
        Some(PickMode::TargetWeldSource) => {
            state.pick_source = Some((node, id));
            state.pick_mode = Some(PickMode::TargetWeldTarget);
            Ok(json!({ "stage": "target" }))
        }
        Some(PickMode::TargetWeldTarget) => {
            let (sn, sv) = state.pick_source.take().ok_or("no source vertex")?;
            state.pick_mode = Some(PickMode::TargetWeldSource);
            if sn != node {
                return Err("Target Weld needs two vertices of the same object".into());
            }
            if sv == id {
                return Ok(json!({ "stage": "source" }));
            }
            edit_meshes(state, "Target Weld", vec![(node, vec![()])], |mesh, _, _| t::weld_pair(mesh, VertexId(sv), VertexId(id)))?;
            state.sub_selection.clear();
            Ok(json!({ "stage": "source" }))
        }
        Some(PickMode::EditTriangulationA) => {
            state.pick_source = Some((node, id));
            state.pick_mode = Some(PickMode::EditTriangulationB);
            Ok(json!({ "stage": "second" }))
        }
        Some(PickMode::EditTriangulationB) => {
            let (sn, sv) = state.pick_source.take().ok_or("no first vertex")?;
            state.pick_mode = Some(PickMode::EditTriangulationA);
            if sn != node || sv == id {
                return Err("Edit Triangulation needs two vertices of one polygon".into());
            }
            edit_meshes(state, "Edit Triangulation", vec![(node, vec![()])], |mesh, _, _| t::connect_vertices(mesh, &[VertexId(sv), VertexId(id)]).map(|_| ()))?;
            Ok(json!({ "stage": "first" }))
        }
        Some(PickMode::HingeEdge) => {
            state.poly_settings.hinge_edge = Some((node, id));
            state.pick_mode = None;
            Ok(json!({ "hinge": id }))
        }
        Some(m @ (PickMode::BridgeEdge1 | PickMode::BridgeEdge2)) => {
            let mesh = state.scene.node(node).ok_or("missing node")?.cage_mesh().into_owned();
            if (id as usize) >= mesh.half_edges().len() || !loom_mesh::select_ops::is_border_edge(&mesh, HalfEdgeId(id)) {
                return Err("Bridge connects border edges: pick an edge with a polygon on one side only".into());
            }
            state.poly_settings.bridge_edges[if m == PickMode::BridgeEdge1 { 0 } else { 1 }] = Some((node, id));
            state.pick_mode = None;
            Ok(json!({ "bridge_edges": state.poly_settings.bridge_edges.map(|e| e.map(|(_, e)| e)) }))
        }
        Some(PickMode::CreatePolygon) => {
            if state.create_verts.first() == Some(&(node, id)) && state.create_verts.len() >= 3 {
                return finish_create(state);
            }
            if state.create_verts.iter().any(|(n, _)| *n != node) {
                return Err("Create: all vertices must belong to one object".into());
            }
            if !state.create_verts.contains(&(node, id)) {
                state.create_verts.push((node, id));
            }
            Ok(json!({ "picked": state.create_verts.len() }))
        }
        _ => Err("no sub-object pick mode active".into()),
    }
}

/// Create (polygon level): build the polygon from the picked vertices. Also
/// what Exit Pick Mode does when three or more are picked.
pub fn finish_create(state: &mut AppState) -> Result<Value, String> {
    let picked = std::mem::take(&mut state.create_verts);
    if picked.len() < 3 {
        return Ok(json!({ "created": false }));
    }
    let node = picked[0].0;
    let verts: Vec<VertexId> = picked.iter().map(|(_, v)| VertexId(*v)).collect();
    let mut made = None;
    edit_meshes(state, "Create", vec![(node, vec![()])], |mesh, _, _| {
        made = Some(t::create_polygon(mesh, &verts)?);
        Ok(())
    })?;
    if let Some(f) = made {
        state.sub_selection.clear();
        state.sub_selection.insert(node, SubObjectId::Face(f));
    }
    Ok(json!({ "created": true }))
}

/// After a chamfer the strip's outer edges are selected (the new
/// edges at edge level; at vertex level the new polygons' edges).
fn select_strip_edges(state: &mut AppState, new_faces: Vec<(NodeId, Vec<FaceId>)>) {
    state.sub_selection.clear();
    for (n, fs) in new_faces {
        let Some(node) = state.scene.node(n) else { continue };
        let mesh = node.cage_mesh().into_owned();
        let set: BTreeSet<FaceId> = fs.iter().copied().collect();
        let hes = mesh.half_edges();
        for (i, he) in hes.iter().enumerate() {
            let twin = &hes[he.twin.0 as usize];
            let a = he.face.is_some_and(|f| set.contains(&f));
            let b = twin.face.is_some_and(|f| set.contains(&f));
            if a != b {
                if let Some(e) = mesh.canonical_edge_id(HalfEdgeId(i as u32)) {
                    state.sub_selection.insert(n, SubObjectId::Edge(e));
                }
            }
        }
    }
}

/// The canonical edge between the vertices at two positions.
fn edge_at(mesh: &Mesh, a: glam::Vec3, b: glam::Vec3) -> Option<HalfEdgeId> {
    let find = |p: glam::Vec3| (0..mesh.vertex_count() as u32).map(VertexId).find(|v| mesh.vertex_position(*v).is_some_and(|q| q.distance(p) < 1e-4));
    let (va, vb) = (find(a)?, find(b)?);
    let e = t::edge_between(mesh, va, vb).or_else(|| t::edge_between(mesh, vb, va))?;
    mesh.canonical_edge_id(e)
}

/// Groups selected border edges into their border loops.
fn border_loops(mesh: &Mesh, edges: &[HalfEdgeId]) -> Vec<Vec<HalfEdgeId>> {
    let mut loops: Vec<(BTreeSet<VertexId>, Vec<HalfEdgeId>)> = Vec::new();
    for e in edges {
        let verts: BTreeSet<VertexId> = t::border_loop(mesh, *e).into_iter().collect();
        if verts.is_empty() {
            continue;
        }
        if let Some(l) = loops.iter_mut().find(|(s, _)| *s == verts) {
            l.1.push(*e);
        } else {
            loops.push((verts, vec![*e]));
        }
    }
    loops.into_iter().map(|(_, es)| es).collect()
}

fn mesh_kind(kind: crate::state::ExtrudeKind) -> po::ExtrudeKind {
    match kind {
        crate::state::ExtrudeKind::Group => po::ExtrudeKind::Group,
        crate::state::ExtrudeKind::LocalNormal => po::ExtrudeKind::LocalNormal,
        crate::state::ExtrudeKind::ByPolygon => po::ExtrudeKind::ByPolygon,
    }
}

/// A polygon operation on the selected polygons; the returned polygons stay
/// selected (the caps stay selected).
fn face_op(state: &mut AppState, label: &str, mut op: impl FnMut(&mut Mesh, &[FaceId]) -> Result<Vec<FaceId>, String>) -> Result<Value, String> {
    let faces = selected_faces(state);
    if faces.is_empty() {
        return Err(format!("{label}: select polygons first"));
    }
    let mut keep: Vec<(NodeId, FaceId)> = Vec::new();
    edit_meshes(state, label, faces, |mesh, node, ids| {
        for f in op(mesh, &ids)? {
            keep.push((node, f));
        }
        Ok(())
    })?;
    state.sub_selection.clear();
    for (n, f) in keep {
        state.sub_selection.insert(n, SubObjectId::Face(f));
    }
    Ok(selection_json(state))
}

/// Make Planar at any level: the selected vertices (of the selected
/// sub-objects) onto their plane; selected polygons give the plane normal.
fn planar(state: &mut AppState, axis: Option<usize>) -> Result<Value, String> {
    let verts = selected_vertices(state);
    if verts.is_empty() {
        return Err("select sub-objects to make planar".into());
    }
    let faces: std::collections::BTreeMap<NodeId, Vec<FaceId>> = selected_faces(state).into_iter().collect();
    edit_meshes(state, "Make Planar", verts, |mesh, node, ids| {
        let fs = faces.get(&node).cloned().unwrap_or_default();
        po::make_planar(mesh, &ids, &fs, axis)
    })?;
    Ok(selection_json(state))
}

/// Flattens the selected vertices onto the plane with world `normal` through
/// their centre (View Align) or the world origin (Grid Align).
fn align(state: &mut AppState, label: &str, normal: glam::Vec3) -> Result<Value, String> {
    let verts = selected_vertices(state);
    if verts.is_empty() {
        return Err("select sub-objects to align".into());
    }
    let grid = label == "Grid Align";
    let mats: std::collections::BTreeMap<NodeId, glam::Mat4> = verts.iter().map(|(n, _)| (*n, state.scene.object_matrix(*n))).collect();
    edit_meshes(state, label, verts, |mesh, node, ids| {
        let m = mats[&node];
        let inv = m.inverse();
        // Normals go world → local with the transpose of the local → world matrix.
        let n_local = glam::Mat3::from_mat4(m).transpose() * normal;
        let origin_local = if grid {
            inv.transform_point3(glam::Vec3::ZERO)
        } else {
            let pts: Vec<glam::Vec3> = ids.iter().filter_map(|v| mesh.vertex_position(*v)).collect();
            pts.iter().sum::<glam::Vec3>() / pts.len().max(1) as f32
        };
        po::align_to_plane(mesh, &ids, origin_local, n_local)
    })?;
    Ok(selection_json(state))
}

/// Constrain to Edges toggles off when pressed again (Shift+X); the others select.
fn set_constraint(state: &mut AppState, c: crate::state::PolyConstraint) -> Result<Value, String> {
    state.poly_constraint = if c == crate::state::PolyConstraint::Edges && state.poly_constraint == c { crate::state::PolyConstraint::None } else { c };
    Ok(json!({ "constraint": format!("{:?}", state.poly_constraint) }))
}

/// Polygons the selection means for a polygon op at any level: the selected
/// polygons, or every polygon touching the selected vertices / edges.
fn faces_of_selection(state: &AppState) -> Vec<(NodeId, Vec<FaceId>)> {
    if state.selection_mode.is_face_level() {
        return selected_faces(state);
    }
    selected_vertices(state)
        .into_iter()
        .filter_map(|(n, vs)| {
            let mesh = state.scene.node(n)?.cage_mesh().into_owned();
            let mut fs: BTreeSet<FaceId> = BTreeSet::new();
            for v in vs {
                fs.extend(mesh.faces_around_vertex(v));
            }
            (!fs.is_empty()).then(|| (n, fs.into_iter().collect()))
        })
        .collect()
}

fn reset_slice_plane(state: &mut AppState) {
    let (lo, hi) = state.selection_bounds().unwrap_or((glam::Vec3::ZERO, glam::Vec3::ZERO));
    state.slice_plane.origin = (lo + hi) * 0.5;
    state.slice_plane.rotation = glam::Quat::IDENTITY;
    state.slice_plane.size = ((hi - lo).max_element() * 0.6).max(1.0);
}

/// Slices the selected polys with a world plane: the selected polygons at a
/// polygon level, else the whole mesh.
fn slice_with(state: &mut AppState, label: &str, origin: glam::Vec3, normal: glam::Vec3) -> Result<Value, String> {
    let split = state.slice_split;
    let targets: Vec<(NodeId, Vec<FaceId>)> = if state.selection_mode.is_face_level() && !state.sub_selection.is_empty() {
        selected_faces(state)
    } else {
        selected_polys(state).into_iter().map(|n| (n, Vec::new())).collect()
    };
    if targets.is_empty() {
        return Err("select an Editable Poly to slice".into());
    }
    let mats: std::collections::BTreeMap<NodeId, glam::Mat4> = targets.iter().map(|(n, _)| (*n, state.scene.object_matrix(*n))).collect();
    let mut new_edges: Vec<(NodeId, HalfEdgeId)> = Vec::new();
    edit_meshes(state, label, targets, |mesh, node, ids| {
        let inv = mats[&node].inverse();
        let o = inv.transform_point3(origin);
        let n = glam::Mat3::from_mat4(mats[&node]).transpose() * normal;
        let faces = (!ids.is_empty()).then_some(ids.as_slice());
        for e in go::slice(mesh, o, n, faces, split)? {
            new_edges.push((node, e));
        }
        Ok(())
    })?;
    state.sub_selection.clear();
    if state.selection_mode.is_edge_level() {
        for (n, e) in new_edges {
            state.sub_selection.insert(n, SubObjectId::Edge(e));
        }
    }
    Ok(selection_json(state))
}

fn pick_face_point(state: &mut AppState, node: NodeId, face: u32, point: glam::Vec3) -> Result<Value, String> {
    match state.pick_mode {
        Some(PickMode::QuickSliceA) => {
            state.pick_point_a = Some((node, face, point));
            state.pick_mode = Some(PickMode::QuickSliceB);
            Ok(json!({ "stage": "second" }))
        }
        Some(PickMode::QuickSliceB) => {
            let (_, _, a) = state.pick_point_a.take().ok_or("no first point")?;
            state.pick_mode = Some(PickMode::QuickSliceA);
            execute(state, C::QuickSlice { a: a.to_array(), b: point.to_array() })
        }
        Some(PickMode::CutA) => {
            state.pick_point_a = Some((node, face, point));
            state.pick_mode = Some(PickMode::CutB);
            Ok(json!({ "stage": "end" }))
        }
        Some(PickMode::CutB) => {
            let (n0, f0, p0) = state.pick_point_a.take().ok_or("no start point")?;
            state.pick_mode = Some(PickMode::CutA);
            if n0 != node {
                return Err("Cut: both points must be on the same object".into());
            }
            let inv = state.scene.object_matrix(node).inverse();
            let view = crate::transform::frame_for_system(state, state.active_viewport, None, crate::CoordSystem::Screen).axes.z_axis;
            let view_local = glam::Mat3::from_mat4(state.scene.object_matrix(node)).transpose() * view;
            let (a, b) = (inv.transform_point3(p0), inv.transform_point3(point));
            let mut new_edges = Vec::new();
            edit_meshes(state, "Cut", vec![(node, vec![()])], |mesh, _, _| {
                new_edges = go::cut(mesh, go::SurfacePoint { face: FaceId(f0), point: a }, go::SurfacePoint { face: FaceId(face), point: b }, view_local)?;
                Ok(())
            })?;
            // The cut goes on from where this one ended until a right-click.
            let local_end = b;
            if let Some(mesh) = state.scene.node(node).map(|n| n.cage_mesh().into_owned()) {
                let end = new_edges
                    .iter()
                    .filter_map(|e| mesh.edge_vertices(*e))
                    .flat_map(|(x, y)| [x, y])
                    .filter_map(|v| Some((v, mesh.vertex_position(v)?.distance(local_end))))
                    .min_by(|x, y| x.1.total_cmp(&y.1));
                if let Some((v, _)) = end {
                    if let (Some(f), Some(p)) = (mesh.faces_around_vertex(v).first().copied(), mesh.vertex_position(v)) {
                        state.pick_point_a = Some((node, f.0, state.scene.object_matrix(node).transform_point3(p)));
                        state.pick_mode = Some(PickMode::CutB);
                    }
                }
            }
            if state.selection_mode.is_edge_level() {
                state.sub_selection.clear();
                for e in new_edges {
                    state.sub_selection.insert(node, SubObjectId::Edge(e));
                }
            }
            Ok(selection_json(state))
        }
        _ => Err("no face-point pick mode active".into()),
    }
}

/// Selection > By Half: at the current level, the sub-objects whose centre
/// lies on the positive side of the pivot along `axis` (`invert`: the
/// negative side), in the object's pivot frame.
fn select_by_half(state: &mut AppState, axis: crate::Axis, invert: bool) -> Result<Value, String> {
    let mode = state.selection_mode;
    if mode == SelectionMode::Object {
        return Err("Select By Half works at a sub-object level".into());
    }
    let k = match axis {
        crate::Axis::X => 0,
        crate::Axis::Y => 1,
        crate::Axis::Z => 2,
    };
    let mut picked: Vec<(NodeId, SubObjectId)> = Vec::new();
    for n in selected_polys(state) {
        let Some(node) = state.scene.node(n) else { continue };
        let offset = node.object_offset.matrix();
        let mesh = node.cage_mesh().into_owned();
        let side = |pts: &[glam::Vec3]| -> bool {
            let c = pts.iter().map(|p| offset.transform_point3(*p)).sum::<glam::Vec3>() / pts.len().max(1) as f32;
            if invert { c[k] < -1e-5 } else { c[k] > 1e-5 }
        };
        let pos = |vs: Vec<VertexId>| -> Vec<glam::Vec3> { vs.into_iter().filter_map(|v| mesh.vertex_position(v)).collect() };
        match mode {
            SelectionMode::Vertex => {
                for v in (0..mesh.vertex_count() as u32).map(VertexId) {
                    if side(&pos(vec![v])) {
                        picked.push((n, SubObjectId::Vertex(v)));
                    }
                }
            }
            SelectionMode::Edge | SelectionMode::Border => {
                for e in (0..mesh.half_edges().len() as u32).map(HalfEdgeId) {
                    if mesh.canonical_edge_id(e) != Some(e) || (mode == SelectionMode::Border && !loom_mesh::select_ops::is_border_edge(&mesh, e)) {
                        continue;
                    }
                    let Some((a, b)) = mesh.edge_vertices(e) else { continue };
                    if side(&pos(vec![a, b])) {
                        picked.push((n, SubObjectId::Edge(e)));
                    }
                }
            }
            _ => {
                for f in (0..mesh.face_count() as u32).map(FaceId) {
                    if side(&pos(mesh.face_vertex_ids(f).unwrap_or_default())) {
                        picked.push((n, SubObjectId::Face(f)));
                    }
                }
            }
        }
    }
    state.sub_selection.clear();
    for (n, id) in picked {
        state.sub_selection.insert(n, id);
    }
    Ok(selection_json(state))
}

fn attach_node(state: &mut AppState, other: NodeId) -> Result<Value, String> {
    let Some(target) = selected_polys(state).first().copied() else { return Err("select an Editable Poly to attach to".into()) };
    if other == target {
        return Err("cannot attach an object to itself".into());
    }
    let onode = state.scene.node(other).ok_or("missing node")?;
    if matches!(onode.object, ObjectKind::Group { .. }) {
        return Err("open the group and attach its members, not the group".into());
    }
    let omesh = onode.cage_mesh().into_owned();
    let other_to_this = state.scene.object_matrix(target).inverse() * state.scene.object_matrix(other);
    let tnode = state.scene.node(target).ok_or("missing node")?;
    let mut mesh = tnode.cage_mesh().into_owned();
    if tnode.edits_tri_mesh() {
        // An Editable Mesh takes the other object as triangles, its edge flags kept.
        let omesh = state.scene.node(other).map(|n| n.object.display_output()).unwrap_or(omesh);
        mesh = loom_mesh::tri_mesh::attach(&mesh, &omesh, other_to_this)?;
    } else {
        go::attach(&mut mesh, &omesh, other_to_this)?;
    }
    let changes = vec![
        crate::execute::cage_change(tnode, mesh),
        loom_scene::Change::RemoveNode(other),
    ];
    crate::execute::commit_change(state, "Attach", loom_scene::Change::Group(changes))?;
    state.selection.remove(&other);
    Ok(json!({ "attached": other.0 }))
}

fn detach_selection(state: &mut AppState, name: Option<String>, to_element: bool, as_clone: bool) -> Result<Value, String> {
    let faces = faces_of_selection(state);
    let Some((node, ids)) = faces.into_iter().next() else { return Err("select sub-objects to detach".into()) };
    let src = state.scene.node(node).ok_or("missing node")?;
    let before = src.object.clone();
    let mut mesh = src.cage_mesh().into_owned();
    let mut changes = Vec::new();
    if src.edits_tri_mesh() {
        // Editable Mesh: the faces keep their edge flags and leave as an Editable Mesh.
        let (rest, part) = loom_mesh::tri_mesh::split(&mesh, &ids)?;
        if to_element {
            let m = if as_clone { loom_mesh::tri_mesh::attach(&mesh, &part, glam::Mat4::IDENTITY)? } else { loom_mesh::tri_mesh::explode_to_elements(&mesh, &ids, 360.0)? };
            changes.push(crate::execute::cage_change(src, m));
        } else {
            let name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| state.scene.next_name("Object"));
            let mut new_node = loom_scene::Node::new(&name, loom_scene::ObjectKind::EditableMesh(loom_scene::PolyObject::from_mesh(part)));
            new_node.transform = src.transform.clone();
            new_node.object_offset = src.object_offset.clone();
            new_node.inherit = src.inherit;
            new_node.layer = src.layer;
            new_node.parent = src.parent;
            new_node.wire_color = src.wire_color;
            if !as_clone {
                changes.push(crate::execute::cage_change(src, rest));
            }
            changes.push(loom_scene::Change::AddNode(new_node));
        }
        crate::execute::commit_change(state, "Detach", loom_scene::Change::Group(changes))?;
        state.sub_selection.clear();
        state.dialog = None;
        return Ok(selection_json(state));
    }
    if to_element {
        if as_clone {
            // Clone to element: duplicate the polygons in place.
            let part = go::detach(&mut mesh, &ids, true)?;
            go::attach(&mut mesh, &part, glam::Mat4::IDENTITY)?;
        } else {
            go::detach_to_element(&mut mesh, &ids)?;
        }
        changes.push(loom_scene::Change::ReplaceObject { id: node, before: Box::new(before.clone()), after: Box::new(before.with_mesh(mesh)) });
    } else {
        let part = go::detach(&mut mesh, &ids, as_clone)?;
        let name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| state.scene.next_name("Object"));
        let mut new_node = loom_scene::Node::new(&name, loom_scene::ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(part)));
        new_node.transform = src.transform.clone();
        new_node.object_offset = src.object_offset.clone();
        new_node.inherit = src.inherit;
        new_node.layer = src.layer;
        new_node.parent = src.parent;
        new_node.wire_color = src.wire_color;
        changes.push(loom_scene::Change::ReplaceObject { id: node, before: Box::new(before.clone()), after: Box::new(before.with_mesh(mesh)) });
        changes.push(loom_scene::Change::AddNode(new_node));
    }
    crate::execute::commit_change(state, "Detach", loom_scene::Change::Group(changes))?;
    state.sub_selection.clear();
    state.dialog = None;
    Ok(selection_json(state))
}
