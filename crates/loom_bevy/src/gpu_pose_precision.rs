//! Test-only higher-precision oracle for the SAME represented current geometry.
//! No CPU reference math is changed and these normals are never renderer output.
use super::{PackedVertex, Topology};
use glam::{DVec3, Vec3};
use loom_mesh::Mesh;

pub(super) fn normals_f64(
    mesh: &Mesh,
    parts: &[(Option<u16>, bool)],
    positions: &[[f32; 4]],
) -> Vec<Vec3> {
    assert_eq!(mesh.vertex_count(), positions.len());
    let topology = Topology::new(mesh, parts).unwrap();
    let point = |v: u32| {
        DVec3::new(
            positions[v as usize][0] as f64,
            positions[v as usize][1] as f64,
            positions[v as usize][2] as f64,
        )
    };
    let mut face_normals = Vec::with_capacity(topology.faces);
    for face in 0..topology.faces {
        let offset = topology.arena[face * 4] as usize;
        let count = topology.arena[face * 4 + 1] as usize;
        let origin = point(topology.arena[offset]);
        let mut normal = DVec3::ZERO;
        for i in 0..count {
            let a = point(topology.arena[offset + i]) - origin;
            let b = point(topology.arena[offset + (i + 1) % count]) - origin;
            normal += a.cross(b);
        }
        face_normals.push(normal.normalize_or_zero());
    }
    topology
        .corners
        .iter()
        .map(|corner| {
            let face = corner.source[1] as usize;
            if corner.flags[1] != 0 {
                return Vec3::from_array(corner.normal[..3].try_into().unwrap());
            }
            let normal = if corner.flags[0] == 0 || corner.flags[2] != 0 {
                face_normals[face]
            } else {
                let mut sum = DVec3::ZERO;
                for i in 0..corner.source[3] {
                    let neighbor = topology.arena[(corner.source[2] + i) as usize] as usize;
                    if topology.arena[neighbor * 4 + 2] & corner.flags[0] != 0 {
                        sum += face_normals[neighbor];
                    }
                }
                if sum.length_squared() < 1e-12 {
                    face_normals[face]
                } else {
                    sum.normalize()
                }
            };
            normal.as_vec3()
        })
        .collect()
}

pub(super) fn packed_error(actual: &[PackedVertex], truth: &[Vec3]) -> f32 {
    assert_eq!(actual.len(), truth.len());
    actual
        .iter()
        .zip(truth)
        .map(|(a, b)| Vec3::from_array(a.normal[..3].try_into().unwrap()).distance(*b))
        .fold(0.0, f32::max)
}
