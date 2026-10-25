//! FBX ↔ scene: `import` adds nodes (one undo entry), `export` writes the
//! given nodes with their hierarchy, both by the FBX Importer / Exporter
//! options in `AppState::exchange`.

use crate::execute::unique_name;
use crate::exchange::{FileContent, UpAxis};
use crate::state::{AppState, SelectionMode};
use glam::Mat4;
use loom_fbx::{FbxBlendShape, FbxDocument, FbxKind, FbxNode, FbxSkin, WriteOpts};
use loom_scene::{Change, Modifier, ModifierSlot, Node, NodeId, ObjectKind, Transform, OBJECT_COLORS};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

pub fn is_fbx(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("fbx"))
}

/// Metres per scene unit (Units Setup).
fn unit_metres(state: &AppState) -> f64 {
    state.units.system_unit.metres() * state.units.system_scale as f64
}

pub fn import(state: &mut AppState, path: &Path) -> Result<Vec<NodeId>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let o = state.exchange.fbx_import.clone();
    // Automatic: into the system unit; else the file's numbers are taken as `units`.
    let mut ro = loom_fbx::ReadOpts::new(if o.units_automatic { unit_metres(state) } else { o.units.metres() });
    ro.smoothing_groups = o.smoothing_groups;
    ro.animation = o.animation;
    ro.take = o.take.clone();
    ro.bake_layers = o.bake_animation_layers;
    ro.skins = o.deformations && o.skins;
    ro.morphs = o.deformations && o.morphs;
    let mut doc = loom_fbx::read_with(&bytes, &ro).map_err(|e| format!("{}: {e}", path.display()))?;
    // Cameras / Lights off: those nodes come in as plain empties (dropped unless they hold children).
    for n in &mut doc.nodes {
        if (!o.cameras && matches!(n.kind, FbxKind::Camera { .. })) || (!o.lights && matches!(n.kind, FbxKind::Light(_))) {
            n.kind = FbxKind::Empty;
        }
    }
    // Empties without children are dropped; their children go to the grandparent.
    let has_children: Vec<bool> = (0..doc.nodes.len()).map(|i| doc.nodes.iter().any(|n| n.parent == Some(i))).collect();
    // Bones always stay (a skeleton's leaf joints have no children).
    let keep: Vec<bool> = doc.nodes.iter().enumerate().map(|(i, n)| !matches!(n.kind, FbxKind::Empty) || has_children[i]).collect();
    if !keep.iter().any(|k| *k) {
        return Err(format!("{}: nothing to import", path.display()));
    }
    // File content: Add and update animation / Update animation match scene nodes by name.
    let by_name: BTreeMap<String, NodeId> = state.scene.nodes().map(|n| (n.name.clone(), n.id)).collect();
    let existing: Vec<Option<NodeId>> = doc.nodes.iter().map(|n| if o.file_content == FileContent::Add { None } else { by_name.get(&n.name).copied() }).collect();
    let add: Vec<bool> = (0..doc.nodes.len()).map(|i| keep[i] && existing[i].is_none() && o.file_content != FileContent::UpdateAnimation).collect();
    let first = state.scene.next_id();
    let mut taken: std::collections::BTreeSet<String> = state.scene.nodes().map(|n| n.name.clone()).collect();
    // Ids are known up front (a skin may name bones that come later in the file).
    let mut id_of: BTreeMap<usize, NodeId> = (0..doc.nodes.len()).filter(|i| add[*i]).enumerate().map(|(k, i)| (i, NodeId(first + k as u64))).collect();
    for (i, e) in existing.iter().enumerate() {
        if let Some(id) = e {
            id_of.insert(i, *id);
        }
    }
    // A bone points at its child joint: FBX skeletons along +Y, Loom bones along +X.
    let mut bone_axis: Vec<u32> = vec![0; doc.nodes.len()];
    for (i, n) in doc.nodes.iter().enumerate() {
        if matches!(n.kind, FbxKind::Bone { .. }) {
            let child = doc.nodes.iter().find(|c| c.parent == Some(i) && c.translation.length() > 1e-4).map(|c| c.translation);
            bone_axis[i] = match child {
                Some(t) => loom_scene::node::bone_axis_of(t),
                None => n.parent.filter(|p| matches!(doc.nodes[*p].kind, FbxKind::Bone { .. })).map(|p| bone_axis[p]).unwrap_or(0),
            };
        }
    }
    let filters = curve_filters(o.constant_key_reducer, [o.translation_precision, o.rotation_precision, o.scaling_precision, o.other_precision], o.key_reducer, o.key_reducer_precision, o.xyz_curve_sync);
    let track = |i: usize| doc.anim.as_ref().and_then(|t| t.tracks.iter().find(|(ti, _)| *ti == i)).map(|(_, a)| a);
    let mut changes = Vec::new();
    // The file's display layers: the scene's layer of that name ("0" is layer 0), else a new one.
    let mut layers = state.scene.layers.clone();
    let mut layer_of: BTreeMap<usize, u32> = BTreeMap::new();
    for fl in &doc.layers {
        let lid = match layers.iter().find(|l| l.name == fl.name || (fl.name == "0" && l.id == 0)) {
            Some(l) => l.id,
            None => {
                let id = layers.iter().map(|l| l.id + 1).max().unwrap_or(1);
                let mut l = loom_scene::Layer::new(id, fl.name.clone());
                l.color = (fl.color * 255.0).round().clamp(glam::Vec3::ZERO, glam::Vec3::splat(255.0)).to_array().map(|v| v as u8);
                l.hidden = !fl.visible;
                l.frozen = fl.frozen;
                layers.push(l);
                id
            }
        };
        for ni in &fl.nodes {
            layer_of.insert(*ni, lid);
        }
    }
    if layers != state.scene.layers {
        changes.push(Change::SetLayers { before: (state.scene.layers.clone(), state.scene.current_layer), after: (layers, state.scene.current_layer) });
    }
    // Materials: each file material once in the library, by name.
    let mut library = state.scene.materials.clone();
    let material_ids = import_materials(&mut library, &doc.materials, path);
    if library != state.scene.materials {
        let slots = state.scene.material_editor.clone();
        changes.push(Change::SetMaterials { before: Box::new((state.scene.materials.clone(), slots.clone())), after: Box::new((library, slots)) });
    }
    let instance_of: BTreeMap<usize, usize> = doc.instances.iter().copied().collect();
    let mut ids = Vec::new();
    for (i, n) in doc.nodes.iter().enumerate() {
        if let Some(id) = existing[i] {
            // Update animation: the file's take replaces the node's transform controllers.
            if let (Some(a), Some(node)) = (track(i), state.scene.node(id)) {
                let after = Transform { position: sampled(&a.frames, &a.translation, &filters, 0), rotation: sampled(&a.frames, &a.rotation_deg, &filters, 1), scale: sampled(&a.frames, &a.scale, &filters, 2) };
                changes.push(Change::SetTransform { id, before: node.transform.clone(), after });
            }
            continue;
        }
        if !add[i] {
            continue;
        }
        let base = if n.name.trim().is_empty() { "Object" } else { n.name.as_str() };
        let name = if taken.contains(base) { unique_name(base, &taken) } else { base.to_owned() };
        taken.insert(name.clone());
        let object = match &n.kind {
            FbxKind::Mesh { mesh, .. } => ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(mesh.clone())),
            FbxKind::Camera { fov_deg } => ObjectKind::camera_object(*fov_deg),
            FbxKind::Light(l) => ObjectKind::Light(Box::new(light_from_fbx(l))),
            FbxKind::Bone { length, .. } if o.bones_as_dummies => ObjectKind::dummy(length.max(0.001) * 0.25),
            FbxKind::Bone { length, size } => {
                let mut b = ObjectKind::bone(length.max(0.001));
                // Bone creation Width / Height / Taper where the file gives no size.
                let (w, h) = if *size > 0.0 { (*size, *size) } else { (o.bone_width, if o.lock_width_height { o.bone_width } else { o.bone_height }) };
                let _ = b.set_parameter("Width", w as f64);
                let _ = b.set_parameter("Height", h as f64);
                if *size <= 0.0 {
                    let _ = b.set_parameter("Taper", (o.bone_taper / 100.0) as f64);
                }
                let _ = b.set_parameter("Bone Axis", bone_axis[i] as f64);
                b
            }
            FbxKind::Empty => ObjectKind::Group { open: true },
        };
        let mut node = Node::new(&name, object);
        if matches!(n.kind, FbxKind::Bone { .. }) {
            node.wire_color = [180, 180, 100];
        }
        if matches!(n.kind, FbxKind::Light(_)) {
            node.wire_color = crate::light::LIGHT_COLOR;
        }
        if let FbxKind::Mesh { materials, .. } = &n.kind {
            node.materials = materials.clone();
            // One file material: assigned; several stay named per material id, each in the library.
            if let [only] = materials.as_slice() {
                node.material = material_ids.get(only).copied();
            }
        }
        // Constants: importing at a later frame with Auto Key on does not key.
        node.transform = Transform { position: loom_scene::Param::new(n.translation), rotation: loom_scene::Param::new(n.rotation_deg), scale: loom_scene::Param::new(n.scale) };
        // The file's take: a Linear key per frame on the components that move.
        if let Some(a) = track(i) {
            node.transform = Transform { position: sampled(&a.frames, &a.translation, &filters, 0), rotation: sampled(&a.frames, &a.rotation_deg, &filters, 1), scale: sampled(&a.frames, &a.scale, &filters, 2) };
        }
        // Parent: the nearest kept ancestor.
        let mut p = n.parent;
        while let Some(pi) = p {
            if id_of.contains_key(&pi) {
                break;
            }
            p = doc.nodes[pi].parent;
        }
        node.parent = p.and_then(|pi| id_of.get(&pi).copied());
        node.wire_color = OBJECT_COLORS[((first as usize) + ids.len()) % OBJECT_COLORS.len()];
        node.layer = layer_of.get(&i).copied().unwrap_or(state.scene.current_layer);
        let id = NodeId(first + ids.len() as u64);
        node.id = id;
        // An FBX instance shares the first node's object.
        node.instance_of = instance_of.get(&i).and_then(|src| id_of.get(src)).copied().filter(|src| src.0 >= first);
        // Morpher below Skin (FBX order); both built now that every id is known.
        if !n.blend_shapes.is_empty() {
            if let FbxKind::Mesh { mesh, .. } = &n.kind {
                let mut data = loom_scene::morph::MorpherData::default();
                for (k, bs) in n.blend_shapes.iter().enumerate() {
                    let mut offsets = vec![glam::Vec3::ZERO; mesh.vertex_count()];
                    for (v, o) in &bs.offsets {
                        if let Some(slot) = offsets.get_mut(*v as usize) {
                            *slot = *o;
                        }
                    }
                    let ch = data.channel(k);
                    ch.name = bs.name.clone();
                    ch.targets = vec![loom_scene::morph::MorphTarget { node: None, offsets, percent: 100.0 }];
                    ch.value.set(bs.weight);
                    ch.valid = true;
                }
                node.modifiers.push(ModifierSlot::new(Modifier::Morpher(Box::new(data))));
            }
        }
        if let Some(skin) = &n.skin {
            let mut bones: Vec<(NodeId, glam::Mat4, loom_scene::skin::Envelope)> = Vec::new();
            let mut remap: Vec<Option<u32>> = Vec::new();
            for (bi, g2b) in &skin.bones {
                match id_of.get(bi) {
                    Some(bid) => {
                        let (len, axis) = match doc.nodes[*bi].kind { FbxKind::Bone { length, .. } => (length.max(0.001), loom_scene::node::bone_axis_dir(bone_axis[*bi])), _ => (1.0, glam::Vec3::X) };
                        remap.push(Some(bones.len() as u32));
                        bones.push((*bid, *g2b, loom_scene::skin::Envelope { start: glam::Vec3::ZERO, end: axis * len, inner: [len * 0.25; 2], outer: [len * 0.5; 2], falloff: 0, abs_effect: 1.0, squash: 1.0 }));
                    }
                    None => remap.push(None),
                }
            }
            if !bones.is_empty() {
                let weights: Vec<Vec<(u32, f32)>> = skin.weights.iter().map(|w| w.iter().filter_map(|(b, x)| remap.get(*b as usize).copied().flatten().map(|nb| (nb, *x))).collect()).collect();
                node.modifiers.push(ModifierSlot::new(Modifier::Skin(Box::new(loom_scene::skin::SkinData::from_file(&bones, skin.mesh_bind, weights)))));
            }
        }
        // A mesh written as a smooth surface's cage gets its TurboSmooth back.
        if let Some((_, preview, render)) = doc.subdivision.iter().find(|s| s.0 == i) {
            let mut m = Modifier::by_name("TurboSmooth").expect("TurboSmooth is a modifier");
            if let Modifier::TurboSmooth { iterations, render_iters, render_iters_on, .. } = &mut m {
                iterations.set((*preview).max(1));
                render_iters.set((*render).max(1));
                render_iters_on.set(*render != *preview);
            }
            node.modifiers.push(ModifierSlot::new(m));
        }
        ids.push(id);
        changes.push(Change::AddNode(node));
    }
    if changes.is_empty() {
        return Err(format!("{}: no objects matched the scene to update", path.display()));
    }
    let label = format!("Import {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    crate::execute::commit_exact(state, &label, Change::Group(changes))?;
    // Selection sets become named selection sets.
    for (name, members) in &doc.selection_sets {
        let set: Vec<NodeId> = members.iter().filter_map(|m| id_of.get(m).copied()).collect();
        if !set.is_empty() {
            state.named_sets.insert(name.clone(), set);
        }
    }
    // Fill timeline: a file with a take sets the active time segment and frame rate to it.
    if let Some(t) = doc.anim.as_ref().filter(|t| !t.tracks.is_empty() && o.fill_timeline) {
        state.frame_range = (t.start, t.end.max(t.start + 1));
        if t.fps > 0.0 {
            state.fps = t.fps;
        }
        let f = state.current_frame.clamp(state.frame_range.0, state.frame_range.1);
        crate::execute::set_frame(state, f)?;
    }
    if !ids.is_empty() {
        state.selection = ids.iter().copied().collect();
        state.sub_selection.clear();
        state.selection_mode = SelectionMode::Object;
    }
    Ok(ids)
}

/// The dialog's curve filter settings as the FBX module takes them.
fn curve_filters(constant: bool, precision: [f32; 4], reducer: bool, reducer_precision: f32, sync: bool) -> loom_fbx::CurveFilters {
    loom_fbx::CurveFilters { constant: constant.then_some(precision), key_reducer: reducer.then_some((reducer_precision, sync)), remove_single_key: false }
}

/// Where an FBX texture's image is: embedded bytes are written to
/// `<file>.fbm/` next to the FBX (as the reference extracts them); else the
/// absolute path, the relative path or the bare file name beside the FBX,
/// whichever exists (else the path as written).
fn resolve_texture(t: &loom_fbx::FbxTexture, fbx: &Path) -> String {
    let dir = fbx.parent().unwrap_or(Path::new("."));
    let file_name = |p: &str| Path::new(&p.replace('\\', "/")).file_name().map(|f| f.to_string_lossy().into_owned());
    if let Some(bytes) = &t.content {
        let name = file_name(&t.path).or_else(|| file_name(&t.relative)).unwrap_or_else(|| "texture.png".into());
        let fbm = dir.join(format!("{}.fbm", fbx.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()));
        let out = fbm.join(&name);
        if std::fs::create_dir_all(&fbm).is_ok() && (out.exists() || std::fs::write(&out, bytes).is_ok()) {
            return out.to_string_lossy().into_owned();
        }
        let tmp = std::env::temp_dir().join(&name);
        if std::fs::write(&tmp, bytes).is_ok() {
            return tmp.to_string_lossy().into_owned();
        }
    }
    let candidates = [Some(std::path::PathBuf::from(&t.path)), (!t.relative.is_empty()).then(|| dir.join(t.relative.replace('\\', "/"))), file_name(&t.path).map(|n| dir.join(n))];
    candidates.into_iter().flatten().find(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| t.path.clone())
}

/// An FBX material as a Standard material: colours, Specular Level from the
/// specular factor, Glossiness from the Phong exponent (10·log2), Opacity,
/// Self-Illumination and the Diffuse / Specular Level / Opacity / Bump maps.
pub fn material_from_fbx(m: &loom_fbx::FbxMaterial, fbx: &Path) -> loom_scene::material::StandardMaterial {
    use loom_scene::material::{BitmapMap, StandardMaterial};
    let mut s = StandardMaterial::new(m.name.clone());
    let v = |c: [f32; 3]| glam::Vec3::from(c).clamp(glam::Vec3::ZERO, glam::Vec3::ONE);
    s.diffuse.set(v(m.diffuse));
    s.ambient.set(v(m.ambient));
    s.ambient_locked = v(m.ambient) == v(m.diffuse);
    s.specular.set(v(m.specular));
    s.specular_level.set((m.specular_factor * 100.0).clamp(0.0, 999.0));
    s.glossiness.set((10.0 * m.shininess.max(1.0).log2()).clamp(0.0, 100.0));
    s.opacity.set((m.opacity * 100.0).clamp(0.0, 100.0));
    s.self_illum.set((m.emissive_factor * 100.0).clamp(0.0, 100.0));
    for t in &m.textures {
        let slot = match t.property.as_str() {
            "DiffuseColor" => &mut s.diffuse_map,
            "SpecularFactor" | "SpecularColor" => &mut s.specular_level_map,
            "TransparentColor" | "TransparencyFactor" => &mut s.opacity_map,
            "Bump" | "NormalMap" => &mut s.bump_map,
            _ => continue,
        };
        if slot.is_none() {
            let amount = if matches!(t.property.as_str(), "Bump" | "NormalMap") { (t.amount * 100.0).clamp(-999.0, 999.0) } else { 100.0 };
            *slot = Some(BitmapMap { path: resolve_texture(t, fbx), amount, enabled: true });
        }
    }
    s.show_map_in_viewport = s.diffuse_map.is_some();
    s
}

/// A Standard material as an FBX Phong (see `material_from_fbx`); `embed`
/// reads the map images into the file.
pub fn material_to_fbx(s: &loom_scene::material::StandardMaterial, embed: bool) -> loom_fbx::FbxMaterial {
    let mut m = loom_fbx::FbxMaterial::new(&s.name);
    m.diffuse = s.diffuse.get().to_array();
    m.ambient = s.ambient.get().to_array();
    m.specular = s.specular.get().to_array();
    m.specular_factor = s.specular_level.get() / 100.0;
    m.shininess = 2f32.powf(s.glossiness.get() / 10.0);
    m.opacity = s.opacity.get() / 100.0;
    m.emissive_factor = s.self_illum.get() / 100.0;
    for (property, map) in [("DiffuseColor", &s.diffuse_map), ("SpecularFactor", &s.specular_level_map), ("TransparentColor", &s.opacity_map), ("Bump", &s.bump_map)] {
        let Some(map) = map.as_ref().filter(|b| b.enabled && !b.path.is_empty()) else { continue };
        let amount = if property == "Bump" { map.amount / 100.0 } else { 1.0 };
        let content = if embed { std::fs::read(&map.path).ok() } else { None };
        m.textures.push(loom_fbx::FbxTexture { property: property.into(), path: map.path.clone(), relative: String::new(), amount, content });
    }
    m
}

/// Adds the file's materials to `library` (one per name, reusing a library
/// material of that name) and returns name → id.
fn import_materials(library: &mut loom_scene::material::MaterialLibrary, materials: &[loom_fbx::FbxMaterial], fbx: &Path) -> BTreeMap<String, loom_scene::material::MaterialId> {
    let mut out = BTreeMap::new();
    for m in materials {
        if out.contains_key(&m.name) {
            continue;
        }
        let id = match library.iter().find(|(_, x)| x.name == m.name) {
            Some((id, _)) => *id,
            None => {
                let id = loom_scene::material::next_material_id(library);
                library.insert(id, material_from_fbx(m, fbx));
                id
            }
        };
        out.insert(m.name.clone(), id);
    }
    out
}

/// What the FBX Importer's Statistics show, plus the Units scale factor.
pub fn file_info(state: &AppState, path: &Path) -> Result<Value, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let i = loom_fbx::info(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let o = &state.exchange.fbx_import;
    let target = if o.units_automatic { unit_metres(state) } else { o.units.metres() };
    let unit_name = |m: f64| crate::state::SystemUnit::ALL.into_iter().find(|u| (u.metres() - m).abs() < m * 1e-6).map(|u| u.label().to_owned()).unwrap_or_else(|| format!("{m} m"));
    Ok(json!({
        "file_name": path.file_name().map(|n| n.to_string_lossy().into_owned()),
        "file_directory": path.parent().map(|d| d.to_string_lossy().into_owned()),
        "file_version": format!("{}.{}", i.version / 1000, i.version % 1000 / 100),
        "ascii": i.ascii,
        "file_creator": i.creator,
        "file_creation_time": i.creation_time,
        "file_axis_direction": i.up_axis,
        "file_units": unit_name(i.unit_metres),
        "system_axis_direction": "Z-up",
        "system_units": unit_name(unit_metres(state)),
        "scale_factor": i.unit_metres / target,
        "file_fps": i.fps,
        "system_fps": state.fps,
        "takes": i.takes,
        "elements": i.nodes,
        "meshes": i.meshes,
        "materials": i.materials,
        "textures": i.textures,
        "cameras": i.cameras,
        "lights": i.lights,
        "caches": i.caches,
    }))
}
/// The bones a node's Skin references, and their ancestors (the skeleton travels with the mesh).
fn skin_bones_with_ancestors(state: &AppState, id: NodeId) -> Vec<NodeId> {
    let Some(node) = state.scene.node(id) else { return Vec::new() };
    let mut out = Vec::new();
    for slot in &node.modifiers {
        if let Modifier::Skin(d) = &slot.modifier {
            for b in &d.bones {
                out.push(b.node);
                out.extend(state.scene.ancestors(b.node));
            }
        }
    }
    out
}

pub fn export(state: &AppState, ids: &[NodeId], path: &Path) -> Result<Value, String> {
    let o = state.exchange.fbx_export.clone();
    let skins = o.deformations && o.skins;
    let morphs = o.deformations && o.morphs;
    let mut set: std::collections::BTreeSet<NodeId> = ids.iter().copied().collect();
    // The bones the exported skins deform with (Convert deforming dummies to Bones).
    let mut deforming: std::collections::BTreeSet<NodeId> = std::collections::BTreeSet::new();
    if skins {
        for id in ids {
            let bones = skin_bones_with_ancestors(state, *id);
            if let Some(n) = state.scene.node(*id) {
                for slot in &n.modifiers {
                    if let Modifier::Skin(d) = &slot.modifier {
                        deforming.extend(d.bones.iter().map(|b| b.node));
                    }
                }
            }
            set.extend(bones);
        }
    }
    let ids: Vec<NodeId> = set.iter().copied().collect();
    let ids = &ids[..];
    // Parents before children.
    let mut order: Vec<NodeId> = Vec::new();
    let mut pending: Vec<NodeId> = ids.to_vec();
    while !pending.is_empty() {
        let before = pending.len();
        pending.retain(|id| {
            let parent_in = state.scene.node(*id).and_then(|n| n.parent).filter(|p| set.contains(p));
            if parent_in.is_none_or(|p| order.contains(&p)) {
                order.push(*id);
                false
            } else {
                true
            }
        });
        if pending.len() == before {
            order.extend(pending.drain(..));
        }
    }
    // Indices first (a skin names bones that may come later in the order), nodes second.
    let exported: Vec<NodeId> = order
        .iter()
        .copied()
        .filter(|id| o.cameras || !state.scene.node(*id).is_some_and(|n| matches!(n.object, ObjectKind::Camera { .. })))
        .filter(|id| o.lights || !state.scene.node(*id).is_some_and(|n| matches!(n.object, ObjectKind::Light(_))))
        .filter(|id| {
            state.scene.node(*id).is_some_and(|n| match &n.object {
                ObjectKind::Camera { .. } | ObjectKind::Light(_) | ObjectKind::Group { .. } | ObjectKind::Bone { .. } | ObjectKind::Dummy { .. } | ObjectKind::Point { .. } | ObjectKind::Tape { .. } | ObjectKind::Protractor { .. } | ObjectKind::Compass { .. } | ObjectKind::Grid { .. } | ObjectKind::IkGoal { .. } | ObjectKind::Biped(_) => true,
                _ => state.scene.eval_mesh(*id).value.mesh.face_count() > 0 || n.object.display_output().face_count() > 0,
            })
        })
        .collect();
    let index_of: BTreeMap<NodeId, usize> = exported.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let mut nodes: Vec<FbxNode> = Vec::new();
    let mut subdivision: Vec<(usize, u32, u32)> = Vec::new();
    let exported_ids = exported.clone();
    for id in exported {
        let Some(node) = state.scene.node(id) else { continue };
        let index = nodes.len();
        let mut skin: Option<FbxSkin> = None;
        let mut blend_shapes: Vec<FbxBlendShape> = Vec::new();
        let kind = match &node.object {
            ObjectKind::Camera { fov, .. } => FbxKind::Camera { fov_deg: fov.get() },
            ObjectKind::Light(l) => match light_to_fbx(l) {
                Some(light) => {
                    // A target light's aim comes from its LookAt constraint: write the evaluated rotation.
                    let parent = node.parent.map(|p| state.scene.world_matrix(p)).unwrap_or(Mat4::IDENTITY);
                    let local = Transform::from_matrix(parent.inverse() * state.scene.world_matrix(id));
                    nodes.push(FbxNode { name: node.name.clone(), parent: node.parent.filter(|p| set.contains(p)).and_then(|p| index_of.get(&p).copied()), translation: node.transform.position.get(), rotation_deg: local.rotation.get(), scale: node.transform.scale.get(), kind: FbxKind::Light(light), skin: None, blend_shapes: Vec::new() });
                    continue;
                }
                None => FbxKind::Empty,
            },
            ObjectKind::Group { .. } => FbxKind::Empty,
            ObjectKind::Bone { length, width, height, .. } => FbxKind::Bone { length: length.get(), size: width.get().max(height.get()) },
            ObjectKind::Dummy { .. } | ObjectKind::Point { .. } | ObjectKind::Tape { .. } | ObjectKind::Protractor { .. } | ObjectKind::Compass { .. } | ObjectKind::Grid { .. } | ObjectKind::IkGoal { .. } => FbxKind::Empty,
            // A biped's centre of mass is the skeleton's root joint.
            ObjectKind::Biped(b) => FbxKind::Bone { length: 0.0, size: loom_scene::biped::root_size(b.height.get()) },
            _ => {
                // A skinned node writes the mesh below its Skin (and Morpher) with the deformers as FBX deformers.
                let skin_index = node.modifiers.iter().position(|s| matches!(s.modifier, Modifier::Skin(_))).filter(|_| skins);
                let morph_index = node.modifiers.iter().position(|s| matches!(s.modifier, Modifier::Morpher(_))).filter(|_| morphs);
                // TurboSmooth: a top TurboSmooth is written as its cage with the subdivision levels.
                let turbo = node.modifiers.iter().rposition(|s| s.enabled).filter(|t| o.turbosmooth && matches!(node.modifiers[*t].modifier, Modifier::TurboSmooth { .. }));
                if let Some(Modifier::TurboSmooth { iterations, render_iters, render_iters_on, .. }) = turbo.map(|t| &node.modifiers[t].modifier) {
                    if skin_index.is_none() && morph_index.is_none() {
                        subdivision.push((index, iterations.get(), if render_iters_on.get() { render_iters.get() } else { iterations.get() }));
                    }
                }
                let below = [skin_index, morph_index, turbo].into_iter().flatten().min();
                let mut mesh = match below {
                    Some(0) => node.object.display_output(),
                    Some(i) => state.scene.evaluated(id, Some(i - 1)).value.mesh.clone(),
                    None => state.scene.eval_mesh(id).value.mesh.clone(),
                };
                if mesh.face_count() == 0 {
                    nodes.push(FbxNode { name: node.name.clone(), parent: node.parent.filter(|p| set.contains(p)).and_then(|p| index_of.get(&p).copied()), translation: node.transform.position.get(), rotation_deg: node.transform.rotation.get(), scale: node.transform.scale.get(), kind: FbxKind::Empty, skin: None, blend_shapes: Vec::new() });
                    continue;
                }
                if let Some(si) = skin_index {
                    if let Modifier::Skin(d) = &node.modifiers[si].modifier {
                        let bone_indices: Vec<Option<usize>> = d.bones.iter().map(|b| index_of.get(&b.node).copied()).collect();
                        // The written geometry has the object offset baked in, so the binding
                        // maps it back out (the node world at bind is mesh_bind · offset⁻¹).
                        let node_bind = d.mesh_bind * node.object_offset.matrix().inverse();
                        let bones: Vec<(usize, Mat4)> = d.bones.iter().zip(&bone_indices).filter_map(|(b, i)| i.map(|i| (i, b.bind_inverse * node_bind))).collect();
                        let remap: Vec<Option<u32>> = bone_indices.iter().scan(0u32, |k, i| Some(i.map(|_| { let v = *k; *k += 1; v }))).collect();
                        let weights: Vec<Vec<(u32, f32)>> = (0..mesh.vertex_count())
                            .map(|v| {
                                let p = d.mesh_bind.transform_point3(mesh.vertices()[v].position);
                                d.vertex_weights(v, p).into_iter().filter_map(|(b, x)| remap.get(b as usize).copied().flatten().map(|nb| (nb, x))).collect()
                            })
                            .collect();
                        skin = Some(FbxSkin { bones, mesh_bind: node_bind, weights });
                    }
                }
                if let Some(mi) = morph_index {
                    if let Modifier::Morpher(d) = &node.modifiers[mi].modifier {
                        for ch in d.channels.iter().filter(|c| c.valid && !c.targets.is_empty()) {
                            let full = ch.targets.iter().max_by(|a, b| a.percent.total_cmp(&b.percent)).unwrap();
                            if full.offsets.len() != mesh.vertex_count() {
                                continue;
                            }
                            let offsets: Vec<(u32, glam::Vec3)> = full.offsets.iter().enumerate().filter(|(_, o)| o.length_squared() > 1e-12).map(|(v, o)| (v as u32, *o)).collect();
                            blend_shapes.push(FbxBlendShape { name: ch.name.clone(), offsets, weight: ch.value.get() });
                        }
                    }
                }
                // The object offset is baked into the geometry so the node is the pivot.
                let off = node.object_offset.matrix();
                if off != Mat4::IDENTITY {
                    for v in mesh.vertices_mut() {
                        v.position = off.transform_point3(v.position);
                    }
                }
                // An assigned material covers every face, as one FBX material.
                let assigned = node.material.and_then(|m| state.scene.materials.get(&m));
                let materials = match assigned {
                    Some(m) => {
                        for f in mesh.faces_mut() {
                            f.material_id = 1;
                        }
                        vec![m.name.clone()]
                    }
                    None => loom_fbx::material_names(&mesh, &node.materials),
                };
                FbxKind::Mesh { materials, mesh }
            }
        };
        // A helper or object a skin deforms with is written as a bone.
        let kind = if o.convert_deforming_dummies_to_bones && deforming.contains(&id) && !matches!(kind, FbxKind::Bone { .. }) {
            let size = match &node.object {
                ObjectKind::Dummy { size } => size.get(),
                _ => {
                    let (lo, hi) = state.scene.eval_mesh(id).value.mesh.aabb();
                    (hi - lo).max_element().max(1.0)
                }
            };
            skin = None;
            blend_shapes.clear();
            FbxKind::Bone { length: size, size: size * 0.25 }
        } else {
            kind
        };
        let parent = node.parent.filter(|p| set.contains(p)).and_then(|p| index_of.get(&p).copied());
        let (translation, rotation_deg, scale) = if parent.is_some() {
            (node.transform.position.get(), node.transform.rotation.get(), node.transform.scale.get())
        } else {
            let t = Transform::from_matrix(state.scene.world_matrix(id));
            (t.position.get(), t.rotation.get(), t.scale.get())
        };
        nodes.push(FbxNode { name: node.name.clone(), parent, translation, rotation_deg, scale, kind, skin, blend_shapes });
    }
    if nodes.is_empty() {
        return Err("nothing to export: no geometry objects".into());
    }
    let count = nodes.len();
    let index_of: BTreeMap<NodeId, usize> = exported_ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    // Preserve Instances: instances whose written meshes match share one geometry.
    let mut instances: Vec<(usize, usize)> = Vec::new();
    if o.preserve_instances {
        let mut first_of: BTreeMap<NodeId, usize> = BTreeMap::new();
        for (i, id) in exported_ids.iter().enumerate() {
            let Some(n) = state.scene.node(*id) else { continue };
            let root = n.instance_of.unwrap_or(*id);
            match first_of.get(&root) {
                Some(j) => {
                    let same = match (&nodes[i].kind, &nodes[*j].kind) {
                        (FbxKind::Mesh { mesh: a, materials: ma }, FbxKind::Mesh { mesh: b, materials: mb }) => {
                            ma == mb && a.face_count() == b.face_count() && a.vertex_count() == b.vertex_count() && a.vertices().iter().zip(b.vertices()).all(|(x, y)| x.position.distance(y.position) < 1e-6)
                        }
                        _ => false,
                    };
                    if same && nodes[i].skin.is_none() && nodes[i].blend_shapes.is_empty() && !subdivision.iter().any(|s| s.0 == i) {
                        instances.push((i, *j));
                    }
                }
                None => {
                    if matches!(nodes[i].kind, FbxKind::Mesh { .. }) {
                        first_of.insert(root, i);
                    }
                }
            }
        }
    }
    let selection_sets: Vec<(String, Vec<usize>)> = if o.selection_sets {
        state.named_sets.iter().map(|(name, ids)| (name.clone(), ids.iter().filter_map(|id| index_of.get(id).copied()).collect::<Vec<_>>())).filter(|(_, m)| !m.is_empty()).collect()
    } else {
        Vec::new()
    };
    let mut anim = if o.animation {
        let (a, b) = (o.bake_start.unwrap_or(state.frame_range.0), o.bake_end.unwrap_or(state.frame_range.1));
        bake_take(state, &exported_ids, &nodes, (a.min(b), a.max(b)), if o.bake_animation { o.bake_step.unwrap_or(1).max(1) } else { 0 })
    } else {
        None
    };
    if let Some(t) = anim.as_mut().filter(|_| o.use_scene_name) {
        t.name = state.file.as_ref().and_then(|f| f.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
    }
    let materials: Vec<loom_fbx::FbxMaterial> = state.scene.materials.values().map(|m| material_to_fbx(m, o.embed_media)).collect();
    // The exported objects' layers, as display layers.
    let mut layers: Vec<loom_fbx::FbxLayer> = Vec::new();
    for (i, id) in exported_ids.iter().enumerate() {
        let Some(l) = state.scene.node(*id).and_then(|n| state.scene.layer(n.layer)) else { continue };
        let at = match layers.iter().position(|x| x.name == l.name) {
            Some(k) => k,
            None => {
                let c = l.color.map(|v| v as f32 / 255.0);
                layers.push(loom_fbx::FbxLayer { name: l.name.clone(), color: glam::Vec3::from_array(c), visible: !l.hidden, frozen: l.frozen, nodes: Vec::new() });
                layers.len() - 1
            }
        };
        layers[at].nodes.push(i);
    }
    let doc = FbxDocument { nodes, unit_metres: unit_metres(state), anim, materials, instances, selection_sets, subdivision, layers };
    let mut filters = curve_filters(o.constant_key_reducer, [o.translation_precision, o.rotation_precision, o.scaling_precision, o.other_precision], o.key_reducer, o.key_reducer_precision, o.xyz_curve_sync);
    filters.remove_single_key = o.remove_single_key;
    let opts = WriteOpts {
        creator: format!("3dloom {}", env!("CARGO_PKG_VERSION")),
        axes: match o.up_axis {
            UpAxis::Y => loom_fbx::WriteAxes::YUp,
            UpAxis::Z => loom_fbx::WriteAxes::ZUp,
        },
        smoothing_layer: o.smoothing_groups,
        split_normals: o.split_normals,
        tangents: o.tangents_and_binormals,
        triangulate: o.triangulate,
        embed_media: o.embed_media,
        ascii: o.ascii,
        version: if o.version >= 7500 { 7500 } else { 7400 },
        file_unit_cm: if o.units_automatic { unit_metres(state) * 100.0 } else { o.units.metres() * 100.0 },
        file_dir: path.parent().map(|d| d.to_path_buf()),
        curve_filters: filters,
    };
    let bytes = loom_fbx::write(&doc, &opts);
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(json!({ "path": path, "objects": count, "scale_factor": unit_metres(state) * 100.0 / opts.file_unit_cm }))
}


/// Euler XYZ degrees of a rotation, each axis unwrapped toward `prev`.
fn euler_near(q: glam::Quat, prev: Option<glam::Vec3>) -> glam::Vec3 {
    let (x, y, z) = q.to_euler(glam::EulerRot::XYZ);
    let mut r = glam::Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
    if let Some(p) = prev {
        for i in 0..3 {
            r[i] += ((p[i] - r[i]) / 360.0).round() * 360.0;
        }
    }
    r
}

/// The take over `range`: each exported node whose local transform
/// (relative to its exported parent) changes gets a track, sampled every
/// `step` frames (Bake Animation), or with `step` 0 at the frames where the
/// node's own position, rotation or scale has keys (plus the range ends).
fn bake_take(state: &AppState, ids: &[NodeId], nodes: &[FbxNode], range: (i32, i32), step: u32) -> Option<loom_fbx::FbxTake> {
    let (a, b) = range;
    let mut tracks = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        let parent = nodes.get(i).and_then(|n| n.parent).and_then(|p| ids.get(p)).copied();
        let mut anim = loom_fbx::FbxAnim::default();
        let mut prev = None;
        let frames: Vec<f32> = if step > 0 {
            let mut v: Vec<f32> = (a..=b).step_by(step as usize).map(|f| f as f32).collect();
            if v.last() != Some(&(b as f32)) {
                v.push(b as f32);
            }
            v
        } else {
            let mut v: Vec<f32> = vec![a as f32, b as f32];
            if let Some(n) = state.scene.node(*id) {
                for k in [n.transform.position.keys(), n.transform.rotation.keys(), n.transform.scale.keys()].into_iter().flatten() {
                    v.extend(k.tracks.iter().flat_map(|t| t.keys.iter().map(|k| k.time)).filter(|t| *t > a as f32 && *t < b as f32));
                }
            }
            v.sort_by(f32::total_cmp);
            v.dedup_by(|x, y| (*x - *y).abs() < 1e-4);
            v
        };
        for f in frames {
            let local = state.scene.at_time(f, || {
                let w = state.scene.world_matrix(*id);
                match parent {
                    Some(p) => state.scene.world_matrix(p).inverse() * w,
                    None => w,
                }
            });
            let (s, q, t) = local.to_scale_rotation_translation();
            let r = euler_near(q, prev);
            prev = Some(r);
            anim.frames.push(f);
            anim.translation.push(t);
            anim.rotation_deg.push(r);
            anim.scale.push(s);
        }
        let moves = |v: &[glam::Vec3]| v.iter().any(|x| x.distance(v[0]) > 1e-5);
        if moves(&anim.translation) || moves(&anim.rotation_deg) || moves(&anim.scale) {
            tracks.push((i, anim));
        }
    }
    (!tracks.is_empty()).then(|| loom_fbx::FbxTake { name: "Take 001".into(), fps: state.fps, start: a, end: b, tracks })
}

/// Per-frame samples as a keyed Param: one Linear key per frame on each
/// component that changes (the curve filters drop the keys they reduce;
/// `kind` 0/1/2 picks the translation / rotation / scale threshold); a
/// component that holds stays a plain value.
fn sampled(frames: &[f32], values: &[glam::Vec3], filters: &loom_fbx::CurveFilters, kind: usize) -> loom_scene::Param<glam::Vec3> {
    use loom_scene::anim::{Interp, TangentType};
    let first = values.first().copied().unwrap_or_default();
    let mut k = loom_scene::param::KeyedTracks::from_rest(loom_scene::param::ParamType::Vec3, first.to_array().to_vec());
    let kept: Vec<Vec<usize>> = (0..3)
        .map(|c| {
            let vals: Vec<f32> = values.iter().map(|v| v[c]).collect();
            loom_fbx::curves::reduce(frames, &vals, filters.constant.map(|p| p[kind]), filters.key_reducer.map(|r| r.0))
        })
        .collect();
    let sync: Option<Vec<usize>> = filters.key_reducer.filter(|r| r.1).map(|_| {
        let mut all = kept.concat();
        all.sort_unstable();
        all.dedup();
        all
    });
    for (c, tr) in k.tracks.iter_mut().enumerate() {
        if values.iter().all(|v| (v[c] - first[c]).abs() < 1e-6) {
            continue;
        }
        tr.interp = Interp::Linear;
        for i in sync.as_ref().unwrap_or(&kept[c]) {
            tr.set_key(frames[*i], values[*i][c], (TangentType::Linear, TangentType::Linear));
        }
    }
    loom_scene::Param { controller: loom_scene::param::Controller::Keys(k) }
}

/// An FBX light as a standard light (free: FBX carries no target).
fn light_from_fbx(l: &loom_fbx::FbxLight) -> loom_scene::light::LightObject {
    use loom_scene::light::{LightObject, LightType};
    let mut o = LightObject::new(match l.light_type {
        1 => LightType::Directional,
        2 => LightType::Spot,
        _ => LightType::Omni,
    });
    o.color.set(glam::Vec3::from(l.color).clamp(glam::Vec3::ZERO, glam::Vec3::ONE));
    o.multiplier.set(l.intensity);
    o.on.set(l.cast_light);
    o.shadows_on.set(l.cast_shadows);
    if o.light_type == LightType::Spot && l.outer_angle > 0.0 {
        let _ = o.set_parameter("Falloff", l.outer_angle as f64);
        let _ = o.set_parameter("Hotspot", l.inner_angle.min(l.outer_angle) as f64);
    }
    o.decay.set(l.decay.min(2));
    o.decay_start.set(l.decay_start.max(0.0));
    if let Some((a, b)) = l.near {
        o.near_use.set(true);
        o.near_start.set(a);
        o.near_end.set(b);
    }
    if let Some((a, b)) = l.far {
        o.far_use.set(true);
        o.far_start.set(a);
        o.far_end.set(b);
    }
    o
}

/// A standard light as an FBX light (a Skylight has no FBX equivalent).
fn light_to_fbx(l: &loom_scene::light::LightObject) -> Option<loom_fbx::FbxLight> {
    use loom_scene::light::LightType;
    let light_type = match l.light_type {
        LightType::Omni => 0,
        LightType::Directional => 1,
        LightType::Spot => 2,
        LightType::Skylight => return None,
    };
    Some(loom_fbx::FbxLight {
        light_type,
        color: l.color.get().to_array(),
        intensity: l.multiplier.get(),
        inner_angle: l.hotspot.get(),
        outer_angle: l.falloff.get(),
        cast_light: l.on.get(),
        cast_shadows: l.shadows_on.get(),
        decay: l.decay.get(),
        decay_start: l.decay_start.get(),
        near: l.near_use.get().then(|| (l.near_start.get(), l.near_end.get())),
        far: l.far_use.get().then(|| (l.far_start.get(), l.far_end.get())),
    })
}
