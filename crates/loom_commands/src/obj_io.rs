//! Wavefront OBJ (+ MTL) ↔ scene by the OBJ Importer / Exporter options
//! in `AppState::exchange`: `import` adds Editable Poly objects (one undo
//! entry) with their MTL materials, `export` writes the objects and their
//! Standard materials.

use crate::exchange::{ObjFaces, ObjNormals};
use crate::execute::{commit, unique_name};
use crate::state::{AppState, SelectionMode};
use glam::Vec3;
use loom_mesh::obj::{MtlMaterial, ObjFaceMode, ObjNormalMode, ObjOut, ObjReadOpts, ObjWriteOpts};
use loom_scene::material::{BitmapMap, MaterialId, StandardMaterial};
use loom_scene::{Change, Node, NodeId, ObjectKind};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Metres per scene unit (Units Setup).
fn unit_metres(state: &AppState) -> f64 {
    state.units.system_unit.metres() * state.units.system_scale as f64
}

fn stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Object".into())
}

/// A map path from an MTL: absolute as written, else beside the MTL.
fn resolve(dir: &Path, p: &str) -> String {
    let q = PathBuf::from(p.replace('\\', "/"));
    if q.is_absolute() {
        return q.to_string_lossy().into_owned();
    }
    dir.join(q).to_string_lossy().into_owned()
}

/// An MTL entry as a Standard material (Glossiness = 10·log2 Ns, Specular
/// Level from the Ks strength, Opacity from d, Self-Illumination from Ke).
pub fn material_from_mtl(m: &MtlMaterial, dir: &Path, default_bump: f32, black_ambient: bool, show_maps: bool) -> StandardMaterial {
    let mut s = StandardMaterial::new(m.name.clone());
    let v = |c: [f32; 3]| Vec3::from(c).clamp(Vec3::ZERO, Vec3::ONE);
    s.diffuse.set(v(m.kd));
    let ambient = if black_ambient { Vec3::ZERO } else { v(m.ka) };
    s.ambient.set(ambient);
    s.ambient_locked = ambient == v(m.kd);
    let ks = v(m.ks);
    let strength = ks.max_element();
    if strength > 0.0 {
        s.specular.set(ks / strength);
    }
    s.specular_level.set(strength * 100.0);
    s.glossiness.set((10.0 * m.ns.max(1.0).log2()).clamp(0.0, 100.0));
    s.opacity.set((m.d * 100.0).clamp(0.0, 100.0));
    let kd = v(m.kd).max_element();
    if kd > 0.0 {
        s.self_illum.set((v(m.ke).max_element() / kd * 100.0).clamp(0.0, 100.0));
    }
    let map = |p: &Option<String>, amount: f32| p.as_ref().filter(|p| !p.is_empty()).map(|p| BitmapMap { path: resolve(dir, p), amount, enabled: true });
    s.diffuse_map = map(&m.map_kd, 100.0);
    s.specular_level_map = map(&m.map_ks, 100.0);
    s.opacity_map = map(&m.map_d, 100.0);
    s.bump_map = m.map_bump.as_ref().filter(|(p, _)| !p.is_empty()).map(|(p, bm)| BitmapMap { path: resolve(dir, p), amount: bm.map_or(default_bump, |b| b * 100.0), enabled: true });
    s.show_map_in_viewport = show_maps && s.diffuse_map.is_some();
    s
}

/// A Standard material as an MTL entry; map paths relative to `dir` when inside it.
pub fn material_to_mtl(s: &StandardMaterial, dir: &Path, black_ambient: bool) -> MtlMaterial {
    let mut m = MtlMaterial::new(&s.name);
    m.kd = s.diffuse.get().to_array();
    m.ka = if black_ambient { [0.0; 3] } else { s.ambient.get().to_array() };
    m.ks = (s.specular.get() * (s.specular_level.get() / 100.0).min(1.0)).to_array();
    m.ns = 2f32.powf(s.glossiness.get() / 10.0);
    m.d = s.opacity.get() / 100.0;
    m.ke = (s.diffuse.get() * s.self_illum.get() / 100.0).to_array();
    let rel = |b: &Option<BitmapMap>| {
        b.as_ref().filter(|b| b.enabled && !b.path.is_empty()).map(|b| Path::new(&b.path).strip_prefix(dir).map(|r| r.to_string_lossy().into_owned()).unwrap_or_else(|_| b.path.clone()))
    };
    m.map_kd = rel(&s.diffuse_map);
    m.map_ks = rel(&s.specular_level_map);
    m.map_d = rel(&s.opacity_map);
    m.map_bump = rel(&s.bump_map).map(|p| (p, s.bump_map.as_ref().map(|b| b.amount / 100.0)));
    m
}

/// The OBJ Importer's file info: objects with face counts and the "LED" features.
pub fn file_info(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let objects = loom_mesh::obj::scan(&text, &stem(path));
    let has = |k: &str| text.lines().any(|l| l.trim_start().starts_with(k));
    Ok(json!({
        "file_name": path.file_name().map(|n| n.to_string_lossy().into_owned()),
        "faces": objects.iter().map(|o| o.1).sum::<usize>(),
        "objects": objects.iter().map(|(n, f)| json!({ "name": n, "faces": f })).collect::<Vec<_>>(),
        "normals": has("vn "),
        "texture_coordinates": has("vt "),
        "smoothing_groups": has("s "),
        "materials": has("mtllib ") || has("usemtl "),
    }))
}

pub fn import(state: &mut AppState, path: &Path) -> Result<Value, String> {
    let o = state.exchange.obj_import.clone();
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    // Units: Convert reads the numbers in the model unit; else Object Scale.
    let scale = if o.convert_units { (o.model_units.metres() / unit_metres(state)) as f32 } else { o.object_scale };
    let ro = ObjReadOpts {
        single_mesh: o.single_mesh,
        texture_coords: o.texture_coordinates,
        smoothing_groups: o.smoothing_groups,
        normals: match o.normals {
            ObjNormals::FromFile => ObjNormalMode::FromFile,
            ObjNormals::FromSmoothingGroups => ObjNormalMode::FromSmoothingGroups,
            ObjNormals::AutoSmooth => ObjNormalMode::AutoSmooth(o.auto_smooth_angle),
            ObjNormals::Faceted => ObjNormalMode::Faceted,
        },
        flip_normals: o.flip_normals,
        retriangulate: o.retriangulate,
        flip_zy: o.flip_zy,
        scale,
    };
    let file = loom_mesh::obj::parse_with(&text, &stem(path), &ro).map_err(|e| format!("{}: {e}", path.display()))?;
    let objects: Vec<_> = file.objects.into_iter().filter(|ob| o.objects.as_ref().is_none_or(|names| names.contains(&ob.name))).collect();
    if objects.is_empty() {
        return Err(format!("{}: no faces", path.display()));
    }
    let mut changes = Vec::with_capacity(objects.len() + 2);
    // Reset scene: the current objects go in the same undo step.
    let mut taken: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    if o.reset_scene {
        changes.extend(state.scene.nodes().map(|n| Change::RemoveNode(n.id)).collect::<Vec<_>>());
    } else {
        taken = state.scene.nodes().map(|n| n.name.clone()).collect();
    }
    // Materials from the MTL libraries, by name.
    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut library = state.scene.materials.clone();
    let mut slots = state.scene.material_editor.clone();
    let mut by_name: BTreeMap<String, MaterialId> = BTreeMap::new();
    if o.import_materials {
        let mut libs = file.mtllibs.clone();
        if libs.is_empty() {
            libs.push(format!("{}.mtl", stem(path)));
        }
        for lib in libs {
            let p = PathBuf::from(resolve(&dir, &lib));
            let Ok(t) = std::fs::read_to_string(&p) else { continue };
            let mdir = p.parent().unwrap_or(&dir).to_path_buf();
            for m in loom_mesh::obj::parse_mtl(&t) {
                if by_name.contains_key(&m.name) {
                    continue;
                }
                let mut s = material_from_mtl(&m, &mdir, o.default_bump, o.force_black_ambient, o.show_maps_in_viewport);
                s.name = format!("{}{}", o.material_prefix, m.name);
                let id = loom_scene::material::next_material_id(&library);
                library.insert(id, s);
                by_name.insert(m.name.clone(), id);
                // Import into Mat-Editor: the first free sample slots.
                if o.import_into_mat_editor {
                    if let Some(slot) = slots.slots.iter_mut().find(|s| s.is_none()) {
                        *slot = Some(id);
                    }
                }
            }
        }
    }
    if library != state.scene.materials || slots != state.scene.material_editor {
        changes.push(Change::SetMaterials { before: Box::new((state.scene.materials.clone(), state.scene.material_editor.clone())), after: Box::new((library, slots)) });
    }
    let first = state.scene.next_id();
    let colour = |i: usize| loom_scene::OBJECT_COLORS[(first as usize + if o.unique_wire_color { i } else { 0 }) % loom_scene::OBJECT_COLORS.len()];
    let mut ids = Vec::with_capacity(objects.len());
    for (i, ob) in objects.into_iter().enumerate() {
        let base = format!("{}{}", o.prefix, ob.name);
        let name = if taken.contains(&base) { unique_name(&base, &taken) } else { base };
        taken.insert(name.clone());
        let mut mesh = ob.mesh;
        // Center Pivots: the pivot at the object's centre (else the world origin).
        let mut at = Vec3::ZERO;
        if o.center_pivots {
            let (lo, hi) = mesh.aabb();
            at = (lo + hi) * 0.5;
            for v in mesh.vertices_mut() {
                v.position -= at;
            }
        }
        // Materials: the names the object's faces use, by material id.
        let used: std::collections::BTreeSet<u16> = mesh.faces().iter().map(|f| f.material_id).collect();
        let names: Vec<String> = (1..=used.iter().copied().max().unwrap_or(0)).map(|id| file.material_names.get(&id).map(|n| format!("{}{n}", o.material_prefix)).unwrap_or_else(|| format!("mat_{id}"))).collect();
        let mut node = Node::new(&name, ObjectKind::EditablePoly(loom_scene::PolyObject::from_mesh(mesh)));
        node.transform.position.set(at);
        node.wire_color = colour(i);
        node.layer = state.scene.current_layer;
        if !by_name.is_empty() {
            let mats: Vec<Option<MaterialId>> = used.iter().map(|id| file.material_names.get(id).and_then(|n| by_name.get(n)).copied()).collect();
            if mats.len() == 1 {
                node.material = mats[0];
            }
            node.materials = names;
        }
        node.id = NodeId(first + i as u64);
        ids.push(node.id);
        changes.push(Change::AddNode(node));
    }
    let label = format!("Import {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    commit(state, &label, Change::Group(changes))?;
    state.selection = ids.iter().copied().collect();
    state.sub_selection.clear();
    state.selection_mode = SelectionMode::Object;
    Ok(json!({ "nodes": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

/// Writes the evaluated meshes of `ids` (geometry only: shapes count when
/// their viewport render makes faces) as one OBJ file, plus the MTL library
/// of their materials (Use material / Create mat-library).
pub fn export(state: &AppState, ids: &[NodeId], path: &Path) -> Result<Value, String> {
    let o = &state.exchange.obj_export;
    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut objects = Vec::new();
    let mut mtl: Vec<MtlMaterial> = Vec::new();
    for id in ids {
        let Some(node) = state.scene.node(*id) else { continue };
        if matches!(node.object, ObjectKind::Camera { .. } | ObjectKind::Light(_) | ObjectKind::Group { .. }) {
            continue;
        }
        let mut mesh = state.scene.eval_mesh(*id).value.mesh.clone();
        if mesh.face_count() == 0 {
            continue;
        }
        let mut materials = node.materials.clone();
        if o.use_material {
            if let Some(m) = node.material.and_then(|m| state.scene.materials.get(&m)) {
                // An assigned material covers every face.
                for f in mesh.faces_mut() {
                    f.material_id = 1;
                }
                materials = vec![m.name.clone()];
                if !mtl.iter().any(|x| x.name == m.name) {
                    mtl.push(material_to_mtl(m, &dir, o.force_black_ambient));
                }
            } else {
                for n in &node.materials {
                    if let Some(m) = state.scene.materials.values().find(|m| m.name == *n) {
                        if !mtl.iter().any(|x| x.name == m.name) {
                            mtl.push(material_to_mtl(m, &dir, o.force_black_ambient));
                        }
                    }
                }
            }
        }
        objects.push(ObjOut { name: node.name.clone(), mesh, world: state.scene.object_matrix(*id), materials });
    }
    if objects.is_empty() {
        return Err("nothing to export: no geometry objects".into());
    }
    let mtl_path = path.with_extension("mtl");
    let opts = ObjWriteOpts {
        flip_yz: o.flip_yz,
        faces: match o.faces {
            ObjFaces::Triangles => ObjFaceMode::Triangles,
            ObjFaces::Quads => ObjFaceMode::Quads,
            ObjFaces::Polygons => ObjFaceMode::Polygons,
        },
        texture_coords: o.texture_coordinates,
        normals: o.normals,
        smoothing_groups: o.smoothing_groups,
        scale: o.scale,
        precision: o.precision as usize,
        relative: o.relative_numbers,
        crlf: o.target_windows,
        mtllib: (o.use_material && !mtl.is_empty()).then(|| mtl_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
    };
    let text = loom_mesh::obj::write_with(&objects, &opts);
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    // Create mat-library off: the OBJ names a library that is not written.
    if o.use_material && o.create_mat_library && !mtl.is_empty() {
        let mut t = loom_mesh::obj::write_mtl(&mtl);
        if o.target_windows {
            t = t.replace('\n', "\r\n");
        }
        std::fs::write(&mtl_path, t).map_err(|e| format!("{}: {e}", mtl_path.display()))?;
    }
    Ok(json!({ "path": path, "objects": objects.len(), "materials": mtl.len() }))
}
