//! Wavefront OBJ (+ MTL) ↔ scene by the OBJ Importer / Exporter options
//! in `AppState::exchange`: `import` adds Editable Poly objects (one undo
//! entry) with their MTL materials, `export` writes the objects and their
//! Standard materials.

use crate::exchange::{ObjFaces, ObjNormals};
use crate::execute::{commit, unique_name};
use crate::state::{AppState, SelectionMode};
use glam::Vec3;
use loom_mesh::obj::{MtlMaterial, ObjFaceMode, ObjNormalMode, ObjOut, ObjReadOpts, ObjWriteOpts};
use loom_scene::material::{Map as BitmapMap, MaterialId, StandardMaterial};
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
    let map = |p: &Option<String>, amount: f32| p.as_ref().filter(|p| !p.is_empty()).map(|p| BitmapMap::bitmap(resolve(dir, p), amount));
    s.diffuse_map = map(&m.map_kd, 100.0);
    s.specular_level_map = map(&m.map_ks, 100.0);
    s.opacity_map = map(&m.map_d, 100.0);
    s.bump_map = m.map_bump.as_ref().filter(|(p, _)| !p.is_empty()).map(|(p, bm)| BitmapMap::bitmap(resolve(dir, p), bm.map_or(default_bump, |b| b * 100.0)));
    s.show_map_in_viewport = show_maps && s.diffuse_map.is_some();
    s
}

/// Map-Export: where the exported material's maps go and how.
pub struct MapExport<'a> {
    /// The OBJ's folder (MTL map paths are relative to it).
    dir: PathBuf,
    /// Where converted bitmaps and rendered procedurals go.
    out: PathBuf,
    o: &'a crate::exchange::ObjExportOptions,
    /// Images written so far.
    pub written: Vec<PathBuf>,
}

impl<'a> MapExport<'a> {
    pub fn new(dir: &Path, o: &'a crate::exchange::ObjExportOptions) -> Self {
        let out = if o.use_map_path && !o.map_path.trim().is_empty() { dir.join(o.map_path.trim()) } else { dir.to_path_buf() };
        Self { dir: dir.to_path_buf(), out, o, written: Vec::new() }
    }

    fn rel(&self, p: &Path) -> String {
        p.strip_prefix(&self.dir).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| p.to_string_lossy().into_owned())
    }

    fn save(&mut self, img: image::RgbaImage, stem: &str) -> Option<String> {
        let ext = self.o.map_format.trim().to_ascii_lowercase();
        let ext = if ["bmp", "jpg", "png", "tga", "tif"].contains(&ext.as_str()) { ext } else { "tga".into() };
        std::fs::create_dir_all(&self.out).ok()?;
        let path = self.out.join(format!("{stem}.{ext}"));
        let saved = if ext == "jpg" { image::DynamicImage::ImageRgba8(img).to_rgb8().save(&path) } else { img.save(&path) };
        saved.ok()?;
        let r = self.rel(&path);
        self.written.push(path);
        Some(r)
    }

    /// The MTL file name of a material map: the bitmap (resaved when
    /// Convert Bitmaps is on) or the procedural rendered to an image.
    fn file(&mut self, m: &BitmapMap, material: &str, slot: &str) -> Option<String> {
        if !m.enabled {
            return None;
        }
        let [w, h] = self.o.map_size.map(|v| v.clamp(1, 8192));
        if m.kind == loom_scene::material::MapType::Bitmap {
            if m.path.is_empty() {
                return None;
            }
            if !self.o.convert_bitmaps {
                return Some(self.rel(Path::new(&m.path)));
            }
            let mut img = image::open(&m.path).ok()?.to_rgba8();
            if self.o.resize_bitmaps {
                img = image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle);
            }
            if self.o.power_of_two {
                let p2 = |v: u32| {
                    let up = v.max(1).next_power_of_two();
                    if up - v <= v - up / 2 || up == 1 { up } else { up / 2 }
                };
                let (pw, ph) = (p2(img.width()), p2(img.height()));
                if (pw, ph) != img.dimensions() {
                    img = image::imageops::resize(&img, pw, ph, image::imageops::FilterType::Triangle);
                }
            }
            let stem = Path::new(&m.path).file_stem()?.to_string_lossy().into_owned();
            return self.save(img, &stem);
        }
        if !self.o.render_procedurals {
            return None;
        }
        let mut images = loom_scene::maps::Images::default();
        loom_scene::maps::load_images(m, &mut images, &mut |p| {
            image::open(p).ok().map(|i| {
                let i = i.to_rgba8();
                std::sync::Arc::new(loom_scene::maps::Image { width: i.width(), height: i.height(), rgba: i.into_raw() })
            })
        });
        let img = image::RgbaImage::from_fn(w, h, |x, y| {
            let c = loom_scene::maps::eval(m, [(x as f32 + 0.5) / w as f32, 1.0 - (y as f32 + 0.5) / h as f32], &images);
            image::Rgba([c[0], c[1], c[2], 1.0].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
        });
        let safe: String = format!("{material}_{slot}").chars().map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
        self.save(img, &safe)
    }

    /// The map statement's argument: `-o` / `-s` (Extended Map-Params) and the file.
    fn arg(&mut self, m: &Option<BitmapMap>, material: &str, slot: &str) -> Option<String> {
        let m = m.as_ref()?;
        let f = self.file(m, material, slot)?;
        if !self.o.extended_map_params {
            return Some(f);
        }
        let (off, tile) = (m.coords.offset, m.coords.tiling);
        Some(format!("-o {} {} 0 -s {} {} 1 {f}", off[0], off[1], tile[0], tile[1]))
    }
}

/// A Standard material as an MTL entry, its maps as Map-Export says (paths
/// relative to the OBJ's folder when inside it).
pub fn material_to_mtl_with(s: &StandardMaterial, maps: &mut MapExport, black_ambient: bool) -> MtlMaterial {
    let mut m = MtlMaterial::new(&s.name);
    m.kd = s.diffuse.get().to_array();
    m.ka = if black_ambient { [0.0; 3] } else { s.ambient.get().to_array() };
    m.ks = (s.specular.get() * (s.specular_level.get() / 100.0).min(1.0)).to_array();
    m.ns = 2f32.powf(s.glossiness.get() / 10.0);
    m.d = s.opacity.get() / 100.0;
    m.ke = (s.diffuse.get() * s.self_illum.get() / 100.0).to_array();
    m.map_kd = maps.arg(&s.diffuse_map, &s.name, "Kd");
    m.map_ks = maps.arg(&s.specular_level_map, &s.name, "Ks");
    m.map_d = maps.arg(&s.opacity_map, &s.name, "d");
    let extended = maps.o.extended_map_params;
    m.map_bump = maps.arg(&s.bump_map, &s.name, "bump").map(|p| (p, s.bump_map.as_ref().map(|b| b.amount / 100.0).filter(|_| extended)));
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
    let mut maps = MapExport::new(&dir, o);
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
                    mtl.push(material_to_mtl_with(m, &mut maps, o.force_black_ambient));
                }
            } else {
                for n in &node.materials {
                    if let Some(m) = state.scene.materials.values().find(|m| m.name == *n) {
                        if !mtl.iter().any(|x| x.name == m.name) {
                            mtl.push(material_to_mtl_with(m, &mut maps, o.force_black_ambient));
                        }
                    }
                }
            }
        }
        // Map Channel, or (Auto) the channel the object's diffuse map reads.
        let auto = node.material.and_then(|m| state.scene.materials.get(&m)).and_then(|m| m.diffuse_map.as_ref()).map(|d| d.coords.map_channel as usize);
        let map_channel = if o.map_channel_auto { auto.unwrap_or(1) } else { o.map_channel.max(1) as usize };
        objects.push(ObjOut { name: node.name.clone(), mesh, world: state.scene.object_matrix(*id), materials, map_channel });
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
    Ok(json!({ "path": path, "objects": objects.len(), "materials": mtl.len(), "maps_written": maps.written }))
}
