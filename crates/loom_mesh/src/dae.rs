//! Collada (.dae, COLLADA 1.4.1) export: each geometry object as a
//! `<geometry>` (object-space positions, map channel 1 as TEXCOORD, the
//! faces as a `<polylist>` keeping n-gons) placed by a `<node>` with its
//! world matrix in the visual scene. Z is up; `metres` per unit goes in
//! `<unit>`. No materials, animation, cameras or lights.

use crate::{FaceId, Mesh};
use glam::Mat4;
use std::fmt::Write;

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The .dae text for `objects` (name, mesh, world matrix of the mesh).
pub fn write(objects: &[(String, Mesh, Mat4)], metres: f64) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<COLLADA xmlns=\"http://www.collada.org/2005/11/COLLADASchema\" version=\"1.4.1\">\n  <asset>\n    <contributor><authoring_tool>3dLoom</authoring_tool></contributor>\n    <unit meter=\"{metres}\" name=\"unit\"/>\n    <up_axis>Z_UP</up_axis>\n  </asset>\n  <library_geometries>\n"
    );
    for (i, (name, mesh, _)) in objects.iter().enumerate() {
        write_geometry(&mut out, i, &xml(name), mesh);
    }
    out.push_str("  </library_geometries>\n  <library_visual_scenes>\n    <visual_scene id=\"Scene\" name=\"Scene\">\n");
    for (i, (name, _, world)) in objects.iter().enumerate() {
        // Collada matrices are row-major.
        let m: Vec<String> = world.transpose().to_cols_array().iter().map(|v| format!("{v}")).collect();
        let _ = writeln!(
            out,
            "      <node id=\"node{i}\" name=\"{name}\" type=\"NODE\">\n        <matrix sid=\"transform\">{}</matrix>\n        <instance_geometry url=\"#geom{i}\"/>\n      </node>",
            m.join(" "),
            name = xml(name)
        );
    }
    out.push_str("    </visual_scene>\n  </library_visual_scenes>\n  <scene>\n    <instance_visual_scene url=\"#Scene\"/>\n  </scene>\n</COLLADA>\n");
    out
}

fn write_geometry(out: &mut String, i: usize, name: &str, mesh: &Mesh) {
    let pos: Vec<String> = mesh.vertices().iter().flat_map(|v| [v.position.x, v.position.y, v.position.z]).map(|x| format!("{x}")).collect();
    let _ = write!(
        out,
        "    <geometry id=\"geom{i}\" name=\"{name}\">\n      <mesh>\n        <source id=\"geom{i}-pos\">\n          <float_array id=\"geom{i}-pos-array\" count=\"{}\">{}</float_array>\n          <technique_common><accessor source=\"#geom{i}-pos-array\" count=\"{}\" stride=\"3\"><param name=\"X\" type=\"float\"/><param name=\"Y\" type=\"float\"/><param name=\"Z\" type=\"float\"/></accessor></technique_common>\n        </source>\n",
        pos.len(),
        pos.join(" "),
        mesh.vertex_count()
    );
    let map = mesh.map(1);
    if let Some(map) = map {
        let uv: Vec<String> = map.verts.iter().flat_map(|t| [t.x, t.y]).map(|x| format!("{x}")).collect();
        let _ = write!(
            out,
            "        <source id=\"geom{i}-uv\">\n          <float_array id=\"geom{i}-uv-array\" count=\"{}\">{}</float_array>\n          <technique_common><accessor source=\"#geom{i}-uv-array\" count=\"{}\" stride=\"2\"><param name=\"S\" type=\"float\"/><param name=\"T\" type=\"float\"/></accessor></technique_common>\n        </source>\n",
            uv.len(),
            uv.join(" "),
            map.verts.len()
        );
    }
    let _ = write!(out, "        <vertices id=\"geom{i}-vtx\"><input semantic=\"POSITION\" source=\"#geom{i}-pos\"/></vertices>\n        <polylist count=\"{}\">\n          <input semantic=\"VERTEX\" source=\"#geom{i}-vtx\" offset=\"0\"/>\n", mesh.face_count());
    if map.is_some() {
        let _ = writeln!(out, "          <input semantic=\"TEXCOORD\" source=\"#geom{i}-uv\" offset=\"1\" set=\"0\"/>");
    }
    let mut vcount = Vec::new();
    let mut p = Vec::new();
    for fi in 0..mesh.face_count() {
        let ids = mesh.face_vertex_ids(FaceId(fi as u32)).unwrap_or_default();
        let corners = mesh.face_corners(FaceId(fi as u32)).unwrap_or_default();
        vcount.push(ids.len().to_string());
        for (k, v) in ids.iter().enumerate() {
            p.push(v.0.to_string());
            if let Some(map) = map {
                p.push(corners.get(k).and_then(|c| map.corners.get(c.0 as usize)).copied().unwrap_or(0).to_string());
            }
        }
    }
    let _ = write!(out, "          <vcount>{}</vcount>\n          <p>{}</p>\n        </polylist>\n      </mesh>\n    </geometry>\n", vcount.join(" "), p.join(" "));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A quad stays one 4-sided polygon; the node carries the world matrix
    /// row by row and the unit its metres.
    #[test]
    fn a_quad_is_one_polygon_placed_by_its_node() {
        let mesh = crate::standard_primitives::plane_segmented(10.0, 10.0, 1, 1);
        let world = Mat4::from_translation(glam::Vec3::new(100.0, 0.0, 0.0));
        let text = write(&[("Plane<1>".into(), mesh.clone(), world)], 0.0254);
        assert!(text.contains("<unit meter=\"0.0254\""), "{text}");
        assert!(text.contains("<up_axis>Z_UP</up_axis>"));
        assert!(text.contains("name=\"Plane&lt;1&gt;\""), "escaped name: {text}");
        assert!(text.contains("<polylist count=\"1\">") && text.contains("<vcount>4</vcount>"), "{text}");
        assert!(text.contains("<matrix sid=\"transform\">1 0 0 100 0 1 0 0 0 0 1 0 0 0 0 1</matrix>"), "{text}");
        assert!(text.contains("count=\"12\""), "4 vertices of 3 floats: {text}");
    }
}
