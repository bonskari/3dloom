//! File > Import of .max scene files (loom_max reads them): nodes keep
//! their names, hierarchy, transforms, pivot offsets and wire colours;
//! Editable Poly / Editable Mesh become Editable Poly, Box / Cylinder /
//! Plane stay parametric, Dummies stay Dummies and anything else becomes a
//! Dummy so its children keep their place. One undo step.

use crate::execute::commit;
use crate::state::{AppState, SelectionMode};
use loom_max::MaxKind;
use loom_scene::{Change, Node, NodeId, ObjectKind, Transform};
use serde_json::{json, Value};
use std::path::Path;

pub fn is_max(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("max"))
}

pub fn import(state: &mut AppState, path: &Path) -> Result<Value, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let doc = loom_max::read(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let first = state.scene.next_id();
    let mut taken: std::collections::BTreeSet<String> = state.scene.nodes().map(|n| n.name.clone()).collect();
    let mut changes = Vec::with_capacity(doc.nodes.len());
    let mut ids = Vec::with_capacity(doc.nodes.len());
    for (i, n) in doc.nodes.iter().enumerate() {
        let base = if n.name.trim().is_empty() { "Object" } else { n.name.as_str() };
        let name = if taken.contains(base) { crate::execute::unique_name(base, &taken) } else { base.to_owned() };
        taken.insert(name.clone());
        let object = match &n.kind {
            MaxKind::Mesh(m) => ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(m.clone())),
            MaxKind::Box { length, width, height, length_segs, width_segs, height_segs } => {
                let mut o = ObjectKind::box_object(*length, *width, *height);
                for (p, v) in [("Length Segs", *length_segs), ("Width Segs", *width_segs), ("Height Segs", *height_segs)] {
                    o.set_parameter(p, v as f64)?;
                }
                o
            }
            MaxKind::Cylinder { radius, height, height_segs, cap_segs, sides, smooth, slice_on, slice_from, slice_to } => {
                let mut o = ObjectKind::cylinder_object(*radius, *height);
                for (p, v) in [("Height Segments", *height_segs as f64), ("Cap Segments", *cap_segs as f64), ("Sides", *sides as f64), ("Slice On", *slice_on as u8 as f64), ("Slice From", *slice_from as f64), ("Slice To", *slice_to as f64)] {
                    o.set_parameter(p, v)?;
                }
                let _ = smooth;
                o
            }
            MaxKind::Plane { length, width, length_segs, width_segs } => {
                let mut o = ObjectKind::plane_object(*length, *width);
                o.set_parameter("Length Segs", *length_segs as f64)?;
                o.set_parameter("Width Segs", *width_segs as f64)?;
                o
            }
            MaxKind::Dummy { size } => ObjectKind::dummy(size.max_element().max(0.001)),
            MaxKind::Other(_) => ObjectKind::dummy(10.0),
        };
        let mut node = Node::new(&name, object);
        node.id = NodeId(first + i as u64);
        node.parent = n.parent.map(|p| NodeId(first + p as u64));
        node.transform = Transform::from_matrix(n.local);
        node.object_offset = Transform::from_matrix(n.offset);
        node.wire_color = n.wire_color;
        node.layer = state.scene.current_layer;
        ids.push(node.id);
        changes.push(Change::AddNode(node));
    }
    let label = format!("Import {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    commit(state, &label, Change::Group(changes))?;
    state.selection = ids.iter().copied().collect();
    state.sub_selection.clear();
    state.selection_mode = SelectionMode::Object;
    state.import_report = doc.warnings.clone();
    Ok(json!({ "nodes": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "warnings": doc.warnings }))
}
