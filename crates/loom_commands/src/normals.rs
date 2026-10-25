//! Edit Normals commands: selection, Break / Unify / Specify / Make
//! Explicit / Reset / Average, Copy / Paste, set and rotate directions.

use crate::command::{Command, SelectOp};
use crate::execute::commit_exact as commit;
use crate::state::AppState;
use loom_mesh::Mesh;
use loom_scene::edit_normals::EditNormalsData;
use loom_scene::{Change, Modifier, NodeId};
use serde_json::{json, Value};

pub fn handles(c: &Command) -> bool {
    use Command as C;
    matches!(c, C::EditNormalsSelect { .. } | C::EditNormalsSelectBy { .. } | C::EditNormalsSetNormal { .. } | C::EditNormalsRotate { .. })
}

/// The selected node's Edit Normals entry: the active one, else the topmost.
pub fn entry(state: &AppState) -> Option<(NodeId, usize)> {
    state.selection.iter().find_map(|id| {
        let n = state.scene.node(*id)?;
        if let Some(a) = n.active_entry {
            if matches!(n.modifiers.get(a).map(|s| &s.modifier), Some(Modifier::EditNormals(_))) {
                return Some((*id, a));
            }
        }
        n.modifiers.iter().rposition(|s| matches!(s.modifier, Modifier::EditNormals(_))).map(|i| (*id, i))
    })
}

/// The mesh entering the entry.
pub fn input_mesh(state: &AppState, id: NodeId, index: usize) -> Mesh {
    match index {
        0 => state.scene.node(id).map(|n| n.object.display_output()).unwrap_or_default(),
        i => state.scene.evaluated(id, Some(i - 1)).value.mesh.clone(),
    }
}

fn data(state: &AppState, id: NodeId, index: usize) -> Option<EditNormalsData> {
    match &state.scene.node(id)?.modifiers.get(index)?.modifier {
        Modifier::EditNormals(d) => Some((**d).clone()),
        _ => None,
    }
}

fn edit(state: &mut AppState, label: &str, f: impl FnOnce(&mut EditNormalsData, &Mesh) -> Result<(), String>) -> Result<Value, String> {
    let (id, index) = entry(state).ok_or("select an object with an Edit Normals modifier")?;
    let input = input_mesh(state, id, index);
    let mut d = data(state, id, index).ok_or("no Edit Normals entry")?;
    f(&mut d, &input)?;
    let n = state.scene.node(id).unwrap();
    let before = n.modifiers.clone();
    let mut after = before.clone();
    after[index].modifier = Modifier::EditNormals(Box::new(d));
    if after != before {
        commit(state, label, Change::SetStack { id, before, after })?;
    }
    Ok(state_json(state))
}

/// Copy Normal: fills the entry's clipboard without an undo step (copying
/// changes no geometry; the Paste that uses it is the undo step).
fn copy(state: &mut AppState) -> Result<Value, String> {
    let (id, index) = entry(state).ok_or("select an object with an Edit Normals modifier")?;
    let input = input_mesh(state, id, index);
    let mut d = data(state, id, index).ok_or("no Edit Normals entry")?;
    let n = d.copy(&input).ok_or("select a normal to copy")?;
    if let Some(Modifier::EditNormals(e)) = state.scene.node_mut(id).and_then(|n| n.modifiers.get_mut(index)).map(|s| &mut s.modifier) {
        e.clipboard = Some(n);
    }
    Ok(state_json(state))
}

/// Sets the entry's sub-object level (no undo, like the level buttons).
fn level(state: &mut AppState, level: u32, select_by: Option<u32>) -> Result<Value, String> {
    let (id, index) = entry(state).ok_or("select an object with an Edit Normals modifier")?;
    if let Some(n) = state.scene.node_mut(id) {
        n.active_entry = Some(index);
        n.modifiers[index].sub_level = level;
        if let (Some(by), Modifier::EditNormals(d)) = (select_by, &mut n.modifiers[index].modifier) {
            d.select_by = by;
        }
    }
    Ok(state_json(state))
}

/// The Edit Normals entry when its Normal level is on: what the viewport
/// clicks and the transform tools work on.
pub fn normal_level(state: &AppState) -> Option<(NodeId, usize)> {
    let (id, index) = entry(state)?;
    let n = state.scene.node(id)?;
    (n.active_entry == Some(index) && n.modifiers.get(index)?.sub_level == 1).then_some((id, index))
}

/// World centre of the selected normals' vertices at the Normal level (the
/// transform gizmo's place).
pub fn selected_center(state: &AppState) -> Option<glam::Vec3> {
    let (id, index) = normal_level(state)?;
    let d = data(state, id, index)?;
    if d.selection.is_empty() {
        return None;
    }
    let input = input_mesh(state, id, index);
    let groups = d.groups_for(&input);
    let world = state.scene.object_matrix(id);
    let mut seen = std::collections::BTreeSet::new();
    let (mut sum, mut n) = (glam::Vec3::ZERO, 0.0);
    for (i, g) in groups.iter().enumerate() {
        if d.selection.contains(g) && seen.insert(*g) {
            if let Some(p) = input.vertex_position(input.half_edges()[i].origin) {
                sum += world.transform_point3(p);
                n += 1.0;
            }
        }
    }
    (n > 0.0).then(|| sum / n)
}

/// Select And Rotate at the Normal level: the selected normals turn by the
/// rotation part of the world matrix `m` (Move and Scale do nothing to them).
/// None when the Normal level is not on.
pub fn transform(state: &mut AppState, label: &str, m: glam::Mat4) -> Option<Result<Value, String>> {
    let (id, index) = normal_level(state)?;
    let world = state.scene.object_matrix(id);
    let (_, q, _) = (world.inverse() * m * world).to_scale_rotation_translation();
    if data(state, id, index)?.selection.is_empty() || q.angle_between(glam::Quat::IDENTITY) < 1e-6 {
        return Some(Ok(state_json(state)));
    }
    Some(edit(state, label, |d, i| {
        d.rotate_by(i, q);
        Ok(())
    }))
}

pub fn state_json(state: &AppState) -> Value {
    let Some((id, index)) = entry(state) else { return Value::Null };
    let Some(d) = data(state, id, index) else { return Value::Null };
    let input = input_mesh(state, id, index);
    let ids = d.normal_ids(&input);
    json!({
        "node": id.0,
        "index": index,
        "normals": ids.len(),
        "selected": d.selection.iter().filter(|g| ids.contains(g)).collect::<Vec<_>>(),
        "select_by": d.select_by,
        "explicit": d.explicit.len(),
        "use_threshold": d.use_threshold,
        "threshold": d.threshold,
        "display_length": d.display_length,
    })
}

pub fn execute(state: &mut AppState, cmd: Command) -> Result<Value, String> {
    use Command as C;
    match cmd {
        C::NormalLevel => level(state, 1, Some(0)),
        C::VertexLevelEditNormals => level(state, 1, Some(1)),
        C::EdgeLevelEditNormals => level(state, 1, Some(2)),
        C::FaceLevelEditNormals => level(state, 1, Some(3)),
        C::ObjectLevelEditNormals => level(state, 0, None),
        C::EditNormalsSelectBy { level: by } => {
            if by > 3 {
                return Err("Select By is 0 Normal, 1 Vertex, 2 Edge or 3 Face".into());
            }
            level(state, 1, Some(by))
        }
        C::EditNormalsSelect { ids, op } => edit(state, "Select Normals", |d, input| {
            d.rebase(input);
            let picked = d.resolve(input, &ids, d.select_by);
            match op {
                SelectOp::Replace => d.selection = picked,
                SelectOp::Add => d.selection.extend(picked),
                SelectOp::Subtract => d.selection.retain(|g| !picked.contains(g)),
            }
            Ok(())
        }),
        C::AverageMode => edit(state, "Average Mode", |d, _| {
            d.use_threshold = !d.use_threshold;
            Ok(())
        }),
        C::AverageNormals => edit(state, "Average Normals", |d, i| {
            d.average(i);
            Ok(())
        }),
        C::BreakNormals => edit(state, "Break Normals", |d, i| {
            d.break_selected(i);
            Ok(())
        }),
        C::UnifyNormals => edit(state, "Unify Normals", |d, i| {
            d.unify_selected(i);
            Ok(())
        }),
        C::SpecifyNormals => edit(state, "Specify Normals", |d, i| {
            d.specify(i);
            Ok(())
        }),
        C::MakeExplicit => edit(state, "Make Explicit", |d, i| {
            d.make_explicit(i);
            Ok(())
        }),
        C::ResetNormals => edit(state, "Reset Normals", |d, i| {
            d.reset(i);
            Ok(())
        }),
        C::CopyNormal => copy(state),
        C::PasteNormal => edit(state, "Paste Normal", |d, i| if d.paste(i) { Ok(()) } else { Err("copy a normal first".into()) }),
        C::EditNormalsSetNormal { direction } => edit(state, "Set Normal", |d, i| {
            d.set_direction(i, glam::Vec3::from(direction));
            Ok(())
        }),
        C::EditNormalsRotate { axis, degrees } => edit(state, "Rotate Normals", |d, i| {
            d.rotate(i, glam::Vec3::from(axis), degrees);
            Ok(())
        }),
        _ => Err("not an Edit Normals command".into()),
    }
}
