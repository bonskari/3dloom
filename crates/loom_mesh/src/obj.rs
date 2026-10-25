//! Wavefront OBJ: text in and out of [`Mesh`] (positions, n-gon faces,
//! smoothing groups, material ids, `vt` as map channel 1) with the
//! importer's and exporter's options, and MTL material libraries. On
//! import `vn` normals can mark the hard edges; export writes normals per
//! smoothing group.

use crate::indexed::{IndexedFace, IndexedMesh};
use crate::Mesh;
use glam::{Mat4, Vec3};
use std::collections::BTreeMap;

/// One `o` block of a file.
#[derive(Debug, Clone)]
pub struct ObjObject {
    pub name: String,
    pub mesh: Mesh,
}

/// Builds a mesh from imported faces the way OBJ import does: degenerate
/// faces dropped, inconsistently wound faces flipped, faces that still do
/// not fit a manifold split off with their own vertices.
pub fn build_repaired(im: IndexedMesh) -> Result<Mesh, String> {
    build_repaired_mapped(im).map(|(m, _)| m)
}

/// `build_repaired` plus, per built vertex, the source vertex of the input
/// (a vertex split off by the manifold repair maps to the one it copies).
/// Exact when every input vertex is kept (`im.keep` holds them all).
pub fn build_repaired_mapped(mut im: IndexedMesh) -> Result<(Mesh, Vec<u32>), String> {
    im.drop_degenerate();
    let n = im.positions.len() as u32;
    if im.faces.is_empty() {
        return Ok((Mesh::default(), Vec::new()));
    }
    let mut map: Vec<u32> = (0..n).collect();
    match im.build() {
        Ok(m) => Ok((m, map)),
        Err(_) => {
            for (fresh, src) in make_manifold(&mut im) {
                if fresh as usize == map.len() {
                    map.push(src);
                }
            }
            im.build().map(|m| (m, map))
        }
    }
}

/// Real-world OBJs are often inconsistently wound or non-manifold. A face
/// that reuses a directed edge is flipped when that makes it fit; one that
/// still cannot fit (three faces on an edge) gets its own copy of the
/// vertices, as a separate element.
fn make_manifold(im: &mut IndexedMesh) -> Vec<(u32, u32)> {
    use std::collections::HashSet;
    let mut copies: Vec<(u32, u32)> = Vec::new();
    let mut used: HashSet<(u32, u32)> = HashSet::new();
    let fits = |ids: &[u32], used: &HashSet<(u32, u32)>| ids.iter().enumerate().all(|(i, a)| !used.contains(&(*a, ids[(i + 1) % ids.len()])));
    for fi in 0..im.faces.len() {
        let ids = im.faces[fi].ids.clone();
        if !fits(&ids, &used) {
            let mut rev = ids.clone();
            rev.reverse();
            if fits(&rev, &used) {
                let f = &mut im.faces[fi];
                for row in f.uv.values_mut() {
                    row.reverse();
                }
                f.ids = rev;
            } else {
                let fresh: Vec<u32> = ids.iter().map(|v| im.add_vertex(im.positions[*v as usize], Some(*v))).collect();
                copies.extend(fresh.iter().zip(&ids).map(|(f, s)| (*f, *s)));
                im.faces[fi].ids = fresh;
            }
        }
        let ids = &im.faces[fi].ids;
        for (i, a) in ids.iter().enumerate() {
            used.insert((*a, ids[(i + 1) % ids.len()]));
        }
    }
    copies
}

/// Parses an OBJ file. `o` (else `g`) starts an object; a file with neither
/// yields one object named `default_name`. Smoothing comes from the `s`
/// lines (none: group 1); `vn` normals are not used.
pub fn parse(text: &str, default_name: &str) -> Result<Vec<ObjObject>, String> {
    parse_with(text, default_name, &ObjReadOpts::default()).map(|f| f.objects)
}

/// How `parse_with` builds the meshes (the OBJ Importer options that act
/// on geometry).
#[derive(Debug, Clone)]
pub struct ObjReadOpts {
    /// Every object in one mesh named `default_name` (Import as single mesh).
    pub single_mesh: bool,
    pub texture_coords: bool,
    /// `s` lines (off: one smoothing group, unless the normals decide).
    pub smoothing_groups: bool,
    pub normals: ObjNormalMode,
    pub flip_normals: bool,
    pub retriangulate: bool,
    /// (x, y, z) → (x, -z, y): a Y-up file into Z-up (Flip ZY-axis).
    pub flip_zy: bool,
    pub scale: f32,
}

/// Where smoothing comes from on import.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ObjNormalMode {
    /// The `vn` normals mark hard edges; without them the smoothing groups.
    FromFile,
    FromSmoothingGroups,
    /// Auto Smooth at this angle (degrees).
    AutoSmooth(f32),
    Faceted,
}

impl Default for ObjReadOpts {
    fn default() -> Self {
        Self { single_mesh: false, texture_coords: true, smoothing_groups: true, normals: ObjNormalMode::FromSmoothingGroups, flip_normals: false, retriangulate: false, flip_zy: false, scale: 1.0 }
    }
}

/// A parsed file: its objects and what it holds.
#[derive(Debug, Clone, Default)]
pub struct ObjFile {
    pub objects: Vec<ObjObject>,
    /// `mtllib` files, as written.
    pub mtllibs: Vec<String>,
    /// Material id → `usemtl` name (ids of `mat_<n>` names have none).
    pub material_names: BTreeMap<u16, String>,
    pub has_normals: bool,
    pub has_texture_coords: bool,
    pub has_smoothing_groups: bool,
}

/// Object names and face counts without building meshes (the Importer's list).
pub fn scan(text: &str, default_name: &str) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    let mut cur = (default_name.to_owned(), 0usize);
    let mut has_o = false;
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        let mut it = line.split_whitespace();
        match it.next() {
            Some(k @ ("o" | "g")) => {
                if k == "o" {
                    has_o = true;
                } else if has_o {
                    continue;
                }
                let name = it.collect::<Vec<_>>().join(" ");
                let done = std::mem::replace(&mut cur, (if name.is_empty() { default_name.to_owned() } else { name }, 0));
                if done.1 > 0 {
                    out.push(done);
                }
            }
            Some("f") => cur.1 += 1,
            _ => {}
        }
    }
    if cur.1 > 0 {
        out.push(cur);
    }
    out
}

/// Hard edges (source vertex pairs) where a vertex's `vn` differs between the
/// two faces of an edge.
fn hard_from_normals(faces: &[IndexedFace], normals: &[Vec<Option<Vec3>>]) -> std::collections::HashSet<(u32, u32)> {
    let mut on_edge: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (fi, f) in faces.iter().enumerate() {
        let k = f.ids.len();
        for i in 0..k {
            let (a, b) = (f.ids[i], f.ids[(i + 1) % k]);
            on_edge.entry((a.min(b), a.max(b))).or_default().push(fi);
        }
    }
    let cos_t = 1f32.to_radians().cos();
    let mut hard = std::collections::HashSet::new();
    for ((a, b), list) in on_edge {
        if list.len() != 2 {
            continue;
        }
        let n_at = |face: usize, v: u32| faces[face].ids.iter().position(|x| *x == v).and_then(|c| normals[face].get(c).copied().flatten());
        let differs = |v: u32| match (n_at(list[0], v), n_at(list[1], v)) {
            (Some(x), Some(y)) => x.normalize_or_zero().dot(y.normalize_or_zero()) < cos_t,
            _ => false,
        };
        if differs(a) || differs(b) {
            hard.insert((a, b));
        }
    }
    hard
}

#[derive(Default)]
struct Builder {
    name: String,
    im: IndexedMesh,
    map: std::collections::HashMap<usize, u32>,
    /// Per face, its corners' `vn` normals.
    normals: Vec<Vec<Option<Vec3>>>,
    has_s: bool,
}

impl Builder {
    fn finish(self, opts: &ObjReadOpts, out: &mut Vec<ObjObject>) -> Result<(), String> {
        let mut im = self.im;
        if im.faces.is_empty() {
            return Ok(());
        }
        let file_normals = opts.normals == ObjNormalMode::FromFile && self.normals.iter().flatten().any(|n| n.is_some());
        let hard = if file_normals { hard_from_normals(&im.faces, &self.normals) } else { Default::default() };
        if file_normals {
            // Keep every vertex so the source ids above still name the built ones.
            im.keep = (0..im.positions.len() as u32).collect();
        }
        if opts.flip_normals {
            for f in &mut im.faces {
                *f = IndexedFace::reversed(f, f.ids.iter().rev().copied().collect());
            }
        }
        let (mut mesh, map) = build_repaired_mapped(im).map_err(|e| format!("object {:?}: {e}", self.name))?;
        if mesh.face_count() == 0 {
            return Ok(());
        }
        match opts.normals {
            ObjNormalMode::FromFile if file_normals => {
                let mut built = std::collections::HashSet::new();
                for f in 0..mesh.face_count() {
                    let ids = mesh.face_vertex_ids(crate::FaceId(f as u32)).unwrap_or_default();
                    for i in 0..ids.len() {
                        let (a, b) = (ids[i].0, ids[(i + 1) % ids.len()].0);
                        let (sa, sb) = (map.get(a as usize).copied().unwrap_or(a), map.get(b as usize).copied().unwrap_or(b));
                        if hard.contains(&(sa.min(sb), sa.max(sb))) {
                            built.insert((a.min(b), a.max(b)));
                        }
                    }
                }
                crate::mesh_mods::smoothing_groups_from_hard_edges(&mut mesh, &built);
            }
            ObjNormalMode::FromFile | ObjNormalMode::FromSmoothingGroups => {
                if !opts.smoothing_groups || !self.has_s {
                    for f in mesh.faces_mut() {
                        f.smoothing_groups = 1;
                    }
                }
            }
            ObjNormalMode::AutoSmooth(angle) => mesh.auto_smooth(angle),
            ObjNormalMode::Faceted => {
                for f in mesh.faces_mut() {
                    f.smoothing_groups = 0;
                }
            }
        }
        if opts.retriangulate {
            mesh = crate::mesh_mods::triangulate(&mesh)?;
        }
        out.push(ObjObject { name: self.name, mesh });
        Ok(())
    }
}

/// Parses an OBJ file with the importer's geometry options.
pub fn parse_with(text: &str, default_name: &str, opts: &ObjReadOpts) -> Result<ObjFile, String> {
    let mut file = ObjFile::default();
    let mut positions: Vec<Vec3> = Vec::new();
    let mut texcoords: Vec<Vec3> = Vec::new();
    let mut vnormals: Vec<Vec3> = Vec::new();
    let mut cur = Builder { name: default_name.to_owned(), ..Default::default() };
    let mut smoothing: u32 = 1;
    let mut named: Vec<String> = Vec::new();
    let mut material: u16 = 1;
    let mut has_o = false;
    let place = |p: Vec3| {
        let p = if opts.flip_zy { Vec3::new(p.x, -p.z, p.y) } else { p };
        p * opts.scale
    };
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut it = line.split_whitespace();
        let key = it.next().unwrap_or("");
        let err = |m: String| format!("line {}: {m}", lineno + 1);
        let three = |it: &mut std::str::SplitWhitespace, what: &str| -> Result<Vec3, String> {
            let mut c = [0.0f32; 3];
            for v in &mut c {
                *v = it.next().ok_or_else(|| err(format!("{what} needs three numbers")))?.parse().map_err(|e| err(format!("{e}")))?;
            }
            Ok(Vec3::from_array(c))
        };
        match key {
            "v" => positions.push(place(three(&mut it, "v")?)),
            "vn" => {
                let n = three(&mut it, "vn")?;
                vnormals.push(if opts.flip_zy { Vec3::new(n.x, -n.z, n.y) } else { n });
                file.has_normals = true;
            }
            "vt" => {
                let mut c = [0.0f32; 3];
                for (k, v) in c.iter_mut().enumerate() {
                    match it.next() {
                        Some(s) => *v = s.parse().map_err(|e| err(format!("{e}")))?,
                        None if k == 0 => return Err(err("vt needs a number".into())),
                        None => {}
                    }
                }
                texcoords.push(Vec3::from_array(c));
                file.has_texture_coords = true;
            }
            "mtllib" => file.mtllibs.push(it.collect::<Vec<_>>().join(" ")),
            "o" | "g" => {
                // `g` splits objects only in a file without `o` lines.
                if key == "o" {
                    has_o = true;
                } else if has_o {
                    continue;
                }
                if opts.single_mesh {
                    continue;
                }
                let name = it.collect::<Vec<_>>().join(" ");
                let name = if name.is_empty() { default_name.to_owned() } else { name };
                let done = std::mem::replace(&mut cur, Builder { name, ..Default::default() });
                done.finish(opts, &mut file.objects)?;
            }
            "s" => {
                let v = it.next().unwrap_or("off");
                smoothing = match v {
                    "off" | "0" => 0,
                    n => {
                        let n: u32 = n.parse().map_err(|_| err(format!("bad smoothing group {n:?}")))?;
                        1 << ((n.max(1) - 1) % 32)
                    }
                };
                file.has_smoothing_groups = true;
            }
            "usemtl" => {
                let name = it.collect::<Vec<_>>().join(" ");
                material = match name.strip_prefix("mat_").and_then(|n| n.parse::<u16>().ok()) {
                    Some(n) if n >= 1 => n,
                    _ => match named.iter().position(|m| *m == name) {
                        Some(i) => i as u16 + 1,
                        None => {
                            named.push(name.clone());
                            file.material_names.insert(named.len() as u16, name);
                            named.len() as u16
                        }
                    },
                };
            }
            "f" => {
                let mut ids: Vec<u32> = Vec::new();
                let mut uvs: Vec<Vec3> = Vec::new();
                let mut ns: Vec<Option<Vec3>> = Vec::new();
                let mut has_uv = false;
                for tok in it {
                    let mut parts = tok.split('/');
                    let first = parts.next().unwrap_or("");
                    let i: i64 = first.parse().map_err(|_| err(format!("bad face index {tok:?}")))?;
                    match parts.next().filter(|s| !s.is_empty()).map(|s| s.parse::<i64>()) {
                        Some(Ok(t)) => {
                            let ti = if t < 0 { texcoords.len() as i64 + t + 1 } else { t };
                            if ti < 1 || ti as usize > texcoords.len() {
                                return Err(err(format!("texture index {t} out of range (1..={})", texcoords.len())));
                            }
                            uvs.push(texcoords[ti as usize - 1]);
                            has_uv = true;
                        }
                        Some(Err(_)) => return Err(err(format!("bad texture index {tok:?}"))),
                        None => uvs.push(Vec3::ZERO),
                    }
                    ns.push(match parts.next().filter(|s| !s.is_empty()).map(|s| s.parse::<i64>()) {
                        Some(Ok(n)) => {
                            let ni = if n < 0 { vnormals.len() as i64 + n + 1 } else { n };
                            if ni < 1 || ni as usize > vnormals.len() {
                                return Err(err(format!("normal index {n} out of range (1..={})", vnormals.len())));
                            }
                            Some(vnormals[ni as usize - 1])
                        }
                        Some(Err(_)) => return Err(err(format!("bad normal index {tok:?}"))),
                        None => None,
                    });
                    let idx = if i < 0 { positions.len() as i64 + i + 1 } else { i };
                    if idx < 1 || idx as usize > positions.len() {
                        return Err(err(format!("face index {i} out of range (1..={})", positions.len())));
                    }
                    let idx = idx as usize;
                    let local = match cur.map.get(&idx) {
                        Some(l) => *l,
                        None => {
                            let l = cur.im.add_vertex(positions[idx - 1], None);
                            cur.map.insert(idx, l);
                            l
                        }
                    };
                    ids.push(local);
                }
                if ids.len() < 3 {
                    continue;
                }
                let uv = if has_uv && opts.texture_coords { std::iter::once((1u32, uvs)).collect() } else { Default::default() };
                let sg = if opts.smoothing_groups { smoothing } else { 1 };
                cur.im.faces.push(IndexedFace { ids, smoothing_groups: sg, material_id: material, hidden: false, uv });
                cur.normals.push(ns);
                cur.has_s |= file.has_smoothing_groups;
            }
            // vp, l, p and anything else: ignored.
            _ => {}
        }
    }
    cur.finish(opts, &mut file.objects)?;
    Ok(file)
}

/// Writes objects as OBJ text: world-space positions, per-vertex normals,
/// n-gon faces, `s` runs from smoothing groups and `usemtl mat_<id>` runs
/// from material ids.
pub fn write(objects: &[(String, Mesh, Mat4)]) -> String {
    let out: Vec<ObjOut> = objects.iter().map(|(name, mesh, world)| ObjOut { name: name.clone(), mesh: mesh.clone(), world: *world, materials: Vec::new() }).collect();
    write_with(&out, &ObjWriteOpts::default())
}

/// One object to write.
#[derive(Debug, Clone)]
pub struct ObjOut {
    pub name: String,
    pub mesh: Mesh,
    pub world: Mat4,
    /// `usemtl` names by material id (`materials[i]` is id i+1); missing ones are `mat_<id>`.
    pub materials: Vec<String>,
}

/// OBJ Export > Faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjFaceMode {
    Triangles,
    Quads,
    Polygons,
}

/// The OBJ Exporter options that act on the text.
#[derive(Debug, Clone)]
pub struct ObjWriteOpts {
    /// (x, y, z) → (x, z, -y): Z-up out as Y-up (Flip YZ-axis).
    pub flip_yz: bool,
    pub faces: ObjFaceMode,
    pub texture_coords: bool,
    pub normals: bool,
    pub smoothing_groups: bool,
    pub scale: f32,
    /// Decimal places of positions, texture coordinates and normals.
    pub precision: usize,
    /// Negative (relative) face indices.
    pub relative: bool,
    /// CR LF line ends.
    pub crlf: bool,
    /// The `mtllib` line, when materials go with the file.
    pub mtllib: Option<String>,
}

impl Default for ObjWriteOpts {
    fn default() -> Self {
        Self { flip_yz: false, faces: ObjFaceMode::Polygons, texture_coords: true, normals: true, smoothing_groups: true, scale: 1.0, precision: 6, relative: false, crlf: false, mtllib: None }
    }
}

/// A face's corners split for the Faces option: Quads cuts an n-gon into a
/// strip of quads (and a closing triangle), Triangles into a fan.
fn split_corners(n: usize, mode: ObjFaceMode) -> Vec<Vec<usize>> {
    match mode {
        ObjFaceMode::Polygons => vec![(0..n).collect()],
        ObjFaceMode::Triangles => (1..n - 1).map(|i| vec![0, i, i + 1]).collect(),
        ObjFaceMode::Quads => {
            if n <= 4 {
                return vec![(0..n).collect()];
            }
            let mut out = Vec::new();
            let mut i = 1;
            while i + 2 < n {
                out.push(vec![0, i, i + 1, i + 2]);
                i += 2;
            }
            if i + 1 < n {
                out.push(vec![0, i, i + 1]);
            }
            out
        }
    }
}

pub fn write_with(objects: &[ObjOut], opts: &ObjWriteOpts) -> String {
    let prec = opts.precision.min(12);
    let num = |x: f32| fmt_prec(x, prec);
    let mut out = String::from("# 3dLoom OBJ\n");
    if let Some(lib) = &opts.mtllib {
        out.push_str(&format!("mtllib {lib}\n"));
    }
    let flip = |p: Vec3| if opts.flip_yz { Vec3::new(p.x, p.z, -p.y) } else { p };
    let mut vbase = 0usize;
    let mut nbase = 0usize;
    let mut tbase = 0usize;
    for o in objects {
        let tri;
        let mesh = if opts.faces == ObjFaceMode::Triangles {
            tri = crate::mesh_mods::triangulate(&o.mesh).unwrap_or_else(|_| o.mesh.clone());
            &tri
        } else {
            &o.mesh
        };
        let world = o.world;
        out.push_str(&format!("o {}\n", o.name.replace(char::is_whitespace, "_")));
        for v in mesh.vertices() {
            let p = flip(world.transform_point3(v.position)) * opts.scale;
            out.push_str(&format!("v {} {} {}\n", num(p.x), num(p.y), num(p.z)));
        }
        let map = mesh.map(1).filter(|_| opts.texture_coords);
        if let Some(m) = map {
            for t in &m.verts {
                out.push_str(&format!("vt {} {} {}\n", num(t.x), num(t.y), num(t.z)));
            }
        }
        // Face normals; a corner's normal averages the faces around its
        // vertex that share a smoothing group with the face (group 0 = flat).
        let face_n: Vec<Vec3> = (0..mesh.face_count())
            .map(|i| {
                let pts = mesh.face_positions(crate::FaceId(i as u32)).unwrap_or_default();
                let mut n = Vec3::ZERO;
                for k in 0..pts.len() {
                    n += pts[k].cross(pts[(k + 1) % pts.len()]);
                }
                n
            })
            .collect();
        let mut around: Vec<Vec<usize>> = vec![Vec::new(); mesh.vertex_count()];
        let face_ids: Vec<Vec<u32>> = (0..mesh.face_count()).map(|i| mesh.face_vertex_ids(crate::FaceId(i as u32)).map(|v| v.iter().map(|x| x.0).collect()).unwrap_or_default()).collect();
        for (fi, ids) in face_ids.iter().enumerate() {
            for v in ids {
                around[*v as usize].push(fi);
            }
        }
        let normal_m = world.inverse().transpose();
        let mut normal_index: std::collections::HashMap<(u32, u32), usize> = std::collections::HashMap::new();
        let mut normals: Vec<Vec3> = Vec::new();
        let mut corner_n: Vec<Vec<usize>> = Vec::with_capacity(face_ids.len());
        for (fi, ids) in face_ids.iter().enumerate() {
            let sg = mesh.faces()[fi].smoothing_groups;
            let mut row = Vec::with_capacity(ids.len());
            for v in ids {
                let key = if sg == 0 { (u32::MAX, fi as u32) } else { (*v, sg) };
                let idx = *normal_index.entry(key).or_insert_with(|| {
                    let n = if sg == 0 {
                        face_n[fi]
                    } else {
                        around[*v as usize].iter().filter(|f| mesh.faces()[**f].smoothing_groups & sg != 0).map(|f| face_n[*f]).sum()
                    };
                    normals.push(flip(normal_m.transform_vector3(n)).normalize_or_zero());
                    normals.len() - 1
                });
                row.push(idx);
            }
            corner_n.push(row);
        }
        if opts.normals {
            for n in &normals {
                out.push_str(&format!("vn {} {} {}\n", num(n.x), num(n.y), num(n.z)));
            }
        }
        // Indices: absolute (1-based) or relative to what has been written so far.
        let (v_end, t_end, n_end) = (vbase + mesh.vertex_count(), tbase + map.map_or(0, |m| m.verts.len()), nbase + if opts.normals { normals.len() } else { 0 });
        let idx = |abs: usize, end: usize| if opts.relative { format!("{}", abs as i64 - end as i64 - 1) } else { format!("{abs}") };
        let mut cur_s: Option<u32> = None;
        let mut cur_m: Option<u16> = None;
        for (fi, ids) in face_ids.iter().enumerate() {
            if ids.is_empty() {
                continue;
            }
            let f = &mesh.faces()[fi];
            if cur_m != Some(f.material_id) {
                match o.materials.get(f.material_id.max(1) as usize - 1) {
                    Some(name) => out.push_str(&format!("usemtl {name}\n")),
                    None => out.push_str(&format!("usemtl mat_{}\n", f.material_id)),
                }
                cur_m = Some(f.material_id);
            }
            if opts.smoothing_groups && cur_s != Some(f.smoothing_groups) {
                match f.smoothing_groups {
                    0 => out.push_str("s off\n"),
                    g => out.push_str(&format!("s {}\n", g.trailing_zeros() + 1)),
                }
                cur_s = Some(f.smoothing_groups);
            }
            let corners = mesh.face_corners(crate::FaceId(fi as u32)).unwrap_or_default();
            for part in split_corners(ids.len(), opts.faces) {
                out.push('f');
                for k in part {
                    let v = idx(vbase + ids[k] as usize + 1, v_end);
                    let t = map.and_then(|m| corners.get(k).map(|he| idx(m.corners[he.0 as usize] as usize + tbase + 1, t_end)));
                    let n = opts.normals.then(|| idx(nbase + corner_n[fi][k] + 1, n_end));
                    match (t, n) {
                        (Some(t), Some(n)) => out.push_str(&format!(" {v}/{t}/{n}")),
                        (Some(t), None) => out.push_str(&format!(" {v}/{t}")),
                        (None, Some(n)) => out.push_str(&format!(" {v}//{n}")),
                        (None, None) => out.push_str(&format!(" {v}")),
                    }
                }
                out.push('\n');
            }
        }
        vbase = v_end;
        nbase = n_end;
        tbase = t_end;
    }
    if opts.crlf {
        out = out.replace('\n', "\r\n");
    }
    out
}

fn fmt(x: f32) -> String {
    fmt_prec(x, 6)
}

fn fmt_prec(x: f32, prec: usize) -> String {
    let s = format!("{x:.prec$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { s.as_str() };
    if s == "-0" { "0".into() } else { s.to_owned() }
}

/// One `newmtl` entry of an MTL library. Colours 0–1.
#[derive(Debug, Clone, PartialEq)]
pub struct MtlMaterial {
    pub name: String,
    pub ka: [f32; 3],
    pub kd: [f32; 3],
    pub ks: [f32; 3],
    pub ke: [f32; 3],
    /// Phong exponent, 0–1000.
    pub ns: f32,
    /// Dissolve: 1 opaque.
    pub d: f32,
    pub map_kd: Option<String>,
    pub map_ks: Option<String>,
    pub map_d: Option<String>,
    /// The bump map and its `-bm` multiplier, when given.
    pub map_bump: Option<(String, Option<f32>)>,
}

impl MtlMaterial {
    pub fn new(name: &str) -> Self {
        Self { name: name.into(), ka: [0.0; 3], kd: [0.8; 3], ks: [0.0; 3], ke: [0.0; 3], ns: 10.0, d: 1.0, map_kd: None, map_ks: None, map_d: None, map_bump: None }
    }
}

/// A map statement's file name: the last token after options like `-bm 0.5`.
fn map_arg(rest: &[&str]) -> (String, Option<f32>) {
    let mut bm = None;
    let mut i = 0;
    let mut name: Vec<&str> = Vec::new();
    while i < rest.len() {
        let t = rest[i];
        if t.starts_with('-') && name.is_empty() {
            // Options take one to three numbers; -bm takes one.
            let n = match t {
                "-bm" | "-boost" | "-texres" | "-imfchan" | "-clamp" | "-blendu" | "-blendv" | "-cc" | "-type" => 1,
                "-mm" => 2,
                "-o" | "-s" | "-t" => 3,
                _ => 0,
            };
            if t == "-bm" {
                bm = rest.get(i + 1).and_then(|v| v.parse().ok());
            }
            let mut k = 0;
            while k < n && rest.get(i + 1 + k).is_some_and(|v| v.parse::<f32>().is_ok() || matches!(*v, "on" | "off" | "r" | "g" | "b" | "m" | "l" | "z")) {
                k += 1;
            }
            i += 1 + k;
            continue;
        }
        name.push(t);
        i += 1;
    }
    (name.join(" "), bm)
}

pub fn parse_mtl(text: &str) -> Vec<MtlMaterial> {
    let mut out: Vec<MtlMaterial> = Vec::new();
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        let toks: Vec<&str> = line.split_whitespace().collect();
        let Some((key, rest)) = toks.split_first() else { continue };
        if *key == "newmtl" {
            out.push(MtlMaterial::new(&rest.join(" ")));
            continue;
        }
        let Some(m) = out.last_mut() else { continue };
        let rgb = |r: &[&str]| -> [f32; 3] {
            let v: Vec<f32> = r.iter().filter_map(|x| x.parse().ok()).collect();
            match v.len() {
                0 => [0.0; 3],
                1 | 2 => [v[0]; 3],
                _ => [v[0], v[1], v[2]],
            }
        };
        let one = |r: &[&str]| r.first().and_then(|x| x.parse::<f32>().ok());
        match key.to_ascii_lowercase().as_str() {
            "ka" => m.ka = rgb(rest),
            "kd" => m.kd = rgb(rest),
            "ks" => m.ks = rgb(rest),
            "ke" => m.ke = rgb(rest),
            "ns" => m.ns = one(rest).unwrap_or(m.ns),
            "d" => m.d = one(rest).unwrap_or(m.d),
            "tr" => m.d = 1.0 - one(rest).unwrap_or(0.0),
            "map_kd" => m.map_kd = Some(map_arg(rest).0),
            "map_ks" | "map_ns" => {
                if m.map_ks.is_none() {
                    m.map_ks = Some(map_arg(rest).0);
                }
            }
            "map_d" => m.map_d = Some(map_arg(rest).0),
            "map_bump" | "bump" => m.map_bump = Some(map_arg(rest)),
            _ => {}
        }
    }
    out
}

pub fn write_mtl(materials: &[MtlMaterial]) -> String {
    let c = |v: [f32; 3]| format!("{} {} {}", fmt(v[0]), fmt(v[1]), fmt(v[2]));
    let mut out = String::from("# 3dLoom MTL\n");
    for m in materials {
        out.push_str(&format!("\nnewmtl {}\n", m.name));
        out.push_str(&format!("Ns {}\nKa {}\nKd {}\nKs {}\n", fmt(m.ns), c(m.ka), c(m.kd), c(m.ks)));
        if m.ke != [0.0; 3] {
            out.push_str(&format!("Ke {}\n", c(m.ke)));
        }
        out.push_str(&format!("d {}\nTr {}\nillum 2\n", fmt(m.d), fmt(1.0 - m.d)));
        for (k, v) in [("map_Kd", &m.map_kd), ("map_Ks", &m.map_ks), ("map_d", &m.map_d)] {
            if let Some(p) = v {
                out.push_str(&format!("{k} {p}\n"));
            }
        }
        if let Some((p, bm)) = &m.map_bump {
            match bm {
                Some(b) => out.push_str(&format!("map_bump -bm {} {p}\n", fmt(*b))),
                None => out.push_str(&format!("map_bump {p}\n")),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives;

    #[test]
    fn parses_ngons_negative_indices_and_groups() {
        let text = "# comment\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 0 1\nvt 0 0\nvn 0 0 1\no Quad\ns 3\nusemtl red\nf 1/1/1 2/1/1 3/1/1 4/1/1\no Tri\ns off\nusemtl blue\nf -5 -4 -1\n";
        let objs = parse(text, "file").unwrap();
        assert_eq!(objs.len(), 2);
        assert_eq!(objs[0].name, "Quad");
        assert_eq!(objs[0].mesh.face_count(), 1);
        assert_eq!(objs[0].mesh.vertex_count(), 4, "only the vertices the object uses");
        assert_eq!(objs[0].mesh.face_vertex_ids(crate::FaceId(0)).unwrap().len(), 4);
        assert_eq!(objs[0].mesh.faces()[0].smoothing_groups, 1 << 2);
        assert_eq!(objs[0].mesh.faces()[0].material_id, 1);
        assert_eq!(objs[1].name, "Tri");
        assert_eq!(objs[1].mesh.faces()[0].smoothing_groups, 0);
        assert_eq!(objs[1].mesh.faces()[0].material_id, 2, "second distinct material name");
        let p: Vec<Vec3> = objs[1].mesh.vertices().iter().map(|v| v.position).collect();
        assert_eq!(p, vec![Vec3::ZERO, Vec3::X, Vec3::Z]);
    }

    #[test]
    fn round_trip_box_keeps_counts_smoothing_and_material() {
        let mut m = primitives::box_primitive(Vec3::new(1.0, 1.0, 1.0));
        for (i, f) in m.faces_mut().iter_mut().enumerate() {
            f.smoothing_groups = 1 << i;
            f.material_id = i as u16 + 1;
        }
        let text = write(&[("Box001".into(), m.clone(), Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0)))]);
        assert!(text.contains("o Box001") && text.contains("usemtl mat_3") && text.contains("s 3") && text.contains("vn "), "{text}");
        let objs = parse(&text, "x").unwrap();
        assert_eq!(objs.len(), 1);
        let b = &objs[0].mesh;
        b.check_invariants().unwrap();
        assert_eq!(b.vertex_count(), 8);
        assert_eq!(b.face_count(), 6);
        for (i, f) in b.faces().iter().enumerate() {
            assert_eq!(f.smoothing_groups, 1 << i);
            assert_eq!(f.material_id, i as u16 + 1);
        }
        assert!((b.aabb().0.x - 9.0).abs() < 1e-5 && (b.aabb().1.x - 11.0).abs() < 1e-5, "world space: {:?}", b.aabb());
    }

    #[test]
    fn missing_o_uses_file_stem_or_g() {
        let objs = parse("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n", "teapot").unwrap();
        assert_eq!(objs.len(), 1);
        assert_eq!(objs[0].name, "teapot");
        let objs = parse("g A\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\ng B\nv 2 0 0\nf 2 1 4\n", "teapot").unwrap();
        assert_eq!(objs.iter().map(|o| o.name.as_str()).collect::<Vec<_>>(), vec!["A", "B"]);
        // With `o` present, `g` is a sub-group and does not split.
        let objs = parse("o X\ng A\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\ng B\nv 2 0 0\nf 2 1 4\n", "t").unwrap();
        assert_eq!(objs.len(), 1);
        assert_eq!(objs[0].mesh.face_count(), 2);
    }

    #[test]
    fn import_flips_inconsistent_winding_and_splits_nonmanifold() {
        // Two triangles share edge 1-2 in the same direction: the second is flipped on import.
        let objs = parse("v 0 0 0\nv 1 0 0\nv 0 1 0\nv 1 1 0\nf 1 2 3\nf 1 2 4\n", "x").unwrap();
        assert_eq!(objs.len(), 1);
        let m = &objs[0].mesh;
        m.check_invariants().unwrap();
        assert_eq!(m.face_count(), 2);
        assert_eq!(m.vertex_count(), 4, "flipping keeps the shared vertices");
        // Three faces on one edge cannot be manifold: the extra face gets its own vertices.
        let objs = parse("v 0 0 0\nv 1 0 0\nv 0 1 0\nv 1 1 0\nv 0 0 1\nf 1 2 3\nf 2 1 4\nf 1 2 5\n", "x").unwrap();
        let m = &objs[0].mesh;
        m.check_invariants().unwrap();
        assert_eq!(m.face_count(), 3);
        assert!(m.vertex_count() > 5, "split vertices: {}", m.vertex_count());
    }

    #[test]
    fn export_normals_follow_smoothing_groups() {
        let mut m = primitives::box_primitive(Vec3::new(1.0, 1.0, 1.0));
        for f in m.faces_mut().iter_mut() {
            f.smoothing_groups = 0;
        }
        let text = write(&[("Box".into(), m.clone(), Mat4::IDENTITY)]);
        let vn: Vec<&str> = text.lines().filter(|l| l.starts_with("vn ")).collect();
        assert_eq!(vn.len(), 6, "one flat normal per face: {vn:?}");
        assert!(vn.contains(&"vn 0 0 1") && vn.contains(&"vn 0 0 -1"), "{vn:?}");
        let f: Vec<&str> = text.lines().filter(|l| l.starts_with("f ")).collect();
        assert!(f[0].split_whitespace().skip(1).all(|c| c.contains("//")), "{}", f[0]);
        parse(&text, "x").unwrap();
        // All in one group: one averaged normal per vertex.
        for f in m.faces_mut().iter_mut() {
            f.smoothing_groups = 1;
        }
        let text = write(&[("Box".into(), m, Mat4::IDENTITY)]);
        assert_eq!(text.lines().filter(|l| l.starts_with("vn ")).count(), 8);
    }

    #[test]
    fn rewound_faces_keep_their_uvs_with_their_vertices() {
        // Two triangles share edge 1-2 in the same direction; the second is flipped, its vt must follow.
        let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 1 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nvt 1 1\nf 1/1 2/2 3/3\nf 1/1 2/2 4/4\n";
        let objs = parse(text, "x").unwrap();
        let m = &objs[0].mesh;
        for f in 0..m.face_count() {
            for he in m.face_corners(crate::FaceId(f as u32)).unwrap() {
                let p = m.vertices()[m.half_edges()[he.0 as usize].origin.0 as usize].position;
                let uv = m.corner_uv(1, he).unwrap();
                assert!((uv.x - p.x).abs() < 1e-6 && (uv.y - p.y).abs() < 1e-6, "{p:?} has uv {uv:?}");
            }
        }
    }

    #[test]
    fn bad_index_is_an_error() {
        let e = parse("v 0 0 0\nf 1 2 3\n", "x").unwrap_err();
        assert!(e.contains("line 2") && e.contains("out of range"), "{e}");
        assert!(parse("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 2\n", "x").unwrap().is_empty(), "a degenerate face is dropped");
    }
}
