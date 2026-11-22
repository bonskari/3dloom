//! ASCII Scene Export (.ase): each geometry object as a GEOMOBJECT with its
//! node transform, world-space vertices, faces cut into triangles (edge
//! visibility marking the cuts), smoothing groups, material IDs and map
//! channel 1 as texture vertices. `read` takes the same back, plus the
//! material list (Standard materials and Multi/Sub-Object children).

use crate::indexed::{IndexedFace, IndexedMesh};
use crate::tds::TdsMaterial;
use crate::{FaceId, HalfEdgeId, Mesh};
use glam::{Mat4, Vec3, Vec4};
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write;

/// The .ase text for `objects` (name, mesh, world matrix of the mesh,
/// index into `materials`).
pub fn write(objects: &[(String, Mesh, Mat4, Option<usize>)], materials: &[AseMaterial]) -> String {
    let mut out = String::from("*3DSMAX_ASCIIEXPORT\t200\n*COMMENT \"3dLoom ASE\"\n");
    if !materials.is_empty() {
        let _ = writeln!(out, "*MATERIAL_LIST {{\n\t*MATERIAL_COUNT {}", materials.len());
        for (i, m) in materials.iter().enumerate() {
            write_material(&mut out, "MATERIAL", i, m, 1);
        }
        out.push_str("}\n");
    }
    for (name, mesh, world, material) in objects {
        write_object(&mut out, name, mesh, *world, *material);
    }
    out
}

fn write_material(out: &mut String, key: &str, index: usize, m: &AseMaterial, depth: usize) {
    let t = "\t".repeat(depth);
    let b = &m.base;
    let class = if m.subs.is_empty() { "Standard" } else { "Multi/Sub-Object" };
    let c = |v: [f32; 3]| format!("{:.4}\t{:.4}\t{:.4}", v[0], v[1], v[2]);
    let _ = writeln!(out, "{t}*{key} {index} {{\n{t}\t*MATERIAL_NAME \"{}\"\n{t}\t*MATERIAL_CLASS \"{class}\"", b.name.replace('"', "'"));
    let _ = writeln!(out, "{t}\t*MATERIAL_AMBIENT {}\n{t}\t*MATERIAL_DIFFUSE {}\n{t}\t*MATERIAL_SPECULAR {}", c(b.ambient), c(b.diffuse), c(b.specular));
    let _ = writeln!(out, "{t}\t*MATERIAL_SHINE {:.4}\n{t}\t*MATERIAL_SHINESTRENGTH {:.4}\n{t}\t*MATERIAL_TRANSPARENCY {:.4}", b.shininess, b.shin_strength, b.transparency);
    if b.two_sided {
        let _ = writeln!(out, "{t}\t*MATERIAL_TWOSIDED");
    }
    if let Some((path, amount)) = &b.texture {
        let _ = writeln!(out, "{t}\t*MAP_DIFFUSE {{\n{t}\t\t*MAP_NAME \"Map #1\"\n{t}\t\t*MAP_CLASS \"Bitmap\"\n{t}\t\t*MAP_AMOUNT {amount:.4}\n{t}\t\t*BITMAP \"{path}\"\n{t}\t}}");
    }
    if !m.subs.is_empty() {
        let _ = writeln!(out, "{t}\t*NUMSUBMTLS {}", m.subs.len());
        for (j, sub) in m.subs.iter().enumerate() {
            write_material(out, "SUBMATERIAL", j, &AseMaterial { base: sub.clone(), subs: Vec::new() }, depth + 1);
        }
    }
    let _ = writeln!(out, "{t}}}");
}

fn write_object(out: &mut String, name: &str, mesh: &Mesh, world: Mat4, material: Option<usize>) {
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
    out.push_str("\t}\n");
    if let Some(m) = material {
        let _ = writeln!(out, "\t*MATERIAL_REF {m}");
    }
    out.push_str("}\n");
}

/// A material of the file's MATERIAL_LIST; `subs` are the SUBMATERIALs of a
/// Multi/Sub-Object material (indexed by MESH_MTLID).
#[derive(Debug, Clone, Default)]
pub struct AseMaterial {
    pub base: TdsMaterial,
    pub subs: Vec<TdsMaterial>,
}

/// A GEOMOBJECT: its node matrix and the mesh in the node's own frame.
#[derive(Debug, Clone)]
pub struct AseObject {
    pub name: String,
    pub matrix: Mat4,
    /// MATERIAL_REF: index into `AseFile::materials`.
    pub material: Option<usize>,
    pub mesh: Mesh,
}

#[derive(Debug, Clone, Default)]
pub struct AseFile {
    pub materials: Vec<AseMaterial>,
    pub objects: Vec<AseObject>,
}

#[derive(Default)]
struct RawObject {
    name: Option<String>,
    rows: [Vec3; 4],
    material: Option<usize>,
    verts: Vec<Vec3>,
    /// Corners, edge visibility (AB, BC, CA), smoothing groups, MESH_MTLID.
    faces: Vec<([u32; 3], [bool; 3], u32, u16)>,
    tverts: Vec<Vec3>,
    tfaces: Vec<[u32; 3]>,
}

fn unquote(rest: &str) -> String {
    let t = rest.trim();
    t.strip_prefix('"').and_then(|t| t.strip_suffix('"')).unwrap_or(t).to_owned()
}

fn floats(rest: &str) -> Vec<f32> {
    rest.split_whitespace().filter_map(|t| t.parse().ok()).collect()
}

fn vec3(rest: &str) -> Vec3 {
    let v = floats(rest);
    Vec3::new(v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0), v.get(2).copied().unwrap_or(0.0))
}

/// "*MESH_VERTEX 3  1.0 2.0 3.0": the index and the rest.
fn indexed(rest: &str) -> Option<(usize, &str)> {
    let rest = rest.trim_start();
    let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
    Some((rest[..end].trim_end_matches(':').parse().ok()?, &rest[end..]))
}

/// "*MESH_FACE 0: A: 0 B: 1 C: 2 AB: 1 BC: 1 CA: 0 *MESH_SMOOTHING 1,2 *MESH_MTLID 0".
fn parse_face(rest: &str) -> Option<([u32; 3], [bool; 3], u32, u16)> {
    let (_, rest) = indexed(rest)?;
    let toks: Vec<&str> = rest.split_whitespace().collect();
    let after = |key: &str| toks.iter().position(|t| *t == key).and_then(|i| toks.get(i + 1)).copied();
    let num = |key: &str| after(key).and_then(|t| t.parse::<u32>().ok());
    let corners = [num("A:")?, num("B:")?, num("C:")?];
    let vis = [num("AB:").is_none_or(|v| v != 0), num("BC:").is_none_or(|v| v != 0), num("CA:").is_none_or(|v| v != 0)];
    let smoothing = after("*MESH_SMOOTHING")
        .filter(|t| !t.starts_with('*'))
        .map(|t| t.split(',').filter_map(|g| g.trim().parse::<u32>().ok()).filter(|g| (1..=32).contains(g)).fold(0, |acc, g| acc | 1 << (g - 1)))
        .unwrap_or(0);
    let mtl = after("*MESH_MTLID").and_then(|t| t.parse::<u16>().ok()).unwrap_or(0);
    Some((corners, vis, smoothing, mtl))
}

/// Reads an .ase file: GEOMOBJECTs and the MATERIAL_LIST. Other objects
/// (shapes, lights, cameras, helpers) are skipped.
pub fn read(text: &str) -> Result<AseFile, String> {
    if !text.trim_start().starts_with("*3DSMAX_ASCIIEXPORT") {
        return Err("not an ASCII Scene Export file".into());
    }
    let mut file = AseFile::default();
    // Open blocks by keyword, the materials being read (a SUBMATERIAL on top of its parent).
    let mut blocks: Vec<String> = Vec::new();
    let mut materials: Vec<AseMaterial> = Vec::new();
    let mut object: Option<RawObject> = None;
    for line in text.lines() {
        let line = line.trim();
        if line == "}" {
            match blocks.pop().as_deref() {
                Some("*MATERIAL") => file.materials.extend(materials.pop()),
                Some("*SUBMATERIAL") => {
                    let sub = materials.pop().map(|m| m.base);
                    if let (Some(parent), Some(sub)) = (materials.last_mut(), sub) {
                        parent.subs.push(sub);
                    }
                }
                Some("*GEOMOBJECT") => {
                    if let Some(o) = object.take() {
                        file.objects.push(build_object(o)?);
                    }
                }
                _ => {}
            }
            continue;
        }
        let Some(body) = line.strip_prefix('*') else { continue };
        let (key, rest) = body.split_once(|c: char| c.is_whitespace()).unwrap_or((body, ""));
        let key = format!("*{key}");
        let opens = rest.trim_end().ends_with('{');
        let rest = rest.trim_end().trim_end_matches('{');
        let parent = blocks.last().map(String::as_str);
        match key.as_str() {
            "*MATERIAL" | "*SUBMATERIAL" if opens && parent.is_some_and(|p| p == "*MATERIAL_LIST" || p == "*MATERIAL" || p == "*SUBMATERIAL") => {
                materials.push(AseMaterial::default());
            }
            "*GEOMOBJECT" if opens => object = Some(RawObject::default()),
            _ => {}
        }
        if opens {
            blocks.push(key);
            continue;
        }
        let in_material = matches!(parent, Some("*MATERIAL" | "*SUBMATERIAL"));
        if in_material {
            if let Some(m) = materials.last_mut() {
                let b = &mut m.base;
                match key.as_str() {
                    "*MATERIAL_NAME" => b.name = unquote(rest),
                    "*MATERIAL_AMBIENT" => b.ambient = vec3(rest).to_array(),
                    "*MATERIAL_DIFFUSE" => b.diffuse = vec3(rest).to_array(),
                    "*MATERIAL_SPECULAR" => b.specular = vec3(rest).to_array(),
                    "*MATERIAL_SHINE" => b.shininess = vec3(rest).x,
                    "*MATERIAL_SHINESTRENGTH" => b.shin_strength = vec3(rest).x,
                    "*MATERIAL_TRANSPARENCY" => b.transparency = vec3(rest).x,
                    "*MATERIAL_TWOSIDED" => b.two_sided = true,
                    _ => {}
                }
            }
            continue;
        }
        if parent == Some("*MAP_DIFFUSE") {
            if let Some(m) = materials.last_mut() {
                let tex = m.base.texture.get_or_insert((String::new(), 1.0));
                match key.as_str() {
                    "*BITMAP" => tex.0 = unquote(rest),
                    "*MAP_AMOUNT" => tex.1 = vec3(rest).x,
                    _ => {}
                }
            }
            continue;
        }
        let Some(o) = object.as_mut() else { continue };
        match (parent, key.as_str()) {
            (Some("*GEOMOBJECT"), "*NODE_NAME") => o.name = Some(unquote(rest)),
            (Some("*GEOMOBJECT"), "*MATERIAL_REF") => o.material = rest.trim().parse().ok(),
            (Some("*NODE_TM"), "*TM_ROW0") => o.rows[0] = vec3(rest),
            (Some("*NODE_TM"), "*TM_ROW1") => o.rows[1] = vec3(rest),
            (Some("*NODE_TM"), "*TM_ROW2") => o.rows[2] = vec3(rest),
            (Some("*NODE_TM"), "*TM_ROW3") => o.rows[3] = vec3(rest),
            (Some("*MESH_VERTEX_LIST"), "*MESH_VERTEX") => {
                let (i, v) = indexed(rest).ok_or_else(|| format!("bad vertex: {line}"))?;
                if o.verts.len() <= i {
                    o.verts.resize(i + 1, Vec3::ZERO);
                }
                o.verts[i] = vec3(v);
            }
            (Some("*MESH_FACE_LIST"), "*MESH_FACE") => o.faces.push(parse_face(rest).ok_or_else(|| format!("bad face: {line}"))?),
            (Some("*MESH_TVERTLIST"), "*MESH_TVERT") => {
                let (i, v) = indexed(rest).ok_or_else(|| format!("bad texture vertex: {line}"))?;
                if o.tverts.len() <= i {
                    o.tverts.resize(i + 1, Vec3::ZERO);
                }
                o.tverts[i] = vec3(v);
            }
            (Some("*MESH_TFACELIST"), "*MESH_TFACE") => {
                let (i, v) = indexed(rest).ok_or_else(|| format!("bad texture face: {line}"))?;
                let t: Vec<u32> = v.split_whitespace().filter_map(|t| t.parse().ok()).collect();
                if let [a, b, c, ..] = t[..] {
                    if o.tfaces.len() <= i {
                        o.tfaces.resize(i + 1, [0; 3]);
                    }
                    o.tfaces[i] = [a, b, c];
                }
            }
            _ => {}
        }
    }
    Ok(file)
}

/// The mesh in the node's frame: the file's vertices are in world space.
fn build_object(o: RawObject) -> Result<AseObject, String> {
    let r = o.rows;
    let matrix = Mat4::from_cols(r[0].extend(0.0), r[1].extend(0.0), r[2].extend(0.0), Vec4::new(r[3].x, r[3].y, r[3].z, 1.0));
    let matrix = if matrix.determinant().abs() < 1e-12 { Mat4::IDENTITY } else { matrix };
    let to_local = matrix.inverse();
    let mut im = IndexedMesh::default();
    for v in &o.verts {
        im.add_vertex(to_local.transform_point3(*v), None);
    }
    let n = o.verts.len() as u32;
    let mut hidden: HashSet<(u32, u32)> = HashSet::new();
    let uvs = o.tfaces.len() == o.faces.len() && !o.tverts.is_empty();
    for (i, (c, vis, smoothing, mtl)) in o.faces.iter().enumerate() {
        if c.iter().any(|v| *v >= n) {
            return Err(format!("face {i} uses a vertex past MESH_NUMVERTEX"));
        }
        for k in 0..3 {
            if !vis[k] {
                hidden.insert((c[k].min(c[(k + 1) % 3]), c[k].max(c[(k + 1) % 3])));
            }
        }
        let mut uv = BTreeMap::new();
        if uvs {
            let t = o.tfaces[i];
            uv.insert(1, t.iter().map(|x| o.tverts.get(*x as usize).copied().unwrap_or_default()).collect());
        }
        im.faces.push(IndexedFace { ids: c.to_vec(), smoothing_groups: *smoothing, material_id: mtl + 1, hidden: false, uv });
    }
    let (mut mesh, source) = crate::obj::build_repaired_mapped(im)?;
    let src = |v: crate::VertexId| source.get(v.0 as usize).copied().unwrap_or(v.0);
    let edges: Vec<HalfEdgeId> = (0..mesh.half_edge_count() as u32)
        .map(HalfEdgeId)
        .filter(|he| mesh.edge_vertices(*he).is_some_and(|(a, b)| hidden.contains(&(src(a).min(src(b)), src(a).max(src(b))))))
        .collect();
    for he in edges {
        mesh.set_edge_visible(he, false);
    }
    Ok(AseObject { name: o.name.unwrap_or_else(|| "Object".into()), matrix, material: o.material, mesh })
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
        let text = write(&[("Plane001".into(), mesh.clone(), world, None)], &[]);
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

    /// What `write` puts out, `read` takes back: names, the node matrix,
    /// local vertices, the hidden diagonal, smoothing groups, ids and uvs.
    #[test]
    fn a_written_plane_reads_back() {
        let mut mesh = crate::standard_primitives::plane_segmented(10.0, 10.0, 1, 1);
        for f in mesh.faces_mut() {
            f.smoothing_groups = 0b101;
            f.material_id = 3;
        }
        let world = Mat4::from_translation(Vec3::new(100.0, 0.0, 0.0));
        let file = read(&write(&[("Plane001".into(), mesh.clone(), world, None)], &[])).unwrap();
        let [o] = &file.objects[..] else { panic!("{:?}", file.objects.len()) };
        assert_eq!(o.name, "Plane001");
        assert!(o.matrix.abs_diff_eq(world, 1e-4));
        assert_eq!((o.mesh.vertex_count(), o.mesh.face_count()), (4, 2));
        assert!(o.mesh.vertices().iter().all(|v| v.position.x.abs() <= 5.0 + 1e-3), "local frame");
        let hidden = (0..o.mesh.half_edge_count() as u32).filter(|h| !o.mesh.edge_visible(HalfEdgeId(*h))).count();
        assert_eq!(hidden, 2, "the diagonal, both half-edges");
        assert!(o.mesh.faces().iter().all(|f| f.smoothing_groups == 0b101 && f.material_id == 3));
        assert_eq!(o.mesh.map(1).is_some(), mesh.map(1).is_some());
    }

    #[test]
    fn materials_and_sub_materials_read() {
        let text = "*3DSMAX_ASCIIEXPORT\t200\n*MATERIAL_LIST {\n\t*MATERIAL_COUNT 1\n\t*MATERIAL 0 {\n\t\t*MATERIAL_NAME \"Multi\"\n\t\t*NUMSUBMTLS 2\n\t\t*SUBMATERIAL 0 {\n\t\t\t*MATERIAL_NAME \"Red\"\n\t\t\t*MATERIAL_DIFFUSE 1.0 0.0 0.0\n\t\t\t*MAP_DIFFUSE {\n\t\t\t\t*BITMAP \"maps\\red.png\"\n\t\t\t\t*MAP_AMOUNT 0.5\n\t\t\t}\n\t\t}\n\t\t*SUBMATERIAL 1 {\n\t\t\t*MATERIAL_NAME \"Blue\"\n\t\t\t*MATERIAL_TWOSIDED\n\t\t}\n\t}\n}\n";
        let file = read(text).unwrap();
        let [m] = &file.materials[..] else { panic!("{:?}", file.materials) };
        assert_eq!(m.base.name, "Multi");
        assert_eq!(m.subs.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Red", "Blue"]);
        assert_eq!(m.subs[0].diffuse, [1.0, 0.0, 0.0]);
        assert_eq!(m.subs[0].texture, Some(("maps\\red.png".into(), 0.5)));
        assert!(m.subs[1].two_sided && !m.subs[0].two_sided);
    }

    /// Written materials (a Multi/Sub-Object with a mapped child) read back
    /// with the object's reference.
    #[test]
    fn written_materials_read_back() {
        let red = TdsMaterial { name: "Red".into(), diffuse: [1.0, 0.0, 0.0], texture: Some(("maps/red.png".into(), 0.5)), ..Default::default() };
        let blue = TdsMaterial { name: "Blue".into(), two_sided: true, ..Default::default() };
        let multi = AseMaterial { base: TdsMaterial { name: "Multi".into(), ..Default::default() }, subs: vec![red.clone(), blue.clone()] };
        let mesh = crate::standard_primitives::plane_segmented(10.0, 10.0, 1, 1);
        let file = read(&write(&[("P".into(), mesh, Mat4::IDENTITY, Some(0))], &[multi])).unwrap();
        assert_eq!(file.objects[0].material, Some(0));
        assert_eq!(file.materials[0].subs, vec![red, blue]);
    }
}
