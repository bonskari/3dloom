//! Reader for .max scene files (clean room): the OLE2 compound file, the
//! ClassDirectory3 class list and the Scene stream's object chunks give
//! nodes (name, parent, transform, wire colour, pivot offset, layer,
//! material, look-at target) and their objects: Editable Poly and Editable
//! Mesh geometry with map channels, smoothing groups and material ids, the
//! Box / Cylinder / Plane primitives' parameters, editable splines, Free
//! cameras, Omni and Target Direct lights; the modifier stacks Loom can
//! rebuild (see `mods`), Standard and Multi/Sub-Object materials (see
//! `mtl`) and the layers. Skins, animation and render objects are not read;
//! what is skipped is reported in `MaxDoc::warnings`.

mod cfb;
mod chunk;
mod geom;
mod mods;
mod mtl;
mod params;
mod shape;

pub use mods::{Lattice, MaxModifier, UnwrapUvs};
pub use mtl::{MaxMaterial, MaxStandard, MaxSub};
pub use shape::{KnotType, MaxKnot, MaxShape, MaxSpline};

use chunk::Chunk;
use glam::{EulerRot, Mat4, Quat, Vec3};
use loom_mesh::Mesh;

/// What a node holds.
#[derive(Debug, Clone)]
pub enum MaxKind {
    /// Editable Poly / Editable Mesh geometry in object space (the pivot offset not applied).
    Mesh(Mesh),
    Box { length: f32, width: f32, height: f32, length_segs: u32, width_segs: u32, height_segs: u32 },
    Cylinder { radius: f32, height: f32, height_segs: u32, cap_segs: u32, sides: u32, smooth: bool, slice_on: bool, slice_from: f32, slice_to: f32 },
    Plane { length: f32, width: f32, length_segs: u32, width_segs: u32 },
    Dummy { size: Vec3 },
    /// An editable spline (SplineShape or Line), in object space.
    Shape(MaxShape),
    /// A Free camera; parameters by Loom name (FOV in degrees).
    Camera { params: Vec<(&'static str, f64)> },
    /// A light: `kind` is "omni" or "target_direct"; parameters by Loom name.
    Light { kind: &'static str, params: Vec<(&'static str, f64)> },
    /// The target node of a target light or camera.
    Target,
    /// Anything else (Biped, …): kept as a helper so the hierarchy holds.
    Other(String),
}

#[derive(Debug, Clone)]
pub struct MaxNode {
    pub name: String,
    pub parent: Option<usize>,
    /// The node relative to its parent.
    pub local: Mat4,
    /// Object offset (pivot): object space → node space.
    pub offset: Mat4,
    pub wire_color: [u8; 3],
    pub kind: MaxKind,
    /// The modifier stack, bottom first.
    pub modifiers: Vec<MaxModifier>,
    /// Index into `MaxDoc::materials`.
    pub material: Option<usize>,
    /// Index into `MaxDoc::layers`.
    pub layer: Option<usize>,
    /// A Look At controller's target (a node index): the node aims its -Z at it.
    pub look_at: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaxLayer {
    pub name: String,
    pub color: [u8; 3],
}

#[derive(Debug, Clone, Default)]
pub struct MaxDoc {
    pub nodes: Vec<MaxNode>,
    pub materials: Vec<MaxMaterial>,
    pub layers: Vec<MaxLayer>,
    pub warnings: Vec<String>,
}

/// One ClassDirectory3 entry.
#[derive(Debug, Clone)]
struct Class {
    name: String,
    /// (class id a, class id b, superclass id): class ids repeat across superclasses.
    id: (u32, u32, u32),
}

// Class ids (ClassDirectory3), as the file stores them.
const NODE: (u32, u32, u32) = (0x1, 0, 0x1);
const ROOT_NODE: (u32, u32, u32) = (0x2, 0, 0x1);
const EDITABLE_POLY: (u32, u32, u32) = (0x1bf8_338d, 0x192f_6098, 0x10);
const EDITABLE_MESH: (u32, u32, u32) = (0xe44f_10b3, 0, 0x10);
const BOX: (u32, u32, u32) = (0x10, 0, 0x10);
const CYLINDER: (u32, u32, u32) = (0x12, 0, 0x10);
const PLANE: (u32, u32, u32) = (0x081f_1dfc, 0x7756_6f65, 0x10);
const DUMMY: (u32, u32, u32) = (0x0087_6234, 0, 0x50);
const PRS: (u32, u32, u32) = (0x2005, 0, 0x9008);
const POSITION_XYZ: (u32, u32, u32) = (0x118f_7e02, 0xffee_238a, 0x900b);
const EULER_XYZ: (u32, u32, u32) = (0x2012, 0, 0x900c);
const SCALE_XYZ: (u32, u32, u32) = (0x118f_7c01, 0xfeee_238b, 0x900d);
/// The Derived Object (a modifier stack) has no class directory entry: its chunk id is fixed.
pub(crate) const DERIVED_OBJECT_CHUNK: u16 = 0x2032;

struct Scene {
    classes: Vec<Class>,
    objects: Vec<Chunk>,
}

impl Scene {
    fn class(&self, i: usize) -> Option<&Class> {
        self.classes.get(self.objects.get(i)?.id as usize)
    }
    fn is(&self, i: usize, id: (u32, u32, u32)) -> bool {
        self.class(i).is_some_and(|c| c.id == id)
    }
    fn name(&self, i: usize) -> String {
        match self.objects.get(i) {
            Some(o) if o.id == DERIVED_OBJECT_CHUNK => "Derived Object".into(),
            _ => self.class(i).map(|c| c.name.clone()).unwrap_or_else(|| "unknown class".into()),
        }
    }
    /// References (0x2034: a list of object indices; 0x2035: flags then slot / object pairs).
    fn refs(&self, i: usize) -> Vec<Option<usize>> {
        let Some(o) = self.objects.get(i) else { return Vec::new() };
        if let Some(c) = o.child(0x2034) {
            return c.i32s().into_iter().map(|x| usize::try_from(x).ok()).collect();
        }
        if let Some(c) = o.child(0x2035) {
            let v = c.i32s();
            let mut out: Vec<Option<usize>> = Vec::new();
            for p in v.get(1..).unwrap_or(&[]).chunks_exact(2) {
                if let (Ok(slot), Ok(obj)) = (usize::try_from(p[0]), usize::try_from(p[1])) {
                    if slot < 4096 {
                        if out.len() <= slot {
                            out.resize(slot + 1, None);
                        }
                        out[slot] = Some(obj);
                    }
                }
            }
            return out;
        }
        Vec::new()
    }
    fn chunk(&self, i: usize, id: u16) -> Option<&Chunk> {
        self.objects.get(i)?.child(id)
    }
}

fn read_classes(b: &[u8]) -> Result<Vec<Class>, String> {
    let mut out = Vec::new();
    for c in chunk::parse(b, 0)? {
        let info = c.child(0x2060).map(|i| i.u32s()).unwrap_or_default();
        let name = c.child(0x2042).map(|n| n.utf16()).unwrap_or_default();
        out.push(Class { name, id: (info.get(1).copied().unwrap_or(0), info.get(2).copied().unwrap_or(0), info.get(3).copied().unwrap_or(0)) });
    }
    Ok(out)
}

/// The file's quaternions are left-handed: the rotation they describe is the
/// conjugate of the same numbers read as a right-handed quaternion.
fn max_quat(v: &[f32]) -> Quat {
    if v.len() < 4 {
        return Quat::IDENTITY;
    }
    Quat::from_xyzw(-v[0], -v[1], -v[2], v[3]).normalize()
}

impl Scene {
    fn float(&self, i: Option<usize>, w: &mut Vec<String>) -> f32 {
        let Some(i) = i else { return 0.0 };
        match self.chunk(i, 0x2501).map(|c| c.f32s()) {
            Some(v) if !v.is_empty() => v[0],
            _ => {
                w.push(format!("a float controller ({}) without a value: 0 used", self.name(i)));
                0.0
            }
        }
    }

    fn position(&self, i: Option<usize>, w: &mut Vec<String>) -> Vec3 {
        let Some(i) = i else { return Vec3::ZERO };
        if self.is(i, POSITION_XYZ) {
            let r = self.refs(i);
            return Vec3::new(self.float(r.first().copied().flatten(), w), self.float(r.get(1).copied().flatten(), w), self.float(r.get(2).copied().flatten(), w));
        }
        match self.chunk(i, 0x2503).map(|c| c.f32s()) {
            Some(v) if v.len() >= 3 => Vec3::new(v[0], v[1], v[2]),
            _ => {
                w.push(format!("position controller {} is not read: the node sits at its parent", self.name(i)));
                Vec3::ZERO
            }
        }
    }

    fn rotation(&self, i: Option<usize>, w: &mut Vec<String>) -> Quat {
        let Some(i) = i else { return Quat::IDENTITY };
        if self.is(i, EULER_XYZ) {
            let r = self.refs(i);
            let (x, y, z) = (self.float(r.first().copied().flatten(), w), self.float(r.get(1).copied().flatten(), w), self.float(r.get(2).copied().flatten(), w));
            // Euler XYZ: X first, then Y, then Z, about the parent's axes.
            return Quat::from_euler(EulerRot::XYZEx, x, y, z);
        }
        match self.chunk(i, 0x2504).map(|c| c.f32s()) {
            Some(v) if v.len() >= 4 => max_quat(&v),
            _ => {
                w.push(format!("rotation controller {} is not read: no rotation used", self.name(i)));
                Quat::IDENTITY
            }
        }
    }

    /// Scale (with its axis) as a matrix.
    fn scale(&self, i: Option<usize>, w: &mut Vec<String>) -> Mat4 {
        let Some(i) = i else { return Mat4::IDENTITY };
        if self.is(i, SCALE_XYZ) {
            let r = self.refs(i);
            let s = Vec3::new(self.float(r.first().copied().flatten(), w), self.float(r.get(1).copied().flatten(), w), self.float(r.get(2).copied().flatten(), w));
            return Mat4::from_scale(s);
        }
        match self.chunk(i, 0x2505).map(|c| c.f32s()) {
            Some(v) if v.len() >= 7 => {
                let axis = max_quat(&v[3..7]);
                let r = Mat4::from_quat(axis);
                r * Mat4::from_scale(Vec3::new(v[0], v[1], v[2])) * r.inverse()
            }
            _ => {
                w.push(format!("scale controller {} is not read: scale 1 used", self.name(i)));
                Mat4::IDENTITY
            }
        }
    }

    fn transform(&self, i: Option<usize>, w: &mut Vec<String>, node: &str) -> Mat4 {
        let Some(i) = i else { return Mat4::IDENTITY };
        if !self.is(i, PRS) {
            w.push(format!("{node}: transform controller {} is not read (only Position/Rotation/Scale): it is placed at its parent", self.name(i)));
            return Mat4::IDENTITY;
        }
        let r = self.refs(i);
        let p = self.position(r.first().copied().flatten(), w);
        let q = self.rotation(r.get(1).copied().flatten(), w);
        let s = self.scale(r.get(2).copied().flatten(), w);
        Mat4::from_translation(p) * Mat4::from_quat(q) * s
    }
}

/// The object's first reference read as a parameter block, values as floats.
fn param_block(scene: &Scene, obj: usize) -> Option<std::collections::BTreeMap<u32, f32>> {
    let pb = scene.refs(obj).into_iter().flatten().next()?;
    let o = scene.objects.get(pb)?;
    Some(params::read(o).into_iter().filter_map(|(k, v)| v.f().map(|f| (k, f))).collect())
}

const LOOK_AT: (u32, u32, u32) = (0x2006, 0, 0x9008);
const FREE_CAMERA: (u32, u32, u32) = (0x1001, 0, 0x20);
const OMNI: (u32, u32, u32) = (0x1011, 0, 0x30);
const TARGET_DIRECT: (u32, u32, u32) = (0x1015, 0, 0x30);
const TARGET: (u32, u32, u32) = (0x1020, 0, 0x10);
const SPLINE_SHAPE: (u32, u32, u32) = (0xa, 0, 0x40);
const LINE: (u32, u32, u32) = (0x1040, 0, 0x40);
const BASE_LAYER: (u32, u32, u32) = (0x7e98_58fe, 0x1dba_1df0, 0x10f0);

/// The parameters of a Free camera's ParamBlock (FOV in radians, Target
/// Distance, Near / Far Clip, Near / Far Range; matched against the
/// reference's defaults 1 / 1000 for the clip planes and 0 / 1000 for the ranges).
fn camera_params(p: &std::collections::BTreeMap<u32, f32>) -> Vec<(&'static str, f64)> {
    let mut out = Vec::new();
    for (i, name) in [(0u32, "FOV"), (1, "Target Distance"), (2, "Near Clip"), (3, "Far Clip"), (4, "Near Range"), (5, "Far Range")] {
        if let Some(v) = p.get(&i).filter(|v| v.is_finite()) {
            out.push((name, if i == 0 { (*v as f64).to_degrees() } else { *v as f64 }));
        }
    }
    out
}

/// A standard light's ParamBlock: 0 colour, 1 multiplier, 2 contrast, 3
/// soften diffuse edge, then (a directional light first has hotspot,
/// falloff and aspect) near start / end, far start / end and decay start,
/// as the reference's defaults 0 / 40 / 80 / 200 / 40 of an Omni show them.
fn light_params(scene: &Scene, obj: usize, directional: bool) -> Vec<(&'static str, f64)> {
    let Some(pb) = scene.refs(obj).into_iter().flatten().next().and_then(|r| scene.objects.get(r)) else { return Vec::new() };
    let p = params::read(pb);
    let mut out = Vec::new();
    if let Some(c) = p.get(&0).and_then(|v| v.v()) {
        out.extend([("Color R", c[0] as f64), ("Color G", c[1] as f64), ("Color B", c[2] as f64)]);
    }
    let mut named: Vec<(u32, &'static str)> = vec![(1, "Multiplier"), (2, "Contrast"), (3, "Soften Diffuse Edge")];
    let at = if directional {
        named.extend([(4, "Hotspot"), (5, "Falloff"), (6, "Aspect")]);
        7
    } else {
        4
    };
    named.extend([(at, "Near Start"), (at + 1, "Near End"), (at + 2, "Far Start"), (at + 3, "Far End"), (at + 4, "Decay Start")]);
    for (i, name) in named {
        if let Some(v) = p.get(&i).and_then(|v| v.f()).filter(|v| v.is_finite()) {
            out.push((name, v as f64));
        }
    }
    out
}

/// A Lathe right on a shape revolves about its gizmo's Z (the sample lathes
/// of profiles drawn in the XY plane store a gizmo turned so Z is the
/// object's Y). Loom's Lathe takes a Direction (X / Y / Z) through the
/// shape's pivot: the axis direction becomes that, and an axis away from the
/// pivot is kept exactly by moving the knots and the object offset the
/// opposite ways (gizmos above follow the knots).
fn lathe_axis(kind: &mut MaxKind, modifiers: &mut Vec<MaxModifier>, offset: &mut Mat4, node: &str, w: &mut Vec<String>) {
    let MaxKind::Shape(shape) = kind else { return };
    let Some(first) = modifiers.first().filter(|m| m.class == "Lathe") else { return };
    let g = first.gizmo.unwrap_or(Mat4::IDENTITY);
    let axis = g.transform_vector3(Vec3::Z).normalize_or_zero();
    let Some(dir) = [Vec3::X, Vec3::Y, Vec3::Z].iter().position(|a| axis.dot(*a).abs() > 0.9999) else {
        w.push(format!("{node}: Lathe axis {axis} is not along X, Y or Z; the Lathe is not imported"));
        modifiers.remove(0);
        return;
    };
    let at = g.w_axis.truncate();
    let shift = at - axis * axis.dot(at);
    for s in &mut shape.splines {
        for k in &mut s.knots {
            k.pos -= shift;
            k.in_handle -= shift;
            k.out_handle -= shift;
        }
    }
    *offset *= Mat4::from_translation(shift);
    let back = Mat4::from_translation(-shift);
    for m in modifiers.iter_mut().skip(1) {
        let motion = m.class == "XForm";
        if let Some(gz) = m.gizmo.as_mut() {
            // XForm's gizmo moves points (conjugate); the others are frames.
            *gz = if motion { back * *gz * Mat4::from_translation(shift) } else { back * *gz };
        }
    }
    let lathe = &mut modifiers[0];
    lathe.gizmo = None;
    lathe.params.extend([("Direction", dir as f64), ("Align", 3.0)]);
}

/// A layer's name (0x1010, single-byte text in the sample files) and colour (0x1030).
fn layer(scene: &Scene, obj: usize) -> MaxLayer {
    let name = scene.chunk(obj, 0x1010).map(|c| c.data.iter().take_while(|b| **b != 0).map(|b| *b as char).collect()).unwrap_or_default();
    let color = scene.chunk(obj, 0x1030).and_then(|c| c.u32s().first().copied()).map(|c| [(c & 0xff) as u8, ((c >> 8) & 0xff) as u8, ((c >> 16) & 0xff) as u8]).unwrap_or([200, 200, 200]);
    MaxLayer { name, color }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Prim {
    Box,
    Cylinder,
    Plane,
}

/// The format's own limit for segment and side counts.
const MAX_SEGMENTS: f32 = 200.0;

/// A primitive from its parameter block values (by the file's parameter index).
/// Counts are held to the format's 1–200 (sides 3–200) and sizes to finite values,
/// so a damaged file cannot ask for a billion segments.
fn primitive(class: Prim, p: &std::collections::BTreeMap<u32, f32>, name: &str, w: &mut Vec<String>) -> MaxKind {
    let mut f = |i: u32, d: f32| match p.get(&i) {
        Some(v) if v.is_finite() => *v,
        Some(_) => {
            w.push(format!("{name}: parameter {i} is not a number; {d} used"));
            d
        }
        None => d,
    };
    let segs = |i: u32, d: u32, min: f32, w: &mut Vec<String>| match p.get(&i) {
        Some(v) if v.is_finite() && (min..=MAX_SEGMENTS).contains(&v.round()) => v.round() as u32,
        Some(v) => {
            let c = if v.is_finite() { v.round().clamp(min, MAX_SEGMENTS) } else { d as f32 };
            w.push(format!("{name}: count {v} outside {min}..{MAX_SEGMENTS}; {c} used"));
            c as u32
        }
        None => d,
    };
    match class {
        Prim::Box => {
            let (length, width, height) = (f(0, 1.0), f(1, 1.0), f(2, 1.0));
            MaxKind::Box { length, width, height, width_segs: segs(3, 1, 1.0, w), length_segs: segs(4, 1, 1.0, w), height_segs: segs(5, 1, 1.0, w) }
        }
        Prim::Cylinder => {
            let (radius, height, smooth, slice_on, slice_from, slice_to) = (f(0, 1.0), f(1, 1.0), f(5, 1.0) != 0.0, f(6, 0.0) != 0.0, f(7, 0.0), f(8, 0.0));
            MaxKind::Cylinder { radius, height, height_segs: segs(2, 1, 1.0, w), cap_segs: segs(3, 1, 1.0, w), sides: segs(4, 18, 3.0, w), smooth, slice_on, slice_from, slice_to }
        }
        Prim::Plane => {
            let (length, width) = (f(0, 1.0), f(1, 1.0));
            MaxKind::Plane { length, width, width_segs: segs(2, 4, 1.0, w), length_segs: segs(3, 4, 1.0, w) }
        }
    }
}

/// Reads a .max file's nodes. A file that is not a compound file, or whose
/// streams or chunks are damaged, is an error; parts that cannot be read are warnings.
pub fn read(bytes: &[u8]) -> Result<MaxDoc, String> {
    let cfb = cfb::Cfb::open(bytes)?;
    let classes = read_classes(&cfb.stream("ClassDirectory3")?)?;
    let scene_stream = cfb.stream("Scene")?;
    let top = chunk::parse(&scene_stream, 0)?;
    let objects = top.into_iter().find(|c| c.children.is_some()).and_then(|c| c.children).ok_or("the Scene stream holds no objects")?;
    let scene = Scene { classes, objects };
    let mut doc = MaxDoc::default();
    let assets = cfb.stream("FileAssetMetaData2").map(|b| mtl::assets(&b)).unwrap_or_default();
    let mut materials = mtl::Reader::new(&scene, &assets);
    // Node objects in file order; the root node is the scene, not a node.
    let node_objs: Vec<usize> = (0..scene.objects.len()).filter(|i| scene.is(*i, NODE)).collect();
    let index_of: std::collections::HashMap<usize, usize> = node_objs.iter().enumerate().map(|(k, o)| (*o, k)).collect();
    for &oi in &node_objs {
        let mut w = Vec::new();
        let name = scene.chunk(oi, 0x0962).map(|c| c.utf16()).unwrap_or_else(|| format!("Object{oi}"));
        let parent_obj = scene.chunk(oi, 0x0960).and_then(|c| c.i32s().first().copied()).and_then(|p| usize::try_from(p).ok());
        let parent = parent_obj.filter(|p| !scene.is(*p, ROOT_NODE)).and_then(|p| index_of.get(&p).copied());
        let refs = scene.refs(oi);
        // A Look At controller: position and scale here, the aim at its target node.
        let tc = refs.first().copied().flatten();
        let (local, look_at_obj) = match tc.filter(|c| scene.is(*c, LOOK_AT)) {
            Some(c) => {
                let r = scene.refs(c);
                let p = scene.position(r.get(1).copied().flatten(), &mut w);
                let s = scene.scale(r.get(3).copied().flatten(), &mut w);
                if scene.float(r.get(2).copied().flatten(), &mut Vec::new()) != 0.0 {
                    w.push(format!("{name}: Look At roll is not imported"));
                }
                (Mat4::from_translation(p) * s, r.first().copied().flatten())
            }
            None => (scene.transform(tc, &mut w, &name), None),
        };
        let look_at = look_at_obj.and_then(|t| index_of.get(&t).copied());
        if look_at_obj.is_some() && look_at.is_none() {
            w.push(format!("{name}: its Look At target is not a node; no aim imported"));
        }
        let f3 = |id: u16, d: Vec3| scene.chunk(oi, id).map(|c| c.f32s()).filter(|v| v.len() >= 3).map(|v| Vec3::new(v[0], v[1], v[2])).unwrap_or(d);
        let off_pos = f3(0x096a, Vec3::ZERO);
        let off_rot = scene.chunk(oi, 0x096b).map(|c| max_quat(&c.f32s())).unwrap_or(Quat::IDENTITY);
        let off_scale = f3(0x096c, Vec3::ONE);
        let mut offset = Mat4::from_translation(off_pos) * Mat4::from_quat(off_rot) * Mat4::from_scale(off_scale);
        let wire_color = scene.chunk(oi, 0x0974).and_then(|c| c.u32s().first().copied()).map(|c| [(c & 0xff) as u8, ((c >> 8) & 0xff) as u8, ((c >> 16) & 0xff) as u8]).unwrap_or([200, 200, 200]);
        // The object, through its modifier stack.
        let (mut modifiers, obj) = mods::stack(&scene, refs.get(1).copied().flatten(), &name, &mut w);
        let mut kind = match obj {
            None => MaxKind::Other("no object".into()),
            Some(o) if scene.is(o, EDITABLE_POLY) => match scene.objects[o].child(0x08fe).ok_or("Editable Poly without geometry".to_string()).and_then(geom::editable_poly) {
                Ok(m) => MaxKind::Mesh(m),
                Err(e) => {
                    w.push(format!("{name}: {e}"));
                    MaxKind::Other("Editable Poly".into())
                }
            },
            Some(o) if scene.is(o, EDITABLE_MESH) => match scene.objects[o].child(0x08fe).ok_or("Editable Mesh without geometry".to_string()).and_then(geom::editable_mesh) {
                Ok(m) => MaxKind::Mesh(m),
                Err(e) => {
                    w.push(format!("{name}: {e}"));
                    MaxKind::Other("Editable Mesh".into())
                }
            },
            Some(o) if scene.is(o, BOX) || scene.is(o, CYLINDER) || scene.is(o, PLANE) => {
                let p = param_block(&scene, o).unwrap_or_default();
                let class = if scene.is(o, BOX) { Prim::Box } else if scene.is(o, CYLINDER) { Prim::Cylinder } else { Prim::Plane };
                primitive(class, &p, &name, &mut w)
            }
            Some(o) if scene.is(o, DUMMY) => {
                let v = scene.chunk(o, 0x09ed).map(|c| c.f32s()).unwrap_or_default();
                let size = if v.len() >= 6 { (Vec3::new(v[3], v[4], v[5]) - Vec3::new(v[0], v[1], v[2])).abs() } else { Vec3::splat(10.0) };
                MaxKind::Dummy { size }
            }
            Some(o) if scene.is(o, SPLINE_SHAPE) || scene.is(o, LINE) => match shape::read(&scene.objects[o]) {
                Ok(s) => MaxKind::Shape(MaxShape { line: scene.is(o, LINE), ..s }),
                Err(e) => {
                    w.push(format!("{name}: {} not read ({e}); a helper keeps its place", scene.name(o)));
                    MaxKind::Other(scene.name(o))
                }
            },
            Some(o) if scene.is(o, FREE_CAMERA) => MaxKind::Camera { params: camera_params(&param_block(&scene, o).unwrap_or_default()) },
            Some(o) if scene.is(o, OMNI) => MaxKind::Light { kind: "omni", params: light_params(&scene, o, false) },
            Some(o) if scene.is(o, TARGET_DIRECT) => MaxKind::Light { kind: "target_direct", params: light_params(&scene, o, true) },
            Some(o) if scene.is(o, TARGET) => MaxKind::Target,
            Some(o) => {
                let c = scene.name(o);
                w.push(format!("{name}: {c} is not imported; a helper keeps its place in the hierarchy"));
                MaxKind::Other(c)
            }
        };
        if !modifiers.is_empty() && !matches!(kind, MaxKind::Mesh(_) | MaxKind::Box { .. } | MaxKind::Cylinder { .. } | MaxKind::Plane { .. } | MaxKind::Shape(_)) {
            w.push(format!("{name}: its modifiers are not imported (the object under them is not)"));
            modifiers.clear();
        }
        lathe_axis(&mut kind, &mut modifiers, &mut offset, &name, &mut w);
        let material = refs.get(3).copied().flatten().and_then(|m| {
            let got = materials.material(m, 0);
            if got.is_none() {
                w.push(format!("{name}: material {} is not imported", scene.name(m)));
            }
            got
        });
        let layer = refs.get(6).copied().flatten().filter(|l| scene.is(*l, BASE_LAYER)).map(|l| {
            let ly = layer(&scene, l);
            match doc.layers.iter().position(|x| *x == ly) {
                Some(i) => i,
                None => {
                    doc.layers.push(ly);
                    doc.layers.len() - 1
                }
            }
        });
        doc.warnings.extend(w);
        doc.nodes.push(MaxNode { name, parent, local, offset, wire_color, kind, modifiers, material, layer, look_at });
    }
    doc.warnings.append(&mut materials.warnings);
    doc.materials = materials.out;
    // A parent that comes later in the file than its child is fine; a cycle is not.
    for i in 0..doc.nodes.len() {
        let mut p = doc.nodes[i].parent;
        let mut steps = 0;
        while let Some(pi) = p {
            steps += 1;
            if pi == i || steps > doc.nodes.len() {
                doc.warnings.push(format!("{}: its parent chain loops; linked to the scene root", doc.nodes[i].name));
                doc.nodes[i].parent = None;
                break;
            }
            p = doc.nodes[pi].parent;
        }
    }
    if doc.nodes.is_empty() {
        return Err("the file has no nodes".into());
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damaged_primitive_counts_are_held_to_max_limits() {
        let mut w = Vec::new();
        let p: std::collections::BTreeMap<u32, f32> = [(0, 3.5), (1, f32::NAN), (2, 687_865_856.0), (3, -4.0), (4, 40.0)].into_iter().collect();
        let MaxKind::Cylinder { radius, height, height_segs, cap_segs, sides, .. } = primitive(Prim::Cylinder, &p, "Cyl", &mut w) else { panic!() };
        assert_eq!((radius, height), (3.5, 1.0));
        assert_eq!((height_segs, cap_segs, sides), (200, 1, 40));
        assert_eq!(w.len(), 3, "{w:?}");
        let MaxKind::Box { length_segs, .. } = primitive(Prim::Box, &[(4, 1e12)].into_iter().collect(), "Box", &mut w) else { panic!() };
        assert_eq!(length_segs, 200);
    }
}
