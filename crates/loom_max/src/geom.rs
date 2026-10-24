//! Editable Poly and Editable Mesh geometry chunks (inside the object's
//! 0x08FE container) to a Loom mesh.
//!
//! Editable Poly: 0x0100 vertices (u32 count; u32 flags + 3 f32 each),
//! 0x011A polygons (u32 count; per face u32 degree, the vertex indices, u16
//! flags, then by flag: 0x01 u32 smoothing groups, 0x08 u16 material id,
//! 0x10 u32 face flags, 0x20 (degree − 3) diagonals of two u32), and per
//! map channel 0x0124 channel, 0x0128 coordinates (u32 count, 3 f32 each),
//! 0x012B map faces (per face u32 degree and the coordinate indices).
//!
//! Editable Mesh: 0x0914 vertices (u32 count, 3 f32 each), 0x0912 faces
//! (u32 count; 3 u32 vertices, u32 smoothing groups, u32 flags: edge
//! visibility in bits 0–2, material id in the top 16 bits), and per map
//! channel 0x2398 channel, 0x2394 coordinates, 0x2396 faces (3 u32 each).
//! Triangles joined by invisible edges become one polygon, as the format intends them.

use crate::chunk::Chunk;
use glam::Vec3;
use loom_mesh::indexed::{IndexedFace, IndexedMesh};
use loom_mesh::Mesh;
use std::collections::{BTreeMap, HashMap};

struct Reader<'a> {
    b: &'a [u8],
    o: usize,
}

impl Reader<'_> {
    fn u32(&mut self) -> Result<u32, String> {
        let v = self.b.get(self.o..self.o + 4).ok_or("geometry chunk truncated")?;
        self.o += 4;
        Ok(u32::from_le_bytes(v.try_into().unwrap()))
    }
    fn u16(&mut self) -> Result<u16, String> {
        let v = self.b.get(self.o..self.o + 2).ok_or("geometry chunk truncated")?;
        self.o += 2;
        Ok(u16::from_le_bytes(v.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn vec3(&mut self) -> Result<Vec3, String> {
        Ok(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }
    fn count(&mut self, item: usize) -> Result<usize, String> {
        let n = self.u32()? as usize;
        if n.saturating_mul(item) > self.b.len().saturating_sub(self.o) {
            return Err("geometry count larger than its chunk".into());
        }
        Ok(n)
    }
    fn done(&self) -> bool {
        self.o == self.b.len()
    }
}

/// A map channel: coordinates and, per face, the coordinate index of each corner.
struct Channel {
    verts: Vec<Vec3>,
    faces: Vec<Vec<u32>>,
}

fn coords(c: &Chunk) -> Result<Vec<Vec3>, String> {
    let mut r = Reader { b: &c.data, o: 0 };
    let n = r.count(12)?;
    (0..n).map(|_| r.vec3()).collect()
}

/// Corner colours of channel 0 / -1 / -2 written as per-vertex colour,
/// illumination and alpha (Loom keeps them per vertex; the first corner wins).
fn vertex_attrs(im: &mut IndexedMesh, faces: &[Vec<u32>], chans: &BTreeMap<i64, Channel>) {
    for (ch, slot) in [(0i64, 0usize), (-1, 1), (-2, 2)] {
        let Some(c) = chans.get(&ch) else { continue };
        let mut done = vec![false; im.positions.len()];
        for (f, ids) in faces.iter().enumerate() {
            let Some(tv) = c.faces.get(f) else { continue };
            for (k, v) in ids.iter().enumerate() {
                let (Some(t), Some(d)) = (tv.get(k), done.get_mut(*v as usize)) else { continue };
                if *d {
                    continue;
                }
                *d = true;
                let Some(col) = c.verts.get(*t as usize) else { continue };
                let a = &mut im.vattrs[*v as usize];
                match slot {
                    0 => a.0 = col.to_array(),
                    1 => a.1 = col.to_array(),
                    _ => a.2 = col.x,
                }
            }
        }
    }
}

fn finish(positions: Vec<Vec3>, faces: Vec<(Vec<u32>, u32, u16)>, chans: BTreeMap<i64, Channel>) -> Result<Mesh, String> {
    let n = positions.len();
    let ids: Vec<Vec<u32>> = faces.iter().map(|f| f.0.clone()).collect();
    if ids.iter().flatten().any(|v| *v as usize >= n) {
        return Err("a face names a vertex the mesh does not have".into());
    }
    let mut im = IndexedMesh { positions, vattrs: vec![([1.0; 3], [1.0; 3], 1.0, false); n], faces: Vec::new(), keep: Default::default() };
    for (f, (vids, smoothing, mat)) in faces.into_iter().enumerate() {
        let mut uv = BTreeMap::new();
        for (ch, c) in chans.iter().filter(|(ch, _)| **ch >= 1) {
            if let Some(tv) = c.faces.get(f).filter(|t| t.len() == vids.len()) {
                uv.insert(*ch as u32, tv.iter().map(|t| c.verts.get(*t as usize).copied().unwrap_or(Vec3::ZERO)).collect());
            }
        }
        im.faces.push(IndexedFace { ids: vids, smoothing_groups: smoothing, material_id: mat.saturating_add(1), hidden: false, uv });
    }
    vertex_attrs(&mut im, &ids, &chans);
    loom_mesh::obj::build_repaired(im)
}

pub fn editable_poly(g: &Chunk) -> Result<Mesh, String> {
    let vc = g.child(0x0100).ok_or("Editable Poly without vertices")?;
    let mut r = Reader { b: &vc.data, o: 0 };
    let nv = r.count(16)?;
    let mut positions = Vec::with_capacity(nv);
    for _ in 0..nv {
        r.u32()?;
        positions.push(r.vec3()?);
    }
    let fc = g.child(0x011a).ok_or("Editable Poly without polygons")?;
    let mut r = Reader { b: &fc.data, o: 0 };
    let nf = r.count(8)?;
    let mut faces = Vec::with_capacity(nf);
    for _ in 0..nf {
        let d = r.count(4)?;
        if d < 3 {
            return Err("a polygon with fewer than three corners".into());
        }
        let vids: Vec<u32> = (0..d).map(|_| r.u32()).collect::<Result<_, _>>()?;
        let flags = r.u16()?;
        if flags & !0x39 != 0 {
            return Err(format!("unknown polygon flags {flags:#06x}"));
        }
        let smoothing = if flags & 0x01 != 0 { r.u32()? } else { 0 };
        let mat = if flags & 0x08 != 0 { r.u16()? } else { 0 };
        if flags & 0x10 != 0 {
            r.u32()?;
        }
        if flags & 0x20 != 0 {
            for _ in 0..(d - 3) * 2 {
                r.u32()?;
            }
        }
        faces.push((vids, smoothing, mat));
    }
    if !r.done() {
        return Err("polygon chunk has trailing data".into());
    }
    // Map channels: 0x0124 channel, then its 0x0128 coordinates and 0x012B faces.
    let mut chans = BTreeMap::new();
    let mut current: Option<i64> = None;
    let mut verts: Option<Vec<Vec3>> = None;
    for c in g.kids() {
        match c.id {
            0x0124 => current = c.i32s().first().map(|x| *x as i64),
            0x0128 => verts = Some(coords(c)?),
            0x012b => {
                let mut r = Reader { b: &c.data, o: 0 };
                let mut mf = Vec::with_capacity(nf);
                for _ in 0..nf {
                    let d = r.count(4)?;
                    mf.push((0..d).map(|_| r.u32()).collect::<Result<Vec<_>, _>>()?);
                }
                if let (Some(ch), Some(v)) = (current, verts.take()) {
                    chans.insert(ch, Channel { verts: v, faces: mf });
                }
            }
            _ => {}
        }
    }
    finish(positions, faces, chans)
}

pub fn editable_mesh(g: &Chunk) -> Result<Mesh, String> {
    let positions = coords(g.child(0x0914).ok_or("Editable Mesh without vertices")?)?;
    let fc = g.child(0x0912).ok_or("Editable Mesh without faces")?;
    let mut r = Reader { b: &fc.data, o: 0 };
    let nf = r.count(20)?;
    let mut tris = Vec::with_capacity(nf);
    for _ in 0..nf {
        let v = [r.u32()?, r.u32()?, r.u32()?];
        let smoothing = r.u32()?;
        let flags = r.u32()?;
        tris.push((v, smoothing, (flags >> 16) as u16, flags & 7));
    }
    let mut chans: BTreeMap<i64, (Vec<Vec3>, Vec<[u32; 3]>)> = BTreeMap::new();
    let mut current: Option<i64> = None;
    let mut verts: Option<Vec<Vec3>> = None;
    for c in g.kids() {
        match c.id {
            0x2398 => current = c.i32s().first().map(|x| *x as i64),
            0x2394 => verts = Some(coords(c)?),
            0x2396 => {
                let mut r = Reader { b: &c.data, o: 0 };
                let n = r.count(12)?;
                let f: Vec<[u32; 3]> = (0..n).map(|_| Ok::<_, String>([r.u32()?, r.u32()?, r.u32()?])).collect::<Result<_, _>>()?;
                if let (Some(ch), Some(v)) = (current, verts.take()) {
                    chans.insert(ch, (v, f));
                }
            }
            _ => {}
        }
    }
    let (polys, corners) = merge_triangles(&tris);
    let faces = polys.iter().map(|(ids, s, m)| (ids.clone(), *s, *m)).collect();
    let chans = chans
        .into_iter()
        .map(|(ch, (v, f))| {
            let faces = corners.iter().map(|cs| cs.iter().map(|(t, k)| f.get(*t).map(|x| x[*k]).unwrap_or(0)).collect()).collect();
            (ch, Channel { verts: v, faces })
        })
        .collect();
    finish(positions, faces, chans)
}

type Poly = (Vec<u32>, u32, u16);

/// Triangles joined across invisible edges (same smoothing and material, opposite
/// directions) into polygons; a group whose outline is not one loop stays
/// triangles. Also returns, per polygon corner, its (triangle, corner) source.
fn merge_triangles(tris: &[([u32; 3], u32, u16, u32)]) -> (Vec<Poly>, Vec<Vec<(usize, usize)>>) {
    let mut parent: Vec<usize> = (0..tris.len()).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    let mut by_edge: HashMap<(u32, u32), (usize, usize)> = HashMap::new();
    for (t, (v, ..)) in tris.iter().enumerate() {
        for k in 0..3 {
            by_edge.insert((v[k], v[(k + 1) % 3]), (t, k));
        }
    }
    for (t, (v, s, m, vis)) in tris.iter().enumerate() {
        for k in 0..3 {
            if vis & (1 << k) != 0 {
                continue;
            }
            if let Some(&(u, _)) = by_edge.get(&(v[(k + 1) % 3], v[k])) {
                let (_, s2, m2, _) = tris[u];
                if u != t && *s == s2 && *m == m2 {
                    let (a, b) = (find(&mut parent, t), find(&mut parent, u));
                    parent[a] = b;
                }
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for t in 0..tris.len() {
        let r = find(&mut parent, t);
        groups.entry(r).or_default().push(t);
    }
    let mut polys = Vec::new();
    let mut corners = Vec::new();
    let mut groups: Vec<Vec<usize>> = groups.into_values().collect();
    groups.sort_by_key(|g| g[0]);
    for g in groups {
        let (_, s, m, _) = tris[g[0]];
        let single = |t: usize| ((tris[t].0.to_vec(), tris[t].1, tris[t].2), vec![(t, 0), (t, 1), (t, 2)]);
        if g.len() == 1 {
            let (p, c) = single(g[0]);
            polys.push(p);
            corners.push(c);
            continue;
        }
        // Outline: directed edges whose reverse is not inside the group.
        let inside: std::collections::HashSet<(u32, u32)> = g.iter().flat_map(|t| (0..3).map(move |k| (tris[*t].0[k], tris[*t].0[(k + 1) % 3]))).collect();
        let mut next: HashMap<u32, (u32, usize, usize)> = HashMap::new();
        let mut ok = true;
        for &t in &g {
            for k in 0..3 {
                let (a, b) = (tris[t].0[k], tris[t].0[(k + 1) % 3]);
                if !inside.contains(&(b, a)) && next.insert(a, (b, t, k)).is_some() {
                    ok = false;
                }
            }
        }
        let mut loop_ids = Vec::new();
        let mut loop_c = Vec::new();
        if ok && !next.is_empty() {
            let start = *next.keys().min().unwrap();
            let mut v = start;
            loop {
                let Some(&(b, t, k)) = next.get(&v) else { ok = false; break };
                loop_ids.push(v);
                loop_c.push((t, k));
                v = b;
                if v == start || loop_ids.len() > next.len() {
                    break;
                }
            }
            ok = ok && v == start && loop_ids.len() == next.len();
            // A vertex inside the group (not on its outline) would be lost.
            ok = ok && g.iter().all(|t| tris[*t].0.iter().all(|x| next.contains_key(x)));
        }
        if ok && loop_ids.len() >= 3 {
            polys.push((loop_ids, s, m));
            corners.push(loop_c);
        } else {
            for t in g {
                let (p, c) = single(t);
                polys.push(p);
                corners.push(c);
            }
        }
    }
    (polys, corners)
}
