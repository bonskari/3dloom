//! Independent f64 current-attribute reference for the rendered tangent basis.
//! Structural groups are separately validated; all derivatives, dynamic
//! subgroup tests and corner weights are recomputed here, never copied from GPU.
use crate::{mikktspace::StructuralRecipe, MeshData};
use glam::DVec3;
fn unit(v: DVec3) -> DVec3 {
    if v.to_array()
        .iter()
        .any(|x| x.abs() > f32::MIN_POSITIVE as f64)
    {
        v / v.length()
    } else {
        v
    }
}
fn project(v: DVec3, n: DVec3) -> DVec3 {
    unit(v - n.dot(v) * n)
}
fn angle(a: DVec3, b: DVec3) -> f64 {
    if a == DVec3::ZERO || b == DVec3::ZERO {
        std::f64::consts::FRAC_PI_2
    } else {
        a.cross(b).length().atan2(a.dot(b))
    }
}
fn decode(index: i32) -> usize {
    (index as usize >> 2) * 3 + (index as usize & 3)
}
fn vertex(data: &MeshData, index: i32) -> usize {
    data.indices[decode(index)] as usize
}
pub(crate) fn tangents_f64(data: &MeshData, recipe: &StructuralRecipe) -> Vec<[f64; 4]> {
    let corners = data.indices.len();
    let active = recipe.active_count;
    let position = |f: usize, c: usize| {
        DVec3::from_array(data.positions[vertex(data, recipe.indices[f * 3 + c])].map(f64::from))
    };
    let uv = |f: usize, c: usize| {
        glam::DVec2::from_array(data.uvs[vertex(data, recipe.indices[f * 3 + c])].map(f64::from))
    };
    let mut derivatives = vec![(DVec3::ZERO, DVec3::ZERO); active];
    for (f, derivative) in derivatives.iter_mut().enumerate() {
        let d1 = position(f, 1) - position(f, 0);
        let d2 = position(f, 2) - position(f, 0);
        let x = uv(f, 1) - uv(f, 0);
        let y = uv(f, 2) - uv(f, 0);
        let area = x.x * y.y - x.y * y.x;
        if area.abs() > f32::MIN_POSITIVE as f64 {
            let sign = if area > 0.0 { 1.0 } else { -1.0 };
            *derivative = (
                unit(y.y * d1 - x.y * d2) * sign,
                unit(-y.x * d1 + x.x * d2) * sign,
            );
        }
    }
    let mut spaces = vec![(DVec3::X, false); corners];
    for (gid, group) in recipe.groups.iter().enumerate() {
        let n = DVec3::from_array(data.normals[vertex(data, group.representative)].map(f64::from));
        let mut members = group.members.clone();
        members.sort_unstable();
        let projections: Vec<_> = members
            .iter()
            .map(|m| {
                let (s, t) = derivatives[*m as usize];
                (project(s, n), project(t, n))
            })
            .collect();
        for (index, &face) in members.iter().enumerate() {
            let f = face as usize;
            let own_flags = recipe.triangles[f].flags;
            let (os, ot) = projections[index];
            let mut sum = DVec3::ZERO;
            for (j, &member) in members.iter().enumerate() {
                let t = member as usize;
                let flags = recipe.triangles[t].flags;
                let (s2, t2) = projections[j];
                let include = ((own_flags | flags) & 4) != 0
                    || recipe.triangles[f].original_face == recipe.triangles[t].original_face
                    || (angle(os, s2) < std::f64::consts::PI
                        && angle(ot, t2) < std::f64::consts::PI);
                if !include || flags & 4 != 0 {
                    continue;
                }
                let c = recipe.assigned_groups[t]
                    .iter()
                    .position(|id| *id == gid as i32)
                    .expect("structural member corner");
                let center = position(t, c);
                let a = project(position(t, (c + 2) % 3) - center, n);
                let b = project(position(t, (c + 1) % 3) - center, n);
                sum += angle(a, b) * s2;
            }
            let corner = recipe.assigned_groups[f]
                .iter()
                .position(|id| *id == gid as i32)
                .unwrap();
            let original = recipe.triangles[f].original_face as usize;
            spaces[original * 3 + corner] = (unit(sum), group.orientation);
        }
    }
    // The renderer's MeshData is triangle-only; original quad recovery does not
    // enter this stage. Preserve the actual first-active-welded-corner epilogue.
    for f in active..recipe.triangles.len() {
        for c in 0..3 {
            let representative = recipe.indices[f * 3 + c];
            if let Some(source) = recipe.indices[..active * 3]
                .iter()
                .position(|id| *id == representative)
            {
                let original_source =
                    recipe.triangles[source / 3].original_face as usize * 3 + source % 3;
                let original = recipe.triangles[f].original_face as usize * 3 + c;
                spaces[original] = spaces[original_source];
            }
        }
    }
    let mut output = vec![[0.0, 0.0, 0.0, -0.0]; data.positions.len()];
    for (corner, &index) in data.indices.iter().enumerate() {
        let (s, orientation) = spaces[corner];
        output[index as usize] = [s.x, s.y, s.z, if orientation { -1.0 } else { 1.0 }];
    }
    output
}
