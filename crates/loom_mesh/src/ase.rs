//! ASCII Scene Export (.ase): each geometry object as a GEOMOBJECT with its
//! node transform, world-space vertices, faces cut into triangles (edge
//! visibility marking the cuts), smoothing groups, material IDs and map
//! channel 1 as texture vertices.

use crate::{FaceId, Mesh};
use glam::Mat4;
use std::fmt::Write;

/// The .ase text for `objects` (name, mesh, world matrix of the mesh).
pub fn write(objects: &[(String, Mesh, Mat4)]) -> String {
    let mut out = String::from("*3DSMAX_ASCIIEXPORT\t200\n*COMMENT \"3dLoom ASE\"\n");
    for (name, mesh, world) in objects {
        write_object(&mut out, name, mesh, *world);
    }
    out
}

fn write_object(out: &mut String, name: &str, mesh: &Mesh, world: Mat4) {
    let name = name.replace('"', "'");
    let f = |x: f32| format!("{x:.4}");
    // Triangles as (face, corner indices into the face, edge visibility).
    let mut tris: Vec<(usize, [usize; 3], [bool; 3])> = Vec::new();
    for fi in 0..mesh.face_count() {
        let n = mesh.face_vertex_ids(FaceId(fi as u32)).map_or(0, |v| v.len());
        for k in 1..n.saturating_sub(1) {
            tris.push((fi, [0, k, k + 1], [k == 1, true, k + 2 == n]));
        }
    }
    let _ = writeln!(out, "*GEOMOBJECT {{\n\t*NODE_NAME \"{name}\"\n\t*NODE_TM {{\n\t\t*NODE_NAME \"{name}\"");
    for (i, c) in [world.x_axis, world.y_axis, world.z_axis, world.w_axis].iter().enumerate() {
        let _ = writeln!(out, "\t\t*TM_ROW{i} {} {} {}", f(c.x), f(c.y), f(c.z));
    }
    let _ = writeln!(out, "\t}}\n\t*MESH {{\n\t\t*TIMEVALUE 0\n\t\t*MESH_NUMVERTEX {}\n\t\t*MESH_NUMFACES {}\n\t\t*MESH_VERTEX_LIST {{", mesh.vertex_count(), tris.len());
    for (i, v) in mesh.vertices().iter().enumerate() {
        let p = world.transform_point3(v.position);
        let _ = writeln!(out, "\t\t\t*MESH_VERTEX {i}\t{}\t{}\t{}", f(p.x), f(p.y), f(p.z));
    }
    out.push_str("\t\t}\n\t\t*MESH_FACE_LIST {\n");
    for (i, (fi, c, vis)) in tris.iter().enumerate() {
        let ids = mesh.face_vertex_ids(FaceId(*fi as u32)).unwrap_or_default();
        let face = &mesh.faces()[*fi];
        let groups: Vec<String> = (0..32).filter(|b| face.smoothing_groups & (1 << b) != 0).map(|b| (b + 1).to_string()).collect();
        let _ = writeln!(
            out,
            "\t\t\t*MESH_FACE {i}: A: {} B: {} C: {} AB: {} BC: {} CA: {}\t*MESH_SMOOTHING {}\t*MESH_MTLID {}",
            ids[c[0]].0, ids[c[1]].0, ids[c[2]].0, vis[0] as u8, vis[1] as u8, vis[2] as u8, groups.join(","), face.material_id.max(1) - 1
        );
    }
    out.push_str("\t\t}\n");
    if let Some(map) = mesh.map(1) {
        let _ = writeln!(out, "\t\t*MESH_NUMTVERTEX {}\n\t\t*MESH_TVERTLIST {{", map.verts.len());
        for (i, t) in map.verts.iter().enumerate() {
            let _ = writeln!(out, "\t\t\t*MESH_TVERT {i}\t{}\t{}\t{}", f(t.x), f(t.y), f(t.z));
        }
        let _ = writeln!(out, "\t\t}}\n\t\t*MESH_NUMTVFACES {}\n\t\t*MESH_TFACELIST {{", tris.len());
        for (i, (fi, c, _)) in tris.iter().enumerate() {
            let corners = mesh.face_corners(FaceId(*fi as u32)).unwrap_or_default();
            let t = |k: usize| map.corners.get(corners[k].0 as usize).copied().unwrap_or(0);
            let _ = writeln!(out, "\t\t\t*MESH_TFACE {i}\t{}\t{}\t{}", t(c[0]), t(c[1]), t(c[2]));
        }
        out.push_str("\t\t}\n");
    }
    out.push_str("\t}\n}\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A quad becomes two triangles sharing the hidden diagonal; vertices
    /// are written in world space and the node transform carries the matrix.
    #[test]
    fn a_quad_writes_two_triangles_in_world_space() {
        let mesh = crate::standard_primitives::plane_segmented(10.0, 10.0, 1, 1);
        let world = Mat4::from_translation(glam::Vec3::new(100.0, 0.0, 0.0));
        let text = write(&[("Plane001".into(), mesh.clone(), world)]);
        assert!(text.starts_with("*3DSMAX_ASCIIEXPORT\t200"));
        assert!(text.contains("*MESH_NUMVERTEX 4") && text.contains("*MESH_NUMFACES 2"), "{text}");
        assert!(text.contains("*TM_ROW3 100.0000 0.0000 0.0000"), "{text}");
        let xs: Vec<f32> = text.lines().filter(|l| l.trim_start().starts_with("*MESH_VERTEX ")).map(|l| l.trim_start().split('\t').nth(1).unwrap().parse().unwrap()).collect();
        assert!(xs.iter().all(|x| (95.0..=105.0).contains(x)), "world space: {xs:?}");
        let faces: Vec<&str> = text.lines().filter(|l| l.trim_start().starts_with("*MESH_FACE ")).collect();
        assert!(faces[0].contains("AB: 1 BC: 1 CA: 0") && faces[1].contains("AB: 0 BC: 1 CA: 1"), "{faces:?}");
        assert!(faces.iter().all(|l| l.contains("*MESH_MTLID 0")));
        if mesh.map(1).is_some() {
            assert!(text.contains("*MESH_NUMTVFACES 2"), "{text}");
        }
    }
}
