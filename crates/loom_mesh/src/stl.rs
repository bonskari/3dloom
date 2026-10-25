//! STL (stereolithography) triangle soups, binary and ASCII: reading with
//! the importer's vertex weld and double-face removal, writing triangles of
//! any [`Mesh`] (n-gons split as Editable Mesh splits them).

use crate::indexed::{IndexedFace, IndexedMesh};
use crate::Mesh;
use glam::{Mat4, Vec3};
use std::collections::{HashMap, HashSet};

/// A file's solid name (ASCII `solid <name>`, else none) and its triangles.
#[derive(Debug, Clone, Default)]
pub struct StlFile {
    pub name: Option<String>,
    pub triangles: Vec<[Vec3; 3]>,
}

fn is_binary(bytes: &[u8]) -> bool {
    if bytes.len() < 84 {
        return false;
    }
    let n = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
    84 + n * 50 == bytes.len()
}

/// Reads a binary or ASCII STL (binary when the size matches its triangle count).
pub fn read(bytes: &[u8]) -> Result<StlFile, String> {
    if is_binary(bytes) {
        let n = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
        let f = |o: usize| f32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
        let v = |o: usize| Vec3::new(f(o), f(o + 4), f(o + 8));
        let triangles = (0..n).map(|i| {
            let o = 84 + i * 50 + 12;
            [v(o), v(o + 12), v(o + 24)]
        }).collect();
        return Ok(StlFile { name: None, triangles });
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "not an STL file (neither binary nor ASCII)".to_owned())?;
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next().ok_or("empty STL file")?;
    let Some(rest) = first.strip_prefix("solid") else {
        return Err("not an STL file: no `solid` line".into());
    };
    let name = Some(rest.trim().to_owned()).filter(|s| !s.is_empty());
    let mut triangles = Vec::new();
    let mut corners: Vec<Vec3> = Vec::new();
    for l in lines {
        if let Some(r) = l.strip_prefix("vertex") {
            let n: Vec<f32> = r.split_whitespace().map(|x| x.parse::<f32>().map_err(|e| format!("bad vertex {r:?}: {e}"))).collect::<Result<_, _>>()?;
            if n.len() != 3 {
                return Err(format!("bad vertex {r:?}"));
            }
            corners.push(Vec3::new(n[0], n[1], n[2]));
        } else if l.starts_with("endloop") {
            // A facet with more corners is a fan (some writers emit polygons).
            for i in 1..corners.len().saturating_sub(1) {
                triangles.push([corners[0], corners[i], corners[i + 1]]);
            }
            corners.clear();
        }
    }
    Ok(StlFile { name, triangles })
}

/// The import's mesh: with `weld`, corners closer than the threshold share a
/// vertex (else every face keeps its own three); `remove_double` drops faces
/// over the same three vertices as an earlier one.
pub fn to_indexed(triangles: &[[Vec3; 3]], weld: Option<f32>, remove_double: bool) -> IndexedMesh {
    let mut im = IndexedMesh::default();
    let mut by_key: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut seen: HashSet<[u32; 3]> = HashSet::new();
    for t in triangles {
        let ids: Vec<u32> = t
            .iter()
            .map(|p| match weld {
                Some(w) => {
                    let w = w.max(1e-7);
                    let key = ((p.x / w).round() as i64, (p.y / w).round() as i64, (p.z / w).round() as i64);
                    *by_key.entry(key).or_insert_with(|| im.add_vertex(*p, None))
                }
                None => im.add_vertex(*p, None),
            })
            .collect();
        if ids[0] == ids[1] || ids[1] == ids[2] || ids[0] == ids[2] {
            continue;
        }
        if remove_double {
            let mut k = [ids[0], ids[1], ids[2]];
            k.sort_unstable();
            if !seen.insert(k) {
                continue;
            }
        }
        im.faces.push(IndexedFace { ids, ..Default::default() });
    }
    im
}

/// The triangles of `mesh` in world space (`world` applied); n-gons are split
/// the way Editable Mesh splits them.
pub fn triangles(mesh: &Mesh, world: Mat4) -> Vec<[Vec3; 3]> {
    let tri = crate::tri_mesh::from_poly(mesh).unwrap_or_else(|_| mesh.clone());
    let mut out = Vec::with_capacity(tri.face_count());
    for i in 0..tri.face_count() {
        let Some(p) = tri.face_positions(crate::FaceId(i as u32)) else { continue };
        for k in 1..p.len().saturating_sub(1) {
            out.push([world.transform_point3(p[0]), world.transform_point3(p[k]), world.transform_point3(p[k + 1])]);
        }
    }
    out
}

fn normal(t: &[Vec3; 3]) -> Vec3 {
    (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero()
}

/// Binary STL: an 80-byte header naming the solid, then 50 bytes a triangle.
pub fn write_binary(name: &str, triangles: &[[Vec3; 3]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(84 + triangles.len() * 50);
    let mut header = [b' '; 80];
    for (h, b) in header.iter_mut().zip(name.bytes()) {
        *h = b;
    }
    out.extend_from_slice(&header);
    out.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
    for t in triangles {
        for v in std::iter::once(normal(t)).chain(t.iter().copied()) {
            for c in v.to_array() {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        out.extend_from_slice(&0u16.to_le_bytes());
    }
    out
}

/// ASCII STL.
pub fn write_ascii(name: &str, triangles: &[[Vec3; 3]]) -> String {
    let mut s = format!("solid {name}\n");
    for t in triangles {
        let n = normal(t);
        s.push_str(&format!("  facet normal {:e} {:e} {:e}\n    outer loop\n", n.x, n.y, n.z));
        for v in t {
            s.push_str(&format!("      vertex {:e} {:e} {:e}\n", v.x, v.y, v.z));
        }
        s.push_str("    endloop\n  endfacet\n");
    }
    s.push_str(&format!("endsolid {name}\n"));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_and_ascii_round_trip_a_box() {
        let mesh = crate::box_primitive(Vec3::splat(5.0));
        let tris = triangles(&mesh, Mat4::from_translation(Vec3::X * 10.0));
        assert_eq!(tris.len(), 12);
        for bytes in [write_binary("box", &tris), write_ascii("box", &tris).into_bytes()] {
            let f = read(&bytes).unwrap();
            assert_eq!(f.triangles.len(), 12);
            assert!(f.triangles.iter().flatten().all(|p| p.x >= 5.0 - 1e-4 && p.x <= 15.0 + 1e-4));
            let im = to_indexed(&f.triangles, Some(0.01), true);
            assert_eq!(im.positions.len(), 8, "welded corners");
            let m = crate::obj::build_repaired(im).unwrap();
            assert_eq!((m.vertex_count(), m.face_count()), (8, 12));
        }
        assert_eq!(read(&write_ascii("box", &tris).into_bytes()).unwrap().name.as_deref(), Some("box"));
    }

    #[test]
    fn without_weld_every_face_keeps_its_vertices_and_doubles_drop() {
        let t = [Vec3::ZERO, Vec3::X, Vec3::Y];
        let im = to_indexed(&[t, t], None, true);
        assert_eq!((im.positions.len(), im.faces.len()), (6, 2), "no weld: no shared vertices, so nothing is a double");
        let im = to_indexed(&[t, t], Some(0.001), true);
        assert_eq!(im.faces.len(), 1, "welded doubles drop");
        let im = to_indexed(&[t, t], Some(0.001), false);
        assert_eq!(im.faces.len(), 2);
    }
}
