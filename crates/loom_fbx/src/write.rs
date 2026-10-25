//! `FbxDocument` → FBX 7.4 binary (Model / Geometry / Material /
//! NodeAttribute objects with OO connections, display layers).

use crate::binary::{encode, object_name, p, Prop, Record};
use crate::document::{FbxDocument, FbxKind, FbxNode, WriteAxes, WriteOpts};
use glam::{Mat4, Vec3};
use loom_mesh::{FaceId, Mesh};

fn axis(v: Vec3, axes: WriteAxes) -> Vec3 {
    match axes {
        WriteAxes::ZUp => v,
        WriteAxes::YUp => Vec3::new(v.x, v.z, -v.y),
    }
}

/// Loom's Euler XYZ (glam intrinsic) → FBX `Lcl Rotation` with
/// `RotationOrder` XYZ, which is extrinsic (Rz·Ry·Rx). A camera also turns
/// so its FBX forward (+X) is loom's forward (-Z).
/// `class` "Camera": FBX cameras look down +X; "Light": FBX lights aim down
/// -Y; the document's cameras and lights look down -Z.
fn fbx_rotation(r: Vec3, axes: WriteAxes, class: &str) -> Vec3 {
    let mut q = glam::Quat::from_euler(glam::EulerRot::XYZ, r.x.to_radians(), r.y.to_radians(), r.z.to_radians());
    match class {
        "Camera" => q *= glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        "Light" => q *= glam::Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        _ => {}
    }
    if axes == WriteAxes::YUp {
        // Conjugate by the Z-up → Y-up change of basis.
        let basis = glam::Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
        q = basis * q * basis.inverse();
    }
    let (x, y, z) = q.to_euler(glam::EulerRot::XYZEx);
    Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
}

/// Centimetres per document unit: the file is written in centimetres
/// (`UnitScaleFactor` 1) because Unreal's default import ignores the
/// scale factor.
fn cm_factor(doc: &FbxDocument) -> f32 {
    (doc.unit_metres * 100.0) as f32
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
        .child(Record::leaf("FBXVersion", vec![Prop::I32(crate::binary::VERSION as i32)]))
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

fn global_settings(doc: &FbxDocument, axes: WriteAxes) -> Record {
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
            .child(p("UnitScaleFactor", "double", "Number", "", vec![Prop::F64(1.0)]))
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
    let t = axis(n.translation * factor, axes);
    let r = fbx_rotation(n.rotation_deg, axes, class);
    let s = match axes {
        WriteAxes::ZUp => n.scale,
        WriteAxes::YUp => Vec3::new(n.scale.x, n.scale.z, n.scale.y),
    };
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

fn geometry(id: i64, name: &str, mesh: &Mesh, opts: &WriteOpts, factor: f32) -> Record {
    let axes = opts.axes;
    let mut vertices: Vec<f64> = Vec::with_capacity(mesh.vertex_count() * 3);
    for v in mesh.vertices() {
        let q = axis(v.position * factor, axes);
        vertices.extend([q.x as f64, q.y as f64, q.z as f64]);
    }
    let normals_by_corner = loom_mesh::mesh_mods::corner_normals(mesh);
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
            let v = mesh.half_edges()[he.0 as usize].origin.0 as i32;
            indices.push(if i + 1 == k { -(v + 1) } else { v });
            let n = axis(normals_by_corner[he.0 as usize], axes);
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
        .child(Record::leaf("GeometryVersion", vec![Prop::I32(124)]))
        .child(layer_element("LayerElementNormal", 0, "", 101, "ByPolygonVertex", "Direct").child(Record::leaf("Normals", vec![Prop::ArrF64(normals)])));
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
    g
}

fn material(id: i64, name: &str) -> Record {
    Record::with("Material", vec![Prop::I64(id), object_name(name, "Material"), Prop::Str("".into())])
        .child(Record::leaf("Version", vec![Prop::I32(102)]))
        .child(Record::leaf("ShadingModel", vec![Prop::Str("phong".into())]))
        .child(Record::leaf("MultiLayer", vec![Prop::I32(0)]))
        .child(
            Record::new("Properties70")
                .child(p("DiffuseColor", "Color", "", "A", vec![Prop::F64(0.8), Prop::F64(0.8), Prop::F64(0.8)]))
                .child(p("AmbientColor", "Color", "", "A", vec![Prop::F64(0.2), Prop::F64(0.2), Prop::F64(0.2)])),
        )
}

fn camera_attribute(id: i64, name: &str, fov_deg: f32) -> Record {
    Record::with("NodeAttribute", vec![Prop::I64(id), object_name(name, "NodeAttribute"), Prop::Str("Camera".into())])
        .child(
            Record::new("Properties70")
                .child(p("FieldOfView", "FieldOfView", "", "A", vec![Prop::F64(fov_deg as f64)]))
                .child(p("ApertureMode", "enum", "", "", vec![Prop::I32(2)]))
                .child(p("AspectRatioMode", "enum", "", "", vec![Prop::I32(2)]))
                .child(p("AspectWidth", "double", "Number", "", vec![Prop::F64(1920.0)]))
                .child(p("AspectHeight", "double", "Number", "", vec![Prop::F64(1080.0)]))
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
    let factor = cm_factor(doc);
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
    for n in &doc.nodes {
        let mid = ids.next();
        model_ids.push(mid);
        n_model += 1;
        match &n.kind {
            FbxKind::Mesh { mesh, materials } => {
                objects.children.push(model(mid, n, "Mesh", opts.axes, factor));
                let gid = ids.next();
                n_geom += 1;
                objects.children.push(geometry(gid, &n.name, mesh, opts, factor));
                connections.children.push(Record::leaf("C", vec![Prop::Str("OO".into()), Prop::I64(gid), Prop::I64(mid)]));
                if n.skin.is_some() || !n.blend_shapes.is_empty() {
                    deferred.push((model_ids.len() - 1, mid, gid));
                }
                for name in material_names(mesh, materials) {
                    let matid = *material_ids.entry(name.clone()).or_insert_with(|| {
                        let id = ids.next();
                        n_mat += 1;
                        objects.children.push(material(id, &name));
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
                            indexes.push(v as i32);
                            weights.push(*x as f64);
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
                        .child(Record::leaf("TransformLink", vec![Prop::ArrF64(matrix16(bone_bind, factor))])),
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
                let indexes: Vec<i32> = bs.offsets.iter().map(|(v, _)| *v as i32).collect();
                let verts: Vec<f64> = bs.offsets.iter().flat_map(|(_, o)| { let q = axis(*o * factor, opts.axes); [q.x as f64, q.y as f64, q.z as f64] }).collect();
                let normals: Vec<f64> = vec![0.0; bs.offsets.len() * 3];
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
            pose = pose.child(Record::new("PoseNode").child(Record::leaf("Node", vec![Prop::I64(*nid)])).child(Record::leaf("Matrix", vec![Prop::ArrF64(matrix16(*m, factor))])));
        }
        objects.children.push(pose);
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
            let t: Vec<Vec3> = a.translation.iter().map(|v| axis(*v * factor, opts.axes)).collect();
            let mut r: Vec<Vec3> = Vec::with_capacity(a.rotation_deg.len());
            for v in &a.rotation_deg {
                let mut e = fbx_rotation(*v, opts.axes, class);
                // Keep each axis continuous so linear interpolation does not spin the long way round.
                if let Some(prev) = r.last() {
                    for i in 0..3 {
                        e[i] += ((prev[i] - e[i]) / 360.0).round() * 360.0;
                    }
                }
                r.push(e);
            }
            let sc: Vec<Vec3> = a.scale.iter().map(|v| match opts.axes {
                WriteAxes::ZUp => *v,
                WriteAxes::YUp => Vec3::new(v.x, v.z, v.y),
            }).collect();
            for (label, prop, values) in [("T", "Lcl Translation", &t), ("R", "Lcl Rotation", &r), ("S", "Lcl Scaling", &sc)] {
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
                    let cid = ids.next();
                    n_curve += 1;
                    let vals: Vec<f32> = values.iter().map(|v| v[c]).collect();
                    objects.children.push(anim_curve(cid, &times, &vals));
                    connections.children.push(Record::leaf("C", vec![Prop::Str("OP".into()), Prop::I64(cid), Prop::I64(cn), Prop::Str((*ch).into())]));
                }
            }
        }
    }
    let object_type = |name: &str, count: i32| Record::with("ObjectType", vec![Prop::Str(name.into())]).child(Record::leaf("Count", vec![Prop::I32(count)]));
    let mut definitions = Record::new("Definitions")
        .child(Record::leaf("Version", vec![Prop::I32(100)]))
        .child(Record::leaf("Count", vec![Prop::I32(1 + n_model + n_geom + n_mat + n_attr + n_deformer + n_pose + n_stack + n_layer + n_cnode + n_curve + n_display)]))
        .child(object_type("GlobalSettings", 1));
    for (name, count) in [("Model", n_model), ("Geometry", n_geom), ("Material", n_mat), ("NodeAttribute", n_attr), ("Deformer", n_deformer), ("Pose", n_pose), ("AnimationStack", n_stack), ("AnimationLayer", n_layer), ("AnimationCurveNode", n_cnode), ("AnimationCurve", n_curve), ("CollectionExclusive", n_display)] {
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
        global_settings(doc, opts.axes),
        documents,
        Record::new("References"),
        definitions,
        objects,
        connections,
        takes(&take_name, take_span),
    ];
    encode(&top)
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
    let doc = FbxDocument { nodes: vec![], unit_metres, anim: None, layers: Vec::new() };
    let top = vec![
        header(&opts),
        global_settings(&doc, WriteAxes::ZUp),
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
