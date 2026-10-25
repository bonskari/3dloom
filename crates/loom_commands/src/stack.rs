//! Modifier stack commands (Modify panel: Modifier List, the stack list and
//! its buttons) and the selection flow into the stack.

use crate::execute::{commit_change, selection_json};
use crate::state::{AppState, PickMode};
use crate::{Command as C, SelectionMode};
use loom_mesh::select_ops::SubLevel;
use loom_scene::{Change, Modifier, ModifierSlot, NodeId, ObjectKind, PolyObject, StoredSelection};
use serde_json::{json, Value};

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::EditModifierStack => {
            state.focus_modify = true;
            Ok(json!({}))
        }
        C::AddModifier { name } => {
            let r = add_modifier(state, &name);
            if r.is_ok() {
                *state.modifier_usage.entry(name).or_insert(0) += 1;
            }
            r
        }
        C::DeleteModifier { index } => {
            let idx = index as usize;
            edit_stack_repointing(state, "Delete Modifier", |stack| {
                if idx < stack.len() {
                    stack.remove(idx);
                    Ok(())
                } else {
                    Err(format!("no stack entry {index}"))
                }
            }, &|i| if i == idx { None } else { Some(if i > idx { i - 1 } else { i }) })
        }
        C::MoveModifier { from, to } => {
            let (f, t) = (from as usize, to as usize);
            edit_stack_repointing(state, "Move Modifier", |stack| {
                if f >= stack.len() || t >= stack.len() {
                    return Err("stack index out of range".into());
                }
                let slot = stack.remove(f);
                stack.insert(t, slot);
                // World-space modifiers stay above the object-space ones.
                if stack.windows(2).any(|w| w[0].modifier.is_wsm() && !w[1].modifier.is_wsm()) {
                    return Err("world-space modifiers stay above the object-space modifiers".into());
                }
                Ok(())
            }, &|i| {
                // The same remove / insert on indices.
                if i == f {
                    Some(t)
                } else if f < t && i > f && i <= t {
                    Some(i - 1)
                } else if t < f && i >= t && i < f {
                    Some(i + 1)
                } else {
                    Some(i)
                }
            })
        }
        C::CopyModifier { from, to, instance } => copy_modifier(state, from as usize, to as usize, instance),
        C::ToggleModifier { index, on } => edit_stack(state, "Modifier On/Off", |stack| {
            stack.get_mut(index as usize).ok_or_else(|| format!("no stack entry {index}"))?.enabled = on;
            Ok(())
        }),
        C::ToggleModifierInViewport { index, on } => edit_stack(state, "Modifier On in Viewport", |stack| {
            stack.get_mut(index as usize).ok_or_else(|| format!("no stack entry {index}"))?.enabled_in_viewport = on;
            Ok(())
        }),
        C::RenameModifier { index, name } => edit_stack(state, "Rename Modifier", |stack| {
            stack.get_mut(index as usize).ok_or_else(|| format!("no stack entry {index}"))?.name = name.clone();
            Ok(())
        }),
        C::SelectStackEntry { index } => {
            state.ffd_selection.clear();
            let ids = selected_ids(state)?;
            for id in ids {
                let len = state.scene.node(id).map(|n| n.modifiers.len()).unwrap_or(0);
                let entry = index.map(|i| i as usize).filter(|i| *i < len);
                if let (Some(wanted), None) = (index, entry) {
                    return Err(format!("no stack entry {wanted}"));
                }
                set_active(state, id, entry);
            }
            state.sub_selection.clear();
            if state.selection_mode != SelectionMode::Object {
                state.selection_mode = SelectionMode::Object;
            }
            Ok(stack_json(state))
        }
        C::SetModifierParameter { index, name, value } => {
            let ids = selected_ids(state)?;
            let mut changes = Vec::new();
            for id in ids {
                let node = state.scene.node(id).ok_or("missing node")?;
                let slot = node.modifiers.get(index as usize).ok_or_else(|| format!("no stack entry {index}"))?;
                // Instanced entries edit their source.
                let (target, ti) = slot.instance_of.unwrap_or((id, index as usize));
                let tnode = state.scene.node(target).ok_or("missing instance source")?;
                let mut stack = tnode.modifiers.clone();
                let entry = stack.get_mut(ti).ok_or("missing instance source entry")?;
                entry.modifier.set_parameter(&name, value)?;
                if name == "Alignment" && matches!(entry.modifier, Modifier::UVWMap { .. }) {
                    // A new alignment refits the gizmo.
                    let input = match ti.checked_sub(1) {
                        Some(prev) => state.scene.evaluated(target, Some(prev)).value.mesh.clone(),
                        None => tnode.object.display_output(),
                    };
                    entry.modifier.fit_uvw_map(&input);
                }
                changes.push(Change::SetStack { id: target, before: tnode.modifiers.clone(), after: stack });
            }
            commit_change(state, &name, Change::Group(changes))?;
            Ok(json!({}))
        }
        C::SwitchToControlPointLevel | C::SwitchToLatticeLevel | C::SwitchToSetVolumeLevel | C::SwitchToTopLevel => {
            let (_, index) = state.ffd_active().ok_or("no FFD entry is active")?;
            let level = match cmd {
                C::SwitchToControlPointLevel => 1,
                C::SwitchToLatticeLevel => 2,
                C::SwitchToSetVolumeLevel => 3,
                _ => 0,
            };
            execute(state, C::ModifierSubobjectLevel { index: index as u32, level })
        }
        C::SelectControlPoints { op, indices } => {
            let (id, index) = state.ffd_active().ok_or("no FFD entry is active")?;
            let Modifier::FFD { points, .. } = &state.scene.node(id).ok_or("missing node")?.modifiers[index].modifier else { unreachable!() };
            let n = points.len() as u32;
            if let Some(bad) = indices.iter().find(|i| **i >= n) {
                return Err(format!("no control point {bad} (lattice has {n})"));
            }
            match op {
                crate::SelectOp::Replace => state.ffd_selection = indices.iter().copied().collect(),
                crate::SelectOp::Add => state.ffd_selection.extend(indices.iter().copied()),
                crate::SelectOp::Subtract => state.ffd_selection.retain(|i| !indices.contains(i)),
            }
            Ok(json!({ "ffd_selection": state.ffd_selection.iter().collect::<Vec<_>>() }))
        }
        C::UvwMapNormalAlign => {
            state.uvw_map_entry().ok_or("no UVW Map on the selection")?;
            state.pick_mode = Some(PickMode::UvwMapNormal);
            Ok(json!({}))
        }
        C::UvwMapFit | C::UvwMapCenter | C::UvwMapBitmapFit { .. } | C::UvwMapViewAlign | C::UvwMapRegionFit { .. } | C::UvwMapReset | C::UvwMapAcquire { .. } | C::UvwMapAlignTo { .. } => {
            let (id, index) = state.uvw_map_entry().ok_or("no UVW Map on the selection")?;
            let node = state.scene.node(id).ok_or("missing node")?;
            let mut stack = node.modifiers.clone();
            let label = cmd.name();
            let input = match index.checked_sub(1) {
                Some(prev) => state.scene.evaluated(id, Some(prev)).value.mesh.clone(),
                None => node.object.display_output(),
            };
            let object = state.scene.object_matrix(id);
            let m = &mut stack[index].modifier;
            match cmd {
                C::UvwMapFit => m.fit_uvw_map(&input),
                C::UvwMapCenter => m.center_uvw_map(&input),
                C::UvwMapReset => {
                    if let Modifier::UVWMap { alignment, .. } = m {
                        alignment.set(2);
                    }
                    m.fit_uvw_map(&input);
                }
                C::UvwMapBitmapFit { path } => {
                    let (w, h) = image::image_dimensions(&path).map_err(|e| format!("{path}: {e}"))?;
                    if let Modifier::UVWMap { length, width, .. } = m {
                        length.set(width.get() * h as f32 / w.max(1) as f32);
                    }
                }
                C::UvwMapViewAlign => {
                    // The gizmo's Z toward the viewer, in object space; then fit along it.
                    let vp = state.active();
                    let forward = (vp.camera.eye_position() - vp.camera.target).normalize_or_zero();
                    let local = object.inverse().transform_vector3(forward).normalize_or_zero();
                    align_uvw_gizmo(m, &input, local, None);
                }
                C::UvwMapAlignTo { point, normal } => {
                    let n = object.inverse().transform_vector3(glam::Vec3::from(normal)).normalize_or_zero();
                    let p = object.inverse().transform_point3(glam::Vec3::from(point));
                    align_uvw_gizmo(m, &input, n, Some(p));
                }
                C::UvwMapRegionFit { a, b } => {
                    if let Modifier::UVWMap { gizmo, length, width, .. } = m {
                        let inv = (object * gizmo.matrix()).inverse();
                        let (pa, pb) = (inv.transform_point3(glam::Vec3::from(a)), inv.transform_point3(glam::Vec3::from(b)));
                        let centre_local = (pa + pb) * 0.5;
                        let centre = gizmo.matrix().transform_point3(centre_local);
                        width.set((pb.x - pa.x).abs().max(1e-4));
                        length.set((pb.y - pa.y).abs().max(1e-4));
                        gizmo.position.set(centre);
                    }
                }
                C::UvwMapAcquire { node: other } => {
                    let other_id = NodeId(other);
                    let on = state.scene.node(other_id).ok_or("missing node")?;
                    let src = on.modifiers.iter().find_map(|s| matches!(s.modifier, Modifier::UVWMap { .. }).then(|| s.modifier.clone())).ok_or("that object has no UVW Map")?;
                    let world_gizmo = state.scene.object_matrix(other_id) * src.gizmo().expect("UVW Map has a gizmo").matrix();
                    *m = src;
                    if let Some(g) = m.gizmo_mut() {
                        *g = loom_scene::Transform::from_matrix(object.inverse() * world_gizmo);
                    }
                }
                _ => {}
            }
            commit_change(state, label, Change::SetStack { id, before: node.modifiers.clone(), after: stack })?;
            Ok(stack_json(state))
        }
        C::ResetLattice | C::ConformToShape | C::SetNumberOfPoints { .. } => {
            let (id, index) = state.ffd_active().ok_or("no FFD entry is active")?;
            let node = state.scene.node(id).ok_or("missing node")?;
            let mut stack = node.modifiers.clone();
            let label = cmd.name();
            match cmd {
                C::ResetLattice => {
                    if let Modifier::FFD { points, rest, .. } = &mut stack[index].modifier {
                        *points = rest.clone();
                    }
                }
                C::SetNumberOfPoints { length, width, height } => stack[index].modifier.set_ffd_dims([length, width, height])?,
                _ => {
                    // Conform To Shape: the input mesh in lattice unit space.
                    let input = match index.checked_sub(1) {
                        Some(prev) => state.scene.evaluated(id, Some(prev)).value.mesh.clone(),
                        None => node.object.display_output(),
                    };
                    if let Modifier::FFD { points, lo, hi, gizmo, .. } = &mut stack[index].modifier {
                        let size = (*hi - *lo).max(glam::Vec3::splat(1e-6));
                        let g = gizmo.matrix();
                        let inv = g.inverse();
                        let verts: Vec<glam::Vec3> = input.vertices().iter().map(|v| (v.position - *lo) / size).collect();
                        let mut unit: Vec<glam::Vec3> = points.iter().map(|p| (g.transform_point3(*p) - *lo) / size).collect();
                        loom_mesh::ffd::conform_to_shape(&mut unit, &verts, 0.0, 0.15);
                        *points = unit.into_iter().map(|u| inv.transform_point3(*lo + u * size)).collect();
                    }
                }
            }
            commit_change(state, label, Change::SetStack { id, before: node.modifiers.clone(), after: stack })?;
            Ok(stack_json(state))
        }
        C::VertexPaintBrushToggle => {
            state.vertex_paint_brush = !state.vertex_paint_brush;
            if state.vertex_paint_brush {
                // One viewport mode at a time: Paint, Pick Color or Face Selection.
                state.vertex_paint_pick = false;
                state.vertex_paint_faces = false;
            }
            Ok(json!({ "vertex_paint_brush": state.vertex_paint_brush }))
        }
        C::VertexPaintStroke { node, points, rgb, size, opacity } => {
            let id = NodeId(node);
            let n = state.scene.node(id).ok_or("missing node")?;
            let a = n.active_entry.filter(|a| matches!(n.modifiers.get(*a).map(|s| &s.modifier), Some(Modifier::VertexPaint { .. }))).ok_or("no VertexPaint entry is active")?;
            let input = match a.checked_sub(1) {
                Some(prev) => state.scene.evaluated(id, Some(prev)).value.mesh.clone(),
                None => n.object.display_output(),
            };
            let mut stack = n.modifiers.clone();
            let Modifier::VertexPaint { paint, coverage, faces, .. } = &mut stack[a].modifier else { unreachable!() };
            let target = [rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0];
            let radius = size.max(1e-3);
            // A face selection restricts the brush to its faces' vertices.
            let only = crate::vertex_paint::face_vertices(&input, faces);
            let mut touched = 0;
            for pt in &points {
                let c = glam::Vec3::from(*pt);
                for (i, v) in input.vertices().iter().enumerate() {
                    let d = v.position.distance(c);
                    if d > radius || only.as_ref().is_some_and(|o| !o.contains(&(i as u32))) {
                        continue;
                    }
                    let k = (opacity.clamp(0.0, 1.0) * (1.0 - d / radius)).clamp(0.0, 1.0);
                    crate::vertex_paint::composite(paint, coverage, i as u32, target, k);
                    touched += 1;
                }
            }
            if touched == 0 {
                return Ok(json!({ "painted": 0 }));
            }
            commit_change(state, "Vertex Paint", Change::SetStack { id, before: n.modifiers.clone(), after: stack })?;
            Ok(json!({ "painted": touched }))
        }
        C::ModifierSubobjectLevel { index, level } => {
            state.ffd_selection.clear();
            let ids = selected_ids(state)?;
            for id in ids {
                let node = state.scene.node(id).ok_or("missing node")?;
                let slot = node.modifiers.get(index as usize).ok_or_else(|| format!("no stack entry {index}"))?;
                let levels = slot.modifier.sub_levels();
                if level as usize > levels.len() {
                    return Err(format!("{} has {} sub-object levels", slot.name, levels.len()));
                }
                let n = state.scene.node_mut(id).unwrap();
                n.active_entry = Some(index as usize);
                n.modifiers[index as usize].sub_level = level;
                // Edit Spline levels: Vertex / Segment / Spline; Unwrap UVW: Vertex / Edge / Face.
                if let Modifier::SplineSelect { level: l, .. } = &mut n.modifiers[index as usize].modifier {
                    *l = level;
                }
                if let Some((mode, polygon, handle)) = crate::mesh_edit::modifier_level(&n.modifiers[index as usize].modifier, level) {
                    // Edit Mesh: Face and Polygon both select faces; Edit Patch: Vertex and Handle both pick lattice points.
                    state.selection_mode = mode;
                    state.mesh_patch.polygon_level = polygon;
                    state.mesh_patch.handle_level = handle;
                    state.sub_selection.clear();
                    state.mesh_dirty = true;
                    continue;
                }
                if matches!(n.modifiers[index as usize].modifier, Modifier::Hsds(_)) {
                    // HSDS: the finest level is the cage the sub-object levels pick on.
                    refresh_hsds_cage(state, id, index as usize);
                    state.selection_mode = match level {
                        1 => SelectionMode::Vertex,
                        2 => SelectionMode::Edge,
                        3 => SelectionMode::Face,
                        4 => SelectionMode::Element,
                        _ => SelectionMode::Object,
                    };
                    state.sub_selection.clear();
                    state.mesh_dirty = true;
                    return Ok(json!({ "level": level }));
                }
                if matches!(n.modifiers[index as usize].modifier, Modifier::EditSpline { .. } | Modifier::UnwrapUVW(_) | Modifier::SplineSelect { .. } | Modifier::FilletChamfer(_)) {
                    state.selection_mode = match level {
                        1 => SelectionMode::Vertex,
                        2 => SelectionMode::Edge,
                        3 => SelectionMode::Face,
                        _ => SelectionMode::Object,
                    };
                    state.sub_selection.clear();
                    state.mesh_dirty = true;
                    return Ok(json!({ "level": level }));
                }
                // Poly Select / Edit Poly levels are the usual sub-object modes; Mesh Select's Face and Polygon both pick polygons.
                let mesh_select = matches!(n.modifiers[index as usize].modifier, Modifier::MeshSelect { .. });
                if matches!(n.modifiers[index as usize].modifier, Modifier::PolySelect { .. } | Modifier::EditPoly { .. } | Modifier::MeshSelect { .. }) {
                    state.selection_mode = match level {
                        3 | 4 if mesh_select => SelectionMode::Face,
                        1 => SelectionMode::Vertex,
                        2 => SelectionMode::Edge,
                        3 => SelectionMode::Border,
                        4 => SelectionMode::Face,
                        5 => SelectionMode::Element,
                        _ => SelectionMode::Object,
                    };
                    state.sub_selection.clear();
                    if let Some(sel) = n.modifiers[index as usize].modifier.stored_selection().cloned() {
                        if sel.level == level {
                            for raw in sel.ids {
                                state.sub_selection.insert(id, crate::poly::sub(state.selection_mode, raw));
                            }
                        }
                    }
                }
            }
            Ok(stack_json(state))
        }
        C::ShowEndResultToggle => {
            state.show_end_result = !state.show_end_result;
            state.mesh_dirty = true;
            Ok(json!({ "show_end_result": state.show_end_result }))
        }
        C::PinStackToggle => {
            state.pin_stack = !state.pin_stack;
            Ok(json!({ "pin_stack": state.pin_stack }))
        }
        C::MakeUnique => {
            let ids = selected_ids(state)?;
            let mut changes = Vec::new();
            for id in &ids {
                let node = state.scene.node(*id).ok_or("missing node")?;
                let mut stack = node.modifiers.clone();
                let mut any = false;
                let targets: Vec<usize> = match node.active_entry {
                    Some(a) => vec![a],
                    None => (0..stack.len()).collect(),
                };
                for i in targets {
                    if let Some((src, si)) = stack[i].instance_of {
                        if let Some(m) = state.scene.node(src).and_then(|n| n.modifiers.get(si)).map(|s| s.modifier.clone()) {
                            stack[i].modifier = m;
                        }
                        stack[i].instance_of = None;
                        any = true;
                    }
                }
                if any {
                    changes.push(Change::SetStack { id: *id, before: node.modifiers.clone(), after: stack });
                }
            }
            if changes.is_empty() {
                // Node-level instances (Clone > Instance) become unique too.
                for id in ids {
                    crate::execute::detach_instance(state, id)?;
                }
                return Ok(json!({}));
            }
            commit_change(state, "Make Unique", Change::Group(changes))
                .map(|_| json!({}))
        }
        C::InstanceModifier { source, index } => {
            let src = NodeId(source);
            let sslot = state.scene.node(src).and_then(|n| n.modifiers.get(index as usize)).ok_or("no such source entry")?.clone();
            let (root, ri) = sslot.instance_of.unwrap_or((src, index as usize));
            let ids = selected_ids(state)?;
            let mut changes = Vec::new();
            for id in ids {
                if id == root {
                    continue;
                }
                let node = state.scene.node(id).ok_or("missing node")?;
                let mut stack = node.modifiers.clone();
                let mut slot = ModifierSlot::new(sslot.modifier.clone());
                slot.name = sslot.name.clone();
                slot.instance_of = Some((root, ri));
                // Above the active entry, as Add Modifier; world-space modifiers stay on top.
                let at = node.active_entry.map(|a| a + 1).unwrap_or(0).min(stack.len());
                let first_wsm = stack.iter().position(|m| m.modifier.is_wsm()).unwrap_or(stack.len());
                let at = if slot.modifier.is_wsm() { stack.len() } else { at.min(first_wsm) };
                stack.insert(at, slot);
                changes.push(Change::SetStack { id, before: node.modifiers.clone(), after: stack });
            }
            commit_change(state, "Paste Instanced", Change::Group(changes))?;
            Ok(stack_json(state))
        }
        C::CollapseTo { index } => collapse(state, Some(index as usize)),
        C::CollapseAll => collapse(state, None),
        C::PickModifierNode { index, node } => {
            let picked = NodeId(node);
            state.scene.node(picked).ok_or("missing node")?;
            let world = state.scene.world_matrix(picked);
            let ids = selected_ids(state)?;
            let mut changes = Vec::new();
            for id in ids {
                if id == picked {
                    return Err("pick another object".into());
                }
                let n = state.scene.node(id).ok_or("missing node")?;
                let mut stack = n.modifiers.clone();
                let slot = stack.get_mut(index as usize).ok_or_else(|| format!("no stack entry {index}"))?;
                match &mut slot.modifier {
                    Modifier::Sweep { custom, section, .. } => {
                        if state.scene.node(picked).and_then(|n| n.object.shape()).is_none() {
                            return Err("Sweep > Custom Section: pick a shape".into());
                        }
                        *custom = Some(picked);
                        section.set(10);
                    }
                    Modifier::LinkedXForm { control, control_inverse_at_pick } => {
                        *control = Some(picked);
                        *control_inverse_at_pick = Some(world.inverse().to_cols_array_2d());
                    }
                    Modifier::Preserve { original, .. } => *original = Some(picked),
                    Modifier::SkinWrapPatch(d) => {
                        crate::morph::check_patch(state, picked)?;
                        d.patch = Some(picked);
                        d.resample(&state.scene, id, index as usize);
                    }
                    // Vol. Select > Mesh Object: the picked mesh is the volume.
                    Modifier::VolSelect(d) => {
                        d.node = Some(picked);
                        d.volume = 3;
                    }
                    Modifier::CameraMap(d) => {
                        let Some(loom_scene::ObjectKind::Camera { fov, .. }) = state.scene.node(picked).map(|n| &n.object) else {
                            return Err("Camera Map: pick a camera".into());
                        };
                        d.camera = Some(picked);
                        d.fov = fov.get();
                        // Object space: the camera as it stands relative to the object now.
                        let rel = state.scene.object_matrix(id).inverse() * state.scene.world_matrix(picked);
                        d.camera_in_object = Some(rel.to_cols_array_2d());
                    }
                    Modifier::PathDeform(d) => {
                        if state.scene.node(picked).and_then(|n| n.object.shape()).is_none() {
                            return Err("PathDeform: pick a shape as the path".into());
                        }
                        d.path = Some(picked);
                    }
                    Modifier::Substitute { node } => {
                        if picked == id {
                            return Err("Substitute: pick another object".into());
                        }
                        *node = Some(picked);
                    }
                    Modifier::BevelProfile { profile, .. } => {
                        if state.scene.node(picked).and_then(|n| n.object.shape()).is_none() {
                            return Err("Bevel Profile: pick a shape as the profile".into());
                        }
                        *profile = Some(picked);
                    }
                    other => return Err(format!("{} does not pick an object", other.name())),
                }
                changes.push(Change::SetStack { id, before: n.modifiers.clone(), after: stack });
            }
            commit_change(state, "Pick Object", Change::Group(changes))?;
            Ok(json!({ "picked": node }))
        }
        C::ConfigureModifierSets { names } => {
            state.modifier_set = names.into_iter().filter(|n| Modifier::by_name(n).is_some()).collect();
            Ok(json!({ "set": state.modifier_set }))
        }
        C::ModifierSetButtonsToggle => {
            state.show_modifier_set = !state.show_modifier_set;
            Ok(json!({ "buttons": state.show_modifier_set }))
        }
        other => Err(format!("not a stack command: {other:?}")),
    }
}

/// Points the UVW Map gizmo's Z along `normal` (object space), optionally
/// at `point`, then sizes it to the input bounds in that frame.
fn align_uvw_gizmo(m: &mut Modifier, input: &loom_mesh::Mesh, normal: glam::Vec3, point: Option<glam::Vec3>) {
    if let Modifier::UVWMap { gizmo, length, width, height, .. } = m {
        let rot = glam::Quat::from_rotation_arc(glam::Vec3::Z, normal);
        gizmo.set_rotation_quat(rot);
        let inv = glam::Mat4::from_quat(rot).inverse();
        let mut lo = glam::Vec3::splat(f32::INFINITY);
        let mut hi = glam::Vec3::splat(f32::NEG_INFINITY);
        for v in input.vertices() {
            let p = inv.transform_point3(v.position);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        if input.vertex_count() > 0 {
            let size = (hi - lo).max(glam::Vec3::splat(1e-4));
            width.set(size.x);
            length.set(size.y);
            height.set(size.z);
            gizmo.position.set(point.unwrap_or(rot * ((lo + hi) * 0.5)));
        }
    }
}

fn selected_ids(state: &AppState) -> Result<Vec<NodeId>, String> {
    let ids: Vec<NodeId> = state.selection.iter().copied().filter(|i| state.scene.node(*i).is_some_and(|n| !matches!(n.object, ObjectKind::Group { .. }))).collect();
    if ids.is_empty() {
        return Err("select an object first".into());
    }
    Ok(ids)
}

/// Stores the current sub-object selection as what each node's edited poly
/// (base Editable Poly or active Edit Poly) passes up the stack.
fn freeze_passes_up(state: &mut AppState, ids: &[NodeId]) {
    let level = sub_level_of(state.selection_mode);
    for id in ids {
        let raw: Vec<u32> = crate::poly::raw_ids(state.selection_mode, &state.sub_selection.by_node().find(|(n, _)| n == id).map(|(_, s)| s.iter().copied().collect::<Vec<_>>()).unwrap_or_default());
        let wanted = level.filter(|_| !raw.is_empty()).map(|l| (l, raw));
        if let Some(p) = state.scene.node_mut(*id).and_then(|n| n.edit_poly_mut()) {
            if wanted.is_some() {
                p.passes_up = wanted;
            }
        }
    }
}

/// Instances on other nodes point at `src`'s slots by index: after `src`'s
/// stack is renumbered, `map(old) -> Some(new)` repoints them and `None`
/// (the slot went away) leaves them their own copy of the modifier.
fn repoint_instances(state: &AppState, src: NodeId, map: &dyn Fn(usize) -> Option<usize>) -> Vec<Change> {
    let mut changes = Vec::new();
    let Some(source) = state.scene.node(src) else { return changes };
    for n in state.scene.nodes() {
        if n.id == src || !n.modifiers.iter().any(|s| matches!(s.instance_of, Some((o, _)) if o == src)) {
            continue;
        }
        let mut stack = n.modifiers.clone();
        for slot in stack.iter_mut() {
            if let Some((o, i)) = slot.instance_of {
                if o != src {
                    continue;
                }
                match map(i) {
                    Some(j) => slot.instance_of = Some((src, j)),
                    None => {
                        if let Some(m) = source.modifiers.get(i).map(|s| s.modifier.clone()) {
                            slot.modifier = m;
                        }
                        slot.instance_of = None;
                    }
                }
            }
        }
        changes.push(Change::SetStack { id: n.id, before: n.modifiers.clone(), after: stack });
    }
    changes
}

fn set_active(state: &mut AppState, id: NodeId, entry: Option<usize>) {
    // The active entry changes what the viewport shows, not the stack result.
    if let Some(n) = state.scene.transform_mut(id) {
        n.active_entry = entry;
    }
    state.mesh_dirty = true;
}

/// `edit_stack` for edits that renumber slots: instances elsewhere follow `map`.
fn edit_stack_repointing(state: &mut AppState, label: &str, f: impl Fn(&mut Vec<ModifierSlot>) -> Result<(), String>, map: &dyn Fn(usize) -> Option<usize>) -> Result<Value, String> {
    let ids = selected_ids(state)?;
    let mut changes = Vec::new();
    for id in ids {
        let node = state.scene.node(id).ok_or("missing node")?;
        let mut stack = node.modifiers.clone();
        f(&mut stack)?;
        repoint_own(&mut stack, &node.modifiers, id, map);
        changes.push(Change::SetStack { id, before: node.modifiers.clone(), after: stack });
        changes.extend(repoint_instances(state, id, map));
    }
    commit_change(state, label, Change::Group(changes))?;
    Ok(stack_json(state))
}

/// `repoint_instances` for instances inside the node's own stack (Ctrl-drag
/// in the stack): `old` is the stack before the edit.
fn repoint_own(stack: &mut [ModifierSlot], old: &[ModifierSlot], id: NodeId, map: &dyn Fn(usize) -> Option<usize>) {
    for slot in stack.iter_mut() {
        let Some((o, i)) = slot.instance_of else { continue };
        if o != id {
            continue;
        }
        match map(i) {
            Some(j) => slot.instance_of = Some((id, j)),
            None => {
                if let Some(m) = old.get(i).map(|s| s.modifier.clone()) {
                    slot.modifier = m;
                }
                slot.instance_of = None;
            }
        }
    }
}

/// Copy Modifier: a copy (or an instance) of entry `from` inserted at `to`.
fn copy_modifier(state: &mut AppState, from: usize, to: usize, instance: bool) -> Result<Value, String> {
    let ids = selected_ids(state)?;
    let mut changes = Vec::new();
    for id in ids {
        let node = state.scene.node(id).ok_or("missing node")?;
        let len = node.modifiers.len();
        if from >= len || to > len {
            return Err("stack index out of range".into());
        }
        let map = |i: usize| Some(if i >= to { i + 1 } else { i });
        let mut stack = node.modifiers.clone();
        repoint_own(&mut stack, &node.modifiers, id, &map);
        let source = &node.modifiers[from];
        let mut slot = source.clone();
        slot.sub_level = 0;
        if instance {
            // Instances share one source: an instance of an instance points at the same.
            slot.instance_of = Some(match source.instance_of {
                Some((o, i)) if o == id => (id, if i >= to { i + 1 } else { i }),
                Some(other) => other,
                None => (id, if from >= to { from + 1 } else { from }),
            });
        } else {
            if source.instance_of.is_some() {
                slot.modifier = state.scene.resolved_modifier(id, from).unwrap_or_else(|| source.modifier.clone());
            }
            slot.instance_of = None;
        }
        stack.insert(to, slot);
        if stack.windows(2).any(|w| w[0].modifier.is_wsm() && !w[1].modifier.is_wsm()) {
            return Err("world-space modifiers stay above the object-space modifiers".into());
        }
        changes.push(Change::SetStack { id, before: node.modifiers.clone(), after: stack });
        changes.extend(repoint_instances(state, id, &map));
    }
    commit_change(state, if instance { "Instance Modifier" } else { "Copy Modifier" }, Change::Group(changes))?;
    Ok(stack_json(state))
}

fn edit_stack(state: &mut AppState, label: &str, f: impl Fn(&mut Vec<ModifierSlot>) -> Result<(), String>) -> Result<Value, String> {
    let ids = selected_ids(state)?;
    let mut changes = Vec::new();
    for id in ids {
        let node = state.scene.node(id).ok_or("missing node")?;
        let mut stack = node.modifiers.clone();
        f(&mut stack)?;
        changes.push(Change::SetStack { id, before: node.modifiers.clone(), after: stack });
    }
    commit_change(state, label, Change::Group(changes))?;
    Ok(stack_json(state))
}

/// Collapse To `index` (None = all): the evaluated mesh up to there becomes
/// an Editable Poly base object; entries above stay.
fn collapse(state: &mut AppState, upto: Option<usize>) -> Result<Value, String> {
    let ids = selected_ids(state)?;
    let mut changes = Vec::new();
    for id in &ids {
        let node = state.scene.node(*id).ok_or("missing node")?;
        if node.modifiers.is_empty() && upto.is_none() {
            continue;
        }
        let upto = upto.map(|u| u.min(node.modifiers.len().saturating_sub(1)));
        let evaluated = state.scene.evaluated(*id, upto);
        if !evaluated.warnings.is_empty() {
            return Err(evaluated.warnings.join("; "));
        }
        let mut poly = match &node.object {
            ObjectKind::EditablePoly(p) => p.with_mesh(evaluated.value.mesh.clone()),
            _ => PolyObject::from_mesh(evaluated.value.mesh.clone()),
        };
        // NURMS was baked into the result.
        poly.subdiv.use_nurms.set(false);
        // An Editable Mesh collapses to an Editable Mesh, an Editable Patch
        // to an Editable Patch while patches reach the top.
        let collapsed = match (&node.object, &evaluated.value.patch) {
            (ObjectKind::EditableMesh(_), _) => ObjectKind::EditableMesh(poly.with_mesh(loom_mesh::tri_mesh::from_poly(&poly.mesh)?)),
            (ObjectKind::EditablePatch(_), Some(p)) => node.object.with_patch(p.clone()),
            _ => ObjectKind::EditablePoly(poly),
        };
        let keep_from = upto.map(|u| u + 1).unwrap_or(node.modifiers.len());
        let mut rest: Vec<ModifierSlot> = node.modifiers.iter().skip(keep_from).cloned().collect();
        repoint_own(&mut rest, &node.modifiers, *id, &|i| if i < keep_from { None } else { Some(i - keep_from) });
        changes.push(Change::ReplaceObject { id: *id, before: Box::new(node.object.clone()), after: Box::new(collapsed) });
        changes.push(Change::SetStack { id: *id, before: node.modifiers.clone(), after: rest });
        changes.extend(repoint_instances(state, *id, &|i| if i < keep_from { None } else { Some(i - keep_from) }));
    }
    if changes.is_empty() {
        return Ok(json!({}));
    }
    // Instances of the whole object would diverge: make them unique first, as one undo entry.
    let label = if upto.is_some() { "Collapse To" } else { "Collapse All" };
    state.undo.begin(label);
    for id in &ids {
        if let Err(e) = crate::execute::detach_instance(state, *id) {
            state.undo.end();
            return Err(e);
        }
    }
    let r = commit_change(state, label, Change::Group(changes));
    state.undo.end();
    r?;
    for id in ids {
        set_active(state, id, None);
    }
    state.sub_selection.clear();
    Ok(stack_json(state))
}

pub fn stack_json(state: &AppState) -> Value {
    let stacks: Vec<Value> = state
        .selection
        .iter()
        .filter_map(|id| state.scene.node(*id))
        .map(|n| {
            json!({
                "node": n.id.0,
                "active": n.active_entry,
                "entries": n.modifiers.iter().map(|s| json!({ "name": s.name, "modifier": s.modifier.name(), "enabled": s.enabled, "in_viewport": s.enabled_in_viewport, "sub_level": s.sub_level, "instance_of": s.instance_of.map(|(n, i)| json!([n.0, i])) })).collect::<Vec<_>>(),
                "warnings": state.scene.eval_mesh(n.id).warnings,
            })
        })
        .collect();
    json!({ "stacks": stacks, "selection": selection_json(state) })
}

fn sub_level_of(mode: SelectionMode) -> Option<SubLevel> {
    crate::poly::level_of(mode)
}

/// After every command: the sub-object selection of the base poly flows up
/// the stack, and a Poly Select entry at a sub-object level stores it.
pub fn sync_selection(state: &mut AppState) {
    // A sub-object level needs a selected object (an undo that removes it, a
    // delete, Select None): without one the level ends, as in the reference,
    // so a viewport click picks objects again.
    if state.selection.is_empty() && state.selection_mode != SelectionMode::Object {
        state.selection_mode = SelectionMode::Object;
        state.sub_selection.clear();
    }
    let ids: Vec<NodeId> = state.selection.iter().copied().collect();
    let level = sub_level_of(state.selection_mode);
    for id in ids {
        let Some(node) = state.scene.node(id) else { continue };
        let raw: Vec<u32> = crate::poly::raw_ids(state.selection_mode, &state.sub_selection.by_node().find(|(n, _)| *n == id).map(|(_, ids)| ids.iter().copied().collect::<Vec<_>>()).unwrap_or_default());
        match node.active_entry {
            None => {
                let ObjectKind::EditablePoly(p) = &node.object else { continue };
                let wanted = level.filter(|_| !node.modifiers.is_empty()).map(|l| (l, raw.clone()));
                if p.passes_up != wanted {
                    if let Some(ObjectKind::EditablePoly(p)) = state.scene.node_mut(id).map(|n| &mut n.object) {
                        p.passes_up = wanted;
                    }
                    state.mesh_dirty = true;
                }
            }
            Some(a) => {
                let Some(slot) = node.modifiers.get(a) else { continue };
                // Leaving to the top level keeps what a Select modifier stored.
                if level.is_none() && matches!(slot.modifier, Modifier::PolySelect { .. } | Modifier::MeshSelect { .. } | Modifier::SplineSelect { .. } | Modifier::FilletChamfer(_)) {
                    continue;
                }
                if let Some(p) = slot.modifier.edit_poly() {
                    // Edit Poly passes its selection up like the base Editable Poly.
                    let wanted = level.map(|l| (l, raw.clone()));
                    if p.passes_up != wanted {
                        if let Some(p) = state.scene.node_mut(id).and_then(|n| n.modifiers[a].modifier.edit_poly_mut()) {
                            p.passes_up = wanted;
                        }
                        state.mesh_dirty = true;
                    }
                    continue;
                }
                if let Modifier::FilletChamfer(d) = &slot.modifier {
                    // Fillet/Chamfer keeps the vertices picked at its Vertex level for Apply.
                    if slot.sub_level == 1 && d.ids != raw {
                        if let Some(Modifier::FilletChamfer(d)) = state.scene.node_mut(id).map(|n| &mut n.modifiers[a].modifier) {
                            d.ids = raw.clone();
                        }
                    }
                    continue;
                }
                if let Modifier::SplineSelect { level, ids, .. } = &slot.modifier {
                    // Spline Select stores the sub-object selection made at its levels.
                    if slot.sub_level == 0 || (*level == slot.sub_level && *ids == raw) {
                        continue;
                    }
                    let knots = state.scene.evaluated(id, a.checked_sub(1)).value.shape.as_ref().or(node.object.shape().as_ref()).map(|s| s.knot_count()).unwrap_or(0);
                    let lvl = slot.sub_level;
                    if let Some(Modifier::SplineSelect { level, ids, knots: k }) = state.scene.node_mut(id).map(|n| &mut n.modifiers[a].modifier) {
                        *level = lvl;
                        *ids = raw.clone();
                        *k = knots;
                    }
                    state.mesh_dirty = true;
                    continue;
                }
                if !matches!(slot.modifier, Modifier::PolySelect { .. } | Modifier::MeshSelect { .. }) || slot.sub_level == 0 {
                    continue;
                }
                let input = state.scene.evaluated(id, a.checked_sub(1)).value.mesh.clone();
                let input = if a == 0 { node.object.display_output() } else { input };
                let wanted = StoredSelection { level: slot.sub_level, ids: raw.clone(), vertices: input.vertex_count(), faces: input.face_count() };
                if slot.modifier.stored_selection() != Some(&wanted) {
                    if let Some(n) = state.scene.node_mut(id) {
                        if let Some(s) = n.modifiers[a].modifier.stored_selection_mut() {
                            *s = wanted;
                        }
                    }
                    state.mesh_dirty = true;
                }
            }
        }
    }
}

/// Spline IK Control > Create Helpers: a Point helper per knot of the shape
/// flowing into the entry, linked as Link Types says, in one undo step.
pub fn spline_ik_create_helpers(state: &mut AppState) -> Result<Value, String> {
    let (id, index) = state
        .selection
        .iter()
        .find_map(|id| {
            let n = state.scene.node(*id)?;
            let is = |m: &Modifier| matches!(m, Modifier::SplineIkControl(_));
            let i = n.active_entry.filter(|a| n.modifiers.get(*a).is_some_and(|s| is(&s.modifier))).or_else(|| n.modifiers.iter().rposition(|s| is(&s.modifier)))?;
            Some((*id, i))
        })
        .ok_or("select a shape with a Spline IK Control modifier")?;
    let node = state.scene.node(id).ok_or("missing node")?;
    let Modifier::SplineIkControl(d) = &node.modifiers[index].modifier else { unreachable!() };
    if d.helpers.iter().any(|h| h.is_some()) {
        return Err("Spline IK Control already has its helpers".into());
    }
    let input = match index.checked_sub(1) {
        Some(prev) => state.scene.evaluated(id, Some(prev)).value.shape.clone(),
        None => node.object.shape(),
    }
    .ok_or("Spline IK Control needs a shape below it")?;
    let world = state.scene.object_matrix(id);
    let pts: Vec<glam::Vec3> = input.knot_positions().iter().map(|p| world.transform_point3(*p)).collect();
    let (link, display, size) = (d.link_type, d.display, d.helper_size);
    let first = state.scene.next_id();
    let mut taken: Vec<String> = Vec::new();
    let mut changes = Vec::new();
    let mut ids = Vec::new();
    for (i, p) in pts.iter().enumerate() {
        let mut n = 1;
        let name = loop {
            let c = format!("Point{n:03}");
            if !taken.contains(&c) && !state.scene.nodes().any(|x| x.name == c) {
                break c;
            }
            n += 1;
        };
        taken.push(name.clone());
        let mut object = loom_scene::ObjectKind::point(size);
        if let loom_scene::ObjectKind::Point { axis_tripod, cross, box_, .. } = &mut object {
            axis_tripod.set(display == 1);
            cross.set(display == 2);
            box_.set(display == 3);
        }
        let mut h = loom_scene::Node::new(&name, object);
        h.id = NodeId(first + i as u64);
        h.wire_color = [120, 200, 120];
        let parent = match link {
            0 if i > 0 => Some(i - 1),
            1 if i > 0 => Some(0),
            _ => None,
        };
        h.parent = parent.map(|j| NodeId(first + j as u64));
        h.transform.position.set(*p - parent.map(|j| pts[j]).unwrap_or(glam::Vec3::ZERO));
        ids.push(h.id);
        changes.push(Change::AddNode(h));
    }
    let before = node.modifiers.clone();
    let mut after = before.clone();
    if let Modifier::SplineIkControl(d) = &mut after[index].modifier {
        d.helpers = ids.iter().map(|h| Some(*h)).collect();
        d.knot_counts = input.splines.iter().map(|sp| sp.knots.len()).collect();
    }
    changes.push(Change::SetStack { id, before, after });
    commit_change(state, "Create Helpers", Change::Group(changes))?;
    Ok(json!({ "helpers": ids.iter().map(|h| h.0).collect::<Vec<_>>() }))
}

/// The mesh flowing into stack entry `index` of `id`.
fn entry_input(state: &AppState, id: NodeId, index: usize) -> loom_mesh::Mesh {
    match index.checked_sub(1) {
        Some(prev) => state.scene.evaluated(id, Some(prev)).value.mesh.clone(),
        None => state.scene.node(id).map(|n| n.object.display_output()).unwrap_or_default(),
    }
}

/// Rebuilds an HSDS entry's cage (the level being edited) from the current input.
fn refresh_hsds_cage(state: &mut AppState, id: NodeId, index: usize) {
    let input = entry_input(state, id, index);
    if let Some(Modifier::Hsds(d)) = state.scene.node_mut(id).and_then(|n| n.modifiers.get_mut(index)).map(|s| &mut s.modifier) {
        d.cage = loom_scene::caches9::hsds_mesh_upto(d, &input, d.edited_level()).map(|(m, _)| m).unwrap_or(input);
    }
}

/// HSDS > level of detail list: the picked level is the one the sub-object
/// levels pick on (not an undo step, like the level buttons).
pub fn hsds_select_level(state: &mut AppState, level: u32) -> Result<Value, String> {
    let (id, index) = entry_of(state, |m| matches!(m, Modifier::Hsds(_))).ok_or("select an object with an HSDS modifier")?;
    if let Some(Modifier::Hsds(d)) = state.scene.node_mut(id).and_then(|n| n.modifiers.get_mut(index)).map(|s| &mut s.modifier) {
        if level as usize > d.levels.len() {
            return Err(format!("HSDS has Base Level and {} level(s)", d.levels.len()));
        }
        d.level = (level as usize != d.levels.len()).then_some(level);
    }
    refresh_hsds_cage(state, id, index);
    state.sub_selection.clear();
    state.mesh_dirty = true;
    Ok(hsds_json(state))
}

/// HSDS > Display next to a level: the result shows only up to it (the finest shows all).
pub fn hsds_display(state: &mut AppState, level: u32) -> Result<Value, String> {
    let (id, index) = entry_of(state, |m| matches!(m, Modifier::Hsds(_))).ok_or("select an object with an HSDS modifier")?;
    let before = state.scene.node(id).ok_or("missing node")?.modifiers.clone();
    let mut after = before.clone();
    if let Modifier::Hsds(d) = &mut after[index].modifier {
        if level as usize > d.levels.len() {
            return Err(format!("HSDS has Base Level and {} level(s)", d.levels.len()));
        }
        d.display = (level as usize != d.levels.len()).then_some(level);
    }
    if after != before {
        commit_change(state, "HSDS Display", Change::SetStack { id, before, after })?;
        state.mesh_dirty = true;
    }
    Ok(hsds_json(state))
}

/// The selected HSDS entry for State: its level count, the edited and the displayed level.
pub fn hsds_json(state: &AppState) -> Value {
    let Some((id, index)) = entry_of(state, |m| matches!(m, Modifier::Hsds(_))) else { return Value::Null };
    let Some(Modifier::Hsds(d)) = state.scene.node(id).and_then(|n| n.modifiers.get(index)).map(|s| &s.modifier) else { return Value::Null };
    json!({ "node": id.0, "index": index, "levels": d.levels.len(), "level": d.edited_level(), "display": d.displayed_levels() })
}

/// The selected node's entry of a modifier kind: the active one, else the topmost.
fn entry_of(state: &AppState, is: impl Fn(&Modifier) -> bool) -> Option<(NodeId, usize)> {
    state.selection.iter().find_map(|id| {
        let n = state.scene.node(*id)?;
        let i = n.active_entry.filter(|a| n.modifiers.get(*a).is_some_and(|s| is(&s.modifier))).or_else(|| n.modifiers.iter().rposition(|s| is(&s.modifier)))?;
        Some((*id, i))
    })
}

pub fn hsds_subdivide(state: &mut AppState, faces: Option<Vec<u32>>) -> Result<Value, String> {
    let (id, index) = entry_of(state, |m| matches!(m, Modifier::Hsds(_))).ok_or("select an object with an HSDS modifier")?;
    let input = entry_input(state, id, index);
    let node = state.scene.node(id).ok_or("missing node")?;
    let Modifier::Hsds(d) = &node.modifiers[index].modifier else { unreachable!() };
    if d.edited_level() != d.levels.len() {
        return Err(format!("HSDS: Subdivide splits polygons of the finest level; pick Level {} in the list", d.levels.len()));
    }
    let (finest, warning) = loom_scene::caches9::hsds_mesh(d, &input)?;
    if let Some(w) = warning {
        return Err(w);
    }
    let faces: Vec<u32> = match faces {
        Some(f) => f,
        None => {
            // The polygon selection on the finest level (Vertex / Edge selections take their polygons).
            let level = match state.selection_mode {
                SelectionMode::Vertex => loom_mesh::select_ops::SubLevel::Vertex,
                SelectionMode::Edge | SelectionMode::Border => loom_mesh::select_ops::SubLevel::Edge,
                _ => loom_mesh::select_ops::SubLevel::Face,
            };
            let raw: Vec<u32> = state.sub_selection.iter().filter(|(n, _)| *n == id).map(|(_, s)| match s { crate::state::SubObjectId::Vertex(v) => v.0, crate::state::SubObjectId::Edge(e) => e.0, crate::state::SubObjectId::Face(f) => f.0 }).collect();
            if level == loom_mesh::select_ops::SubLevel::Face {
                raw
            } else {
                loom_mesh::select_ops::convert_selection(&finest, level, &raw, loom_mesh::select_ops::SubLevel::Face, false)
            }
        }
    };
    let faces: Vec<u32> = faces.into_iter().filter(|f| (*f as usize) < finest.face_count()).collect();
    if faces.is_empty() {
        return Err("HSDS: select polygons to subdivide".into());
    }
    let before = node.modifiers.clone();
    let mut after = before.clone();
    if let Modifier::Hsds(d) = &mut after[index].modifier {
        d.levels.push(loom_scene::caches9::HsdsLevel { faces, vertex_count: finest.vertex_count(), face_count: finest.face_count() });
        // The new level is the one edited and shown.
        d.level = None;
        d.display = None;
        d.cage = loom_scene::caches9::hsds_mesh(d, &input).map(|(m, _)| m)?;
    }
    commit_change(state, "Subdivide", Change::SetStack { id, before, after })?;
    state.sub_selection.clear();
    state.mesh_dirty = true;
    Ok(json!({ "faces": state.scene.eval_mesh(id).value.mesh.face_count() }))
}

pub fn hsds_adaptive(state: &mut AppState, max_level: u32, max_angle: f32) -> Result<Value, String> {
    let (id, index) = entry_of(state, |m| matches!(m, Modifier::Hsds(_))).ok_or("select an object with an HSDS modifier")?;
    let input = entry_input(state, id, index);
    let node = state.scene.node(id).ok_or("missing node")?;
    let before = node.modifiers.clone();
    let mut after = before.clone();
    if let Modifier::Hsds(d) = &mut after[index].modifier {
        loom_scene::caches9::hsds_adaptive(d, &input, max_level.min(8), max_angle.clamp(0.0, 180.0))?;
    }
    commit_change(state, "Adaptive Subdivision", Change::SetStack { id, before, after })?;
    state.mesh_dirty = true;
    Ok(json!({ "faces": state.scene.eval_mesh(id).value.mesh.face_count() }))
}

fn point_cache_entry(state: &AppState, index: usize) -> Result<NodeId, String> {
    let id = state.selection.iter().copied().find(|id| state.scene.node(*id).and_then(|n| n.modifiers.get(index)).is_some_and(|s| matches!(s.modifier, Modifier::PointCache(_)))).ok_or_else(|| format!("stack entry {index} of the selection is not a Point Cache"))?;
    Ok(id)
}

pub fn point_cache_record(state: &mut AppState, index: usize, path: &str, start: i32, end: i32) -> Result<Value, String> {
    let id = point_cache_entry(state, index)?;
    if end < start {
        return Err("Point Cache: the end frame comes before the start".into());
    }
    let world = matches!(&state.scene.node(id).unwrap().modifiers[index].modifier, Modifier::PointCache(d) if d.world);
    let mut samples = Vec::new();
    for f in start..=end {
        let pts = state.scene.at_time(f as f32, || {
            let m = entry_input(state, id, index);
            let to_world = if world { state.scene.object_matrix(id) } else { glam::Mat4::IDENTITY };
            m.vertices().iter().map(|v| to_world.transform_point3(v.position)).collect::<Vec<_>>()
        });
        samples.push(pts);
    }
    if let Some((f, n)) = samples.iter().enumerate().find(|(_, s)| s.len() != samples[0].len()).map(|(k, s)| (start + k as i32, s.len())) {
        return Err(format!("Point Cache: the vertex count changes over the range ({} at frame {start}, {n} at frame {f}); a .pc2 needs one count", samples[0].len()));
    }
    let pc = loom_mesh::pc2::Pc2 { start: start as f32, rate: 1.0, samples };
    let file = std::fs::File::create(path).map_err(|e| format!("{path}: {e}"))?;
    pc.write(std::io::BufWriter::new(file)).map_err(|e| format!("{path}: {e}"))?;
    point_cache_load(state, index, Some(path))?;
    Ok(json!({ "path": path, "samples": pc.samples.len(), "points": pc.points() }))
}

pub fn point_cache_load(state: &mut AppState, index: usize, path: Option<&str>) -> Result<Value, String> {
    let id = point_cache_entry(state, index)?;
    let node = state.scene.node(id).ok_or("missing node")?;
    let before = node.modifiers.clone();
    let mut after = before.clone();
    if let Modifier::PointCache(d) = &mut after[index].modifier {
        match path {
            Some(p) => d.load(p)?,
            None => {
                d.path.clear();
                d.data = Default::default();
            }
        }
    }
    commit_change(state, if path.is_some() { "Load Point Cache" } else { "Unload Point Cache" }, Change::SetStack { id, before, after })?;
    Ok(json!({ "path": path }))
}

fn add_modifier(state: &mut AppState, name: &str) -> Result<Value, String> {
    {
            let modifier = Modifier::by_name(name).ok_or_else(|| format!("no modifier {name:?} (Modifier List: {}, {})", Modifier::WSM_LIST.join(", "), Modifier::LIST.join(", ")))?;
            let ids = selected_ids(state)?;
            // The sub-object selection the poly being edited passes up stays
            // with it (the Editable Poly's level stays under the new entry).
            freeze_passes_up(state, &ids);
            let mut changes = Vec::new();
            let mut inserted = Vec::new();
            for id in &ids {
                let node = state.scene.node(*id).ok_or("missing node")?;
                let mut stack = node.modifiers.clone();
                // Inserts above the active entry (just above the base object when it is active).
                let at = node.active_entry.map(|a| a + 1).unwrap_or(0).min(stack.len());
                // World-space modifiers stay above every object-space one (stack order).
                let first_wsm = stack.iter().position(|m| m.modifier.is_wsm()).unwrap_or(stack.len());
                let at = if modifier.is_wsm() { stack.len() } else { at.min(first_wsm) };
                let mut modifier = modifier.clone();
                if modifier.needs_shape() {
                    // Refuses Extrude / Lathe / Bevel / Sweep / Edit Spline on a mesh.
                    let input_shape = match at.checked_sub(1) {
                        Some(prev) => state.scene.evaluated(*id, Some(prev)).value.shape.clone(),
                        None => node.object.shape(),
                    };
                    let Some(input_shape) = input_shape else {
                        return Err(format!("{name} requires a shape (spline) object below it"));
                    };
                    modifier.init_edit_spline(&input_shape);
                }
                if matches!(modifier, Modifier::UVWMap { .. }) {
                    let input = match at.checked_sub(1) {
                        Some(prev) => state.scene.evaluated(*id, Some(prev)).value.mesh.clone(),
                        None => node.object.display_output(),
                    };
                    modifier.fit_uvw_map(&input);
                }
                if matches!(modifier, Modifier::EditPatch { .. }) {
                    // Edit Patch edits the patches flowing in (a mesh becomes patches).
                    let (input, patch) = match at.checked_sub(1) {
                        Some(prev) => {
                            let e = state.scene.evaluated(*id, Some(prev));
                            (e.value.mesh.clone(), e.value.patch.clone())
                        }
                        None => (node.object.display_output(), node.object.patch_mesh()),
                    };
                    let input_poly = if input.has_invisible_edges() && patch.is_none() { loom_mesh::tri_mesh::to_poly(&input).unwrap_or_else(|_| input.clone()) } else { input.clone() };
                    modifier.init_edit_patch(&input, patch.as_ref());
                    if patch.is_none() {
                        if let Modifier::EditPatch { patch: p, .. } = &mut modifier {
                            p.patch = loom_mesh::patch::PatchMesh::from_mesh(&input_poly);
                        }
                    }
                }
                if matches!(modifier, Modifier::FFD { .. } | Modifier::EditPoly { .. } | Modifier::EditMesh { .. }) {
                    // An FFD lattice is fitted to what flows into it; an Edit Poly starts from it.
                    let input = match at.checked_sub(1) {
                        Some(prev) => state.scene.evaluated(*id, Some(prev)).value.mesh.clone(),
                        None => node.object.display_output(),
                    };
                    let (lo, hi) = input.aabb();
                    modifier.fit_ffd(lo, hi);
                    modifier.init_edit_poly(&input);
                }
                let mut slot = ModifierSlot::new(modifier);
                if name == "Edit Mesh" {
                    slot.name = "Edit Mesh".to_owned();
                }
                if matches!(slot.modifier, Modifier::EditNormals(_)) {
                    // Edit Normals starts at its Normal level: the normals show at once.
                    slot.sub_level = 1;
                }
                stack.insert(at, slot);
                inserted.push((*id, at));
                changes.push(Change::SetStack { id: *id, before: node.modifiers.clone(), after: stack });
                changes.extend(repoint_instances(state, *id, &|i| Some(if i >= at { i + 1 } else { i })));
            }
            commit_change(state, &format!("Add {name}"), Change::Group(changes))?;
            for (id, at) in inserted {
                set_active(state, id, Some(at));
            }
            state.selection_mode = SelectionMode::Object;
            state.sub_selection.clear();
            Ok(stack_json(state))
        }
}
