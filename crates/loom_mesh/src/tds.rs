//! The 3D Studio (.3ds) chunk format: triangle meshes with texture
//! coordinates, smoothing groups, edge visibility, material groups and the
//! mesh matrix (pivot), materials (colours, shininess, transparency,
//! diffuse map), omni / spot lights and cameras. The keyframer section is
//! skipped on read and not written.

use glam::{Mat4, Vec3};

const MAIN: u16 = 0x4D4D;
const VERSION: u16 = 0x0002;
const EDITOR: u16 = 0x3D3D;
const MESH_VERSION: u16 = 0x3D3E;
const MASTER_SCALE: u16 = 0x0100;
const NAMED_OBJECT: u16 = 0x4000;
const TRI_OBJECT: u16 = 0x4100;
const POINT_ARRAY: u16 = 0x4110;
const FACE_ARRAY: u16 = 0x4120;
const MSH_MAT_GROUP: u16 = 0x4130;
const TEX_VERTS: u16 = 0x4140;
const SMOOTH_GROUP: u16 = 0x4150;
const MESH_MATRIX: u16 = 0x4160;
const DIRECT_LIGHT: u16 = 0x4600;
const DL_SPOTLIGHT: u16 = 0x4610;
const DL_OFF: u16 = 0x4620;
const DL_MULTIPLIER: u16 = 0x465B;
const CAMERA: u16 = 0x4700;
const COLOR_F: u16 = 0x0010;
const COLOR_24: u16 = 0x0011;
const LIN_COLOR_24: u16 = 0x0012;
const INT_PERCENT: u16 = 0x0030;
const FLOAT_PERCENT: u16 = 0x0031;
const MATERIAL: u16 = 0xAFFF;
const MAT_NAME: u16 = 0xA000;
const MAT_AMBIENT: u16 = 0xA010;
const MAT_DIFFUSE: u16 = 0xA020;
const MAT_SPECULAR: u16 = 0xA030;
const MAT_SHININESS: u16 = 0xA040;
const MAT_SHIN_STRENGTH: u16 = 0xA041;
const MAT_TRANSPARENCY: u16 = 0xA050;
const MAT_TWO_SIDE: u16 = 0xA081;
const MAT_TEXMAP: u16 = 0xA200;
const MAT_MAPNAME: u16 = 0xA300;

/// Names in the file are at most this long (the format's limit).
pub const NAME_LEN: usize = 10;

/// Whether a face's edge from corner `k` to corner `k + 1` is visible.
pub fn edge_visible(flags: u16, k: usize) -> bool {
    flags & (1 << (2 - k.min(2))) != 0
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TdsMaterial {
    pub name: String,
    pub ambient: [f32; 3],
    pub diffuse: [f32; 3],
    pub specular: [f32; 3],
    /// Shininess and shininess strength, 0..1.
    pub shininess: f32,
    pub shin_strength: f32,
    /// 0 opaque .. 1 clear.
    pub transparency: f32,
    pub two_sided: bool,
    /// Diffuse map file and its amount (0..1).
    pub texture: Option<(String, f32)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TdsMesh {
    pub name: String,
    /// World-space vertices (as the format stores them).
    pub positions: Vec<Vec3>,
    /// One per vertex, or empty.
    pub uvs: Vec<[f32; 2]>,
    pub faces: Vec<[u16; 3]>,
    /// Edge visibility: bit 2 AB, bit 1 BC, bit 0 CA (see [`edge_visible`]).
    pub flags: Vec<u16>,
    /// One per face, or empty.
    pub smoothing: Vec<u32>,
    /// Material name → faces using it.
    pub material_groups: Vec<(String, Vec<u16>)>,
    /// The object's local frame (its pivot) in world space.
    pub matrix: Mat4,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TdsLight {
    pub name: String,
    pub position: Vec3,
    pub color: [f32; 3],
    pub multiplier: f32,
    pub off: bool,
    /// Target, hotspot and falloff (degrees) of a spotlight.
    pub spot: Option<(Vec3, f32, f32)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TdsCamera {
    pub name: String,
    pub position: Vec3,
    pub target: Vec3,
    /// Bank (roll) in degrees.
    pub roll: f32,
    /// Lens in mm.
    pub lens: f32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TdsFile {
    pub master_scale: f32,
    pub materials: Vec<TdsMaterial>,
    pub meshes: Vec<TdsMesh>,
    pub lights: Vec<TdsLight>,
    pub cameras: Vec<TdsCamera>,
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    fn need(&self, n: usize) -> Result<(), String> {
        if self.p + n > self.b.len() { Err("3DS file is cut short".into()) } else { Ok(()) }
    }
    fn u8(&mut self) -> Result<u8, String> {
        self.need(1)?;
        self.p += 1;
        Ok(self.b[self.p - 1])
    }
    fn u16(&mut self) -> Result<u16, String> {
        self.need(2)?;
        let v = u16::from_le_bytes([self.b[self.p], self.b[self.p + 1]]);
        self.p += 2;
        Ok(v)
    }
    fn u32(&mut self) -> Result<u32, String> {
        self.need(4)?;
        let v = u32::from_le_bytes(self.b[self.p..self.p + 4].try_into().expect("4 bytes"));
        self.p += 4;
        Ok(v)
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn vec3(&mut self) -> Result<Vec3, String> {
        Ok(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }
    fn cstr(&mut self) -> Result<String, String> {
        let start = self.p;
        while self.p < self.b.len() && self.b[self.p] != 0 {
            self.p += 1;
        }
        let s = String::from_utf8_lossy(&self.b[start..self.p]).into_owned();
        self.p = (self.p + 1).min(self.b.len());
        Ok(s)
    }
    /// The next chunk's id and end offset (clamped to `end`).
    fn chunk(&mut self, end: usize) -> Result<(u16, usize), String> {
        let id = self.u16()?;
        let len = self.u32()? as usize;
        if len < 6 {
            return Err(format!("3DS chunk {id:#06x} has a bad length {len}"));
        }
        Ok((id, (self.p - 6 + len).min(end)))
    }
}

/// A colour sub-chunk (24-bit or float, linear 24-bit preferred last).
fn read_color(r: &mut Reader, end: usize) -> Result<[f32; 3], String> {
    let mut c = [0.0; 3];
    while r.p + 6 <= end {
        let (id, e) = r.chunk(end)?;
        match id {
            COLOR_24 | LIN_COLOR_24 => c = [r.u8()? as f32 / 255.0, r.u8()? as f32 / 255.0, r.u8()? as f32 / 255.0],
            COLOR_F => c = [r.f32()?, r.f32()?, r.f32()?],
            _ => {}
        }
        r.p = e;
    }
    Ok(c)
}

fn read_percent(r: &mut Reader, end: usize) -> Result<f32, String> {
    let mut v = 0.0;
    while r.p + 6 <= end {
        let (id, e) = r.chunk(end)?;
        match id {
            INT_PERCENT => v = r.u16()? as f32 / 100.0,
            FLOAT_PERCENT => v = r.f32()? / 100.0,
            _ => {}
        }
        r.p = e;
    }
    Ok(v)
}

fn read_material(r: &mut Reader, end: usize) -> Result<TdsMaterial, String> {
    let mut m = TdsMaterial::default();
    while r.p + 6 <= end {
        let (id, e) = r.chunk(end)?;
        match id {
            MAT_NAME => m.name = r.cstr()?,
            MAT_AMBIENT => m.ambient = read_color(r, e)?,
            MAT_DIFFUSE => m.diffuse = read_color(r, e)?,
            MAT_SPECULAR => m.specular = read_color(r, e)?,
            MAT_SHININESS => m.shininess = read_percent(r, e)?,
            MAT_SHIN_STRENGTH => m.shin_strength = read_percent(r, e)?,
            MAT_TRANSPARENCY => m.transparency = read_percent(r, e)?,
            MAT_TWO_SIDE => m.two_sided = true,
            MAT_TEXMAP => {
                let (mut file, mut amount) = (String::new(), 1.0);
                while r.p + 6 <= e {
                    let (sid, se) = r.chunk(e)?;
                    match sid {
                        MAT_MAPNAME => file = r.cstr()?,
                        INT_PERCENT => amount = r.u16()? as f32 / 100.0,
                        FLOAT_PERCENT => amount = r.f32()? / 100.0,
                        _ => {}
                    }
                    r.p = se;
                }
                if !file.is_empty() {
                    m.texture = Some((file, amount));
                }
            }
            _ => {}
        }
        r.p = e;
    }
    Ok(m)
}

fn read_mesh(r: &mut Reader, end: usize, name: String) -> Result<TdsMesh, String> {
    let mut m = TdsMesh { name, matrix: Mat4::IDENTITY, ..Default::default() };
    while r.p + 6 <= end {
        let (id, e) = r.chunk(end)?;
        match id {
            POINT_ARRAY => {
                let n = r.u16()? as usize;
                m.positions = (0..n).map(|_| r.vec3()).collect::<Result<_, _>>()?;
            }
            TEX_VERTS => {
                let n = r.u16()? as usize;
                m.uvs = (0..n).map(|_| Ok::<_, String>([r.f32()?, r.f32()?])).collect::<Result<_, _>>()?;
            }
            MESH_MATRIX => {
                let x = r.vec3()?;
                let y = r.vec3()?;
                let z = r.vec3()?;
                let o = r.vec3()?;
                m.matrix = Mat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), o.extend(1.0));
            }
            FACE_ARRAY => {
                let n = r.u16()? as usize;
                for _ in 0..n {
                    m.faces.push([r.u16()?, r.u16()?, r.u16()?]);
                    m.flags.push(r.u16()?);
                }
                while r.p + 6 <= e {
                    let (sid, se) = r.chunk(e)?;
                    match sid {
                        MSH_MAT_GROUP => {
                            let name = r.cstr()?;
                            let k = r.u16()? as usize;
                            let faces = (0..k).map(|_| r.u16()).collect::<Result<_, _>>()?;
                            m.material_groups.push((name, faces));
                        }
                        SMOOTH_GROUP => m.smoothing = (0..n).map(|_| r.u32()).collect::<Result<_, _>>()?,
                        _ => {}
                    }
                    r.p = se;
                }
            }
            _ => {}
        }
        r.p = e;
    }
    if m.faces.iter().flatten().any(|i| *i as usize >= m.positions.len()) {
        return Err(format!("3DS object {}: a face refers to a missing vertex", m.name));
    }
    Ok(m)
}

/// Reads a .3ds file.
pub fn read(bytes: &[u8]) -> Result<TdsFile, String> {
    let mut r = Reader { b: bytes, p: 0 };
    let (id, end) = r.chunk(bytes.len())?;
    if id != MAIN {
        return Err("not a 3DS file".into());
    }
    let mut f = TdsFile { master_scale: 1.0, ..Default::default() };
    while r.p + 6 <= end {
        let (id, e) = r.chunk(end)?;
        if id == EDITOR {
            while r.p + 6 <= e {
                let (sid, se) = r.chunk(e)?;
                match sid {
                    MASTER_SCALE => f.master_scale = r.f32()?,
                    MATERIAL => f.materials.push(read_material(&mut r, se)?),
                    NAMED_OBJECT => {
                        let name = r.cstr()?;
                        while r.p + 6 <= se {
                            let (oid, oe) = r.chunk(se)?;
                            match oid {
                                TRI_OBJECT => f.meshes.push(read_mesh(&mut r, oe, name.clone())?),
                                DIRECT_LIGHT => {
                                    let mut l = TdsLight { name: name.clone(), position: r.vec3()?, multiplier: 1.0, ..Default::default() };
                                    while r.p + 6 <= oe {
                                        let (lid, le) = r.chunk(oe)?;
                                        match lid {
                                            COLOR_F => l.color = [r.f32()?, r.f32()?, r.f32()?],
                                            COLOR_24 => l.color = [r.u8()? as f32 / 255.0, r.u8()? as f32 / 255.0, r.u8()? as f32 / 255.0],
                                            DL_OFF => l.off = true,
                                            DL_MULTIPLIER => l.multiplier = r.f32()?,
                                            DL_SPOTLIGHT => l.spot = Some((r.vec3()?, r.f32()?, r.f32()?)),
                                            _ => {}
                                        }
                                        r.p = le;
                                    }
                                    f.lights.push(l);
                                }
                                CAMERA => f.cameras.push(TdsCamera { name: name.clone(), position: r.vec3()?, target: r.vec3()?, roll: r.f32()?, lens: r.f32()? }),
                                _ => {}
                            }
                            r.p = oe;
                        }
                    }
                    _ => {}
                }
                r.p = se;
            }
        }
        r.p = e;
    }
    Ok(f)
}

/// Writes chunks: `begin` returns the length slot, `end` fills it.
struct Writer {
    b: Vec<u8>,
}

impl Writer {
    fn begin(&mut self, id: u16) -> usize {
        self.b.extend_from_slice(&id.to_le_bytes());
        self.b.extend_from_slice(&[0; 4]);
        self.b.len() - 6
    }
    fn end(&mut self, at: usize) {
        let len = (self.b.len() - at) as u32;
        self.b[at + 2..at + 6].copy_from_slice(&len.to_le_bytes());
    }
    fn u16(&mut self, v: u16) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn vec3(&mut self, v: Vec3) {
        for c in v.to_array() {
            self.f32(c);
        }
    }
    fn cstr(&mut self, s: &str) {
        self.b.extend_from_slice(s.as_bytes());
        self.b.push(0);
    }
    fn color(&mut self, id: u16, c: [f32; 3]) {
        let at = self.begin(id);
        let s = self.begin(COLOR_24);
        for x in c {
            self.b.push((x.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
        self.end(s);
        self.end(at);
    }
    fn percent(&mut self, id: u16, v: f32) {
        let at = self.begin(id);
        let s = self.begin(INT_PERCENT);
        self.u16((v.clamp(0.0, 1.0) * 100.0).round() as u16);
        self.end(s);
        self.end(at);
    }
}

/// Writes a .3ds file. Callers keep names within [`NAME_LEN`] and meshes
/// within 65535 vertices and faces (see [`split_for_3ds`]).
pub fn write(f: &TdsFile) -> Vec<u8> {
    let mut w = Writer { b: Vec::new() };
    let main = w.begin(MAIN);
    let v = w.begin(VERSION);
    w.u32(3);
    w.end(v);
    let ed = w.begin(EDITOR);
    let v = w.begin(MESH_VERSION);
    w.u32(3);
    w.end(v);
    for m in &f.materials {
        let at = w.begin(MATERIAL);
        let n = w.begin(MAT_NAME);
        w.cstr(&m.name);
        w.end(n);
        w.color(MAT_AMBIENT, m.ambient);
        w.color(MAT_DIFFUSE, m.diffuse);
        w.color(MAT_SPECULAR, m.specular);
        w.percent(MAT_SHININESS, m.shininess);
        w.percent(MAT_SHIN_STRENGTH, m.shin_strength);
        w.percent(MAT_TRANSPARENCY, m.transparency);
        if m.two_sided {
            let t = w.begin(MAT_TWO_SIDE);
            w.end(t);
        }
        if let Some((file, amount)) = &m.texture {
            let t = w.begin(MAT_TEXMAP);
            let p = w.begin(INT_PERCENT);
            w.u16((amount.clamp(0.0, 1.0) * 100.0).round() as u16);
            w.end(p);
            let n = w.begin(MAT_MAPNAME);
            w.cstr(file);
            w.end(n);
            w.end(t);
        }
        w.end(at);
    }
    let s = w.begin(MASTER_SCALE);
    w.f32(if f.master_scale > 0.0 { f.master_scale } else { 1.0 });
    w.end(s);
    for m in &f.meshes {
        let ob = w.begin(NAMED_OBJECT);
        w.cstr(&m.name);
        let tri = w.begin(TRI_OBJECT);
        let pa = w.begin(POINT_ARRAY);
        w.u16(m.positions.len() as u16);
        for p in &m.positions {
            w.vec3(*p);
        }
        w.end(pa);
        if !m.uvs.is_empty() {
            let tv = w.begin(TEX_VERTS);
            w.u16(m.uvs.len() as u16);
            for uv in &m.uvs {
                w.f32(uv[0]);
                w.f32(uv[1]);
            }
            w.end(tv);
        }
        let mm = w.begin(MESH_MATRIX);
        for c in 0..4 {
            w.vec3(m.matrix.col(c).truncate());
        }
        w.end(mm);
        let fa = w.begin(FACE_ARRAY);
        w.u16(m.faces.len() as u16);
        for (i, t) in m.faces.iter().enumerate() {
            for x in t {
                w.u16(*x);
            }
            w.u16(m.flags.get(i).copied().unwrap_or(7));
        }
        for (name, faces) in &m.material_groups {
            let g = w.begin(MSH_MAT_GROUP);
            w.cstr(name);
            w.u16(faces.len() as u16);
            for x in faces {
                w.u16(*x);
            }
            w.end(g);
        }
        if !m.smoothing.is_empty() {
            let sg = w.begin(SMOOTH_GROUP);
            for s in &m.smoothing {
                w.u32(*s);
            }
            w.end(sg);
        }
        w.end(fa);
        w.end(tri);
        w.end(ob);
    }
    for l in &f.lights {
        let ob = w.begin(NAMED_OBJECT);
        w.cstr(&l.name);
        let dl = w.begin(DIRECT_LIGHT);
        w.vec3(l.position);
        let c = w.begin(COLOR_F);
        for x in l.color {
            w.f32(x);
        }
        w.end(c);
        if l.off {
            let o = w.begin(DL_OFF);
            w.end(o);
        }
        let mu = w.begin(DL_MULTIPLIER);
        w.f32(l.multiplier);
        w.end(mu);
        if let Some((target, hot, fall)) = l.spot {
            let sp = w.begin(DL_SPOTLIGHT);
            w.vec3(target);
            w.f32(hot);
            w.f32(fall);
            w.end(sp);
        }
        w.end(dl);
        w.end(ob);
    }
    for c in &f.cameras {
        let ob = w.begin(NAMED_OBJECT);
        w.cstr(&c.name);
        let cam = w.begin(CAMERA);
        w.vec3(c.position);
        w.vec3(c.target);
        w.f32(c.roll);
        w.f32(c.lens);
        w.end(cam);
        w.end(ob);
    }
    w.end(ed);
    w.end(main);
    w.b
}

/// A mesh as 3DS objects: triangles (n-gons split, inner diagonals hidden),
/// one vertex per position and texture coordinate (3DS keeps one per
/// vertex), split into parts of at most 65535 vertices / faces. Vertices go
/// to world space by `world`; `matrix` becomes the objects' mesh matrix.
/// Material groups use `material_names[id - 1]`.
pub fn split_for_3ds(name: &str, mesh: &crate::Mesh, world: Mat4, matrix: Mat4, material_names: &[String]) -> Vec<TdsMesh> {
    use std::collections::HashMap;
    let tri = crate::tri_mesh::from_poly(mesh).unwrap_or_else(|_| mesh.clone());
    let map = tri.map(1);
    let mut parts: Vec<TdsMesh> = Vec::new();
    let mut cur = TdsMesh { name: name.to_owned(), matrix, ..Default::default() };
    let mut index: HashMap<(u32, u32, u32), u16> = HashMap::new();
    let mut groups: HashMap<u16, Vec<u16>> = HashMap::new();
    let finish = |cur: &mut TdsMesh, groups: &mut HashMap<u16, Vec<u16>>, parts: &mut Vec<TdsMesh>| {
        let mut ids: Vec<u16> = groups.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            if let Some(n) = material_names.get(id as usize - 1).filter(|_| id > 0) {
                cur.material_groups.push((n.clone(), groups.remove(&id).unwrap_or_default()));
            }
        }
        groups.clear();
        parts.push(std::mem::take(cur));
    };
    for fi in 0..tri.face_count() {
        let Some(corners) = tri.face_corners(crate::FaceId(fi as u32)) else { continue };
        if corners.len() != 3 {
            continue;
        }
        if cur.positions.len() + 3 > 65535 || cur.faces.len() + 1 > 65535 {
            let next = TdsMesh { name: name.to_owned(), matrix, ..Default::default() };
            let mut done = std::mem::replace(&mut cur, next);
            finish(&mut done, &mut groups, &mut parts);
            index.clear();
        }
        let mut ids = [0u16; 3];
        let mut flags = 0u16;
        for (k, he) in corners.iter().enumerate() {
            let h = &tri.half_edges()[he.0 as usize];
            let v = h.origin.0;
            let t = map.map(|m| m.corners[he.0 as usize]).unwrap_or(u32::MAX);
            let uv = map.and_then(|m| m.verts.get(t as usize)).copied().unwrap_or(Vec3::ZERO);
            let key = (v, t, 0);
            ids[k] = *index.entry(key).or_insert_with(|| {
                cur.positions.push(world.transform_point3(tri.vertices()[v as usize].position));
                if map.is_some() {
                    cur.uvs.push([uv.x, uv.y]);
                }
                (cur.positions.len() - 1) as u16
            });
            if tri.edge_visible(*he) {
                flags |= 1 << (2 - k);
            }
        }
        let f = &tri.faces()[fi];
        groups.entry(f.material_id).or_default().push(cur.faces.len() as u16);
        cur.faces.push(ids);
        cur.flags.push(flags);
        cur.smoothing.push(f.smoothing_groups);
    }
    if !cur.faces.is_empty() || parts.is_empty() {
        finish(&mut cur, &mut groups, &mut parts);
    }
    parts
}

/// Shortens names to [`NAME_LEN`] bytes, keeping them unique with a number.
pub fn short_names(names: &[String]) -> Vec<String> {
    let mut used = std::collections::HashSet::new();
    names
        .iter()
        .map(|n| {
            let clip = |s: &str, max: usize| s.char_indices().take_while(|(i, c)| i + c.len_utf8() <= max).map(|(_, c)| c).collect::<String>();
            let mut s = clip(n, NAME_LEN);
            let mut k = 1;
            while !used.insert(s.clone()) {
                let suffix = k.to_string();
                s = format!("{}{suffix}", clip(n, NAME_LEN - suffix.len()));
                k += 1;
            }
            s
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_with_a_material_light_and_camera_round_trips() {
        let mut mesh = crate::box_primitive(Vec3::splat(5.0));
        for f in mesh.faces_mut() {
            f.smoothing_groups = 1;
        }
        let world = Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let parts = split_for_3ds("Box001", &mesh, world, world, &["Red".into()]);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].faces.len(), 12);
        let hidden = parts[0].flags.iter().filter(|f| **f != 7).count();
        assert_eq!(hidden, 12, "each quad's diagonal is hidden in both of its triangles");
        let file = TdsFile {
            master_scale: 1.0,
            materials: vec![TdsMaterial { name: "Red".into(), diffuse: [1.0, 0.0, 0.0], shininess: 0.25, texture: Some(("red.png".into(), 1.0)), ..Default::default() }],
            meshes: parts,
            lights: vec![TdsLight { name: "Spot01".into(), position: Vec3::Z * 50.0, color: [1.0, 1.0, 1.0], multiplier: 1.5, off: false, spot: Some((Vec3::ZERO, 43.0, 45.0)) }],
            cameras: vec![TdsCamera { name: "Camera01".into(), position: Vec3::new(0.0, -100.0, 20.0), target: Vec3::ZERO, roll: 0.0, lens: 43.456 }],
        };
        let back = read(&write(&file)).unwrap();
        assert_eq!(back.materials, file.materials.iter().map(|m| TdsMaterial { diffuse: [1.0, 0.0, 0.0], ..m.clone() }).collect::<Vec<_>>());
        assert_eq!(back.meshes[0].faces, file.meshes[0].faces);
        assert_eq!(back.meshes[0].flags, file.meshes[0].flags);
        assert_eq!(back.meshes[0].smoothing, vec![1; 12]);
        assert_eq!(back.meshes[0].material_groups, vec![("Red".to_owned(), (0..12).collect())]);
        assert_eq!(back.meshes[0].matrix, world);
        assert_eq!(back.lights, file.lights);
        assert_eq!(back.cameras, file.cameras);
    }

    #[test]
    fn names_are_cut_to_ten_bytes_and_stay_unique() {
        let n = short_names(&["ABCDEFGHIJKLMN".into(), "ABCDEFGHIJKLMNOP".into(), "Box".into()]);
        assert_eq!(n, ["ABCDEFGHIJ", "ABCDEFGHI1", "Box"]);
    }
}
