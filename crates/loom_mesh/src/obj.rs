//! Wavefront OBJ: text in and out of [`Mesh`] (positions, n-gon faces,
//! smoothing groups, material ids, `vt` as map channel 1). Normals are
//! parsed and ignored on import (they come from the smoothing groups);
//! export writes normals per smoothing group.

use crate::indexed::{IndexedFace, IndexedMesh};
use crate::Mesh;
use glam::{Mat4, Vec3};

/// One `o` block of a file.
#[derive(Debug, Clone)]
pub struct ObjObject {
    pub name: String,
    pub mesh: Mesh,
}

#[derive(Default)]
struct Builder {
    name: String,
    im: IndexedMesh,
    /// Global OBJ vertex index (1-based) → local index.
    map: std::collections::HashMap<usize, u32>,
}

impl Builder {
    fn finish(self, out: &mut Vec<ObjObject>) -> Result<(), String> {
        let im = self.im;
        if im.faces.is_empty() {
            return Ok(());
        }
        let mesh = build_repaired(im).map_err(|e| format!("object {:?}: {e}", self.name))?;
        if mesh.face_count() == 0 {
            return Ok(());
        }
        out.push(ObjObject { name: self.name, mesh });
        Ok(())
    }
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
/// yields one object named `default_name`.
pub fn parse(text: &str, default_name: &str) -> Result<Vec<ObjObject>, String> {
    let mut positions: Vec<Vec3> = Vec::new();
    let mut texcoords: Vec<Vec3> = Vec::new();
    let mut out = Vec::new();
    let mut cur = Builder { name: default_name.to_owned(), ..Default::default() };
    let mut smoothing: u32 = 0;
    let mut materials: Vec<String> = Vec::new();
    let mut material: u16 = 1;
    let mut has_o = false;
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut it = line.split_whitespace();
        let key = it.next().unwrap_or("");
        let err = |m: String| format!("line {}: {m}", lineno + 1);
        match key {
            "v" => {
                let mut c = [0.0f32; 3];
                for v in &mut c {
                    *v = it.next().ok_or_else(|| err("v needs three numbers".into()))?.parse().map_err(|e| err(format!("{e}")))?;
                }
                positions.push(Vec3::from_array(c));
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
            }
            "o" | "g" => {
                // `g` splits objects only in a file without `o` lines.
                if key == "o" {
                    has_o = true;
                } else if has_o {
                    continue;
                }
                let name = it.collect::<Vec<_>>().join(" ");
                let name = if name.is_empty() { default_name.to_owned() } else { name };
                let done = std::mem::replace(&mut cur, Builder { name, ..Default::default() });
                done.finish(&mut out)?;
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
            }
            "usemtl" => {
                let name = it.collect::<Vec<_>>().join(" ");
                material = match name.strip_prefix("mat_").and_then(|n| n.parse::<u16>().ok()) {
                    Some(n) if n >= 1 => n,
                    _ => match materials.iter().position(|m| *m == name) {
                        Some(i) => i as u16 + 1,
                        None => {
                            materials.push(name);
                            materials.len() as u16
                        }
                    },
                };
            }
            "f" => {
                let mut ids: Vec<u32> = Vec::new();
                let mut uvs: Vec<Vec3> = Vec::new();
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
                let uv = if has_uv { std::iter::once((1u32, uvs)).collect() } else { Default::default() };
                cur.im.faces.push(IndexedFace { ids, smoothing_groups: smoothing, material_id: material, hidden: false, uv });
            }
            // vn, vp, mtllib, l, p and anything else: ignored.
            _ => {}
        }
    }
    cur.finish(&mut out)?;
    Ok(out)
}

/// Writes objects as OBJ text: world-space positions, per-vertex normals,
/// n-gon faces, `s` runs from smoothing groups and `usemtl mat_<id>` runs
/// from material ids.
pub fn write(objects: &[(String, Mesh, Mat4)]) -> String {
    let mut out = String::from("# 3dLoom OBJ\n");
    let mut vbase = 0usize;
    let mut nbase = 0usize;
    let mut tbase = 0usize;
    for (name, mesh, world) in objects {
        out.push_str(&format!("o {}\n", name.replace(char::is_whitespace, "_")));
        for v in mesh.vertices() {
            let p = world.transform_point3(v.position);
            out.push_str(&format!("v {} {} {}\n", fmt(p.x), fmt(p.y), fmt(p.z)));
        }
        let map = mesh.map(1);
        if let Some(m) = map {
            for t in &m.verts {
                out.push_str(&format!("vt {} {} {}\n", fmt(t.x), fmt(t.y), fmt(t.z)));
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
                    normals.push(normal_m.transform_vector3(n).normalize_or_zero());
                    normals.len() - 1
                });
                row.push(idx);
            }
            corner_n.push(row);
        }
        for n in &normals {
            out.push_str(&format!("vn {} {} {}\n", fmt(n.x), fmt(n.y), fmt(n.z)));
        }
        let mut cur_s: Option<u32> = None;
        let mut cur_m: Option<u16> = None;
        for (fi, ids) in face_ids.iter().enumerate() {
            if ids.is_empty() {
                continue;
            }
            let f = &mesh.faces()[fi];
            if cur_m != Some(f.material_id) {
                out.push_str(&format!("usemtl mat_{}\n", f.material_id));
                cur_m = Some(f.material_id);
            }
            if cur_s != Some(f.smoothing_groups) {
                match f.smoothing_groups {
                    0 => out.push_str("s off\n"),
                    g => out.push_str(&format!("s {}\n", g.trailing_zeros() + 1)),
                }
                cur_s = Some(f.smoothing_groups);
            }
            out.push('f');
            let corners = mesh.face_corners(crate::FaceId(fi as u32)).unwrap_or_default();
            for (k, v) in ids.iter().enumerate() {
                let t = map.and_then(|m| corners.get(k).map(|he| m.corners[he.0 as usize] as usize + tbase + 1));
                match t {
                    Some(t) => out.push_str(&format!(" {}/{}/{}", vbase + *v as usize + 1, t, nbase + corner_n[fi][k] + 1)),
                    None => out.push_str(&format!(" {}//{}", vbase + *v as usize + 1, nbase + corner_n[fi][k] + 1)),
                }
            }
            out.push('\n');
        }
        vbase += mesh.vertex_count();
        nbase += normals.len();
        tbase += map.map_or(0, |m| m.verts.len());
    }
    out
}

fn fmt(x: f32) -> String {
    let s = format!("{x:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_owned() }
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
