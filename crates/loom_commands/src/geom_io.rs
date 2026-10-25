//! 3D Studio (.3ds), STL and DXF import / export: the glue between the
//! format codecs in `loom_mesh` and the scene. Imports add Editable Mesh
//! objects (the reference imports these formats as meshes) in one undo
//! entry; exports write the evaluated meshes of the given nodes.

use crate::execute::{commit, unique_name};
use crate::state::{AppState, SelectionMode};
use glam::{Mat4, Vec3};
use loom_mesh::indexed::{IndexedFace, IndexedMesh};
use loom_mesh::tds::{TdsCamera, TdsFile, TdsLight, TdsMaterial};
use loom_scene::light::LightType;
use loom_scene::material::{Map as BitmapMap, MaterialId, StandardMaterial};
use loom_scene::{Change, Node, NodeId, ObjectKind, PolyObject, Transform};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

fn ext(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

pub fn is_3ds(path: &Path) -> bool {
    ext(path) == "3ds"
}

pub fn is_stl(path: &Path) -> bool {
    ext(path) == "stl"
}

pub fn is_dxf(path: &Path) -> bool {
    ext(path) == "dxf"
}

fn stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Object".into())
}

fn label(verb: &str, path: &Path) -> String {
    format!("{verb} {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
}

/// A map path from the file: absolute as written, else beside the file.
fn resolve(dir: &Path, p: &str) -> String {
    let q = PathBuf::from(p.replace('\\', "/"));
    if q.is_absolute() {
        return q.to_string_lossy().into_owned();
    }
    dir.join(q).to_string_lossy().into_owned()
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Adds `nodes` (ids assigned here) as one undo entry and selects them.
fn add_nodes(state: &mut AppState, label: &str, mut changes: Vec<Change>, nodes: Vec<Node>) -> Result<Vec<NodeId>, String> {
    let first = state.scene.next_id();
    let mut taken: BTreeSet<String> = state.scene.nodes().map(|n| n.name.clone()).collect();
    let mut ids = Vec::new();
    for (i, mut node) in nodes.into_iter().enumerate() {
        if taken.contains(&node.name) {
            node.name = unique_name(&node.name, &taken);
        }
        taken.insert(node.name.clone());
        node.id = NodeId(first + i as u64);
        node.layer = state.scene.current_layer;
        node.wire_color = loom_scene::OBJECT_COLORS[(first as usize + i) % loom_scene::OBJECT_COLORS.len()];
        ids.push(node.id);
        changes.push(Change::AddNode(node));
    }
    commit(state, label, Change::Group(changes))?;
    state.selection = ids.iter().copied().collect();
    state.sub_selection.clear();
    state.selection_mode = SelectionMode::Object;
    Ok(ids)
}

fn mesh_node(name: &str, mesh: loom_mesh::Mesh) -> Node {
    Node::new(name, ObjectKind::EditableMesh(PolyObject::from_mesh(mesh)))
}

// ---- 3DS -------------------------------------------------------------------

fn material_from_3ds(m: &TdsMaterial, dir: &Path) -> StandardMaterial {
    let mut s = StandardMaterial::new(m.name.clone());
    let v = |c: [f32; 3]| Vec3::from(c).clamp(Vec3::ZERO, Vec3::ONE);
    s.ambient.set(v(m.ambient));
    s.diffuse.set(v(m.diffuse));
    s.specular.set(v(m.specular));
    s.ambient_locked = v(m.ambient) == v(m.diffuse);
    s.glossiness.set((m.shininess * 100.0).clamp(0.0, 100.0));
    s.specular_level.set((m.shin_strength * 100.0).clamp(0.0, 999.0));
    s.opacity.set(((1.0 - m.transparency) * 100.0).clamp(0.0, 100.0));
    s.two_sided = m.two_sided;
    if let Some((file, amount)) = &m.texture {
        s.diffuse_map = Some(BitmapMap::bitmap(resolve(dir, file), amount * 100.0));
        s.show_map_in_viewport = true;
    }
    s
}

fn material_to_3ds(s: &StandardMaterial) -> TdsMaterial {
    TdsMaterial {
        name: s.name.clone(),
        ambient: s.ambient.get().to_array(),
        diffuse: s.diffuse.get().to_array(),
        specular: s.specular.get().to_array(),
        shininess: s.glossiness.get() / 100.0,
        shin_strength: (s.specular_level.get() / 100.0).min(1.0),
        transparency: 1.0 - s.opacity.get() / 100.0,
        two_sided: s.two_sided,
        texture: s.diffuse_map.as_ref().filter(|m| m.enabled && !m.path.is_empty()).map(|m| (m.path.clone(), m.amount / 100.0)),
    }
}

/// A 3DS object as a mesh in its own frame: vertices back through the mesh
/// matrix, edge visibility, smoothing groups, material ids by `names`.
fn mesh_from_3ds(m: &loom_mesh::tds::TdsMesh, names: &[String]) -> Result<loom_mesh::Mesh, String> {
    let to_local = m.matrix.inverse();
    let mut im = IndexedMesh::default();
    // 3DS splits vertices where texture coordinates differ; the corners keep
    // their coordinates, the positions weld again.
    let mut by_pos: BTreeMap<(u32, u32, u32), u32> = BTreeMap::new();
    let welded: Vec<u32> = m
        .positions
        .iter()
        .map(|p| {
            let q = to_local.transform_point3(*p);
            *by_pos.entry((p.x.to_bits(), p.y.to_bits(), p.z.to_bits())).or_insert_with(|| im.add_vertex(q, None))
        })
        .collect();
    let mut material_of = vec![1u16; m.faces.len()];
    for (name, faces) in &m.material_groups {
        let id = names.iter().position(|n| n == name).map_or(1, |i| i as u16 + 1);
        for f in faces {
            if let Some(slot) = material_of.get_mut(*f as usize) {
                *slot = id;
            }
        }
    }
    let key = |p: Vec3| (p.x.to_bits(), p.y.to_bits(), p.z.to_bits());
    let mut hidden: HashSet<((u32, u32, u32), (u32, u32, u32))> = HashSet::new();
    for (i, f) in m.faces.iter().enumerate() {
        let mut uv = BTreeMap::new();
        if m.uvs.len() == m.positions.len() {
            uv.insert(1, f.iter().map(|x| Vec3::new(m.uvs[*x as usize][0], m.uvs[*x as usize][1], 0.0)).collect());
        }
        let ids: Vec<u32> = f.iter().map(|x| welded[*x as usize]).collect();
        let flags = m.flags.get(i).copied().unwrap_or(7);
        for k in 0..3 {
            if !loom_mesh::tds::edge_visible(flags, k) {
                let (a, b) = (im.positions[ids[k] as usize], im.positions[ids[(k + 1) % 3] as usize]);
                hidden.insert((key(a), key(b)));
                hidden.insert((key(b), key(a)));
            }
        }
        im.faces.push(IndexedFace { ids, smoothing_groups: m.smoothing.get(i).copied().unwrap_or(0), material_id: material_of[i], hidden: false, uv });
    }
    let mut mesh = loom_mesh::obj::build_repaired(im)?;
    let edges: Vec<loom_mesh::HalfEdgeId> = (0..mesh.half_edge_count() as u32)
        .map(loom_mesh::HalfEdgeId)
        .filter(|he| mesh.edge_vertices(*he).is_some_and(|(a, b)| hidden.contains(&(key(mesh.vertices()[a.0 as usize].position), key(mesh.vertices()[b.0 as usize].position)))))
        .collect();
    for he in edges {
        mesh.set_edge_visible(he, false);
    }
    Ok(mesh)
}

pub fn import_3ds(state: &mut AppState, path: &Path) -> Result<Value, String> {
    let file = loom_mesh::tds::read(&read_bytes(path)?).map_err(|e| format!("{}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let lbl = label("Import", path);
    state.undo.begin(&lbl);
    let r = import_3ds_into(state, &file, &dir, &lbl);
    state.undo.end();
    r
}

fn import_3ds_into(state: &mut AppState, file: &TdsFile, dir: &Path, lbl: &str) -> Result<Value, String> {
    let mut changes = Vec::new();
    if state.exchange.tds_import.replace_scene {
        changes.extend(state.scene.nodes().map(|n| Change::RemoveNode(n.id)).collect::<Vec<_>>());
    }
    let mut library = state.scene.materials.clone();
    let mut by_name: BTreeMap<String, MaterialId> = BTreeMap::new();
    for m in &file.materials {
        let id = loom_scene::material::next_material_id(&library);
        library.insert(id, material_from_3ds(m, dir));
        by_name.insert(m.name.clone(), id);
    }
    if library != state.scene.materials {
        changes.push(Change::SetMaterials {
            before: Box::new((state.scene.materials.clone(), state.scene.material_editor.clone())),
            after: Box::new((library, state.scene.material_editor.clone())),
        });
    }
    let mut nodes = Vec::new();
    for m in &file.meshes {
        let names: Vec<String> = m.material_groups.iter().map(|g| g.0.clone()).collect();
        let mesh = mesh_from_3ds(m, &names)?;
        if mesh.face_count() == 0 {
            continue;
        }
        let mut node = mesh_node(&m.name, mesh);
        node.transform = Transform::from_matrix(m.matrix);
        if let [one] = &names[..] {
            node.material = by_name.get(one).copied();
        }
        node.materials = names;
        nodes.push(node);
    }
    let mut ids = add_nodes(state, lbl, changes, nodes)?;
    for l in &file.lights {
        ids.extend(import_light(state, l)?);
    }
    for c in &file.cameras {
        ids.extend(import_camera(state, c)?);
    }
    state.selection = ids.iter().copied().collect();
    Ok(json!({ "nodes": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "materials": file.materials.len() }))
}

/// Gives an imported light or camera the file's name (its target follows).
fn rename(state: &mut AppState, id: NodeId, name: &str) -> Result<(), String> {
    let taken: BTreeSet<String> = state.scene.nodes().filter(|n| n.id != id).map(|n| n.name.clone()).collect();
    let name = if taken.contains(name) { unique_name(name, &taken) } else { name.to_owned() };
    crate::execute::execute(state, crate::Command::Rename { id: id.0, name }).map(|_| ())
}

fn import_light(state: &mut AppState, l: &TdsLight) -> Result<Vec<NodeId>, String> {
    use crate::light::LightKind;
    let (kind, target) = match l.spot {
        Some((t, _, _)) => (LightKind::TargetSpot, t),
        None => (LightKind::Omni, l.position),
    };
    let r = crate::light::create(state, kind, l.position, target)?;
    let id = NodeId(r["id"].as_u64().ok_or("no light id")?);
    let node = state.scene.node(id).ok_or("no light")?;
    let before = node.object.clone();
    let mut after = before.clone();
    if let ObjectKind::Light(o) = &mut after {
        o.color.set(Vec3::from(l.color).clamp(Vec3::ZERO, Vec3::ONE));
        o.multiplier.set(l.multiplier);
        o.on.set(!l.off);
        if let Some((_, hot, fall)) = l.spot {
            o.falloff.set(fall.max(1.0));
            o.hotspot.set(hot.min(fall - loom_scene::light::HOTSPOT_GAP).max(0.5));
        }
    }
    commit(state, "Light", Change::ReplaceObject { id, before: Box::new(before), after: Box::new(after) })?;
    rename(state, id, &l.name)?;
    let mut ids = vec![id];
    ids.extend(r["target"].as_u64().map(NodeId));
    Ok(ids)
}

fn import_camera(state: &mut AppState, c: &TdsCamera) -> Result<Vec<NodeId>, String> {
    let r = crate::camera::create(state, true, c.position, c.target)?;
    let id = NodeId(r["id"].as_u64().ok_or("no camera id")?);
    let fov = loom_scene::camera::lens_to_fov(c.lens.max(1.0));
    crate::execute::execute(state, crate::Command::SetParameter { id: id.0, name: "FOV".into(), value: fov as f64 })?;
    rename(state, id, &c.name)?;
    let mut ids = vec![id];
    ids.extend(r["target"].as_u64().map(NodeId));
    Ok(ids)
}

pub fn export_3ds(state: &AppState, ids: &[NodeId], path: &Path) -> Result<Value, String> {
    let mut file = TdsFile { master_scale: 1.0, ..Default::default() };
    let wanted: Vec<&Node> = ids.iter().filter_map(|id| state.scene.node(*id)).collect();
    let names = loom_mesh::tds::short_names(&wanted.iter().map(|n| n.name.clone()).collect::<Vec<_>>());
    let mut materials: BTreeMap<String, TdsMaterial> = BTreeMap::new();
    let mut add_material = |s: &StandardMaterial, materials: &mut BTreeMap<String, TdsMaterial>| {
        materials.entry(s.name.clone()).or_insert_with(|| material_to_3ds(s));
    };
    for (node, name) in wanted.iter().zip(&names) {
        let world = state.scene.world_matrix(node.id);
        match &node.object {
            ObjectKind::Light(l) if l.light_type != LightType::Skylight => {
                let target = loom_scene::light::target_distance(&state.scene, node.id).unwrap_or(loom_scene::light::DEFAULT_TARGET_DISTANCE);
                let eye = world.w_axis.truncate();
                let forward = -world.z_axis.truncate().normalize_or_zero();
                file.lights.push(TdsLight {
                    name: name.clone(),
                    position: eye,
                    color: l.color.get().to_array(),
                    multiplier: l.multiplier.get(),
                    off: !l.on.get(),
                    spot: (l.light_type == LightType::Spot).then(|| (eye + forward * target, l.hotspot.get(), l.falloff.get())),
                });
            }
            ObjectKind::Camera { fov, .. } => {
                let distance = crate::camera::target_distance(&state.scene, node.id).unwrap_or(loom_scene::camera::DEFAULT_TARGET_DISTANCE);
                let eye = world.w_axis.truncate();
                file.cameras.push(TdsCamera { name: name.clone(), position: eye, target: eye - world.z_axis.truncate().normalize_or_zero() * distance, roll: 0.0, lens: loom_scene::camera::fov_to_lens(fov.get()) });
            }
            ObjectKind::Light(_) | ObjectKind::Group { .. } => {}
            _ => {
                let mesh = state.scene.eval_mesh(node.id).value.mesh.clone();
                if mesh.face_count() == 0 {
                    continue;
                }
                let mut mesh = mesh;
                let mut mat_names = node.materials.clone();
                if let Some(m) = node.material.and_then(|m| state.scene.materials.get(&m)) {
                    for f in mesh.faces_mut() {
                        f.material_id = 1;
                    }
                    mat_names = vec![m.name.clone()];
                    add_material(m, &mut materials);
                } else {
                    for n in &node.materials {
                        if let Some(m) = state.scene.materials.values().find(|m| m.name == *n) {
                            add_material(m, &mut materials);
                        }
                    }
                }
                let parts = loom_mesh::tds::split_for_3ds(name, &mesh, state.scene.object_matrix(node.id), world, &mat_names);
                file.meshes.extend(parts);
            }
        }
    }
    if file.meshes.is_empty() && file.lights.is_empty() && file.cameras.is_empty() {
        return Err("nothing to export".into());
    }
    file.materials = materials.into_values().collect();
    std::fs::write(path, loom_mesh::tds::write(&file)).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(json!({ "path": path, "objects": file.meshes.len(), "lights": file.lights.len(), "cameras": file.cameras.len(), "materials": file.materials.len() }))
}

// ---- STL -------------------------------------------------------------------

pub fn import_stl(state: &mut AppState, path: &Path) -> Result<Value, String> {
    let o = state.exchange.stl_import.clone();
    let f = loom_mesh::stl::read(&read_bytes(path)?).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut im = loom_mesh::stl::to_indexed(&f.triangles, o.weld.then_some(o.weld_threshold), o.remove_double_faces);
    im.drop_degenerate();
    // Unify Normals off: faces keep their winding where the mesh allows it.
    let mut mesh = if o.unify_normals { loom_mesh::obj::build_repaired(im)? } else { im.clone().build().or_else(|_| loom_mesh::obj::build_repaired(im))? };
    if mesh.face_count() == 0 {
        return Err(format!("{}: no faces", path.display()));
    }
    if o.auto_smooth {
        mesh.auto_smooth(o.smooth_angle);
    }
    let name = f.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| stem(path));
    let ids = add_nodes(state, &label("Import", path), Vec::new(), vec![mesh_node(&name, mesh)])?;
    Ok(json!({ "nodes": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

fn geometry(state: &AppState, ids: &[NodeId]) -> Vec<(String, loom_mesh::Mesh, Mat4)> {
    ids.iter()
        .filter_map(|id| state.scene.node(*id))
        .filter(|n| !matches!(n.object, ObjectKind::Camera { .. } | ObjectKind::Light(_) | ObjectKind::Group { .. }))
        .map(|n| (n.name.clone(), state.scene.eval_mesh(n.id).value.mesh.clone(), state.scene.object_matrix(n.id)))
        .filter(|(_, m, _)| m.face_count() > 0)
        .collect()
}

pub fn export_stl(state: &AppState, ids: &[NodeId], path: &Path) -> Result<Value, String> {
    let objects = geometry(state, ids);
    if objects.is_empty() {
        return Err("nothing to export: no geometry objects".into());
    }
    let tris: Vec<[Vec3; 3]> = objects.iter().flat_map(|(_, m, w)| loom_mesh::stl::triangles(m, *w)).collect();
    let name = stem(path);
    let bytes = if state.exchange.stl_export.binary { loom_mesh::stl::write_binary(&name, &tris) } else { loom_mesh::stl::write_ascii(&name, &tris).into_bytes() };
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(json!({ "path": path, "triangles": tris.len(), "binary": state.exchange.stl_export.binary }))
}

// ---- DXF -------------------------------------------------------------------

pub fn import_dxf(state: &mut AppState, path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let layers = loom_mesh::dxf::read(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut nodes = Vec::new();
    for l in layers {
        // An Editable Mesh: quads split into triangles, the cuts invisible.
        let mesh = loom_mesh::tri_mesh::from_poly(&loom_mesh::obj::build_repaired(loom_mesh::dxf::to_indexed(&l.faces))?)?;
        if mesh.face_count() > 0 {
            nodes.push(mesh_node(&l.name, mesh));
        }
    }
    if nodes.is_empty() {
        return Err(format!("{}: no 3D faces", path.display()));
    }
    let ids = add_nodes(state, &label("Import", path), Vec::new(), nodes)?;
    Ok(json!({ "nodes": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

pub fn export_dxf(state: &AppState, ids: &[NodeId], path: &Path) -> Result<Value, String> {
    let objects = geometry(state, ids);
    if objects.is_empty() {
        return Err("nothing to export: no geometry objects".into());
    }
    std::fs::write(path, loom_mesh::dxf::write(&objects)).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(json!({ "path": path, "objects": objects.len() }))
}
