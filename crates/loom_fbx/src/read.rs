//! FBX (any version, binary or ASCII) → `FbxDocument` through ufbx, with
//! the scene converted to right-handed Z-up in the requested unit.

use crate::document::{FbxBlendShape, FbxDocument, FbxInfo, FbxKind, FbxLayer, FbxMaterial, FbxNode, FbxSkin, FbxTexture, ReadOpts};
use glam::{Mat4, Vec3};
use loom_mesh::indexed::{IndexedFace, IndexedMesh};
use loom_mesh::Mesh;
use std::collections::{BTreeMap, HashSet};

fn v3(v: ufbx::Vec3) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}

fn euler_deg(q: ufbx::Quat) -> Vec3 {
    let q = glam::Quat::from_xyzw(q.x as f32, q.y as f32, q.z as f32, q.w as f32).normalize();
    let (x, y, z) = q.to_euler(glam::EulerRot::XYZ);
    Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
}

/// `LayerElementSmoothing/Smoothing` as written ByPolygon, when the file
/// has one: the group bits from the binary file (`smoothing`, keyed by
/// geometry id), else ufbx's per-face bool. (ufbx parses an ASCII file's
/// `Smoothing` as bools, so its bits are gone there.)
fn dom_smoothing_groups(mesh: &ufbx::Mesh, smoothing: &std::collections::HashMap<i64, Vec<u32>>) -> Option<Vec<u32>> {
    let dom = mesh.element.dom_node.as_ref()?;
    let id = dom.values.first()?.value_int;
    let list = smoothing.get(&id)?;
    (list.len() == mesh.num_faces).then(|| list.clone())
}

/// Hard edges from `edge_smoothing` when the file has edges, else from the
/// corner normals: an edge is hard when a vertex's normals on its two
/// faces differ by more than one degree.
fn hard_edges(mesh: &ufbx::Mesh) -> HashSet<(u32, u32)> {
    let mut hard = HashSet::new();
    if !mesh.edges.is_empty() && mesh.edge_smoothing.len() == mesh.edges.len() {
        for (i, e) in mesh.edges.iter().enumerate() {
            if !mesh.edge_smoothing[i] {
                let (a, b) = (mesh.vertex_indices[e.a as usize], mesh.vertex_indices[e.b as usize]);
                hard.insert((a.min(b), a.max(b)));
            }
        }
        return hard;
    }
    if !mesh.vertex_normal.exists {
        return hard;
    }
    // Normal per (vertex, face) from the corner list, then compare across each edge.
    let mut per_face_vertex: BTreeMap<(u32, u32), Vec3> = BTreeMap::new();
    for (fi, f) in mesh.faces.iter().enumerate() {
        for c in f.index_begin..f.index_begin + f.num_indices {
            let v = mesh.vertex_indices[c as usize];
            let n = v3(mesh.vertex_normal.values[mesh.vertex_normal.indices[c as usize] as usize]);
            per_face_vertex.insert((fi as u32, v), n);
        }
    }
    let mut faces_on_edge: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for (fi, f) in mesh.faces.iter().enumerate() {
        let k = f.num_indices;
        for i in 0..k {
            let a = mesh.vertex_indices[(f.index_begin + i) as usize];
            let b = mesh.vertex_indices[(f.index_begin + (i + 1) % k) as usize];
            faces_on_edge.entry((a.min(b), a.max(b))).or_default().push(fi as u32);
        }
    }
    let cos_t = 1f32.to_radians().cos();
    for ((a, b), faces) in faces_on_edge {
        if faces.len() != 2 {
            continue;
        }
        let differs = |v: u32| {
            let (Some(n0), Some(n1)) = (per_face_vertex.get(&(faces[0], v)), per_face_vertex.get(&(faces[1], v))) else { return false };
            n0.normalize_or_zero().dot(n1.normalize_or_zero()) < cos_t
        };
        if differs(a) || differs(b) {
            hard.insert((a, b));
        }
    }
    hard
}

fn m4(m: &ufbx::Matrix) -> Mat4 {
    Mat4::from_cols_array(&[
        m.m00 as f32, m.m10 as f32, m.m20 as f32, 0.0,
        m.m01 as f32, m.m11 as f32, m.m21 as f32, 0.0,
        m.m02 as f32, m.m12 as f32, m.m22 as f32, 0.0,
        m.m03 as f32, m.m13 as f32, m.m23 as f32, 1.0,
    ])
}

/// The mesh plus, per built vertex, its ufbx vertex index. `use_groups`:
/// take the file's smoothing-group bits (off: groups from the normals).
fn convert_mesh(m: &ufbx::Mesh, smoothing: &std::collections::HashMap<i64, Vec<u32>>, use_groups: bool) -> Result<(Mesh, Vec<u32>), String> {
    let mut im = IndexedMesh::default();
    for v in m.vertices.iter() {
        im.add_vertex(v3(*v), None);
    }
    // Every file vertex stays (isolated vertices are kept), so the hard-edge
    // vertex ids below still name the built mesh's vertices.
    im.keep = (0..im.positions.len() as u32).collect();
    // Vertex colours: the first colour set, averaged over a vertex's corners.
    if let Some(set) = m.color_sets.first() {
        let vc = &set.vertex_color;
        if vc.exists {
            let mut sum: Vec<([f64; 4], u32)> = vec![([0.0; 4], 0); im.positions.len()];
            for c in 0..m.num_indices {
                let v = m.vertex_indices[c] as usize;
                let col = vc.values[vc.indices[c] as usize];
                let s = &mut sum[v];
                s.0[0] += col.x;
                s.0[1] += col.y;
                s.0[2] += col.z;
                s.0[3] += col.w;
                s.1 += 1;
            }
            for (i, (c, n)) in sum.iter().enumerate() {
                if *n > 0 {
                    let k = *n as f64;
                    let a = &mut im.vattrs[i];
                    a.0 = [(c[0] / k) as f32, (c[1] / k) as f32, (c[2] / k) as f32];
                    a.2 = (c[3] / k) as f32;
                }
            }
        }
    }
    // Real group bits (3ds Max, Blender "Smoothing Groups"). ByPolygon 0/1
    // (Blender "Face", or ufbx's per-face bool) only says which faces are
    // flat: the hard edges between smooth faces come from the normals.
    let groups = dom_smoothing_groups(m, smoothing).filter(|g| use_groups && g.iter().any(|x| *x > 1));
    let from_faces = m.face_smoothing.len() == m.num_faces;
    for (fi, f) in m.faces.iter().enumerate() {
        let mut face = IndexedFace { ids: (f.index_begin..f.index_begin + f.num_indices).map(|c| m.vertex_indices[c as usize]).collect(), ..Default::default() };
        face.material_id = m.face_material.get(fi).map(|i| *i as u16 + 1).unwrap_or(1);
        face.smoothing_groups = match &groups {
            Some(g) => g[fi],
            None if from_faces => m.face_smoothing[fi] as u32,
            None => 1,
        };
        for (k, set) in m.uv_sets.iter().enumerate() {
            let uv = &set.vertex_uv;
            if !uv.exists {
                continue;
            }
            let channel = set.name.strip_prefix("UVChannel_").and_then(|s| s.parse::<u32>().ok()).filter(|c| *c >= 1).unwrap_or(k as u32 + 1);
            let row: Vec<Vec3> = (f.index_begin..f.index_begin + f.num_indices).map(|c| { let t = uv.values[uv.indices[c as usize] as usize]; Vec3::new(t.x as f32, t.y as f32, 0.0) }).collect();
            face.uv.insert(channel, row);
        }
        im.faces.push(face);
    }
    let (mut mesh, map) = loom_mesh::obj::build_repaired_mapped(im)?;
    if groups.is_none() {
        let flat: Vec<bool> = mesh.faces().iter().map(|f| f.smoothing_groups == 0).collect();
        let hard = hard_edges(m);
        loom_mesh::mesh_mods::smoothing_groups_from_hard_edges(&mut mesh, &hard);
        for (f, flat) in mesh.faces_mut().iter_mut().zip(flat) {
            if flat {
                f.smoothing_groups = 0;
            }
        }
    }
    Ok((mesh, map))
}

/// The skin deformer of a mesh: (bone element id, geometry_to_bone) per cluster
/// and per built vertex its (cluster, weight) list.
fn convert_skin(m: &ufbx::Mesh, map: &[u32]) -> Option<(Vec<(u32, Mat4)>, Mat4, Vec<Vec<(u32, f32)>>)> {
    let sd = m.skin_deformers.first()?;
    let mut bones: Vec<(u32, Mat4)> = Vec::new();
    let mut cluster_index: Vec<Option<u32>> = Vec::new();
    for c in sd.clusters.iter() {
        match c.bone_node.as_ref() {
            Some(b) => {
                cluster_index.push(Some(bones.len() as u32));
                bones.push((b.element.element_id, m4(&c.geometry_to_bone)));
            }
            None => cluster_index.push(None),
        }
    }
    let mesh_bind = sd.clusters.first().map(|c| m4(&c.bind_to_world) * m4(&c.geometry_to_bone)).unwrap_or(Mat4::IDENTITY);
    let weights: Vec<Vec<(u32, f32)>> = map
        .iter()
        .map(|src| {
            let Some(v) = sd.vertices.get(*src as usize) else { return Vec::new() };
            let mut w: Vec<(u32, f32)> = (v.weight_begin..v.weight_begin + v.num_weights)
                .filter_map(|k| {
                    let sw = sd.weights.get(k as usize)?;
                    let ci = cluster_index.get(sw.cluster_index as usize).copied().flatten()?;
                    (sw.weight > 0.0).then_some((ci, sw.weight as f32))
                })
                .collect();
            let sum: f32 = w.iter().map(|x| x.1).sum();
            if sum > 1e-9 {
                for x in &mut w {
                    x.1 /= sum;
                }
            }
            w.sort_by_key(|x| x.0);
            w
        })
        .collect();
    Some((bones, mesh_bind, weights))
}

fn convert_blend_shapes(m: &ufbx::Mesh, map: &[u32]) -> Vec<FbxBlendShape> {
    let mut out = Vec::new();
    for bd in m.blend_deformers.iter() {
        for ch in bd.channels.iter() {
            let Some(shape) = ch.target_shape.as_ref() else { continue };
            let mut by_src: std::collections::HashMap<u32, Vec3> = std::collections::HashMap::new();
            for (i, v) in shape.offset_vertices.iter().enumerate() {
                if let Some(o) = shape.position_offsets.get(i) {
                    by_src.insert(*v, v3(*o));
                }
            }
            let offsets: Vec<(u32, Vec3)> = map.iter().enumerate().filter_map(|(i, src)| by_src.get(src).map(|o| (i as u32, *o))).collect();
            out.push(FbxBlendShape { name: ch.element.name.to_string(), offsets, weight: (ch.weight * 100.0) as f32 });
        }
    }
    out
}

/// Euler XYZ degrees of a quaternion, each axis unwrapped toward `prev`.
fn euler_near(q: ufbx::Quat, prev: Option<Vec3>) -> Vec3 {
    let mut r = euler_deg(q);
    if let Some(p) = prev {
        for i in 0..3 {
            r[i] += ((p[i] - r[i]) / 360.0).round() * 360.0;
        }
    }
    r
}

/// An animation stack (`take` by name, else the first), sampled every frame
/// of its span: the local transform of each node that moves (in the same
/// converted frame as `local_transform`). `bake_layers` off evaluates the
/// base layer only.
fn read_take(scene: &ufbx::Scene, index_of: &BTreeMap<u32, usize>, take: Option<&str>, bake_layers: bool) -> Option<crate::document::FbxTake> {
    let stack = match take {
        Some(name) => scene.anim_stacks.iter().find(|s| s.element.name == name)?,
        None => scene.anim_stacks.iter().next()?,
    };
    let anim: &ufbx::Anim = if bake_layers { &stack.anim } else { stack.layers.iter().next().map(|l| &*l.anim).unwrap_or(&stack.anim) };
    let fps = if scene.settings.frames_per_second > 0.0 { scene.settings.frames_per_second } else { 30.0 };
    let start = (stack.time_begin * fps).round() as i32;
    let end = (stack.time_end * fps).round() as i32;
    if end < start {
        return None;
    }
    let mut tracks = Vec::new();
    for n in scene.nodes.iter() {
        let Some(&index) = index_of.get(&n.element.element_id) else { continue };
        let mut a = crate::document::FbxAnim::default();
        let mut prev = None;
        for f in start..=end {
            let t = ufbx::evaluate_transform(anim, n, f as f64 / fps);
            let r = euler_near(t.rotation, prev);
            prev = Some(r);
            a.frames.push(f as f32);
            a.translation.push(v3(t.translation));
            a.rotation_deg.push(r);
            a.scale.push(v3(t.scale));
        }
        let moves = |v: &[Vec3]| v.iter().any(|x| x.distance(v[0]) > 1e-5);
        if moves(&a.translation) || moves(&a.rotation_deg) || moves(&a.scale) {
            tracks.push((index, a));
        }
    }
    Some(crate::document::FbxTake { name: stack.element.name.to_string(), fps: fps as f32, start, end, tracks })
}

/// A material's colours, factors and file textures.
fn convert_material(m: &ufbx::Material) -> FbxMaterial {
    let f = &m.fbx;
    let rgb = |x: &ufbx::MaterialMap| [x.value_vec4.x as f32, x.value_vec4.y as f32, x.value_vec4.z as f32];
    let num = |x: &ufbx::MaterialMap, d: f32| if x.has_value { x.value_vec4.x as f32 } else { d };
    let lambert = m.shading_model_name.to_ascii_lowercase().contains("lambert");
    let opacity = match m.element.props.find_prop("Opacity") {
        Some(p) => p.value_vec4.x as f32,
        None => 1.0 - num(&f.transparency_factor, 0.0),
    };
    let mut out = FbxMaterial {
        name: m.element.name.to_string(),
        shading: if lambert { "lambert".into() } else { "phong".into() },
        ambient: rgb(&f.ambient_color),
        diffuse: rgb(&f.diffuse_color),
        specular: rgb(&f.specular_color),
        specular_factor: if lambert { 0.0 } else { num(&f.specular_factor, 1.0) },
        shininess: num(&f.specular_exponent, 20.0),
        opacity: opacity.clamp(0.0, 1.0),
        // Self-illumination: the emitted colour's strength relative to the diffuse colour.
        emissive_factor: {
            let peak = |c: [f32; 3]| c.iter().fold(0.0f32, |a, b| a.max(*b));
            let d = peak(rgb(&f.diffuse_color));
            let e = num(&f.emission_factor, 0.0) * peak(rgb(&f.emission_color));
            if d > 1e-6 { (e / d).min(1.0) } else { e.min(1.0) }
        },
        textures: Vec::new(),
    };
    for t in m.textures.iter() {
        let tex = &t.texture;
        let path = if !tex.absolute_filename.is_empty() { tex.absolute_filename.to_string() } else { tex.filename.to_string() };
        let content = (!tex.content.is_empty()).then(|| tex.content.to_vec());
        let property = t.material_prop.to_string();
        let amount = if property == "Bump" || property == "NormalMap" { num(&f.bump_factor, 1.0) } else { 1.0 };
        out.textures.push(FbxTexture { property, path, relative: tex.relative_filename.to_string(), amount, content });
    }
    out
}

/// The file's statistics (Import dialog): version, creator, axes, units, contents.
pub fn info(bytes: &[u8]) -> Result<FbxInfo, String> {
    let opts = ufbx::LoadOpts { ignore_geometry: true, ignore_embedded: true, ..Default::default() };
    let scene = ufbx::load_memory(bytes, opts).map_err(|e| format!("FBX: {}", e.description))?;
    let axis = match scene.settings.axes.up {
        ufbx::CoordinateAxis::PositiveY | ufbx::CoordinateAxis::NegativeY => "Y-up",
        ufbx::CoordinateAxis::PositiveZ | ufbx::CoordinateAxis::NegativeZ => "Z-up",
        ufbx::CoordinateAxis::PositiveX | ufbx::CoordinateAxis::NegativeX => "X-up",
        _ => "unknown",
    };
    let creation_time = match crate::binary::parse(bytes) {
        Some(top) => top.iter().find(|r| r.name == "CreationTime").and_then(|r| match r.props.first() { Some(crate::binary::RawProp::Str(s)) => Some(s.clone()), _ => None }).unwrap_or_default(),
        None => String::from_utf8_lossy(bytes).lines().find_map(|l| l.trim().strip_prefix("CreationTime:").map(|v| v.trim().trim_matches('"').to_owned())).unwrap_or_default(),
    };
    Ok(FbxInfo {
        version: scene.metadata.version,
        ascii: scene.metadata.ascii,
        creator: scene.metadata.creator.to_string(),
        creation_time,
        up_axis: axis.into(),
        unit_metres: scene.settings.unit_meters,
        fps: scene.settings.frames_per_second,
        takes: scene.anim_stacks.iter().map(|s| s.element.name.to_string()).collect(),
        nodes: scene.nodes.iter().filter(|n| !n.is_root).count(),
        meshes: scene.meshes.len(),
        materials: scene.materials.len(),
        textures: scene.textures.len(),
        cameras: scene.cameras.len(),
        lights: scene.lights.len(),
        caches: scene.cache_files.len(),
    })
}

/// Metres per unit of the file's own distances (UnitScaleFactor is in centimetres).
fn file_unit_metres(scene: &ufbx::Scene) -> f64 {
    scene.settings.props.find_prop("UnitScaleFactor").map_or(0.01, |p| p.value_vec4.x * 0.01)
}

/// An FBX light; `unit` scales the file's distances to document units.
fn convert_light(l: &ufbx::Light, unit: f32) -> crate::document::FbxLight {
    let prop = |name: &str| l.element.props.find_prop(name);
    let real = |name: &str, d: f32| prop(name).map_or(d, |p| p.value_vec4.x as f32);
    let flag = |name: &str| prop(name).is_some_and(|p| p.value_int != 0);
    let range = |on: &str, a: &str, b: &str| flag(on).then(|| (real(a, 0.0) * unit, real(b, 0.0) * unit));
    crate::document::FbxLight {
        light_type: match l.type_ {
            ufbx::LightType::Directional => 1,
            ufbx::LightType::Spot => 2,
            _ => 0,
        },
        color: [l.color.x as f32, l.color.y as f32, l.color.z as f32],
        intensity: l.intensity as f32,
        inner_angle: l.inner_angle as f32,
        outer_angle: l.outer_angle as f32,
        cast_light: l.cast_light,
        cast_shadows: l.cast_shadows,
        decay: l.decay as u32,
        decay_start: real("DecayStart", 0.0) * unit,
        near: range("EnableNearAttenuation", "NearAttenuationStart", "NearAttenuationEnd"),
        far: range("EnableFarAttenuation", "FarAttenuationStart", "FarAttenuationEnd"),
    }
}

pub fn read(bytes: &[u8], target_unit_metres: f64) -> Result<FbxDocument, String> {
    read_with(bytes, &ReadOpts::new(target_unit_metres))
}

pub fn read_with(bytes: &[u8], ro: &ReadOpts) -> Result<FbxDocument, String> {
    let target_unit_metres = ro.unit_metres;
    let opts = ufbx::LoadOpts {
        target_axes: ufbx::CoordinateAxes::right_handed_z_up(),
        // Loom cameras look down local -Z with +Y up (FBX: +X forward).
        target_camera_axes: ufbx::CoordinateAxes { right: ufbx::CoordinateAxis::PositiveX, up: ufbx::CoordinateAxis::PositiveY, front: ufbx::CoordinateAxis::PositiveZ },
        // Loom lights aim down local -Z too.
        target_light_axes: ufbx::CoordinateAxes { right: ufbx::CoordinateAxis::PositiveX, up: ufbx::CoordinateAxis::PositiveY, front: ufbx::CoordinateAxis::PositiveZ },
        target_unit_meters: target_unit_metres,
        space_conversion: ufbx::SpaceConversion::ModifyGeometry,
        geometry_transform_handling: ufbx::GeometryTransformHandling::ModifyGeometry,
        generate_missing_normals: true,
        retain_dom: true,
        ..Default::default()
    };
    let scene = ufbx::load_memory(bytes, opts).map_err(|e| format!("FBX: {}", e.description))?;
    let smoothing = crate::binary::smoothing_by_geometry(bytes);
    let mut nodes: Vec<FbxNode> = Vec::new();
    let mut index_of: BTreeMap<u32, usize> = BTreeMap::new();
    // Skins resolve their bone nodes after every node has an index.
    let mut pending_skins: Vec<(usize, Vec<(u32, Mat4)>, Mat4, Vec<Vec<(u32, f32)>>)> = Vec::new();
    // Mesh element → the first node that shows it (later ones are instances).
    let mut first_of_mesh: BTreeMap<u32, usize> = BTreeMap::new();
    let mut instances: Vec<(usize, usize)> = Vec::new();
    let mut subdivision: Vec<(usize, u32, u32)> = Vec::new();
    for n in scene.nodes.iter() {
        if n.is_root {
            continue;
        }
        let mut skin = None;
        let mut blend_shapes = Vec::new();
        let kind = if let Some(m) = n.mesh.as_ref() {
            let materials: Vec<String> = m.materials.iter().map(|mat| mat.element.name.to_string()).collect();
            let (mesh, map) = convert_mesh(m, &smoothing, ro.smoothing_groups)?;
            if ro.skins {
                skin = convert_skin(m, &map);
            }
            if ro.morphs {
                blend_shapes = convert_blend_shapes(m, &map);
            }
            match first_of_mesh.get(&m.element.element_id) {
                Some(first) => instances.push((nodes.len(), *first)),
                None => {
                    first_of_mesh.insert(m.element.element_id, nodes.len());
                }
            }
            if m.subdivision_preview_levels > 0 || m.subdivision_render_levels > 0 {
                subdivision.push((nodes.len(), m.subdivision_preview_levels, m.subdivision_render_levels));
            }
            FbxKind::Mesh { mesh, materials }
        } else if let Some(c) = n.camera.as_ref() {
            // The horizontal angle: 3dLoom's FOV is horizontal (ufbx resolves the aperture mode).
            FbxKind::Camera { fov_deg: c.field_of_view_deg.x as f32 }
        } else if let Some(l) = n.light.as_ref() {
            FbxKind::Light(convert_light(l, (file_unit_metres(&scene) / target_unit_metres) as f32))
        } else if let Some(b) = n.bone.as_ref() {
            // A file without a bone size gives 0 (the Importer's Bone Creation sizes it).
            let sized = b.element.props.find_prop("Size").is_some();
            FbxKind::Bone { length: 0.0, size: if sized { (b.radius as f32).max(0.01) } else { 0.0 } }
        } else {
            FbxKind::Empty
        };
        if let Some((bones, mesh_bind, weights)) = skin {
            pending_skins.push((nodes.len(), bones, mesh_bind, weights));
        }
        let parent = n.parent.as_ref().filter(|p| !p.is_root).and_then(|p| index_of.get(&p.element.element_id).copied());
        index_of.insert(n.element.element_id, nodes.len());
        nodes.push(FbxNode {
            name: n.element.name.to_string(),
            parent,
            translation: v3(n.local_transform.translation),
            rotation_deg: euler_deg(n.local_transform.rotation),
            scale: v3(n.local_transform.scale),
            kind,
            skin: None,
            blend_shapes,
        });
    }
    for (node, bones, mesh_bind, weights) in pending_skins {
        let mut idx: Vec<(usize, Mat4)> = Vec::with_capacity(bones.len());
        let mut remap: Vec<Option<u32>> = Vec::with_capacity(bones.len());
        for (eid, g2b) in bones {
            match index_of.get(&eid) {
                Some(i) => {
                    remap.push(Some(idx.len() as u32));
                    idx.push((*i, g2b));
                }
                None => remap.push(None),
            }
        }
        let weights: Vec<Vec<(u32, f32)>> = weights.into_iter().map(|w| w.into_iter().filter_map(|(b, x)| remap.get(b as usize).copied().flatten().map(|nb| (nb, x))).collect()).collect();
        nodes[node].skin = Some(FbxSkin { bones: idx, mesh_bind, weights });
    }
    // A bone's length: the distance to its nearest child joint, else its parent's length, else its size.
    let n = nodes.len();
    for i in 0..n {
        if !matches!(nodes[i].kind, FbxKind::Bone { .. }) {
            continue;
        }
        let child = (0..n).filter(|c| nodes[*c].parent == Some(i)).map(|c| nodes[c].translation.length()).filter(|l| *l > 1e-4).fold(f32::INFINITY, f32::min);
        let FbxKind::Bone { length, size } = &mut nodes[i].kind else { unreachable!() };
        *length = if child.is_finite() { child } else { 0.0 };
        if *length <= 0.0 {
            let parent_len = nodes[i].parent.and_then(|p| match nodes[p].kind { FbxKind::Bone { length, .. } => Some(length), _ => None }).unwrap_or(0.0);
            let FbxKind::Bone { length, size } = &mut nodes[i].kind else { unreachable!() };
            *length = if parent_len > 0.0 { parent_len * 0.1 } else { (*size * 2.0).max(0.1) };
        } else {
            let _ = size;
        }
    }
    let anim = if ro.animation { read_take(&scene, &index_of, ro.take.as_deref(), ro.bake_layers) } else { None };
    let materials = scene.materials.iter().map(|m| convert_material(m)).collect();
    let selection_sets = scene
        .selection_sets
        .iter()
        .map(|set| {
            let members = set.nodes.iter().filter(|sn| sn.include_node).filter_map(|sn| sn.target_node.as_ref().and_then(|t| index_of.get(&t.element.element_id).copied())).collect();
            (set.element.name.to_string(), members)
        })
        .collect();
    // Display layers: name, colour, Show / Freeze and their nodes.
    let layers: Vec<FbxLayer> = scene
        .display_layers
        .iter()
        .map(|l| FbxLayer {
            name: l.element.name.to_string(),
            color: Vec3::new(l.ui_color.x as f32, l.ui_color.y as f32, l.ui_color.z as f32),
            visible: l.visible,
            frozen: l.frozen,
            nodes: l.nodes.iter().filter_map(|n| index_of.get(&n.element.element_id).copied()).collect(),
        })
        .collect();
    Ok(FbxDocument { nodes, unit_metres: target_unit_metres, anim, materials, instances, selection_sets, subdivision, layers })
}
