//! VRML97 (.wrl) reading: the scene graph (Transform, Group, Anchor,
//! Billboard, Collision, Switch, LOD) flattened into shapes with their world
//! matrices, IndexedFaceSet / Box / Sphere / Cylinder / Cone geometry and
//! Appearance (Material, ImageTexture). DEF / USE are followed; PROTO,
//! ROUTE and Script are skipped.

use crate::indexed::{IndexedFace, IndexedMesh};
use crate::Mesh;
use glam::{Mat4, Quat, Vec3};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

/// A Material (with its ImageTexture) of a shape's Appearance.
#[derive(Debug, Clone, PartialEq)]
pub struct VrmlMaterial {
    /// The DEF name of the Material node, when it has one.
    pub name: Option<String>,
    pub diffuse: [f32; 3],
    pub specular: [f32; 3],
    pub emissive: [f32; 3],
    pub shininess: f32,
    pub transparency: f32,
    pub texture: Option<String>,
}

impl Default for VrmlMaterial {
    fn default() -> Self {
        Self { name: None, diffuse: [0.8; 3], specular: [0.0; 3], emissive: [0.0; 3], shininess: 0.2, transparency: 0.0, texture: None }
    }
}

/// One Shape: its name (the nearest DEF above it), the world matrix of its
/// frame, its geometry in that frame and its appearance.
#[derive(Debug, Clone)]
pub struct VrmlShape {
    pub name: String,
    pub matrix: Mat4,
    pub mesh: Mesh,
    pub material: Option<VrmlMaterial>,
}

#[derive(Debug, Clone)]
enum Item {
    Num(f64),
    Word(String),
    Str(String),
    Node(Rc<Node>),
}

#[derive(Debug, Clone, Default)]
struct Node {
    def: Option<String>,
    kind: String,
    fields: Vec<(String, Vec<Item>)>,
}

impl Node {
    fn field(&self, name: &str) -> Option<&[Item]> {
        self.fields.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_slice())
    }
    fn nums(&self, name: &str) -> Vec<f64> {
        self.field(name).map(|v| v.iter().filter_map(|i| if let Item::Num(x) = i { Some(*x) } else { None }).collect()).unwrap_or_default()
    }
    fn num(&self, name: &str, default: f32) -> f32 {
        self.nums(name).first().map_or(default, |x| *x as f32)
    }
    fn vec3(&self, name: &str, default: Vec3) -> Vec3 {
        match self.nums(name)[..] {
            [x, y, z, ..] => Vec3::new(x as f32, y as f32, z as f32),
            _ => default,
        }
    }
    fn flag(&self, name: &str, default: bool) -> bool {
        match self.field(name).and_then(|v| v.first()) {
            Some(Item::Word(w)) => w == "TRUE",
            _ => default,
        }
    }
    fn nodes(&self, name: &str) -> Vec<Rc<Node>> {
        self.field(name).map(|v| v.iter().filter_map(|i| if let Item::Node(n) = i { Some(n.clone()) } else { None }).collect()).unwrap_or_default()
    }
    fn node(&self, name: &str) -> Option<Rc<Node>> {
        self.nodes(name).into_iter().next()
    }
    fn strings(&self, name: &str) -> Vec<String> {
        self.field(name).map(|v| v.iter().filter_map(|i| if let Item::Str(s) = i { Some(s.clone()) } else { None }).collect()).unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Open,
    Close,
    LBracket,
    RBracket,
    Str(String),
    Word(String),
}

fn tokens(text: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            '#' => {
                while chars.peek().is_some_and(|c| *c != '\n') {
                    chars.next();
                }
            }
            '{' | '}' | '[' | ']' => {
                chars.next();
                out.push(match c {
                    '{' => Tok::Open,
                    '}' => Tok::Close,
                    '[' => Tok::LBracket,
                    _ => Tok::RBracket,
                });
            }
            '"' => {
                chars.next();
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => s.extend(chars.next()),
                        '"' => break,
                        c => s.push(c),
                    }
                }
                out.push(Tok::Str(s));
            }
            c if c.is_whitespace() || c == ',' => {
                chars.next();
            }
            _ => {
                let mut w = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || matches!(c, ',' | '{' | '}' | '[' | ']' | '"' | '#') {
                        break;
                    }
                    w.push(c);
                    chars.next();
                }
                out.push(Tok::Word(w));
            }
        }
    }
    out
}

struct Parser {
    toks: Vec<Tok>,
    at: usize,
    defs: HashMap<String, Rc<Node>>,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.at).cloned();
        self.at += 1;
        t
    }
    /// Skips a balanced { } or [ ] group starting at the next token.
    fn skip_group(&mut self) {
        let mut depth = 0i32;
        while let Some(t) = self.next() {
            match t {
                Tok::Open | Tok::LBracket => depth += 1,
                Tok::Close | Tok::RBracket => {
                    depth -= 1;
                    if depth <= 0 {
                        return;
                    }
                }
                _ if depth == 0 => return,
                _ => {}
            }
        }
    }
    /// Statements at the top or in a children list: nodes, with PROTO /
    /// EXTERNPROTO / ROUTE skipped. Stops at `]` or the end.
    fn statements(&mut self) -> Result<Vec<Rc<Node>>, String> {
        let mut out = Vec::new();
        while let Some(t) = self.peek().cloned() {
            match t {
                Tok::RBracket | Tok::Close => break,
                Tok::Word(w) if w == "ROUTE" => self.at += 4,
                Tok::Word(w) if w == "PROTO" => {
                    self.at += 2;
                    self.skip_group();
                    self.skip_group();
                }
                Tok::Word(w) if w == "EXTERNPROTO" => {
                    self.at += 2;
                    self.skip_group();
                    self.next();
                    if matches!(self.peek(), Some(Tok::LBracket)) {
                        self.skip_group();
                    }
                }
                Tok::Word(_) => out.extend(self.node()?),
                _ => {
                    self.next();
                }
            }
        }
        Ok(out)
    }
    /// `[DEF name] Type { fields }`, `USE name` or NULL.
    fn node(&mut self) -> Result<Option<Rc<Node>>, String> {
        let Some(Tok::Word(mut w)) = self.next() else { return Err("a node was expected".into()) };
        if w == "NULL" {
            return Ok(None);
        }
        if w == "USE" {
            let Some(Tok::Word(name)) = self.next() else { return Err("USE without a name".into()) };
            return self.defs.get(&name).cloned().map(Some).ok_or_else(|| format!("USE {name} before its DEF"));
        }
        let mut def = None;
        if w == "DEF" {
            let Some(Tok::Word(name)) = self.next() else { return Err("DEF without a name".into()) };
            def = Some(name);
            let Some(Tok::Word(k)) = self.next() else { return Err("DEF without a node".into()) };
            w = k;
        }
        if self.peek() != Some(&Tok::Open) {
            return Err(format!("{w}: '{{' expected"));
        }
        let mut node = Node { def: def.clone(), kind: w, fields: Vec::new() };
        if node.kind == "Script" {
            self.skip_group();
        } else {
            self.next();
            while let Some(t) = self.peek().cloned() {
                match t {
                    Tok::Close => {
                        self.next();
                        break;
                    }
                    Tok::Word(f) => {
                        self.next();
                        let v = self.value()?;
                        node.fields.push((f, v));
                    }
                    _ => {
                        self.next();
                    }
                }
            }
        }
        let node = Rc::new(node);
        if let Some(d) = def {
            self.defs.insert(d, node.clone());
        }
        Ok(Some(node))
    }
    fn atom(w: &str) -> Item {
        w.parse::<f64>().map(Item::Num).unwrap_or_else(|_| Item::Word(w.to_owned()))
    }
    /// A field value: `[ ... ]`, a node, or atoms (numbers, TRUE / FALSE, strings).
    fn value(&mut self) -> Result<Vec<Item>, String> {
        let mut out = Vec::new();
        match self.peek().cloned() {
            Some(Tok::LBracket) => {
                self.next();
                while let Some(t) = self.peek().cloned() {
                    match t {
                        Tok::RBracket => {
                            self.next();
                            break;
                        }
                        Tok::Str(s) => {
                            self.next();
                            out.push(Item::Str(s));
                        }
                        Tok::Word(w) if w.parse::<f64>().is_ok() || w == "TRUE" || w == "FALSE" => {
                            self.next();
                            out.push(Self::atom(&w));
                        }
                        Tok::Word(_) => out.extend(self.node()?.map(Item::Node)),
                        _ => {
                            self.next();
                        }
                    }
                }
            }
            Some(Tok::Str(s)) => {
                self.next();
                out.push(Item::Str(s));
            }
            Some(Tok::Word(w)) if w.parse::<f64>().is_ok() => {
                while let Some(Tok::Word(w)) = self.peek().cloned() {
                    let Ok(x) = w.parse::<f64>() else { break };
                    self.next();
                    out.push(Item::Num(x));
                }
            }
            Some(Tok::Word(w)) if w == "TRUE" || w == "FALSE" => {
                self.next();
                out.push(Item::Word(w));
            }
            Some(Tok::Word(w)) if w == "IS" => {
                self.at += 2;
            }
            Some(Tok::Word(_)) => out.extend(self.node()?.map(Item::Node)),
            _ => {}
        }
        Ok(out)
    }
}

/// Reads a VRML97 file. `z_up`: Turn to 3DS Coordinates (VRML's +Y up
/// becomes +Z).
pub fn read(text: &str, z_up: bool) -> Result<Vec<VrmlShape>, String> {
    let head = text.lines().next().unwrap_or_default();
    if !head.starts_with("#VRML V2.0") {
        return Err(if head.starts_with("#VRML V1.0") { "VRML 1.0 files are not read (VRML97 / 2.0 only)".into() } else { "not a VRML97 file".into() });
    }
    let mut p = Parser { toks: tokens(text), at: 0, defs: HashMap::new() };
    let roots = p.statements()?;
    let root = if z_up { Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2) } else { Mat4::IDENTITY };
    let mut out = Vec::new();
    for n in &roots {
        walk(n, root, None, &mut out);
    }
    Ok(out)
}

fn walk(n: &Node, m: Mat4, name: Option<&str>, out: &mut Vec<VrmlShape>) {
    let name = n.def.as_deref().filter(|_| n.kind != "Shape").or(name);
    match n.kind.as_str() {
        "Transform" => {
            let t = n.vec3("translation", Vec3::ZERO);
            let c = n.vec3("center", Vec3::ZERO);
            let s = n.vec3("scale", Vec3::ONE);
            let rot = |f: &str| match n.nums(f)[..] {
                [x, y, z, a, ..] if Vec3::new(x as f32, y as f32, z as f32).length_squared() > 0.0 => Quat::from_axis_angle(Vec3::new(x as f32, y as f32, z as f32).normalize(), a as f32),
                _ => Quat::IDENTITY,
            };
            let (r, so) = (rot("rotation"), rot("scaleOrientation"));
            let local = Mat4::from_translation(t + c) * Mat4::from_quat(r) * Mat4::from_quat(so) * Mat4::from_scale(s) * Mat4::from_quat(so.inverse()) * Mat4::from_translation(-c);
            for ch in n.nodes("children") {
                walk(&ch, m * local, name, out);
            }
        }
        "Group" | "Anchor" | "Billboard" | "Collision" => {
            for ch in n.nodes("children") {
                walk(&ch, m, name, out);
            }
        }
        "Switch" => {
            let which = n.num("whichChoice", -1.0);
            if which >= 0.0 {
                if let Some(ch) = n.nodes("choice").get(which as usize) {
                    walk(ch, m, name, out);
                }
            }
        }
        // The most detailed level.
        "LOD" => {
            if let Some(ch) = n.nodes("level").first() {
                walk(ch, m, name, out);
            }
        }
        "Shape" => {
            let Some(geometry) = n.node("geometry") else { return };
            let Some(mesh) = geometry_mesh(&geometry) else { return };
            if mesh.face_count() == 0 {
                return;
            }
            let material = n.node("appearance").map(|a| {
                let mut v = VrmlMaterial::default();
                if let Some(mat) = a.node("material") {
                    v.name = mat.def.clone();
                    v.diffuse = mat.vec3("diffuseColor", Vec3::splat(0.8)).to_array();
                    v.specular = mat.vec3("specularColor", Vec3::ZERO).to_array();
                    v.emissive = mat.vec3("emissiveColor", Vec3::ZERO).to_array();
                    v.shininess = mat.num("shininess", 0.2);
                    v.transparency = mat.num("transparency", 0.0);
                }
                v.texture = a.node("texture").filter(|t| t.kind == "ImageTexture").and_then(|t| t.strings("url").into_iter().next());
                v
            });
            out.push(VrmlShape { name: name.unwrap_or(geometry.kind.as_str()).to_owned(), matrix: m, mesh, material });
        }
        _ => {}
    }
}

/// VRML primitives are centred and +Y up; 3dLoom's stand on their base, +Z up.
fn y_up_centred(mut mesh: Mesh, height: f32) -> Mesh {
    let m = Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2) * Mat4::from_translation(Vec3::new(0.0, 0.0, -height * 0.5));
    for v in mesh.vertices_mut() {
        v.position = m.transform_point3(v.position);
    }
    mesh
}

fn geometry_mesh(g: &Node) -> Option<Mesh> {
    use crate::standard_primitives as sp;
    match g.kind.as_str() {
        "IndexedFaceSet" => face_set(g),
        "Box" => {
            let s = g.vec3("size", Vec3::splat(2.0));
            // Box: Length along Y, Width X, Height Z; VRML's Y (height) is the primitive's Z.
            Some(y_up_centred(sp::box_segmented(s.z, s.x, s.y, [1, 1, 1]), s.y))
        }
        "Sphere" => {
            let r = g.num("radius", 1.0);
            Some(y_up_centred(sp::sphere_segmented(r, 24, 0.0), 0.0))
        }
        "Cylinder" => {
            let (r, h) = (g.num("radius", 1.0), g.num("height", 2.0));
            Some(y_up_centred(sp::cylinder_segmented(r, h, 1, 1, 18, None), h))
        }
        "Cone" => {
            let (r, h) = (g.num("bottomRadius", 1.0), g.num("height", 2.0));
            Some(y_up_centred(sp::cone_primitive(r, 0.0, h, 1, 1, 18, None), h))
        }
        _ => None,
    }
}

fn split_faces(idx: &[f64]) -> Vec<Vec<i64>> {
    let mut out = vec![Vec::new()];
    for i in idx {
        let i = *i as i64;
        if i < 0 {
            out.push(Vec::new());
        } else if let Some(f) = out.last_mut() {
            f.push(i);
        }
    }
    out.retain(|f| f.len() >= 3);
    out
}

fn face_set(g: &Node) -> Option<Mesh> {
    let pts = g.node("coord")?.nums("point");
    let positions: Vec<Vec3> = pts.chunks_exact(3).map(|c| Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32)).collect();
    let faces = split_faces(&g.nums("coordIndex"));
    let uvs: Vec<Vec3> = g.node("texCoord").map(|t| t.nums("point").chunks_exact(2).map(|c| Vec3::new(c[0] as f32, c[1] as f32, 0.0)).collect()).unwrap_or_default();
    let tex_faces = {
        let t = split_faces(&g.nums("texCoordIndex"));
        if t.is_empty() { faces.clone() } else { t }
    };
    let ccw = g.flag("ccw", true);
    let mut im = IndexedMesh::default();
    for p in &positions {
        im.add_vertex(*p, None);
    }
    let n = positions.len() as i64;
    for (fi, f) in faces.iter().enumerate() {
        if f.iter().any(|i| *i >= n) {
            return None;
        }
        let mut ids: Vec<u32> = f.iter().map(|i| *i as u32).collect();
        let mut uv = BTreeMap::new();
        if !uvs.is_empty() {
            if let Some(t) = tex_faces.get(fi).filter(|t| t.len() == f.len()) {
                uv.insert(1, t.iter().map(|i| uvs.get(*i as usize).copied().unwrap_or_default()).collect::<Vec<_>>());
            }
        }
        if !ccw {
            ids.reverse();
            if let Some(v) = uv.get_mut(&1) {
                v.reverse();
            }
        }
        im.faces.push(IndexedFace { ids, uv, ..Default::default() });
    }
    let mut mesh = crate::obj::build_repaired(im).ok()?;
    let crease = g.num("creaseAngle", 0.0);
    if crease > 0.0 {
        mesh.auto_smooth(crease.to_degrees());
    }
    Some(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORLD: &str = r#"#VRML V2.0 utf8
# a comment
DEF Crate Transform {
  translation 10 0 0
  rotation 0 1 0 1.5708
  children [
    Shape {
      appearance Appearance {
        material DEF Wood Material { diffuseColor 0.6 0.4 0.2 transparency 0.25 }
        texture ImageTexture { url [ "wood.png" ] }
      }
      geometry IndexedFaceSet {
        coord Coordinate { point [ 0 0 0, 1 0 0, 1 1 0, 0 1 0 ] }
        coordIndex [ 0 1 2 3 -1 ]
        texCoord TextureCoordinate { point [ 0 0, 1 0, 1 1, 0 1 ] }
      }
    }
  ]
}
ROUTE A.b TO C.d
DEF Ball Transform { translation 0 5 0 children Shape { geometry Sphere { radius 2 } } }
Transform { translation 0 0 -3 children USE Ball }
"#;

    #[test]
    fn transforms_shapes_materials_and_use() {
        let shapes = read(WORLD, false).unwrap();
        assert_eq!(shapes.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Crate", "Ball", "Ball"]);
        let quad = &shapes[0];
        assert_eq!((quad.mesh.vertex_count(), quad.mesh.face_count()), (4, 1));
        assert!(quad.matrix.transform_point3(Vec3::X).abs_diff_eq(Vec3::new(10.0, 0.0, -1.0), 1e-3), "rotated about Y, then moved");
        let m = quad.material.as_ref().unwrap();
        assert_eq!((m.name.as_deref(), m.diffuse, m.transparency, m.texture.as_deref()), (Some("Wood"), [0.6, 0.4, 0.2], 0.25, Some("wood.png")));
        assert!(quad.mesh.map(1).is_some(), "texture coordinates");
        // USE: the same sphere again, under another transform.
        assert!(shapes[2].matrix.transform_point3(Vec3::ZERO).abs_diff_eq(Vec3::new(0.0, 5.0, -3.0), 1e-4));
        let ext = shapes[1].mesh.vertices().iter().fold(0.0f32, |a, v| a.max(v.position.y));
        assert!((ext - 2.0).abs() < 1e-3, "the sphere is centred on its frame: {ext}");
    }

    #[test]
    fn turn_to_3ds_coordinates_puts_y_up_on_z() {
        let shapes = read(WORLD, true).unwrap();
        assert!(shapes[1].matrix.transform_point3(Vec3::ZERO).abs_diff_eq(Vec3::new(0.0, 0.0, 5.0), 1e-4));
        assert!(read("#VRML V1.0 ascii\nSeparator {}", true).is_err());
    }
}
