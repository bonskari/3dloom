//! Modifier stacks. A Derived Object references its modifiers, top of the
//! stack first, then the object below (a base object or another Derived
//! Object); per modifier it keeps a 0x2500 context: 0x2510 the context
//! matrix (object space → modifier context; four rows of three floats),
//! 0x2511 the bounds of the modifier's input in context space, 0x2512 the
//! modifier's own per-object data. Which way the references run was read off
//! the stored bounds (a Symmetry's input is the half, the FFD above it sees
//! the whole), and each modifier's parameter numbers off sample files whose
//! values are known (defaults, and results the stored bounds confirm).

use crate::params::{self, PVal};
use crate::Scene;
use glam::{Mat4, Vec3};
use std::collections::BTreeMap;

/// One modifier in Loom's terms: the Modifier List name, the parameters by
/// their Loom names, and the gizmo / centre / lattice / UVs it carries.
#[derive(Debug, Clone, PartialEq)]
pub struct MaxModifier {
    pub class: &'static str,
    /// The stack entry's own name when it was renamed in the file, else empty.
    pub name: String,
    pub params: Vec<(&'static str, f64)>,
    /// Gizmo → object space.
    pub gizmo: Option<Mat4>,
    /// The Center sub-object, in gizmo space.
    pub center: Option<Vec3>,
    pub lattice: Option<Lattice>,
    pub uvs: Option<UnwrapUvs>,
}

/// An FFD lattice: its volume in gizmo space and the control points as
/// fractions of it, x fastest, then y, then z.
#[derive(Debug, Clone, PartialEq)]
pub struct Lattice {
    pub dims: [u32; 3],
    pub lo: Vec3,
    pub hi: Vec3,
    pub points: Vec<Vec3>,
}

/// Unwrap UVW's texture vertices and, per polygon of its input, the texture
/// vertex and the geometry vertex of each corner.
#[derive(Debug, Clone, PartialEq)]
pub struct UnwrapUvs {
    pub verts: Vec<Vec3>,
    pub faces: Vec<(Vec<u32>, Vec<u32>)>,
}

const PARAM_BLOCK: (u32, u32, u32) = (0x8, 0, 0x8);
const PARAM_BLOCK2: (u32, u32, u32) = (0x82, 0, 0x82);
const PRS: (u32, u32, u32) = (0x2005, 0, 0x9008);
const POSITION_XYZ: (u32, u32, u32) = (0x118f_7e02, 0xffee_238a, 0x900b);

const BEND: (u32, u32, u32) = (0x10, 0, 0x810);
const EXTRUDE: (u32, u32, u32) = (0xa0, 0, 0x810);
pub(crate) const LATHE: (u32, u32, u32) = (0xb0, 0, 0x810);
const UVW_MAP: (u32, u32, u32) = (0xf_72b1, 0, 0x810);
const SMOOTH: (u32, u32, u32) = (0xf_8613, 0, 0x810);
const XFORM: (u32, u32, u32) = (0x2521_5824, 0, 0x810);
const TURBOSMOOTH: (u32, u32, u32) = (0x0d72_7b3e, 0x491d_29a7, 0x810);
const SYMMETRY: (u32, u32, u32) = (0x2d7e_702b, 0x0be4_1a6e, 0x810);
const SHELL: (u32, u32, u32) = (0x3b9b_1a16, 0x6d84_e8d0, 0x810);
const PUSH: (u32, u32, u32) = (0x0618_372d, 0x67ad_0adf, 0x810);
const SPHERIFY: (u32, u32, u32) = (0xde17_a34e, 0x8a41_e2a0, 0x810);
const FFD_4X4X4: (u32, u32, u32) = (0x2132_5596, 0x2cd1_0bd8, 0x810);
const UNWRAP_UVW: (u32, u32, u32) = (0x02df_2e3a, 0x72ba_4e1f, 0x810);

/// How a file value becomes a Loom parameter value.
#[derive(Clone, Copy)]
enum Conv {
    Same,
    RadToDeg,
}

/// (file parameter number, Loom name, conversion) of one modifier class.
type ParamMap = &'static [(u32, &'static str, Conv)];

/// Per modifier class: the Loom name and its parameter map.
fn table(id: (u32, u32, u32)) -> Option<(&'static str, ParamMap)> {
    use Conv::*;
    Some(match id {
        BEND => ("Bend", &[(0, "Angle", Same), (1, "Direction", Same), (2, "Bend Axis", Same), (3, "Limit Effect", Same), (4, "Lower Limit", Same), (5, "Upper Limit", Same)]),
        TURBOSMOOTH => ("TurboSmooth", &[(0, "Iterations", Same), (1, "Render Iters On", Same), (2, "Render Iters", Same), (3, "Isoline Display", Same), (5, "Materials", Same), (6, "Smoothing Groups", Same), (8, "Explicit Normals", Same)]),
        SYMMETRY => ("Symmetry", &[(0, "Mirror Axis", Same), (1, "Slice Along Mirror", Same), (2, "Weld Seam", Same), (3, "Threshold", Same), (4, "Flip", Same)]),
        SHELL => ("Shell", &[(0, "Inner Amount", Same), (1, "Outer Amount", Same), (3, "Segments", Same), (14, "Auto Smooth Edge", Same), (15, "Angle", Same)]),
        PUSH => ("Push", &[(0, "Push Value", Same)]),
        SPHERIFY => ("Spherify", &[(0, "Percent", Same)]),
        EXTRUDE => ("Extrude", &[(0, "Amount", Same), (1, "Segments", Same), (2, "Cap Start", Same), (3, "Cap End", Same)]),
        SMOOTH => ("Smooth", &[(0, "Auto Smooth", Same), (1, "Threshold", RadToDeg), (2, "Smoothing Groups", Same), (3, "Prevent Indirect Smoothing", Same)]),
        UVW_MAP => ("UVW Map", &[(0, "Mapping", Same), (1, "U Tile", Same), (2, "V Tile", Same), (3, "W Tile", Same), (4, "U Flip", Same), (5, "V Flip", Same), (6, "W Flip", Same), (9, "Length", Same), (10, "Width", Same), (11, "Height", Same), (12, "Alignment", Same), (13, "Channel", Same)]),
        LATHE => ("Lathe", &[(0, "Degrees", Same), (1, "Segments", Same), (2, "Cap Start", Same), (3, "Cap End", Same)]),
        XFORM => ("XForm", &[]),
        FFD_4X4X4 => ("FFD 4x4x4", &[]),
        UNWRAP_UVW => ("Unwrap UVW", &[]),
        _ => return None,
    })
}

/// A row-major 4×3 matrix (three axes, then the translation) as a Mat4.
pub(crate) fn rows_to_mat(v: &[f32]) -> Option<Mat4> {
    if v.len() < 12 || v[..12].iter().any(|x| !x.is_finite()) {
        return None;
    }
    let r = |i: usize| Vec3::new(v[i * 3], v[i * 3 + 1], v[i * 3 + 2]);
    Some(Mat4::from_cols(r(0).extend(0.0), r(1).extend(0.0), r(2).extend(0.0), r(3).extend(1.0)))
}

/// One modifier's context in its Derived Object.
pub struct Context<'a> {
    pub tm: Mat4,
    pub bounds: Option<(Vec3, Vec3)>,
    pub local: Option<&'a crate::chunk::Chunk>,
}

pub fn contexts(scene: &Scene, derived: usize) -> Vec<Context<'_>> {
    let Some(d) = scene.objects.get(derived) else { return Vec::new() };
    d.kids()
        .iter()
        .filter(|c| c.id == 0x2500)
        .map(|c| {
            let tm = c.child(0x2510).and_then(|t| rows_to_mat(&t.f32s())).unwrap_or(Mat4::IDENTITY);
            let bounds = c.child(0x2511).map(|b| b.f32s()).filter(|b| b.len() >= 6 && b.iter().all(|x| x.is_finite())).map(|b| (Vec3::new(b[0], b[1], b[2]), Vec3::new(b[3], b[4], b[5])));
            Context { tm, bounds, local: c.child(0x2512) }
        })
        .collect()
}

/// Reads modifier `m` (a scene object) with its context; None (and a warning) when Loom cannot take it.
pub fn read(scene: &Scene, m: usize, ctx: &Context, node: &str, w: &mut Vec<String>) -> Option<MaxModifier> {
    let class = scene.class(m).map(|c| c.id);
    let Some((name, map)) = class.and_then(table) else {
        w.push(format!("{node}: modifier {} is not imported; the stack is evaluated without it", scene.name(m)));
        return None;
    };
    let refs: Vec<usize> = scene.refs(m).into_iter().flatten().collect();
    let pb = refs.iter().copied().find(|r| scene.is(*r, PARAM_BLOCK) || scene.is(*r, PARAM_BLOCK2)).and_then(|r| scene.objects.get(r)).map(params::read).unwrap_or_default();
    let mut out = MaxModifier { class: name, name: String::new(), params: Vec::new(), gizmo: None, center: None, lattice: None, uvs: None };
    for (idx, loom, conv) in map {
        let Some(v) = pb.get(idx).and_then(|v: &PVal| v.f()) else { continue };
        if !v.is_finite() {
            w.push(format!("{node}: {name} {loom} is not a number; left at its default"));
            continue;
        }
        out.params.push((loom, match conv {
            Conv::Same => v as f64,
            Conv::RadToDeg => (v as f64).to_degrees(),
        }));
    }
    // A renamed entry keeps its name in 0x39BF > 0x0100.
    if let Some(n) = scene.chunk(m, 0x39bf).and_then(|c| c.child(0x0100)).map(|c| c.utf16()).filter(|n| !n.is_empty() && n != name) {
        out.name = n;
    }
    let ctx_inv = ctx.tm.inverse();
    let prs = refs.iter().copied().find(|r| scene.is(*r, PRS)).map(|r| scene.transform(Some(r), w, node));
    let center = refs.iter().copied().find(|r| scene.is(*r, POSITION_XYZ)).map(|r| scene.position(Some(r), w));
    match class {
        Some(XFORM) => {
            // The gizmo moves the points (about the centre), in context space.
            let g = prs.unwrap_or(Mat4::IDENTITY);
            let c = center.unwrap_or(Vec3::ZERO);
            out.gizmo = Some(ctx_inv * Mat4::from_translation(c) * g * Mat4::from_translation(-c) * ctx.tm);
        }
        Some(FFD_4X4X4) => {
            let pts = scene.chunk(m, 0x0200).map(|c| c.f32s()).unwrap_or_default();
            let Some((lo, hi)) = ctx.bounds.filter(|_| pts.len() == 64 * 3 && pts.iter().all(|x| x.is_finite())) else {
                w.push(format!("{node}: FFD 4x4x4 lattice not read; the stack is evaluated without it"));
                return None;
            };
            // The file runs z fastest, then y, then x.
            let mut points = vec![Vec3::ZERO; 64];
            for (k, p) in pts.as_chunks::<3>().0.iter().enumerate() {
                let (i, j, l) = (k / 16, (k / 4) % 4, k % 4);
                points[i + 4 * (j + 4 * l)] = Vec3::new(p[0], p[1], p[2]);
            }
            out.lattice = Some(Lattice { dims: [4, 4, 4], lo, hi, points });
            out.gizmo = Some(ctx_inv * prs.unwrap_or(Mat4::IDENTITY));
        }
        Some(UNWRAP_UVW) => match ctx.local.ok_or_else(|| "no per-object data".to_string()).and_then(unwrap_uvs) {
            Ok(u) => out.uvs = Some(u),
            Err(e) => {
                w.push(format!("{node}: Unwrap UVW texture vertices not read ({e}); the stack is evaluated without it"));
                return None;
            }
        },
        _ => {
            if let Some(g) = prs {
                out.gizmo = Some(ctx_inv * g);
            } else if ctx.tm != Mat4::IDENTITY && matches!(class, Some(BEND) | Some(SYMMETRY) | Some(UVW_MAP)) {
                out.gizmo = Some(ctx_inv);
            }
            out.center = center;
        }
    }
    Some(out)
}

/// Unwrap UVW per-object data: 0x0860 texture vertices (u32 count; 24 bytes
/// each, the UVW first), 0x0870 faces (u32 count; per face u32 degree, the
/// texture vertices, three u32, the geometry vertices).
fn unwrap_uvs(local: &crate::chunk::Chunk) -> Result<UnwrapUvs, String> {
    let tv = local.child(0x0860).ok_or("no texture vertices")?;
    let n = tv.data.get(..4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize).ok_or("texture vertices truncated")?;
    if tv.data.len() != 4 + n.checked_mul(24).ok_or("count too large")? {
        return Err("texture vertex layout not recognised".into());
    }
    let verts: Vec<Vec3> = tv.data[4..]
        .as_chunks::<24>()
        .0
        .iter()
        .map(|c| {
            let f = |o: usize| f32::from_le_bytes(c[o..o + 4].try_into().unwrap());
            Vec3::new(f(0), f(4), f(8))
        })
        .collect();
    if verts.iter().any(|v| !v.is_finite()) {
        return Err("a texture vertex is not a number".into());
    }
    let fc = local.child(0x0870).ok_or("no faces")?;
    let u: Vec<u32> = fc.u32s();
    if fc.data.len() % 4 != 0 || u.is_empty() {
        return Err("face layout not recognised".into());
    }
    let count = u[0] as usize;
    let mut o = 1usize;
    let mut faces = Vec::with_capacity(count.min(u.len()));
    for _ in 0..count {
        let d = *u.get(o).ok_or("faces truncated")? as usize;
        if !(3..=1024).contains(&d) || o + 1 + 2 * d + 3 > u.len() {
            return Err("face layout not recognised".into());
        }
        let t = u[o + 1..o + 1 + d].to_vec();
        let v = u[o + 4 + d..o + 4 + 2 * d].to_vec();
        if t.iter().any(|x| *x as usize >= verts.len()) {
            return Err("a face names a texture vertex there is not".into());
        }
        faces.push((t, v));
        o += 4 + 2 * d;
    }
    if o != u.len() {
        return Err("face layout not recognised".into());
    }
    Ok(UnwrapUvs { verts, faces })
}

/// The modifiers of a node's object, bottom of the stack first, and the base
/// object under them.
pub fn stack(scene: &Scene, obj: Option<usize>, node: &str, w: &mut Vec<String>) -> (Vec<MaxModifier>, Option<usize>) {
    let mut top_first = Vec::new();
    let mut obj = obj;
    let mut guard = 0;
    let mut seen = BTreeMap::new();
    while let Some(o) = obj.filter(|o| scene.objects.get(*o).is_some_and(|c| c.id == crate::DERIVED_OBJECT_CHUNK)) {
        guard += 1;
        if guard > 32 || seen.insert(o, ()).is_some() {
            w.push(format!("{node}: modifier stack loops; not imported"));
            return (Vec::new(), None);
        }
        let r: Vec<Option<usize>> = scene.refs(o);
        let ctxs = contexts(scene, o);
        for (i, m) in r.iter().take(r.len().saturating_sub(1)).enumerate() {
            let Some(m) = m else { continue };
            let Some(ctx) = ctxs.get(i) else {
                w.push(format!("{node}: modifier {} has no context; not imported", scene.name(*m)));
                continue;
            };
            if let Some(md) = read(scene, *m, ctx, node, w) {
                top_first.push(md);
            }
        }
        obj = r.last().copied().flatten();
    }
    top_first.reverse();
    (top_first, obj)
}
