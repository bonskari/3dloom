//! AutoCAD DXF (ASCII) as a simple mesh interchange: import reads 3DFACE
//! entities and polyface-mesh POLYLINEs, one object per layer; export
//! writes each object's triangles and quads as 3DFACEs on a layer named
//! after the object (an R12 file with only an ENTITIES section).

use crate::indexed::{IndexedFace, IndexedMesh};
use crate::Mesh;
use glam::{Mat4, Vec3};
use std::collections::BTreeMap;

/// The faces of one layer, in world space.
#[derive(Debug, Clone, Default)]
pub struct DxfLayer {
    pub name: String,
    pub faces: Vec<Vec<Vec3>>,
}

fn pairs(text: &str) -> Vec<(i32, &str)> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
    lines.chunks(2).filter(|c| c.len() == 2).filter_map(|c| c[0].parse::<i32>().ok().map(|k| (k, c[1]))).collect()
}

/// Reads the 3DFACEs and polyface meshes of a DXF file by layer (in file order).
pub fn read(text: &str) -> Result<Vec<DxfLayer>, String> {
    let g = pairs(text);
    if !g.iter().any(|(c, v)| *c == 0 && *v == "SECTION") {
        return Err("not a DXF file: no SECTION".into());
    }
    let mut layers: Vec<DxfLayer> = Vec::new();
    let mut add = |layer: &str, face: Vec<Vec3>| {
        let i = match layers.iter().position(|l| l.name == layer) {
            Some(i) => i,
            None => {
                layers.push(DxfLayer { name: layer.to_owned(), faces: Vec::new() });
                layers.len() - 1
            }
        };
        layers[i].faces.push(face);
    };
    // Entities: from each `0` group to the next.
    let starts: Vec<usize> = g.iter().enumerate().filter(|(_, (c, _))| *c == 0).map(|(i, _)| i).collect();
    let mut k = 0;
    let num = |v: &str| v.parse::<f32>().unwrap_or(0.0);
    while k < starts.len() {
        let s = starts[k];
        let e = starts.get(k + 1).copied().unwrap_or(g.len());
        let body = &g[s + 1..e];
        let layer = body.iter().find(|(c, _)| *c == 8).map(|(_, v)| *v).unwrap_or("0");
        match g[s].1 {
            "3DFACE" => {
                let mut p = [Vec3::ZERO; 4];
                for (c, v) in body {
                    let (i, axis) = match c {
                        10..=13 => (c - 10, 0),
                        20..=23 => (c - 20, 1),
                        30..=33 => (c - 30, 2),
                        _ => continue,
                    };
                    p[i as usize][axis] = num(v);
                }
                let mut face = vec![p[0], p[1], p[2]];
                if p[3] != p[2] {
                    face.push(p[3]);
                }
                add(layer, face);
                k += 1;
            }
            "POLYLINE" => {
                let flags = body.iter().find(|(c, _)| *c == 70).map(|(_, v)| v.parse::<i32>().unwrap_or(0)).unwrap_or(0);
                let mut verts: Vec<Vec3> = Vec::new();
                let mut faces: Vec<Vec<i32>> = Vec::new();
                k += 1;
                while k < starts.len() && g[starts[k]].1 == "VERTEX" {
                    let s = starts[k];
                    let e = starts.get(k + 1).copied().unwrap_or(g.len());
                    let b = &g[s + 1..e];
                    let get = |code: i32| b.iter().find(|(c, _)| *c == code).map(|(_, v)| *v);
                    let vflags = get(70).map(|v| v.parse::<i32>().unwrap_or(0)).unwrap_or(0);
                    if vflags & 128 != 0 && vflags & 64 == 0 {
                        // A face record: 71..74 are 1-based vertex indices (negative: hidden edge).
                        faces.push((71..=74).filter_map(|c| get(c).and_then(|v| v.parse::<i32>().ok())).filter(|i| *i != 0).collect());
                    } else {
                        verts.push(Vec3::new(get(10).map_or(0.0, num), get(20).map_or(0.0, num), get(30).map_or(0.0, num)));
                    }
                    k += 1;
                }
                if flags & 64 != 0 {
                    for f in faces {
                        let pts: Vec<Vec3> = f.iter().filter_map(|i| verts.get(i.unsigned_abs() as usize - 1).copied()).collect();
                        if pts.len() >= 3 {
                            add(layer, pts);
                        }
                    }
                }
            }
            _ => k += 1,
        }
    }
    Ok(layers)
}

/// A layer's faces as one mesh: corners at the same position share a vertex.
pub fn to_indexed(faces: &[Vec<Vec3>]) -> IndexedMesh {
    let mut im = IndexedMesh::default();
    let mut by: BTreeMap<(u32, u32, u32), u32> = BTreeMap::new();
    for f in faces {
        let ids = f.iter().map(|p| *by.entry((p.x.to_bits(), p.y.to_bits(), p.z.to_bits())).or_insert_with(|| im.add_vertex(*p, None))).collect();
        im.faces.push(IndexedFace { ids, ..Default::default() });
    }
    im
}

/// Writes objects (name, mesh, world matrix) as 3DFACEs: triangles, and
/// quads where a polygon has four corners; larger polygons are split.
pub fn write(objects: &[(String, Mesh, Mat4)]) -> String {
    let mut s = String::from("0\nSECTION\n2\nENTITIES\n");
    let layer_name = |n: &str| n.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect::<String>();
    for (name, mesh, world) in objects {
        let layer = layer_name(name);
        let tri = crate::tri_mesh::from_poly(mesh).unwrap_or_else(|_| mesh.clone());
        let mut emit = |pts: &[Vec3]| {
            s.push_str(&format!("0\n3DFACE\n8\n{layer}\n"));
            for i in 0..4 {
                let p = world.transform_point3(pts[i.min(pts.len() - 1)]);
                for (axis, v) in [p.x, p.y, p.z].into_iter().enumerate() {
                    s.push_str(&format!("{}\n{v}\n", 10 + axis * 10 + i));
                }
            }
        };
        // A mesh with polygons over four corners goes as the triangles
        // Editable Mesh makes of it.
        let big = (0..mesh.face_count()).any(|i| mesh.face_positions(crate::FaceId(i as u32)).is_some_and(|p| p.len() > 4));
        let src = if big { &tri } else { mesh };
        for i in 0..src.face_count() {
            if let Some(p) = src.face_positions(crate::FaceId(i as u32)) {
                emit(&p);
            }
        }
    }
    s.push_str("0\nENDSEC\n0\nEOF\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_round_trips_as_quads_on_its_layer() {
        let mesh = crate::box_primitive(Vec3::splat(5.0));
        let text = write(&[("Box 001".into(), mesh, Mat4::from_translation(Vec3::Z * 5.0))]);
        let layers = read(&text).unwrap();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].name, "Box_001");
        assert_eq!(layers[0].faces.len(), 6);
        assert!(layers[0].faces.iter().all(|f| f.len() == 4));
        let m = crate::obj::build_repaired(to_indexed(&layers[0].faces)).unwrap();
        assert_eq!((m.vertex_count(), m.face_count()), (8, 6));
        assert!((m.aabb().0.z - 0.0).abs() < 1e-5 && (m.aabb().1.z - 10.0).abs() < 1e-5);
    }

    #[test]
    fn polyface_meshes_are_read() {
        let text = "0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n8\nL1\n66\n1\n70\n64\n0\nVERTEX\n8\nL1\n10\n0\n20\n0\n30\n0\n70\n192\n0\nVERTEX\n8\nL1\n10\n1\n20\n0\n30\n0\n70\n192\n0\nVERTEX\n8\nL1\n10\n0\n20\n1\n30\n0\n70\n192\n0\nVERTEX\n8\nL1\n10\n0\n20\n0\n30\n0\n70\n128\n71\n1\n72\n2\n73\n-3\n0\nSEQEND\n0\nENDSEC\n0\nEOF\n";
        let layers = read(text).unwrap();
        assert_eq!(layers[0].faces, vec![vec![Vec3::ZERO, Vec3::X, Vec3::Y]]);
    }
}
