//! `FbxDocument` → FBX 7.4 / 7.5, binary or ASCII (Model / Geometry /
//! Material / Texture / Video / NodeAttribute objects with OO and OP
//! connections, selection sets).

use crate::binary::{encode, encode_version, object_name, p, Prop, Record};
use crate::document::{FbxDocument, FbxKind, FbxMaterial, FbxNode, WriteAxes, WriteOpts};
use glam::{Mat4, Vec3};
use loom_mesh::{FaceId, HalfEdgeId, Mesh};

/// Axis conversion, as the reference applies it: only root nodes turn
/// (the Z-up → Y-up change of basis, (x, y, z) → (x, z, -y), on the left of
/// their transform); children, geometry and bone-local data stay as they are.
fn axis(v: Vec3, axes: WriteAxes) -> Vec3 {
    match axes {
        WriteAxes::ZUp => v,
        WriteAxes::YUp => Vec3::new(v.x, v.z, -v.y),
    }
}

/// Loom's Euler XYZ (glam intrinsic) → FBX `Lcl Rotation` with
/// `RotationOrder` XYZ, which is extrinsic (Rz·Ry·Rx). A camera also turns
/// so its FBX forward (+X) is loom's forward (-Z) (`class` "Camera"; a
/// "Light" aims down FBX -Y; the document's cameras and lights look down
/// -Z); a root node in a Y-up file turns by the change of basis.
fn fbx_rotation(r: Vec3, axes: WriteAxes, class: &str, root: bool) -> Vec3 {
    let mut q = glam::Quat::from_euler(glam::EulerRot::XYZ, r.x.to_radians(), r.y.to_radians(), r.z.to_radians());
    match class {
        "Camera" => q *= glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        "Light" => q *= glam::Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        _ => {}
    }
    if axes == WriteAxes::YUp && root {
        q = glam::Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2) * q;
    }
    let (x, y, z) = q.to_euler(glam::EulerRot::XYZEx);
    Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
}

/// File units per document unit. By default the file is written in
/// centimetres (`UnitScaleFactor` 1) because Unreal's default import
/// ignores the scale factor; `file_unit_cm` picks another file unit.
fn cm_factor(doc: &FbxDocument, opts: &WriteOpts) -> f32 {
    (doc.unit_metres * 100.0 / opts.file_unit_cm.max(1e-12)) as f32
}

/// A world matrix in the written frame (Y-up: the change of basis on the left).
fn world(m: Mat4, axes: WriteAxes) -> Mat4 {
    match axes {
        WriteAxes::ZUp => m,
        WriteAxes::YUp => Mat4::from_cols(glam::Vec4::X, -glam::Vec4::Z, glam::Vec4::Y, glam::Vec4::W) * m,
    }
}

/// FBX time: 46186158000 ticks per second.
const KTIME_PER_SECOND: f64 = 46_186_158_000.0;

fn ktime(frame: f32, fps: f32) -> i64 {
    (frame as f64 / fps as f64 * KTIME_PER_SECOND).round() as i64
}

/// FBX `TimeMode` for a frame rate (14 = custom, with `CustomFrameRate`).
fn time_mode(fps: f32) -> i32 {
    match fps.round() as i32 {
        120 => 1,
        100 => 2,
        60 => 3,
        50 => 4,
        48 => 5,
        30 => 6,
        25 => 10,
        24 => 11,
        1000 => 12,
        96 => 15,
        72 => 16,
        _ => 14,
    }
}

/// One linear AnimationCurve.
fn anim_curve(id: i64, times: &[i64], values: &[f32]) -> Record {
    Record::with("AnimationCurve", vec![Prop::I64(id), object_name("", "AnimCurve"), Prop::Str("".into())])
        .child(Record::leaf("Default", vec![Prop::F64(values.first().copied().unwrap_or(0.0) as f64)]))
        .child(Record::leaf("KeyVer", vec![Prop::I32(4009)]))
        .child(Record::leaf("KeyTime", vec![Prop::ArrI64(times.to_vec())]))
        .child(Record::leaf("KeyValueFloat", vec![Prop::ArrF32(values.to_vec())]))
        // One shared attribute: linear interpolation (0x4).
        .child(Record::leaf("KeyAttrFlags", vec![Prop::ArrI32(vec![0x4])]))
        .child(Record::leaf("KeyAttrDataFloat", vec![Prop::ArrF32(vec![0.0, 0.0, 0.0, 0.0])]))
        .child(Record::leaf("KeyAttrRefCount", vec![Prop::ArrI32(vec![times.len() as i32])]))
}

struct Ids(i64);
impl Ids {
    fn next(&mut self) -> i64 {
        self.0 += 1;
        self.0
    }
}

fn header(opts: &WriteOpts) -> Record {
    Record::new("FBXHeaderExtension")
        .child(Record::leaf("FBXHeaderVersion", vec![Prop::I32(1003)]))
        .child(Record::leaf("FBXVersion", vec![Prop::I32(opts.version as i32)]))
        .child(Record::leaf("EncryptionType", vec![Prop::I32(0)]))
        .child(
            Record::new("CreationTimeStamp")
                .child(Record::leaf("Version", vec![Prop::I32(1000)]))
                .child(Record::leaf("Year", vec![Prop::I32(1970)]))
                .child(Record::leaf("Month", vec![Prop::I32(1)]))
                .child(Record::leaf("Day", vec![Prop::I32(1)]))
                .child(Record::leaf("Hour", vec![Prop::I32(10)]))
                .child(Record::leaf("Minute", vec![Prop::I32(0)]))
                .child(Record::leaf("Second", vec![Prop::I32(0)]))
                .child(Record::leaf("Millisecond", vec![Prop::I32(0)])),
        )
        .child(Record::leaf("Creator", vec![Prop::Str(opts.creator.clone())]))
}

fn global_settings(doc: &FbxDocument, axes: WriteAxes, file_unit_cm: f64) -> Record {
    let (up, up_sign, front, front_sign) = match axes {
        WriteAxes::ZUp => (2, 1, 1, -1),
        WriteAxes::YUp => (1, 1, 2, 1),
    };
    let cm = doc.unit_metres * 100.0;
    let fps = doc.anim.as_ref().map(|a| a.fps).unwrap_or(30.0);
    let span = doc.anim.as_ref().map(|a| (a.start, a.end)).unwrap_or((0, 30));
    let int = |n: &str, v: i32| p(n, "int", "Integer", "", vec![Prop::I32(v)]);
    Record::new("GlobalSettings").child(Record::leaf("Version", vec![Prop::I32(1000)])).child(
        Record::new("Properties70")
            .child(int("UpAxis", up))
            .child(int("UpAxisSign", up_sign))
            .child(int("FrontAxis", front))
            .child(int("FrontAxisSign", front_sign))
            .child(int("CoordAxis", 0))
            .child(int("CoordAxisSign", 1))
            .child(int("OriginalUpAxis", up))
            .child(int("OriginalUpAxisSign", up_sign))
            .child(p("UnitScaleFactor", "double", "Number", "", vec![Prop::F64(file_unit_cm)]))
            .child(p("OriginalUnitScaleFactor", "double", "Number", "", vec![Prop::F64(cm)]))
            .child(p("AmbientColor", "ColorRGB", "Color", "", vec![Prop::F64(0.0), Prop::F64(0.0), Prop::F64(0.0)]))
            .child(p("DefaultCamera", "KString", "", "", vec![Prop::Str("Producer Perspective".into())]))
            .child(p("TimeMode", "enum", "", "", vec![Prop::I32(time_mode(fps))]))
            .child(p("TimeSpanStart", "KTime", "Time", "", vec![Prop::I64(ktime(span.0 as f32, fps))]))
            .child(p("TimeSpanStop", "KTime", "Time", "", vec![Prop::I64(ktime(span.1 as f32, fps))]))
            .child(p("CustomFrameRate", "double", "Number", "", vec![Prop::F64(if time_mode(fps) == 14 { fps as f64 } else { -1.0 })])),
    )
}

fn model(id: i64, n: &FbxNode, class: &str, axes: WriteAxes, factor: f32) -> Record {
    let root = n.parent.is_none();
    let t = if root { axis(n.translation * factor, axes) } else { n.translation * factor };
    let r = fbx_rotation(n.rotation_deg, axes, class, root);
    let s = n.scale;
    let v3 = |name: &str, v: Vec3| p(name, name, "", "A", vec![Prop::F64(v.x as f64), Prop::F64(v.y as f64), Prop::F64(v.z as f64)]);
    Record::with("Model", vec![Prop::I64(id), object_name(&n.name, "Model"), Prop::Str(class.into())])
        .child(Record::leaf("Version", vec![Prop::I32(232)]))
        .child(
            Record::new("Properties70")
                .child(p("RotationActive", "bool", "", "", vec![Prop::I32(1)]))
                .child(p("InheritType", "enum", "", "", vec![Prop::I32(1)]))
                .child(p("RotationOrder", "enum", "", "", vec![Prop::I32(0)]))
                .child(p("DefaultAttributeIndex", "int", "Integer", "", vec![Prop::I32(0)]))
                .child(v3("Lcl Translation", t))
                .child(v3("Lcl Rotation", r))
                .child(v3("Lcl Scaling", s)),
        )
        .child(Record::leaf("Shading", vec![Prop::Bool(true)]))
        .child(Record::leaf("Culling", vec![Prop::Str("CullingOff".into())]))
}

fn layer_element(kind: &str, index: i32, name: &str, version: i32, mapping: &str, reference: &str) -> Record {
    Record::with(kind, vec![Prop::I32(index)])
        .child(Record::leaf("Version", vec![Prop::I32(version)]))
        .child(Record::leaf("Name", vec![Prop::Str(name.into())]))
        .child(Record::leaf("MappingInformationType", vec![Prop::Str(mapping.into())]))
        .child(Record::leaf("ReferenceInformationType", vec![Prop::Str(reference.into())]))
}

fn layer_entry(ty: &str, index: i32) -> Record {
    Record::new("LayerElement").child(Record::leaf("Type", vec![Prop::Str(ty.into())])).child(Record::leaf("TypedIndex", vec![Prop::I32(index)]))
}

/// Per corner (in `face_corners` order, face by face) a tangent and a
/// binormal from map channel 1: each face's uv gradient, averaged over the
/// corners that share a vertex, normal and uv, then made orthogonal to the
/// corner normal. None without channel 1.
fn corner_tangents(mesh: &Mesh, normals: &[Vec3]) -> Option<Vec<(Vec3, Vec3)>> {
    let map = mesh.map(1)?;
    let pos = |h: HalfEdgeId| mesh.vertices()[mesh.half_edges()[h.0 as usize].origin.0 as usize].position;
    let uv = |h: HalfEdgeId| map.verts[map.corners[h.0 as usize] as usize];
    let mut raw: Vec<(u32, Vec3, Vec3)> = Vec::new(); // (half-edge, tangent, bitangent)
    for f in 0..mesh.face_count() {
        let corners = mesh.face_corners(FaceId(f as u32)).unwrap_or_default();
        let k = corners.len();
        for i in 0..k {
            let he = corners[i];
            let (nx, pv) = (corners[(i + 1) % k], corners[(i + k - 1) % k]);
            let (dp1, dp2) = (pos(nx) - pos(he), pos(pv) - pos(he));
            let (d1, d2) = (uv(nx) - uv(he), uv(pv) - uv(he));
            let det = d1.x * d2.y - d2.x * d1.y;
            let (t, b) = if det.abs() > 1e-12 { ((dp1 * d2.y - dp2 * d1.y) / det, (dp2 * d1.x - dp1 * d2.x) / det) } else { (Vec3::ZERO, Vec3::ZERO) };
            raw.push((he.0, t, b));
        }
    }
    let key = |he: u32| {
        let n = normals[he as usize];
        let v = mesh.half_edges()[he as usize].origin.0;
        (v, map.corners[he as usize], [(n.x * 1e4).round() as i32, (n.y * 1e4).round() as i32, (n.z * 1e4).round() as i32])
    };
    let mut sum: std::collections::HashMap<(u32, u32, [i32; 3]), (Vec3, Vec3)> = std::collections::HashMap::new();
    for (he, t, b) in &raw {
        let e = sum.entry(key(*he)).or_insert((Vec3::ZERO, Vec3::ZERO));
        e.0 += *t;
        e.1 += *b;
    }
    Some(
        raw.iter()
            .map(|(he, _, _)| {
                let (t, b) = sum[&key(*he)];
                let n = normals[*he as usize].normalize_or_zero();
                let mut t = (t - n * n.dot(t)).normalize_or_zero();
                if t == Vec3::ZERO {
                    t = n.any_orthonormal_vector();
                }
                let sign = if n.cross(t).dot(b) < 0.0 { -1.0 } else { 1.0 };
                (t, n.cross(t) * sign)
            })
            .collect(),
    )
}

/// The Geometry record and, when `split_normals` splits control points,
/// per mesh vertex the control points it became.
fn geometry(id: i64, name: &str, mesh: &Mesh, opts: &WriteOpts, factor: f32, subdivision: Option<(u32, u32)>) -> (Record, Option<Vec<Vec<u32>>>) {
    let tri;
    let mesh = if opts.triangulate {
        tri = loom_mesh::mesh_mods::triangulate(mesh).unwrap_or_else(|_| mesh.clone());
        &tri
    } else {
        mesh
    };
    let normals_by_corner = loom_mesh::mesh_mods::corner_normals(mesh);
    // Split per-vertex normals: one control point per (vertex, distinct normal).
    let mut control: Vec<Vec3> = mesh.vertices().iter().map(|v| v.position).collect();
    let mut point_of_corner: Vec<u32> = mesh.half_edges().iter().map(|h| h.origin.0).collect();
    let mut split: Option<Vec<Vec<u32>>> = None;
    if opts.split_normals {
        let mut points: std::collections::HashMap<(u32, [i32; 3]), u32> = std::collections::HashMap::new();
        let mut of_vertex: Vec<Vec<u32>> = vec![Vec::new(); mesh.vertex_count()];
        control.clear();
        for f in 0..mesh.face_count() {
            for he in mesh.face_corners(FaceId(f as u32)).unwrap_or_default() {
                let v = mesh.half_edges()[he.0 as usize].origin.0;
                let n = normals_by_corner[he.0 as usize];
                let k = (v, [(n.x * 1e4).round() as i32, (n.y * 1e4).round() as i32, (n.z * 1e4).round() as i32]);
                let cp = *points.entry(k).or_insert_with(|| {
                    control.push(mesh.vertices()[v as usize].position);
                    of_vertex[v as usize].push(control.len() as u32 - 1);
                    control.len() as u32 - 1
                });
                point_of_corner[he.0 as usize] = cp;
            }
        }
        // A vertex no face uses keeps a point of its own.
        for (v, list) in of_vertex.iter_mut().enumerate() {
            if list.is_empty() {
                control.push(mesh.vertices()[v].position);
                list.push(control.len() as u32 - 1);
            }
        }
        split = Some(of_vertex);
    }
    let mut vertices: Vec<f64> = Vec::with_capacity(control.len() * 3);
    for v in &control {
        let q = *v * factor;
        vertices.extend([q.x as f64, q.y as f64, q.z as f64]);
    }
    let mut indices: Vec<i32> = Vec::new();
    let mut normals: Vec<f64> = Vec::new();
    let mut corner_hes: Vec<u32> = Vec::new();
    let mut smoothing: Vec<i32> = Vec::with_capacity(mesh.face_count());
    let mut materials: Vec<i32> = Vec::with_capacity(mesh.face_count());
    for f in 0..mesh.face_count() {
        let fid = FaceId(f as u32);
        let corners = mesh.face_corners(fid).unwrap_or_default();
        let k = corners.len();
        for (i, he) in corners.iter().enumerate() {
            let v = point_of_corner[he.0 as usize] as i32;
            indices.push(if i + 1 == k { -(v + 1) } else { v });
            let n = normals_by_corner[he.0 as usize];
            normals.extend([n.x as f64, n.y as f64, n.z as f64]);
            corner_hes.push(he.0);
        }
        let face = &mesh.faces()[f];
        smoothing.push(face.smoothing_groups as i32);
        materials.push(face.material_id.max(1) as i32 - 1);
    }
    let mut g = Record::with("Geometry", vec![Prop::I64(id), object_name(name, "Geometry"), Prop::Str("Mesh".into())])
        .child(Record::new("Properties70"))
        .child(Record::leaf("Vertices", vec![Prop::ArrF64(vertices)]))
        .child(Record::leaf("PolygonVertexIndex", vec![Prop::ArrI32(indices)]))
        .child(Record::leaf("GeometryVersion", vec![Prop::I32(124)]));
    if let Some((preview, render)) = subdivision {
        g = g
            .child(Record::leaf("PreviewDivisionLevels", vec![Prop::I32(preview as i32)]))
            .child(Record::leaf("RenderDivisionLevels", vec![Prop::I32(render as i32)]))
            .child(Record::leaf("Smoothness", vec![Prop::I32(3)]));
    }
    g = g.child(layer_element("LayerElementNormal", 0, "", 101, "ByPolygonVertex", "Direct").child(Record::leaf("Normals", vec![Prop::ArrF64(normals)])));
    let tangents = if opts.tangents { corner_tangents(mesh, &normals_by_corner) } else { None };
    if let Some(tb) = &tangents {
        let flat = |pick: fn(&(Vec3, Vec3)) -> Vec3| -> Vec<f64> {
            tb.iter()
                .flat_map(|x| {
                    let v = pick(x);
                    [v.x as f64, v.y as f64, v.z as f64]
                })
                .collect()
        };
        g = g
            .child(layer_element("LayerElementTangent", 0, "UVChannel_1", 102, "ByPolygonVertex", "Direct").child(Record::leaf("Tangents", vec![Prop::ArrF64(flat(|x| x.0))])))
            .child(layer_element("LayerElementBinormal", 0, "UVChannel_1", 102, "ByPolygonVertex", "Direct").child(Record::leaf("Binormals", vec![Prop::ArrF64(flat(|x| x.1))])));
    }
    // UV channels: channel k ≥ 1 → LayerElementUV k-1 in layer k-1.
    let mut uv_layers: Vec<i32> = Vec::new();
    for (ch, map) in mesh.texture_maps() {
        if *ch == 0 {
            continue;
        }
        let idx = (*ch as i32) - 1;
        let mut uv: Vec<f64> = Vec::with_capacity(map.verts.len() * 2);
        for t in &map.verts {
            uv.extend([t.x as f64, t.y as f64]);
        }
        let uv_index: Vec<i32> = corner_hes.iter().map(|he| map.corners[*he as usize] as i32).collect();
        g = g.child(
            layer_element("LayerElementUV", idx, &format!("UVChannel_{ch}"), 101, "ByPolygonVertex", "IndexToDirect")
                .child(Record::leaf("UV", vec![Prop::ArrF64(uv)]))
                .child(Record::leaf("UVIndex", vec![Prop::ArrI32(uv_index)])),
        );
        uv_layers.push(idx);
    }
    let has_colour = mesh.vertices().iter().any(|v| v.color != [1.0; 3] || v.alpha != 1.0);
    if has_colour {
        let mut colours: Vec<f64> = Vec::with_capacity(mesh.vertex_count() * 4);
        for v in mesh.vertices() {
            colours.extend([v.color[0] as f64, v.color[1] as f64, v.color[2] as f64, v.alpha as f64]);
        }
        let index: Vec<i32> = corner_hes.iter().map(|he| mesh.half_edges()[*he as usize].origin.0 as i32).collect();
        g = g.child(
            layer_element("LayerElementColor", 0, "VertexColors", 101, "ByPolygonVertex", "IndexToDirect")
                .child(Record::leaf("Colors", vec![Prop::ArrF64(colours)]))
                .child(Record::leaf("ColorIndex", vec![Prop::ArrI32(index)])),
        );
    }
    if opts.smoothing_layer {
        g = g.child(layer_element("LayerElementSmoothing", 0, "", 102, "ByPolygon", "Direct").child(Record::leaf("Smoothing", vec![Prop::ArrI32(smoothing)])));
    }
    g = g.child(layer_element("LayerElementMaterial", 0, "", 101, "ByPolygon", "IndexToDirect").child(Record::leaf("Materials", vec![Prop::ArrI32(materials)])));
    // Layer 0 carries everything plus the first UV; extra UVs get their own layer.
    let mut layer0 = Record::with("Layer", vec![Prop::I32(0)]).child(Record::leaf("Version", vec![Prop::I32(100)])).child(layer_entry("LayerElementNormal", 0));
    if tangents.is_some() {
        layer0 = layer0.child(layer_entry("LayerElementTangent", 0)).child(layer_entry("LayerElementBinormal", 0));
    }
    if let Some(first) = uv_layers.first() {
        layer0 = layer0.child(layer_entry("LayerElementUV", *first));
    }
    if has_colour {
        layer0 = layer0.child(layer_entry("LayerElementColor", 0));
    }
    if opts.smoothing_layer {
        layer0 = layer0.child(layer_entry("LayerElementSmoothing", 0));
    }
    layer0 = layer0.child(layer_entry("LayerElementMaterial", 0));
    g = g.child(layer0);
    for (n, idx) in uv_layers.iter().enumerate().skip(1) {
        g = g.child(Record::with("Layer", vec![Prop::I32(n as i32)]).child(Record::leaf("Version", vec![Prop::I32(100)])).child(layer_entry("LayerElementUV", *idx)));
    }
    (g, split)
}

/// A Phong / Lambert material with its colours and factors.
fn material(id: i64, m: &FbxMaterial) -> Record {
    let col = |n: &str, c: [f32; 3]| p(n, "Color", "", "A", c.iter().map(|x| Prop::F64(*x as f64)).collect());
    let num = |n: &str, v: f32| p(n, "Number", "", "A", vec![Prop::F64(v as f64)]);
    let phong = m.shading != "lambert";
    let bump = m.textures.iter().find(|t| t.property == "Bump" || t.property == "NormalMap").map_or(1.0, |t| t.amount);
    let mut props = Record::new("Properties70")
        .child(p("ShadingModel", "KString", "", "", vec![Prop::Str(if phong { "Phong" } else { "Lambert" }.into())]))
        .child(col("EmissiveColor", m.diffuse))
        .child(num("EmissiveFactor", m.emissive_factor))
        .child(col("AmbientColor", m.ambient))
        .child(num("AmbientFactor", 1.0))
        .child(col("DiffuseColor", m.diffuse))
        .child(num("DiffuseFactor", 1.0))
        .child(col("TransparentColor", [1.0; 3]))
        .child(num("TransparencyFactor", 1.0 - m.opacity))
        .child(num("Opacity", m.opacity))
        .child(p("BumpFactor", "double", "Number", "", vec![Prop::F64(bump as f64)]));
    if phong {
        props = props.child(col("SpecularColor", m.specular)).child(num("SpecularFactor", m.specular_factor)).child(num("ShininessExponent", m.shininess)).child(num("Shininess", m.shininess));
    }
    Record::with("Material", vec![Prop::I64(id), object_name(&m.name, "Material"), Prop::Str("".into())])
        .child(Record::leaf("Version", vec![Prop::I32(102)]))
        .child(Record::leaf("ShadingModel", vec![Prop::Str(if phong { "phong" } else { "lambert" }.into())]))
        .child(Record::leaf("MultiLayer", vec![Prop::I32(0)]))
        .child(props)
}

/// The written path of a texture: absolute, plus relative to the file's folder when it is inside it.
fn texture_paths(path: &str, opts: &WriteOpts) -> (String, String) {
    let p = std::path::Path::new(path);
    let rel = opts.file_dir.as_ref().and_then(|d| p.strip_prefix(d).ok()).map(|r| r.to_string_lossy().into_owned()).unwrap_or_else(|| p.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default());
    (path.to_owned(), rel)
}

fn video(id: i64, name: &str, abs: &str, rel: &str, content: Option<Vec<u8>>) -> Record {
    let mut v = Record::with("Video", vec![Prop::I64(id), object_name(name, "Video"), Prop::Str("Clip".into())])
        .child(Record::leaf("Type", vec![Prop::Str("Clip".into())]))
        .child(Record::new("Properties70").child(p("Path", "KString", "XRefUrl", "", vec![Prop::Str(abs.into())])))
        .child(Record::leaf("UseMipMap", vec![Prop::I32(0)]))
        .child(Record::leaf("Filename", vec![Prop::Str(abs.into())]))
        .child(Record::leaf("RelativeFilename", vec![Prop::Str(rel.into())]));
    if let Some(bytes) = content {
        v = v.child(Record::leaf("Content", vec![Prop::Raw(bytes)]));
    }
    v
}

fn texture(id: i64, name: &str, abs: &str, rel: &str) -> Record {
    Record::with("Texture", vec![Prop::I64(id), object_name(name, "Texture"), Prop::Str("".into())])
        .child(Record::leaf("Type", vec![Prop::Str("TextureVideoClip".into())]))
        .child(Record::leaf("Version", vec![Prop::I32(202)]))
        .child(Record::leaf("TextureName", vec![object_name(name, "Texture")]))
        .child(Record::new("Properties70").child(p("UVSet", "KString", "", "", vec![Prop::Str("UVChannel_1".into())])).child(p("UseMaterial", "bool", "", "", vec![Prop::I32(1)])))
        .child(Record::leaf("Media", vec![object_name(name, "Video")]))
        .child(Record::leaf("FileName", vec![Prop::Str(abs.into())]))
        .child(Record::leaf("RelativeFilename", vec![Prop::Str(rel.into())]))
        .child(Record::leaf("ModelUVTranslation", vec![Prop::F64(0.0), Prop::F64(0.0)]))
        .child(Record::leaf("ModelUVScaling", vec![Prop::F64(1.0), Prop::F64(1.0)]))
        .child(Record::leaf("Texture_Alpha_Source", vec![Prop::Str("None".into())]))
        .child(Record::leaf("Cropping", vec![Prop::I32(0), Prop::I32(0), Prop::I32(0), Prop::I32(0)]))
}

/// A camera with its horizontal field of view (`ApertureMode` 1 =
/// Horizontal), the matching focal length on a 36 × 27 mm film back (the
/// document's lens/FOV link and its 4:3 render aspect), and a 640 × 480
/// resolution. Readers that go by the lens (Blender) and ones that go by
/// the angle get the same view.
fn camera_attribute(id: i64, name: &str, fov_deg: f32) -> Record {
    const FILM_W_MM: f64 = 36.0;
    const FILM_H_MM: f64 = 27.0;
    let focal = FILM_W_MM * 0.5 / (fov_deg as f64 * 0.5).to_radians().tan();
    let inch = |mm: f64| mm / 25.4;
    Record::with("NodeAttribute", vec![Prop::I64(id), object_name(name, "NodeAttribute"), Prop::Str("Camera".into())])
        .child(
            Record::new("Properties70")
                .child(p("FieldOfView", "FieldOfView", "", "A", vec![Prop::F64(fov_deg as f64)]))
                .child(p("FocalLength", "Number", "", "A", vec![Prop::F64(focal)]))
                .child(p("FilmWidth", "double", "Number", "", vec![Prop::F64(inch(FILM_W_MM))]))
                .child(p("FilmHeight", "double", "Number", "", vec![Prop::F64(inch(FILM_H_MM))]))
                .child(p("FilmAspectRatio", "double", "Number", "", vec![Prop::F64(FILM_W_MM / FILM_H_MM)]))
                .child(p("ApertureMode", "enum", "", "", vec![Prop::I32(1)]))
                .child(p("GateFit", "enum", "", "", vec![Prop::I32(2)]))
                .child(p("AspectRatioMode", "enum", "", "", vec![Prop::I32(2)]))
                .child(p("AspectWidth", "double", "Number", "", vec![Prop::F64(640.0)]))
                .child(p("AspectHeight", "double", "Number", "", vec![Prop::F64(480.0)]))
                .child(p("NearPlane", "double", "Number", "", vec![Prop::F64(1.0)]))
                .child(p("FarPlane", "double", "Number", "", vec![Prop::F64(100000.0)])),
        )
        .child(Record::leaf("TypeFlags", vec![Prop::Str("Camera".into())]))
        .child(Record::leaf("GeometryVersion", vec![Prop::I32(124)]))
}

fn light_attribute(id: i64, name: &str, l: &crate::document::FbxLight, factor: f32) -> Record {
    let num = |n: &str, v: f32| p(n, "Number", "", "A", vec![Prop::F64(v as f64)]);
    let flag = |n: &str, v: bool| p(n, "bool", "", "", vec![Prop::I32(v as i32)]);
    let (near, far) = (l.near.unwrap_or((0.0, 0.0)), l.far.unwrap_or((0.0, 0.0)));
    Record::with("NodeAttribute", vec![Prop::I64(id), object_name(name, "NodeAttribute"), Prop::Str("Light".into())])
        .child(
            Record::new("Properties70")
                .child(p("LightType", "enum", "", "", vec![Prop::I32(l.light_type as i32)]))
                .child(p("Color", "Color", "", "A", l.color.iter().map(|c| Prop::F64(*c as f64)).collect()))
                .child(num("Intensity", l.intensity * 100.0))
                .child(num("InnerAngle", l.inner_angle))
                .child(num("OuterAngle", l.outer_angle))
                .child(flag("CastLightOnObject", l.cast_light))
                .child(flag("CastShadows", l.cast_shadows))
                .child(p("DecayType", "enum", "", "", vec![Prop::I32(l.decay as i32)]))
                .child(num("DecayStart", l.decay_start * factor))
                .child(flag("EnableNearAttenuation", l.near.is_some()))
                .child(num("NearAttenuationStart", near.0 * factor))
                .child(num("NearAttenuationEnd", near.1 * factor))
                .child(flag("EnableFarAttenuation", l.far.is_some()))
                .child(num("FarAttenuationStart", far.0 * factor))
                .child(num("FarAttenuationEnd", far.1 * factor)),
        )
        .child(Record::leaf("TypeFlags", vec![Prop::Str("Light".into())]))
        .child(Record::leaf("GeometryVersion", vec![Prop::I32(124)]))
}

/// A matrix as FBX stores it (column-major 16 doubles), its translation scaled to centimetres.
fn matrix16(m: Mat4, factor: f32) -> Vec<f64> {
    let mut c = m.to_cols_array();
    c[12] *= factor;
    c[13] *= factor;
    c[14] *= factor;
    c.iter().map(|x| *x as f64).collect()
}

fn limb_node_attribute(id: i64, name: &str, size: f32, factor: f32) -> Record {
    Record::with("NodeAttribute", vec![Prop::I64(id), object_name(name, "NodeAttribute"), Prop::Str("LimbNode".into())])
        .child(Record::new("Properties70").child(p("Size", "double", "Number", "", vec![Prop::F64((size * factor) as f64)])))
        .child(Record::leaf("TypeFlags", vec![Prop::Str("Skeleton".into())]))
}

/// Names of the materials a mesh node writes: `materials` padded with
/// `Material #<id>` up to the highest material id its faces use.
pub fn material_names(mesh: &Mesh, materials: &[String]) -> Vec<String> {
    let hi = mesh.faces().iter().map(|f| f.material_id.max(1) as usize).max().unwrap_or(0);
    (0..hi.max(materials.len())).map(|i| materials.get(i).cloned().unwrap_or_else(|| format!("Material #{}", i + 1))).collect()
}

pub fn write(doc: &FbxDocument, opts: &WriteOpts) -> Vec<u8> {
    let factor = cm_factor(doc, opts);
    let mut ids = Ids(1000);
    let mut objects = Record::new("Objects");
    let mut connections = Record::new("Connections");
    let mut model_ids: Vec<i64> = Vec::with_capacity(doc.nodes.len());
    let (mut n_model, mut n_geom, mut n_mat, mut n_attr) = (0, 0, 0, 0);
    let mut material_ids: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    let (mut n_deformer, mut n_pose) = (0, 0);
    // Pose nodes: (model id, world at bind) for the bind pose.
    let mut pose_nodes: Vec<(i64, Mat4)> = Vec::new();
    // Skins and blend shapes are written after every model has an id.
    let mut deferred: Vec<(usize, i64, i64)> = Vec::new(); // (node index, model id, geometry id)
    // Instances: a node that shares an earlier node's geometry connects to it.
    let shares: std::collections::HashMap<usize, usize> = doc.instances.iter().copied().collect();
    let mut geometry_ids: std::collections::HashMap<usize, i64> = std::collections::HashMap::new();
    // Split control points per node (Split per-vertex Normals).
    let mut splits: std::collections::HashMap<usize, Vec<Vec<u32>>> = std::collections::HashMap::new();
    let mut written_materials: Vec<(i64, FbxMaterial)> = Vec::new();
    for (index, n) in doc.nodes.iter().enumerate() {
        let mid = ids.next();
        model_ids.push(mid);
        n_model += 1;
        match &n.kind {
            FbxKind::Mesh { mesh, materials } => {
                objects.children.push(model(mid, n, "Mesh", opts.axes, factor));
                let shared = shares.get(&index).and_then(|src| geometry_ids.get(src)).copied();
                let gid = match shared {
                    Some(g) => g,
                    None => {
                        let gid = ids.next();
                        n_geom += 1;
                        let sub = doc.subdivision.iter().find(|s| s.0 == index).map(|s| (s.1, s.2));
                        let (g, split) = geometry(gid, &n.name, mesh, opts, factor, sub);
                        objects.children.push(g);
                        if let Some(s) = split {
                            splits.insert(index, s);
                        }
                        gid
                    }
                };
                geometry_ids.insert(index, gid);
                connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(gid), Prop::I64(mid)]));
                if shared.is_none() && (n.skin.is_some() || !n.blend_shapes.is_empty()) {
                    deferred.push((index, mid, gid));
                }
                for name in material_names(mesh, materials) {
                    let matid = *material_ids.entry(name.clone()).or_insert_with(|| {
                        let id = ids.next();
                        n_mat += 1;
                        let m = doc.materials.iter().find(|m| m.name == name).cloned().unwrap_or_else(|| FbxMaterial::new(&name));
                        objects.children.push(material(id, &m));
                        written_materials.push((id, m));
                        id
                    });
                    connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(matid), Prop::I64(mid)]));
                }
            }
            FbxKind::Camera { fov_deg } => {
                objects.children.push(model(mid, n, "Camera", opts.axes, factor));
                let aid = ids.next();
                n_attr += 1;
                objects.children.push(camera_attribute(aid, &n.name, *fov_deg));
                connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(aid), Prop::I64(mid)]));
            }
            FbxKind::Light(light) => {
                objects.children.push(model(mid, n, "Light", opts.axes, factor));
                let aid = ids.next();
                n_attr += 1;
                objects.children.push(light_attribute(aid, &n.name, light, factor));
                connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(aid), Prop::I64(mid)]));
            }
            FbxKind::Bone { size, .. } => {
                objects.children.push(model(mid, n, "LimbNode", opts.axes, factor));
                let aid = ids.next();
                n_attr += 1;
                objects.children.push(limb_node_attribute(aid, &n.name, *size, factor));
                connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(aid), Prop::I64(mid)]));
            }
            FbxKind::Empty => objects.children.push(model(mid, n, "Null", opts.axes, factor)),
        }
    }
    for (ni, mid, gid) in deferred {
        let n = &doc.nodes[ni];
        // A mesh vertex → the control points written for it.
        let points = |v: u32| -> Vec<u32> { splits.get(&ni).and_then(|s| s.get(v as usize)).cloned().unwrap_or_else(|| vec![v]) };
        if let Some(skin) = &n.skin {
            let sid = ids.next();
            n_deformer += 1;
            objects.children.push(
                Record::with("Deformer", vec![Prop::I64(sid), object_name(&n.name, "Deformer"), Prop::Str("Skin".into())])
                    .child(Record::leaf("Version", vec![Prop::I32(101)]))
                    .child(Record::leaf("Link_DeformAcuracy", vec![Prop::F64(50.0)]))
                    .child(Record::leaf("SkinningType", vec![Prop::Str("Linear".into())])),
            );
            connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(sid), Prop::I64(gid)]));
            pose_nodes.push((mid, skin.mesh_bind));
            for (bi, (bone_index, g2b)) in skin.bones.iter().enumerate() {
                let Some(bone_mid) = model_ids.get(*bone_index).copied() else { continue };
                let cid = ids.next();
                n_deformer += 1;
                let mut indexes: Vec<i32> = Vec::new();
                let mut weights: Vec<f64> = Vec::new();
                for (v, w) in skin.weights.iter().enumerate() {
                    for (b, x) in w {
                        if *b as usize == bi {
                            for cp in points(v as u32) {
                                indexes.push(cp as i32);
                                weights.push(*x as f64);
                            }
                        }
                    }
                }
                let bone_bind = skin.mesh_bind * g2b.inverse();
                objects.children.push(
                    Record::with("Deformer", vec![Prop::I64(cid), object_name(&doc.nodes[*bone_index].name, "SubDeformer"), Prop::Str("Cluster".into())])
                        .child(Record::leaf("Version", vec![Prop::I32(100)]))
                        .child(Record::leaf("UserData", vec![Prop::Str("".into()), Prop::Str("".into())]))
                        .child(Record::leaf("Indexes", vec![Prop::ArrI32(indexes)]))
                        .child(Record::leaf("Weights", vec![Prop::ArrF64(weights)]))
                        .child(Record::leaf("Transform", vec![Prop::ArrF64(matrix16(*g2b, factor))]))
                        .child(Record::leaf("TransformLink", vec![Prop::ArrF64(matrix16(world(bone_bind, opts.axes), factor))])),
                );
                connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(cid), Prop::I64(sid)]));
                connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(bone_mid), Prop::I64(cid)]));
                if !pose_nodes.iter().any(|(id, _)| *id == bone_mid) {
                    pose_nodes.push((bone_mid, bone_bind));
                }
            }
        }
        if !n.blend_shapes.is_empty() {
            let bid = ids.next();
            n_deformer += 1;
            objects.children.push(Record::with("Deformer", vec![Prop::I64(bid), object_name(&n.name, "Deformer"), Prop::Str("BlendShape".into())]).child(Record::leaf("Version", vec![Prop::I32(100)])));
            connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(bid), Prop::I64(gid)]));
            for bs in &n.blend_shapes {
                let chid = ids.next();
                n_deformer += 1;
                let shid = ids.next();
                n_geom += 1;
                let offsets: Vec<(u32, Vec3)> = bs.offsets.iter().flat_map(|(v, o)| points(*v).into_iter().map(move |cp| (cp, *o))).collect();
                let indexes: Vec<i32> = offsets.iter().map(|(v, _)| *v as i32).collect();
                let verts: Vec<f64> = offsets.iter().flat_map(|(_, o)| { let q = *o * factor; [q.x as f64, q.y as f64, q.z as f64] }).collect();
                let normals: Vec<f64> = vec![0.0; offsets.len() * 3];
                objects.children.push(
                    Record::with("Geometry", vec![Prop::I64(shid), object_name(&bs.name, "Geometry"), Prop::Str("Shape".into())])
                        .child(Record::leaf("Version", vec![Prop::I32(100)]))
                        .child(Record::leaf("Indexes", vec![Prop::ArrI32(indexes)]))
                        .child(Record::leaf("Vertices", vec![Prop::ArrF64(verts)]))
                        .child(Record::leaf("Normals", vec![Prop::ArrF64(normals)])),
                );
                objects.children.push(
                    Record::with("Deformer", vec![Prop::I64(chid), object_name(&bs.name, "SubDeformer"), Prop::Str("BlendShapeChannel".into())])
                        .child(Record::leaf("Version", vec![Prop::I32(100)]))
                        .child(Record::leaf("DeformPercent", vec![Prop::F64(bs.weight as f64)]))
                        .child(Record::leaf("FullWeights", vec![Prop::ArrF64(vec![100.0])]))
                        .child(Record::new("Properties70").child(p("DeformPercent", "Number", "", "A", vec![Prop::F64(bs.weight as f64)]))),
                );
                connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(shid), Prop::I64(chid)]));
                connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(chid), Prop::I64(bid)]));
            }
        }
    }
    if !pose_nodes.is_empty() {
        let pid = ids.next();
        n_pose += 1;
        let mut pose = Record::with("Pose", vec![Prop::I64(pid), object_name("BindPose", "Pose"), Prop::Str("BindPose".into())])
            .child(Record::leaf("Type", vec![Prop::Str("BindPose".into())]))
            .child(Record::leaf("Version", vec![Prop::I32(100)]))
            .child(Record::leaf("NbPoseNodes", vec![Prop::I32(pose_nodes.len() as i32)]));
        for (nid, m) in &pose_nodes {
            pose = pose.child(Record::new("PoseNode").child(Record::leaf("Node", vec![Prop::I64(*nid)])).child(Record::leaf("Matrix", vec![Prop::ArrF64(matrix16(world(*m, opts.axes), factor))])));
        }
        objects.children.push(pose);
    }
    // Textures: one Texture per material property, one Video per image file.
    let (mut n_texture, mut n_video) = (0, 0);
    let mut videos: std::collections::BTreeMap<String, (i64, String)> = std::collections::BTreeMap::new();
    for (matid, m) in &written_materials {
        for t in &m.textures {
            let (abs, rel) = texture_paths(&t.path, opts);
            let stem = std::path::Path::new(&t.path).file_stem().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| "Map".into());
            let (vid, vname) = match videos.get(&abs) {
                Some(v) => v.clone(),
                None => {
                    let vid = ids.next();
                    n_video += 1;
                    let content = if opts.embed_media { t.content.clone() } else { None };
                    objects.children.push(video(vid, &stem, &abs, &rel, content));
                    videos.insert(abs.clone(), (vid, stem.clone()));
                    (vid, stem.clone())
                }
            };
            let tid = ids.next();
            n_texture += 1;
            let mut tex = texture(tid, &vname, &abs, &rel);
            tex.props[1] = object_name(&format!("{}_{}", m.name, t.property), "Texture");
            objects.children.push(tex);
            connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(vid), Prop::I64(tid)]));
            connections.children.push(Record::leaf("C", vec![Prop::Str("OP".into()), Prop::I64(tid), Prop::I64(*matid), Prop::Str(t.property.clone())]));
        }
    }
    // Selection sets: a Collection per set, a SelectionNode per member node.
    let (mut n_set, mut n_selnode) = (0, 0);
    for (name, members) in &doc.selection_sets {
        let sid = ids.next();
        n_set += 1;
        objects.children.push(Record::with("Collection", vec![Prop::I64(sid), object_name(name, "SelectionSet"), Prop::Str("SelectionSet".into())]).child(Record::new("Properties70")));
        for m in members {
            let Some(&mid) = model_ids.get(*m) else { continue };
            let nid = ids.next();
            n_selnode += 1;
            objects.children.push(
                Record::with("SelectionNode", vec![Prop::I64(nid), object_name(&doc.nodes[*m].name, "SelectionNode"), Prop::Str("".into())])
                    .child(Record::leaf("Version", vec![Prop::I32(100)]))
                    .child(Record::leaf("IsTheNodeInSet", vec![Prop::I32(1)])),
            );
            connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(nid), Prop::I64(sid)]));
            connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(mid), Prop::I64(nid)]));
        }
    }
    for (i, n) in doc.nodes.iter().enumerate() {
        let parent = n.parent.and_then(|p| model_ids.get(p).copied()).unwrap_or(0);
        connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(model_ids[i]), Prop::I64(parent)]));
    }
    // Display layers: a CollectionExclusive each, its models connected to it.
    let mut n_display = 0;
    for l in &doc.layers {
        let lid = ids.next();
        n_display += 1;
        let c = l.color;
        objects.children.push(
            Record::with("CollectionExclusive", vec![Prop::I64(lid), object_name(&l.name, "DisplayLayer"), Prop::Str("DisplayLayer".into())]).child(
                Record::new("Properties70")
                    .child(p("Color", "ColorRGB", "Color", "", vec![Prop::F64(c.x as f64), Prop::F64(c.y as f64), Prop::F64(c.z as f64)]))
                    .child(p("Show", "bool", "", "", vec![Prop::I32(l.visible as i32)]))
                    .child(p("Freeze", "bool", "", "", vec![Prop::I32(l.frozen as i32)])),
            ),
        );
        for ni in &l.nodes {
            if let Some(mid) = model_ids.get(*ni) {
                connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(*mid), Prop::I64(lid)]));
            }
        }
    }
    // The take: one stack, one base layer, T / R / S curve nodes per moving node.
    let (mut n_stack, mut n_layer, mut n_cnode, mut n_curve) = (0, 0, 0, 0);
    let mut take_name = String::new();
    let mut take_span = (0i64, 0i64);
    if let Some(take) = doc.anim.as_ref().filter(|t| !t.tracks.is_empty()) {
        let fps = take.fps.max(1.0);
        take_name = if take.name.is_empty() { "Take 001".into() } else { take.name.clone() };
        take_span = (ktime(take.start as f32, fps), ktime(take.end as f32, fps));
        let stack = ids.next();
        let layer = ids.next();
        n_stack += 1;
        n_layer += 1;
        let kt = |n: &str, v: i64| p(n, "KTime", "Time", "", vec![Prop::I64(v)]);
        objects.children.push(
            Record::with("AnimationStack", vec![Prop::I64(stack), object_name(&take_name, "AnimStack"), Prop::Str("".into())]).child(
                Record::new("Properties70").child(kt("LocalStart", take_span.0)).child(kt("LocalStop", take_span.1)).child(kt("ReferenceStart", take_span.0)).child(kt("ReferenceStop", take_span.1)),
            ),
        );
        objects.children.push(Record::with("AnimationLayer", vec![Prop::I64(layer), object_name("BaseLayer", "AnimLayer"), Prop::Str("".into())]));
        connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(layer), Prop::I64(stack)]));
        for (index, a) in &take.tracks {
            let Some(&mid) = model_ids.get(*index) else { continue };
            let class = match doc.nodes[*index].kind {
                FbxKind::Camera { .. } => "Camera",
                FbxKind::Light(_) => "Light",
                _ => "",
            };
            let times: Vec<i64> = a.frames.iter().map(|f| ktime(*f, fps)).collect();
            let root = doc.nodes[*index].parent.is_none();
            let t: Vec<Vec3> = a.translation.iter().map(|v| if root { axis(*v * factor, opts.axes) } else { *v * factor }).collect();
            let mut r: Vec<Vec3> = Vec::with_capacity(a.rotation_deg.len());
            for v in &a.rotation_deg {
                let mut e = fbx_rotation(*v, opts.axes, class, root);
                // Keep each axis continuous so linear interpolation does not spin the long way round.
                if let Some(prev) = r.last() {
                    for i in 0..3 {
                        e[i] += ((prev[i] - e[i]) / 360.0).round() * 360.0;
                    }
                }
                r.push(e);
            }
            let sc: Vec<Vec3> = a.scale.clone();
            let filters = &opts.curve_filters;
            for (kind, (label, prop, values)) in [("T", "Lcl Translation", &t), ("R", "Lcl Rotation", &r), ("S", "Lcl Scaling", &sc)].into_iter().enumerate() {
                // Curve filters: per component the keys that stay (XYZ synchronization: the union).
                let frames: Vec<f32> = a.frames.clone();
                let mut kept: Vec<Vec<usize>> = (0..3)
                    .map(|c| {
                        let vals: Vec<f32> = values.iter().map(|v| v[c]).collect();
                        crate::curves::reduce(&frames, &vals, filters.constant.map(|p| p[kind]), filters.key_reducer.map(|k| k.0))
                    })
                    .collect();
                if filters.key_reducer.is_some_and(|k| k.1) {
                    let mut all: Vec<usize> = kept.concat();
                    all.sort_unstable();
                    all.dedup();
                    kept = vec![all; 3];
                }
                let cn = ids.next();
                n_cnode += 1;
                let first = values.first().copied().unwrap_or(Vec3::ZERO);
                let d = |n: &str, v: f32| p(n, "Number", "", "A", vec![Prop::F64(v as f64)]);
                objects.children.push(
                    Record::with("AnimationCurveNode", vec![Prop::I64(cn), object_name(label, "AnimCurveNode"), Prop::Str("".into())])
                        .child(Record::new("Properties70").child(d("d|X", first.x)).child(d("d|Y", first.y)).child(d("d|Z", first.z))),
                );
                connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(cn), Prop::I64(layer)]));
                connections.children.push(Record::leaf("C", vec![Prop::Str("OP".into()), Prop::I64(cn), Prop::I64(mid), Prop::Str(prop.into())]));
                for (c, ch) in ["d|X", "d|Y", "d|Z"].iter().enumerate() {
                    // Remove single key: a curve left with one key is the curve node's value.
                    if filters.remove_single_key && kept[c].len() == 1 {
                        continue;
                    }
                    let cid = ids.next();
                    n_curve += 1;
                    let key_times: Vec<i64> = kept[c].iter().map(|i| times[*i]).collect();
                    let vals: Vec<f32> = kept[c].iter().map(|i| values[*i][c]).collect();
                    objects.children.push(anim_curve(cid, &key_times, &vals));
                    connections.children.push(Record::leaf("C", vec![Prop::Str("OP".into()), Prop::I64(cid), Prop::I64(cn), Prop::Str((*ch).into())]));
                }
            }
        }
    }
    let object_type = |name: &str, count: i32| Record::with("ObjectType", vec![Prop::Str(name.into())]).child(Record::leaf("Count", vec![Prop::I32(count)]));
    let mut definitions = Record::new("Definitions")
        .child(Record::leaf("Version", vec![Prop::I32(100)]))
        .child(Record::leaf("Count", vec![Prop::I32(1 + n_model + n_geom + n_mat + n_attr + n_deformer + n_pose + n_stack + n_layer + n_cnode + n_curve + n_texture + n_video + n_set + n_selnode + n_display)]))
        .child(object_type("GlobalSettings", 1));
    for (name, count) in [
        ("Model", n_model),
        ("Geometry", n_geom),
        ("Material", n_mat),
        ("Texture", n_texture),
        ("Video", n_video),
        ("NodeAttribute", n_attr),
        ("Deformer", n_deformer),
        ("Pose", n_pose),
        ("AnimationStack", n_stack),
        ("AnimationLayer", n_layer),
        ("AnimationCurveNode", n_cnode),
        ("AnimationCurve", n_curve),
        ("Collection", n_set),
        ("SelectionNode", n_selnode),
        ("CollectionExclusive", n_display),
    ] {
        if count > 0 {
            definitions = definitions.child(object_type(name, count));
        }
    }
    let documents = Record::new("Documents").child(Record::leaf("Count", vec![Prop::I32(1)])).child(
        Record::with("Document", vec![Prop::I64(ids.next()), Prop::Str("".into()), Prop::Str("Scene".into())])
            .child(Record::new("Properties70").child(p("SourceObject", "object", "", "", vec![])).child(p("ActiveAnimStackName", "KString", "", "", vec![Prop::Str(take_name.clone())])))
            .child(Record::leaf("RootNode", vec![Prop::I64(0)])),
    );
    let top = vec![
        header(opts),
        Record::leaf("FileId", vec![Prop::Raw(crate::binary::FILE_ID.to_vec())]),
        Record::leaf("CreationTime", vec![Prop::Str("1970-01-01 10:00:00:000".into())]),
        Record::leaf("Creator", vec![Prop::Str(opts.creator.clone())]),
        global_settings(doc, opts.axes, opts.file_unit_cm),
        documents,
        Record::new("References"),
        definitions,
        objects,
        connections,
        takes(&take_name, take_span),
    ];
    if opts.ascii {
        crate::ascii::encode(&top, opts.version)
    } else {
        encode_version(&top, opts.version)
    }
}

fn takes(name: &str, span: (i64, i64)) -> Record {
    let mut t = Record::new("Takes").child(Record::leaf("Current", vec![Prop::Str(name.into())]));
    if !name.is_empty() {
        t = t.child(
            Record::with("Take", vec![Prop::Str(name.into())])
                .child(Record::leaf("FileName", vec![Prop::Str(format!("{}.tak", name.replace(' ', "_")))]))
                .child(Record::leaf("LocalTime", vec![Prop::I64(span.0), Prop::I64(span.1)]))
                .child(Record::leaf("ReferenceTime", vec![Prop::I64(span.0), Prop::I64(span.1)])),
        );
    }
    t
}

/// One mesh straight from index lists, no `Mesh` in between: lets tests
/// write faces no manifold mesh could hold (duplicates, repeated corners).
pub fn write_raw_faces(positions: &[Vec3], faces: &[Vec<u32>], normals: Option<&[Vec3]>, unit_metres: f64) -> Vec<u8> {
    let factor = (unit_metres * 100.0) as f32;
    let mut vertices: Vec<f64> = Vec::new();
    for v in positions {
        let v = *v * factor;
        vertices.extend([v.x as f64, v.y as f64, v.z as f64]);
    }
    let mut indices: Vec<i32> = Vec::new();
    for f in faces {
        for (i, v) in f.iter().enumerate() {
            indices.push(if i + 1 == f.len() { -(*v as i32 + 1) } else { *v as i32 });
        }
    }
    let node = FbxNode { name: "Raw".into(), parent: None, translation: Vec3::ZERO, rotation_deg: Vec3::ZERO, scale: Vec3::ONE, kind: FbxKind::Empty, skin: None, blend_shapes: Vec::new() };
    let opts = WriteOpts::default();
    let mut geometry = Record::with("Geometry", vec![Prop::I64(1002), object_name("Raw", "Geometry"), Prop::Str("Mesh".into())])
        .child(Record::leaf("Vertices", vec![Prop::ArrF64(vertices)]))
        .child(Record::leaf("PolygonVertexIndex", vec![Prop::ArrI32(indices)]))
        .child(Record::leaf("GeometryVersion", vec![Prop::I32(124)]));
    if let Some(normals) = normals {
        let n: Vec<f64> = normals.iter().flat_map(|n| [n.x as f64, n.y as f64, n.z as f64]).collect();
        geometry = geometry
            .child(layer_element("LayerElementNormal", 0, "", 101, "ByPolygonVertex", "Direct").child(Record::leaf("Normals", vec![Prop::ArrF64(n)])))
            .child(Record::with("Layer", vec![Prop::I32(0)]).child(Record::leaf("Version", vec![Prop::I32(100)])).child(layer_entry("LayerElementNormal", 0)));
    }
    let doc = FbxDocument { nodes: vec![], unit_metres, ..Default::default() };
    let top = vec![
        header(&opts),
        global_settings(&doc, WriteAxes::ZUp, 1.0),
        Record::new("Definitions")
            .child(Record::leaf("Version", vec![Prop::I32(100)]))
            .child(Record::leaf("Count", vec![Prop::I32(3)]))
            .child(Record::with("ObjectType", vec![Prop::Str("GlobalSettings".into())]).child(Record::leaf("Count", vec![Prop::I32(1)])))
            .child(Record::with("ObjectType", vec![Prop::Str("Model".into())]).child(Record::leaf("Count", vec![Prop::I32(1)])))
            .child(Record::with("ObjectType", vec![Prop::Str("Geometry".into())]).child(Record::leaf("Count", vec![Prop::I32(1)]))),
        Record::new("Objects").child(model(1001, &node, "Mesh", WriteAxes::ZUp, factor)).child(geometry),
        Record::new("Connections")
            .child(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(1002), Prop::I64(1001)]))
            .child(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(1001), Prop::I64(0)])),
        Record::new("Takes").child(Record::leaf("Current", vec![Prop::Str("".into())])),
    ];
    encode(&top)
}
