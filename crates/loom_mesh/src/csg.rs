//! Mesh CSG on BSP trees (the csg.js construction): union, intersection,
//! subtraction and the Cut modes (Refine, Split, Remove Inside, Remove
//! Outside) on triangulated operands. Face attributes ride along with the
//! polygons. Degenerate input (overlapping coplanar faces that leave the
//! result non-manifold) comes back as an `Err`, never a panic.

use crate::indexed::{IndexedFace, IndexedMesh};
use crate::mesh_mods::triangulate;
use crate::{FaceId, Mesh};
use glam::Vec3;

const EPS: f32 = 1e-5;
const MAX_DEPTH: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsgOp {
    Union,
    Intersection,
    /// A minus B.
    Subtraction,
    /// B minus A.
    SubtractionBA,
    /// Cut: A's faces split by B, everything kept.
    CutRefine,
    /// Cut: A's faces split by B, inside and outside as separate elements.
    CutSplit,
    CutRemoveInside,
    CutRemoveOutside,
}

#[derive(Debug, Clone)]
struct Polygon {
    pts: Vec<Vec3>,
    normal: Vec3,
    w: f32,
    material_id: u16,
    smoothing_groups: u32,
    /// Which operand the polygon came from (0 = A, 1 = B).
    source: u8,
}

impl Polygon {
    fn new(pts: Vec<Vec3>, material_id: u16, smoothing_groups: u32, source: u8) -> Option<Self> {
        if pts.len() < 3 {
            return None;
        }
        let n = (pts[1] - pts[0]).cross(pts[2] - pts[0]);
        if n.length_squared() < 1e-14 {
            return None;
        }
        let normal = n.normalize();
        Some(Self { w: normal.dot(pts[0]), pts, normal, material_id, smoothing_groups, source })
    }

    fn flip(&mut self) {
        self.pts.reverse();
        self.normal = -self.normal;
        self.w = -self.w;
    }
}

#[derive(Debug, Clone, Copy)]
struct Plane {
    normal: Vec3,
    w: f32,
}

const COPLANAR: u8 = 0;
const FRONT: u8 = 1;
const BACK: u8 = 2;
const SPANNING: u8 = 3;

impl Plane {
    fn flip(&mut self) {
        self.normal = -self.normal;
        self.w = -self.w;
    }

    /// Splits `polygon` by this plane: (coplanar front, coplanar back, front, back).
    fn split(&self, polygon: &Polygon) -> (Vec<Polygon>, Vec<Polygon>, Vec<Polygon>, Vec<Polygon>) {
        let (mut cf, mut cb, mut front, mut back) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut ptype = 0u8;
        let types: Vec<u8> = polygon
            .pts
            .iter()
            .map(|p| {
                let t = self.normal.dot(*p) - self.w;
                let ty = if t < -EPS { BACK } else if t > EPS { FRONT } else { COPLANAR };
                ptype |= ty;
                ty
            })
            .collect();
        match ptype {
            COPLANAR => {
                if self.normal.dot(polygon.normal) > 0.0 { cf.push(polygon.clone()) } else { cb.push(polygon.clone()) }
            }
            FRONT => front.push(polygon.clone()),
            BACK => back.push(polygon.clone()),
            _ => {
                let (mut f, mut b) = (Vec::new(), Vec::new());
                let n = polygon.pts.len();
                for i in 0..n {
                    let j = (i + 1) % n;
                    let (ti, tj) = (types[i], types[j]);
                    let (vi, vj) = (polygon.pts[i], polygon.pts[j]);
                    if ti != BACK {
                        f.push(vi);
                    }
                    if ti != FRONT {
                        b.push(vi);
                    }
                    if (ti | tj) == SPANNING {
                        let t = (self.w - self.normal.dot(vi)) / self.normal.dot(vj - vi);
                        let v = vi.lerp(vj, t);
                        f.push(v);
                        b.push(v);
                    }
                }
                if let Some(p) = Polygon::new(f, polygon.material_id, polygon.smoothing_groups, polygon.source) {
                    front.push(p);
                }
                if let Some(p) = Polygon::new(b, polygon.material_id, polygon.smoothing_groups, polygon.source) {
                    back.push(p);
                }
            }
        }
        (cf, cb, front, back)
    }
}

#[derive(Default)]
struct Node {
    plane: Option<Plane>,
    front: Option<Box<Node>>,
    back: Option<Box<Node>>,
    polygons: Vec<Polygon>,
}

impl Node {
    fn invert(&mut self) {
        for p in &mut self.polygons {
            p.flip();
        }
        if let Some(pl) = &mut self.plane {
            pl.flip();
        }
        if let Some(f) = &mut self.front {
            f.invert();
        }
        if let Some(b) = &mut self.back {
            b.invert();
        }
        std::mem::swap(&mut self.front, &mut self.back);
    }

    /// Polygons of `polygons` that lie outside this tree's solid.
    fn clip_polygons(&self, polygons: Vec<Polygon>, depth: usize) -> Result<Vec<Polygon>, String> {
        if depth > MAX_DEPTH {
            return Err("an operand has too many faces for the BSP Boolean (about 4000 triangles per convex region)".into());
        }
        let Some(plane) = self.plane else { return Ok(polygons) };
        let (mut front, mut back) = (Vec::new(), Vec::new());
        for p in &polygons {
            let (cf, cb, f, b) = plane.split(p);
            front.extend(cf);
            front.extend(f);
            back.extend(cb);
            back.extend(b);
        }
        let front = match &self.front {
            Some(f) => f.clip_polygons(front, depth + 1)?,
            None => front,
        };
        let back = match &self.back {
            Some(b) => b.clip_polygons(back, depth + 1)?,
            None => Vec::new(),
        };
        let mut out = front;
        out.extend(back);
        Ok(out)
    }

    fn clip_to(&mut self, other: &Node) -> Result<(), String> {
        self.polygons = other.clip_polygons(std::mem::take(&mut self.polygons), 0)?;
        if let Some(f) = &mut self.front {
            f.clip_to(other)?;
        }
        if let Some(b) = &mut self.back {
            b.clip_to(other)?;
        }
        Ok(())
    }

    fn all_polygons(&self, out: &mut Vec<Polygon>) {
        out.extend(self.polygons.iter().cloned());
        if let Some(f) = &self.front {
            f.all_polygons(out);
        }
        if let Some(b) = &self.back {
            b.all_polygons(out);
        }
    }

    fn build(&mut self, polygons: Vec<Polygon>, depth: usize) -> Result<(), String> {
        if polygons.is_empty() {
            return Ok(());
        }
        if depth > MAX_DEPTH {
            return Err("an operand has too many faces for the BSP Boolean (about 4000 triangles per convex region)".into());
        }
        if self.plane.is_none() {
            self.plane = Some(Plane { normal: polygons[0].normal, w: polygons[0].w });
        }
        let plane = self.plane.unwrap();
        let (mut front, mut back) = (Vec::new(), Vec::new());
        for p in &polygons {
            let (cf, cb, f, b) = plane.split(p);
            self.polygons.extend(cf);
            self.polygons.extend(cb);
            front.extend(f);
            back.extend(b);
        }
        if !front.is_empty() {
            self.front.get_or_insert_with(Default::default).build(front, depth + 1)?;
        }
        if !back.is_empty() {
            self.back.get_or_insert_with(Default::default).build(back, depth + 1)?;
        }
        Ok(())
    }
}

fn polygons_of(mesh: &Mesh, source: u8) -> Result<Vec<Polygon>, String> {
    let tri = triangulate(mesh)?;
    let mut out = Vec::new();
    for f in 0..tri.face_count() {
        let id = FaceId(f as u32);
        let Some(pts) = tri.face_positions(id) else { continue };
        let face = &tri.faces()[f];
        if let Some(p) = Polygon::new(pts, face.material_id, face.smoothing_groups, source) {
            out.push(p);
        }
    }
    Ok(out)
}

fn mesh_of(polygons: &[Polygon]) -> Result<Mesh, String> {
    let mut im = IndexedMesh::default();
    let mut by_key: std::collections::HashMap<(i64, i64, i64), u32> = std::collections::HashMap::new();
    for p in polygons {
        let ids: Vec<u32> = p
            .pts
            .iter()
            .map(|v| {
                let key = ((v.x * 1e4).round() as i64, (v.y * 1e4).round() as i64, (v.z * 1e4).round() as i64);
                *by_key.entry(key).or_insert_with(|| im.add_vertex(*v, None))
            })
            .collect();
        let mut ids2 = ids.clone();
        ids2.dedup();
        while ids2.len() > 1 && ids2.first() == ids2.last() {
            ids2.pop();
        }
        if ids2.len() >= 3 {
            im.faces.push(IndexedFace { ids: ids2, smoothing_groups: p.smoothing_groups, material_id: p.material_id, hidden: false, uv: Default::default() });
        }
    }
    im.build().map_err(|e| format!("the Boolean result is not manifold ({e}); the operands probably share a face: move one slightly"))
}

/// Boolean of two closed meshes given in the same space.
pub fn boolean(a: &Mesh, b: &Mesh, op: CsgOp) -> Result<Mesh, String> {
    let pa = polygons_of(a, 0)?;
    let pb = polygons_of(b, 1)?;
    if pa.is_empty() || pb.is_empty() {
        return Err("both operands need faces".into());
    }
    let mut na = Node::default();
    na.build(pa.clone(), 0)?;
    let mut nb = Node::default();
    nb.build(pb.clone(), 0)?;
    let polys = match op {
        CsgOp::Union => {
            na.clip_to(&nb)?;
            nb.clip_to(&na)?;
            nb.invert();
            nb.clip_to(&na)?;
            nb.invert();
            let mut rest = Vec::new();
            nb.all_polygons(&mut rest);
            na.build(rest, 0)?;
            let mut out = Vec::new();
            na.all_polygons(&mut out);
            out
        }
        CsgOp::Subtraction | CsgOp::SubtractionBA => {
            let (mut na, mut nb) = if op == CsgOp::Subtraction { (na, nb) } else { (nb, na) };
            na.invert();
            na.clip_to(&nb)?;
            nb.clip_to(&na)?;
            nb.invert();
            nb.clip_to(&na)?;
            nb.invert();
            let mut rest = Vec::new();
            nb.all_polygons(&mut rest);
            na.build(rest, 0)?;
            na.invert();
            let mut out = Vec::new();
            na.all_polygons(&mut out);
            out
        }
        CsgOp::Intersection => {
            na.invert();
            nb.clip_to(&na)?;
            nb.invert();
            na.clip_to(&nb)?;
            nb.clip_to(&na)?;
            let mut rest = Vec::new();
            nb.all_polygons(&mut rest);
            na.build(rest, 0)?;
            na.invert();
            let mut out = Vec::new();
            na.all_polygons(&mut out);
            out
        }
        CsgOp::CutRefine | CsgOp::CutSplit | CsgOp::CutRemoveInside | CsgOp::CutRemoveOutside => {
            // A's polygons split by B's planes: outside and inside parts.
            let outside = nb.clip_polygons(pa.clone(), 0)?;
            let mut inv = Node::default();
            inv.build(pb, 0)?;
            inv.invert();
            let mut inside = inv.clip_polygons(pa, 0)?;
            match op {
                CsgOp::CutRemoveInside => outside,
                CsgOp::CutRemoveOutside => inside,
                _ => {
                    if op == CsgOp::CutSplit {
                        // Split: the inside part becomes its own element (separate smoothing group marks it).
                        for p in &mut inside {
                            p.smoothing_groups = p.smoothing_groups.rotate_left(16).max(1 << 16);
                        }
                    }
                    let mut out = outside;
                    out.extend(inside);
                    out
                }
            }
        }
    };
    if polys.is_empty() {
        return Err("the Boolean result is empty (the operands do not overlap)".into());
    }
    mesh_of(&polys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{box_primitive, edit_ops};

    fn moved(m: &Mesh, d: Vec3) -> Mesh {
        let mut c = m.clone();
        edit_ops::translate_mesh(&mut c, d).unwrap();
        c
    }

    #[test]
    fn subtracting_a_smaller_box_cuts_a_hole() {
        let a = box_primitive(Vec3::splat(5.0));
        let b = moved(&box_primitive(Vec3::new(2.0, 2.0, 8.0)), Vec3::ZERO);
        let r = boolean(&a, &b, CsgOp::Subtraction).unwrap();
        r.check_invariants().unwrap();
        assert_eq!(r.aabb(), a.aabb(), "the outer box keeps its bounds");
        assert!(r.face_count() > 12, "{}", r.face_count());
        // The hole: no face centre lies inside the removed column.
        for f in 0..r.face_count() {
            let pts = r.face_positions(FaceId(f as u32)).unwrap();
            let c = pts.iter().sum::<Vec3>() / pts.len() as f32;
            assert!(!(c.x.abs() < 1.9 && c.y.abs() < 1.9), "face inside the hole: {c:?}");
        }
        let u = boolean(&a, &moved(&box_primitive(Vec3::splat(5.0)), Vec3::new(5.0, 0.0, 0.0)), CsgOp::Union).unwrap();
        u.check_invariants().unwrap();
        assert!((u.aabb().1.x - 10.0).abs() < 1e-4 && (u.aabb().0.x + 5.0).abs() < 1e-4);
        let i = boolean(&a, &moved(&box_primitive(Vec3::splat(5.0)), Vec3::new(5.0, 0.0, 0.0)), CsgOp::Intersection).unwrap();
        assert!((i.aabb().0.x - 0.0).abs() < 1e-4 && (i.aabb().1.x - 5.0).abs() < 1e-4);
        let ba = boolean(&b, &a, CsgOp::SubtractionBA).unwrap();
        assert_eq!(ba.aabb(), a.aabb(), "B minus A swaps the operands");
    }

    #[test]
    fn cut_modes_keep_or_drop_the_parts_and_disjoint_operands_report() {
        // B shares no face plane with A (coplanar faces count as outside, as in csg.js).
        let a = box_primitive(Vec3::splat(5.0));
        let b = moved(&box_primitive(Vec3::new(5.0, 3.0, 3.0)), Vec3::new(5.0, 0.0, 0.0));
        let refine = boolean(&a, &b, CsgOp::CutRefine).unwrap();
        refine.check_invariants().unwrap();
        assert!(refine.face_count() > 12, "refine {} faces", refine.face_count());
        let inside = boolean(&a, &b, CsgOp::CutRemoveOutside).unwrap();
        assert!(inside.aabb().0.x >= -1e-4 && inside.aabb().1.y <= 3.0 + 1e-4, "only the part inside B stays: {:?}", inside.aabb());
        let outside = boolean(&a, &b, CsgOp::CutRemoveInside).unwrap();
        assert_eq!(outside.aabb(), a.aabb(), "{:?}", outside.aabb());
        assert!(outside.face_count() > 12);
        let split = boolean(&a, &b, CsgOp::CutSplit).unwrap();
        assert!(split.faces().iter().any(|f| f.smoothing_groups >= 1 << 16));
        let far = moved(&box_primitive(Vec3::splat(1.0)), Vec3::new(50.0, 0.0, 0.0));
        assert!(boolean(&a, &far, CsgOp::Intersection).is_err());
    }
}
