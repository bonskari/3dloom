//! File menu commands beyond Open / Save: New Scene keeping objects, Save
//! Selected, Archive, Summary Info and File Properties.

use crate::file::{Document, SceneSettings, FORMAT};
use crate::state::AppState;
use crate::viewport::Dialog;
use crate::Command as C;
use loom_scene::{NodeId, ObjectKind};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::NewSceneKeep { keep } => new_keep(state, &keep),
        C::SaveSelected { path } => {
            let Some(path) = path else {
                state.dialog = Some(Dialog::SaveSelected);
                return Ok(json!({ "dialog": "SaveSelected" }));
            };
            let doc = selected_document(state)?;
            let n = doc.scene.len();
            crate::file::write(Path::new(&path), &doc)?;
            Ok(json!({ "path": path, "nodes": n }))
        }
        C::ArchiveFile { path } => {
            let Some(path) = path else {
                state.dialog = Some(Dialog::ArchiveFile);
                return Ok(json!({ "dialog": "ArchiveFile" }));
            };
            archive(state, Path::new(&path))
        }
        C::SummaryInfo => {
            state.dialog = Some(Dialog::SummaryInfo);
            Ok(summary_info(state))
        }
        C::FileProperties => {
            state.dialog = Some(Dialog::FileProperties);
            Ok(serde_json::to_value(&state.file_properties).unwrap_or(Value::Null))
        }
        C::SetFileProperty { name, value } => {
            let field = state.file_properties.field_mut(&name).ok_or_else(|| format!("no file property {name:?} ({})", crate::file::FileProperties::FIELDS.join(", ")))?;
            if *field != value {
                *field = value;
                state.undo.mark_unsaved();
            }
            Ok(json!({}))
        }
        C::SetCustomFileProperty { name, value } => {
            let name = name.trim().to_owned();
            if name.is_empty() {
                return Err("a custom property needs a name".into());
            }
            let changed = match value {
                Some(v) => state.file_properties.custom.insert(name, v.clone()).as_ref() != Some(&v),
                None => state.file_properties.custom.remove(&name).is_some(),
            };
            if changed {
                state.undo.mark_unsaved();
            }
            Ok(json!({ "custom": state.file_properties.custom }))
        }
        other => Err(format!("not a file command: {other:?}")),
    }
}

/// New Scene dialog > Keep Objects and Hierarchy / Keep Objects.
fn new_keep(state: &mut AppState, keep: &str) -> Result<Value, String> {
    let unlink = match keep {
        "all" => return crate::execute(state, C::NewScene),
        "objects_and_hierarchy" => false,
        "objects" => true,
        other => return Err(format!("unknown New Scene option {other:?} (objects_and_hierarchy / objects / all)")),
    };
    let all: Vec<u64> = state.scene.nodes().map(|n| n.id.0).collect();
    let rig_worlds: Vec<_> = if unlink && !state.scene.loom_rigs.is_empty() {
        state.scene.nodes().map(|n| (n.id, state.scene.world_matrix(n.id))).collect()
    } else { Vec::new() };
    if !all.is_empty() {
        state.selection_mode = crate::state::SelectionMode::Object;
        state.selection = all.iter().map(|i| NodeId(*i)).collect();
        // The animation keys go; each object keeps its pose at the current frame.
        crate::execute(state, C::DeleteSelectedAnimation)?;
        if unlink && !rig_worlds.is_empty() {
            // Flatten the visible solved pose before discarding its hierarchy-dependent rig.
            state.scene.loom_rigs.clear();
            for (id, world) in rig_worlds {
                if let Some(node) = state.scene.node_mut(id) {
                    node.parent = None;
                    node.transform = loom_scene::Transform::from_matrix(world);
                }
            }
        } else if unlink {
            let linked: Vec<u64> = state.scene.nodes().filter(|n| n.parent.is_some()).map(|n| n.id.0).collect();
            for id in linked {
                crate::execute(state, C::SetParent { id, parent: None })?;
            }
        }
    }
    state.scene.animation_clips.clear();
    if unlink { state.scene.loom_rigs.clear(); }
    for rig in &mut state.scene.loom_rigs {
        for limb in &mut rig.limbs {
            limb.blend = loom_scene::Param::new(limb.blend.get());
            for offset in &mut limb.pose_offsets { *offset = loom_scene::Param::new(offset.get()); }
        }
    }
    state.scene.bump_version();
    let valid_rigs: Vec<_> = state.scene.loom_rigs.iter().filter(|r| r.mapping.validate(&state.scene).is_ok() && r.node_ids().iter().all(|id| state.scene.node(*id).is_some())).cloned().collect();
    state.scene.loom_rigs = valid_rigs;
    state.scene.renew_instance();
    state.selection.clear();
    state.sub_selection.clear();
    state.undo = loom_scene::UndoStack::new(state.undo.limit());
    state.file = None;
    state.undo.mark_saved();
    state.mesh_dirty = true;
    Ok(json!({ "nodes": state.scene.len() }))
}

/// The nodes Save Selected writes: the selection, members of selected
/// groups, camera targets, and every ancestor up to the root.
fn saved_nodes(state: &AppState) -> BTreeSet<NodeId> {
    let scene = &state.scene;
    let mut keep: BTreeSet<NodeId> = BTreeSet::new();
    for id in &state.selection {
        keep.insert(*id);
        if scene.node(*id).is_some_and(|n| matches!(n.object, ObjectKind::Group { .. })) {
            keep.extend(scene.descendants(*id));
        }
    }
    let targets: Vec<NodeId> = keep.iter().filter_map(|id| scene.node(*id)).filter_map(|n| if let ObjectKind::Camera { target, .. } = &n.object { *target } else { None }).collect();
    keep.extend(targets);
    let mut ancestors = Vec::new();
    for id in &keep {
        let mut cur = scene.node(*id).and_then(|n| n.parent);
        while let Some(p) = cur {
            ancestors.push(p);
            cur = scene.node(p).and_then(|n| n.parent);
        }
    }
    keep.extend(ancestors);
    keep
}

fn selected_document(state: &AppState) -> Result<Document, String> {
    if state.selection.is_empty() {
        return Err("select objects to save".into());
    }
    let keep = saved_nodes(state);
    let mut scene = state.scene.clone();
    let mut gone: Vec<NodeId> = scene.nodes().map(|n| n.id).filter(|id| !keep.contains(id)).collect();
    gone.sort_by_key(|id| std::cmp::Reverse(scene.depth(*id)));
    for id in gone {
        scene.remove_node(id);
    }
    // References to objects left behind: instances become copies, picks are dropped.
    for id in keep.iter().copied() {
        let resolved: Vec<Option<loom_scene::Modifier>> = state.scene.node(id).map(|n| (0..n.modifiers.len()).map(|i| state.scene.resolved_modifier(id, i)).collect()).unwrap_or_default();
        let Some(node) = scene.node_mut(id) else { continue };
        if node.instance_of.is_some_and(|s| !keep.contains(&s)) {
            node.instance_of = None;
        }
        for (i, slot) in node.modifiers.iter_mut().enumerate() {
            if slot.instance_of.is_some_and(|(s, _)| !keep.contains(&s)) {
                if let Some(Some(m)) = resolved.get(i) {
                    slot.modifier = m.clone();
                }
                slot.instance_of = None;
            }
        }
        node.remap_ids(&|n| keep.contains(&n).then_some(n));
    }
    scene.ik_chains.retain(|c| [c.start, c.end, c.goal].iter().all(|n| keep.contains(n)));
    let mut settings = SceneSettings::from_state(state);
    settings.selection.retain(|id| keep.contains(id));
    settings.named_sets.values_mut().for_each(|v| v.retain(|id| keep.contains(id)));
    settings.named_sets.retain(|_, v| !v.is_empty());
    settings.named_sub_selections.values_mut().for_each(|v| v.retain(|(id, _)| keep.contains(id)));
    settings.named_sub_selections.retain(|_, v| !v.is_empty());
    Ok(Document { format: FORMAT, app: format!("3dloom {}", env!("CARGO_PKG_VERSION")), scene, settings })
}

/// Every existing image file the scene names (material maps, Show Map in
/// Viewport bitmaps, viewport backgrounds), in first-use order.
pub fn scene_bitmaps(state: &AppState) -> Vec<PathBuf> {
    const EXT: [&str; 12] = ["png", "jpg", "jpeg", "tga", "bmp", "tif", "tiff", "dds", "hdr", "exr", "gif", "psd"];
    fn walk(v: &Value, out: &mut Vec<PathBuf>) {
        match v {
            Value::String(s) => {
                let p = PathBuf::from(s);
                let image = p.extension().and_then(|e| e.to_str()).is_some_and(|e| EXT.contains(&e.to_ascii_lowercase().as_str()));
                if image && p.is_file() && !out.contains(&p) {
                    out.push(p);
                }
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => o.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(&serde_json::to_value(&state.scene).unwrap_or(Value::Null), &mut out);
    walk(&serde_json::to_value(SceneSettings::from_state(state)).unwrap_or(Value::Null), &mut out);
    out
}

/// Archive: a .zip with the scene and its bitmaps, or (.txt) the list of files.
fn archive(state: &AppState, path: &Path) -> Result<Value, String> {
    let bitmaps = scene_bitmaps(state);
    let stem = state.file.as_ref().and_then(|f| f.file_stem()).or_else(|| path.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
    let doc = Document { format: FORMAT, app: format!("3dloom {}", env!("CARGO_PKG_VERSION")), scene: state.scene.clone(), settings: SceneSettings::from_state(state) };
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt")) {
        let mut text = String::new();
        let scene = state.file.as_ref().map(|f| f.display().to_string()).unwrap_or_else(|| format!("{stem}.loom (unsaved)"));
        text.push_str(&scene);
        text.push('\n');
        for b in &bitmaps {
            text.push_str(&b.display().to_string());
            text.push('\n');
        }
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        return Ok(json!({ "path": path, "files": bitmaps.len() + 1 }));
    }
    let scene_text = serde_json::to_vec_pretty(&doc).map_err(|e| e.to_string())?;
    let mut entries: Vec<(String, Vec<u8>)> = vec![(format!("{stem}.loom"), scene_text)];
    for b in &bitmaps {
        let data = std::fs::read(b).map_err(|e| format!("{}: {e}", b.display()))?;
        let file = b.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "map".into());
        let mut name = format!("maps/{file}");
        let mut n = 1;
        while entries.iter().any(|(e, _)| *e == name) {
            name = format!("maps/{n}_{file}");
            n += 1;
        }
        entries.push((name, data));
    }
    let bytes = zip(&entries)?;
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(json!({ "path": path, "files": entries.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>() }))
}

/// A zip archive of deflated entries.
pub fn zip(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    use std::io::Write;
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    // 1980-01-01 00:00 (DOS date / time).
    let (time, date) = (0u16, 0x21u16);
    for (name, data) in entries {
        let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(data).map_err(|e| e.to_string())?;
        let packed = enc.finish().map_err(|e| e.to_string())?;
        let crc = crc32fast::hash(data);
        let offset = out.len() as u32;
        let header = |sig: u32, central: bool| {
            let mut h: Vec<u8> = Vec::new();
            h.extend(sig.to_le_bytes());
            if central {
                h.extend(20u16.to_le_bytes()); // made by
            }
            h.extend(20u16.to_le_bytes()); // needed
            h.extend(0x0800u16.to_le_bytes()); // UTF-8 names
            h.extend(8u16.to_le_bytes()); // deflate
            h.extend(time.to_le_bytes());
            h.extend(date.to_le_bytes());
            h.extend(crc.to_le_bytes());
            h.extend((packed.len() as u32).to_le_bytes());
            h.extend((data.len() as u32).to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes()); // extra
            if central {
                h.extend(0u16.to_le_bytes()); // comment
                h.extend(0u16.to_le_bytes()); // disk
                h.extend(0u16.to_le_bytes()); // internal attributes
                h.extend(0u32.to_le_bytes()); // external attributes
                h.extend(offset.to_le_bytes());
            }
            h.extend(name.as_bytes());
            h
        };
        out.extend(header(0x0403_4b50, false));
        out.extend(&packed);
        central.extend(header(0x0201_4b50, true));
    }
    let cd_offset = out.len() as u32;
    let cd_len = central.len() as u32;
    out.extend(central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend(cd_len.to_le_bytes());
    out.extend(cd_offset.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    Ok(out)
}

/// Summary Info: scene totals by object type, mesh totals and the materials.
pub fn summary_info(state: &AppState) -> Value {
    let scene = &state.scene;
    let (mut objects, mut shapes, mut cameras, mut helpers, mut groups) = (0, 0, 0, 0, 0);
    let (mut vertices, mut faces) = (0usize, 0usize);
    for n in scene.nodes() {
        match &n.object {
            ObjectKind::Group { .. } => groups += 1,
            ObjectKind::Camera { .. } => cameras += 1,
            k if k.is_shape() => shapes += 1,
            k if k.is_helper() => helpers += 1,
            _ => {
                objects += 1;
                let e = scene.eval_mesh(n.id);
                vertices += e.value.mesh.vertex_count();
                faces += e.value.mesh.face_count();
            }
        }
    }
    let materials: Vec<String> = scene.nodes().filter_map(|n| scene.node_material(n.id)).map(|m| m.name.clone()).collect::<BTreeSet<_>>().into_iter().collect();
    json!({
        "file": state.file.as_ref().map(|f| f.display().to_string()),
        "objects": objects,
        "shapes": shapes,
        "cameras": cameras,
        "helpers": helpers,
        "groups": groups,
        "total": scene.len(),
        "vertices": vertices,
        "faces": faces,
        "materials": materials,
        "layers": scene.layers.len(),
        "frame_range": state.frame_range,
        "description": state.file_properties.description,
    })
}
