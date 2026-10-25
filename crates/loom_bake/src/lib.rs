//! Render To Texture on the CPU: a mesh rasterised in its texture space
//! (which texel lies on which triangle, and where), the elements that are
//! geometry rather than lighting (normals, from the mesh itself or projected
//! from high-resolution objects, height and alpha) and edge padding. The
//! lit elements are rendered by the GPU renderer in the same texture space.

use glam::{Mat3, Vec2, Vec3};
use image::{Rgba, RgbaImage};
use loom_mesh::{FaceId, Mesh};

/// One triangle of a mesh in world space with its corner normals and texture coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Triangle {
    pub p: [Vec3; 3],
    pub n: [Vec3; 3],
    pub uv: [Vec2; 3],
    pub material_id: u16,
}

impl Triangle {
    pub fn point(&self, b: Vec3) -> Vec3 {
        self.p[0] * b.x + self.p[1] * b.y + self.p[2] * b.z
    }

    /// The interpolated (smoothed) normal.
    pub fn normal(&self, b: Vec3) -> Vec3 {
        let n = self.n[0] * b.x + self.n[1] * b.y + self.n[2] * b.z;
        if n.length_squared() > 1e-12 {
            n.normalize()
        } else {
            self.face_normal()
        }
    }

    pub fn face_normal(&self) -> Vec3 {
        (self.p[1] - self.p[0]).cross(self.p[2] - self.p[0]).normalize_or_zero()
    }

    /// The directions of increasing U and V on the triangle (not normalised);
    /// None when its texture coordinates are degenerate.
    pub fn uv_axes(&self) -> Option<(Vec3, Vec3)> {
        let (e1, e2) = (self.p[1] - self.p[0], self.p[2] - self.p[0]);
        let (d1, d2) = (self.uv[1] - self.uv[0], self.uv[2] - self.uv[0]);
        let det = d1.x * d2.y - d2.x * d1.y;
        if det.abs() < 1e-12 {
            return None;
        }
        let r = 1.0 / det;
        Some(((e1 * d2.y - e2 * d1.y) * r, (e2 * d1.x - e1 * d2.x) * r))
    }

    fn bounds(&self) -> (Vec3, Vec3) {
        (self.p[0].min(self.p[1]).min(self.p[2]), self.p[0].max(self.p[1]).max(self.p[2]))
    }
}

/// The visible faces of `mesh` fan-triangulated, with normals as the shaded
/// viewport computes them (explicit normals, then smoothing groups) and
/// texture coordinates from map channel `channel` ((0, 0) without one).
pub fn triangles(mesh: &Mesh, channel: u32) -> Vec<Triangle> {
    let mut out = Vec::new();
    for (fi, face) in mesh.faces().iter().enumerate() {
        if face.hidden {
            continue;
        }
        let fid = FaceId(fi as u32);
        let Some(corners) = mesh.face_corners(fid) else { continue };
        if corners.len() < 3 {
            continue;
        }
        let flat = face.smoothing_groups == 0;
        let face_n = mesh.face_normal(fid);
        let data: Vec<(Vec3, Vec3, Vec2)> = corners
            .iter()
            .map(|he| {
                let v = mesh.half_edges()[he.0 as usize].origin;
                let p = mesh.vertices()[v.0 as usize].position;
                let n = match mesh.explicit_normal(*he) {
                    Some(e) => e,
                    None if flat => face_n,
                    None => mesh.vertex_normal(v, fid),
                };
                let uv = mesh.corner_uv(channel, *he).map(|t| Vec2::new(t.x, t.y)).unwrap_or(Vec2::ZERO);
                (p, n, uv)
            })
            .collect();
        for i in 1..data.len() - 1 {
            let c = [data[0], data[i], data[i + 1]];
            out.push(Triangle { p: c.map(|d| d.0), n: c.map(|d| d.1), uv: c.map(|d| d.2), material_id: face.material_id });
        }
    }
    out
}

/// Which triangle (and where on it) each texel centre of a `width` x
/// `height` map lies on. Row 0 is the top of the map (V = 1).
#[derive(Debug, Clone)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    /// Per texel, row by row: (triangle index, barycentric coordinates).
    pub texels: Vec<Option<(u32, Vec3)>>,
}

impl Raster {
    pub fn get(&self, x: u32, y: u32) -> Option<(u32, Vec3)> {
        self.texels.get((y * self.width + x) as usize).copied().flatten()
    }

    /// Texels that lie on the mesh.
    pub fn coverage(&self) -> Vec<bool> {
        self.texels.iter().map(|t| t.is_some()).collect()
    }
}

/// Rasterises the triangles' texture coordinates (U across, V up) into a
/// `width` x `height` map; where islands overlap, the later triangle wins.
pub fn rasterize(tris: &[Triangle], width: u32, height: u32) -> Raster {
    let (w, h) = (width.max(1), height.max(1));
    let mut texels = vec![None; (w * h) as usize];
    let edge = |a: Vec2, b: Vec2, c: Vec2| (b - a).perp_dot(c - a);
    for (i, t) in tris.iter().enumerate() {
        let q = t.uv.map(|uv| Vec2::new(uv.x * w as f32, (1.0 - uv.y) * h as f32));
        let area = edge(q[0], q[1], q[2]);
        if area.abs() < 1e-12 {
            continue;
        }
        let lo = q[0].min(q[1]).min(q[2]);
        let hi = q[0].max(q[1]).max(q[2]);
        let (x0, y0) = ((lo.x - 0.5).floor().max(0.0) as u32, (lo.y - 0.5).floor().max(0.0) as u32);
        let (x1, y1) = (((hi.x - 0.5).ceil().max(0.0) as u32).min(w - 1), ((hi.y - 0.5).ceil().max(0.0) as u32).min(h - 1));
        if lo.x > w as f32 || lo.y > h as f32 || hi.x < 0.0 || hi.y < 0.0 {
            continue;
        }
        let eps = -1e-5;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let c = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let b = Vec3::new(edge(q[1], q[2], c), edge(q[2], q[0], c), edge(q[0], q[1], c)) / area;
                if b.x >= eps && b.y >= eps && b.z >= eps {
                    texels[(y * w + x) as usize] = Some((i as u32, b));
                }
            }
        }
    }
    Raster { width: w, height: h, texels }
}

/// Edge padding: `pixels` rings of uncovered texels around the covered
/// ones take the average colour of their covered neighbours (8-connected).
/// `covered` grows with them.
pub fn dilate(img: &mut RgbaImage, covered: &mut [bool], pixels: u32) {
    let (w, h) = img.dimensions();
    if covered.len() != (w * h) as usize {
        return;
    }
    for _ in 0..pixels {
        let mut grown = Vec::new();
        for y in 0..h {
            for x in 0..w {
                if covered[(y * w + x) as usize] {
                    continue;
                }
                let mut sum = [0u32; 4];
                let mut n = 0u32;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if (dx == 0 && dy == 0) || nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 || !covered[(ny as u32 * w + nx as u32) as usize] {
                            continue;
                        }
                        let p = img.get_pixel(nx as u32, ny as u32).0;
                        for k in 0..4 {
                            sum[k] += p[k] as u32;
                        }
                        n += 1;
                    }
                }
                if n > 0 {
                    grown.push((x, y, sum.map(|v| ((v + n / 2) / n) as u8)));
                }
            }
        }
        if grown.is_empty() {
            break;
        }
        for (x, y, c) in grown {
            img.put_pixel(x, y, Rgba(c));
            covered[(y * w + x) as usize] = true;
        }
    }
}

/// A bounding volume hierarchy over triangles, for projection rays.
pub struct Bvh {
    tris: Vec<Triangle>,
    nodes: Vec<Node>,
    order: Vec<u32>,
}

struct Node {
    lo: Vec3,
    hi: Vec3,
    /// Leaf: `count` triangles from `start` in `order`; inner: children `start` and `start + 1`.
    start: u32,
    count: u32,
}

impl Bvh {
    pub fn new(tris: Vec<Triangle>) -> Self {
        let mut bvh = Bvh { order: (0..tris.len() as u32).collect(), tris, nodes: Vec::new() };
        if !bvh.tris.is_empty() {
            bvh.nodes.push(Node { lo: Vec3::ZERO, hi: Vec3::ZERO, start: 0, count: 0 });
            bvh.build(0, 0, bvh.tris.len());
        }
        bvh
    }

    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }

    pub fn triangle(&self, i: u32) -> &Triangle {
        &self.tris[i as usize]
    }

    fn build(&mut self, node: usize, start: usize, end: usize) {
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for &i in &self.order[start..end] {
            let (a, b) = self.tris[i as usize].bounds();
            lo = lo.min(a);
            hi = hi.max(b);
        }
        self.nodes[node].lo = lo;
        self.nodes[node].hi = hi;
        if end - start <= 4 {
            self.nodes[node].start = start as u32;
            self.nodes[node].count = (end - start) as u32;
            return;
        }
        let ext = hi - lo;
        let axis = if ext.x >= ext.y && ext.x >= ext.z { 0 } else if ext.y >= ext.z { 1 } else { 2 };
        let centre = |t: &Triangle| (t.p[0] + t.p[1] + t.p[2])[axis];
        let tris = &self.tris;
        self.order[start..end].sort_by(|a, b| centre(&tris[*a as usize]).total_cmp(&centre(&tris[*b as usize])));
        let mid = (start + end) / 2;
        let left = self.nodes.len();
        self.nodes.push(Node { lo: Vec3::ZERO, hi: Vec3::ZERO, start: 0, count: 0 });
        self.nodes.push(Node { lo: Vec3::ZERO, hi: Vec3::ZERO, start: 0, count: 0 });
        self.nodes[node].start = left as u32;
        self.build(left, start, mid);
        self.build(left + 1, mid, end);
    }

    /// Every hit of the ray `origin + t * dir` with `t` in `[t0, t1]`:
    /// (t, triangle index, barycentric coordinates). Both sides count.
    pub fn hits(&self, origin: Vec3, dir: Vec3, t0: f32, t1: f32) -> Vec<(f32, u32, Vec3)> {
        let mut out = Vec::new();
        if self.nodes.is_empty() {
            return out;
        }
        let inv = Vec3::new(1.0 / dir.x, 1.0 / dir.y, 1.0 / dir.z);
        let mut stack = vec![0usize];
        while let Some(n) = stack.pop() {
            let node = &self.nodes[n];
            if !slab(node.lo, node.hi, origin, inv, t0, t1) {
                continue;
            }
            if node.count > 0 {
                for &i in &self.order[node.start as usize..(node.start + node.count) as usize] {
                    if let Some((t, b)) = intersect(&self.tris[i as usize], origin, dir) {
                        if t >= t0 && t <= t1 {
                            out.push((t, i, b));
                        }
                    }
                }
            } else {
                stack.push(node.start as usize);
                stack.push(node.start as usize + 1);
            }
        }
        out
    }
}

fn slab(lo: Vec3, hi: Vec3, o: Vec3, inv: Vec3, t0: f32, t1: f32) -> bool {
    let (mut tmin, mut tmax) = (t0, t1);
    for k in 0..3 {
        let (a, b) = ((lo[k] - o[k]) * inv[k], (hi[k] - o[k]) * inv[k]);
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        // NaN (a ray parallel to a face of the box, starting on it) keeps the range.
        if a > tmin {
            tmin = a;
        }
        if b < tmax {
            tmax = b;
        }
        if tmin > tmax + 1e-6 {
            return false;
        }
    }
    true
}

/// Ray / triangle intersection (both sides): (t, barycentric coordinates).
fn intersect(t: &Triangle, o: Vec3, d: Vec3) -> Option<(f32, Vec3)> {
    let (e1, e2) = (t.p[1] - t.p[0], t.p[2] - t.p[0]);
    let pv = d.cross(e2);
    let det = e1.dot(pv);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let tv = o - t.p[0];
    let u = tv.dot(pv) * inv;
    let eps = 1e-6;
    if u < -eps || u > 1.0 + eps {
        return None;
    }
    let qv = tv.cross(e1);
    let v = d.dot(qv) * inv;
    if v < -eps || u + v > 1.0 + eps {
        return None;
    }
    Some((e2.dot(qv) * inv, Vec3::new(1.0 - u - v, u, v)))
}

/// Projection Options > Normal Map Space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NormalSpace {
    #[default]
    Tangent,
    World,
    Screen,
    Local,
}

/// How a normals map encodes its normals.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalOptions {
    pub space: NormalSpace,
    /// Tangent: Red Left (off: Right); the other spaces: Red -X (off: +X).
    pub red_flip: bool,
    /// Tangent: Green Up (off: Down, the reference's default); the other
    /// spaces: Green -Y (off: +Y).
    pub green_flip: bool,
    /// World to the space's axes: the object's rotation inverse for Local
    /// XYZ, the view's for Screen (identity for the others).
    pub basis: Mat3,
}

impl Default for NormalOptions {
    fn default() -> Self {
        Self { space: NormalSpace::Tangent, red_flip: false, green_flip: false, basis: Mat3::IDENTITY }
    }
}

/// Normal projection from high-resolution objects (Projection Mapping).
pub struct Projection<'a> {
    /// The high-resolution objects in world space (the working model not among them).
    pub high: &'a Bvh,
    /// Rays start at the cage, `push` above the surface along its normal
    /// (Use Cage), or `offset` above it.
    pub use_cage: bool,
    pub push: f32,
    pub offset: f32,
    /// Resolve Hit: Closest (to the surface) or Furthest (from it, nearest the cage).
    pub closest: bool,
    pub match_material_id: bool,
    /// A ray that misses takes the working model's own normal.
    pub include_working: bool,
    /// A ray that misses is baked in `miss_color` (else the working model's normal).
    pub ray_miss_check: bool,
    pub miss_color: [u8; 3],
}

/// What a texel sees: the normal to encode and the height of the hit above
/// the surface (0 without projection); Err(()) for a missed ray.
fn probe(low: &Triangle, b: Vec3, proj: Option<&Projection>) -> Result<(Vec3, f32), ()> {
    let n = low.normal(b);
    let Some(pr) = proj else { return Ok((n, 0.0)) };
    let p = low.point(b);
    let dist = if pr.use_cage { pr.push } else { pr.offset }.max(1e-4);
    let origin = p + n * dist;
    let hits = pr.high.hits(origin, -n, 0.0, 2.0 * dist);
    let hits = hits.into_iter().filter(|(_, i, _)| !pr.match_material_id || pr.high.triangle(*i).material_id == low.material_id);
    let best = if pr.closest { hits.min_by(|a, b| (a.0 - dist).abs().total_cmp(&(b.0 - dist).abs())) } else { hits.min_by(|a, b| a.0.total_cmp(&b.0)) };
    match best {
        Some((t, i, hb)) => Ok((pr.high.triangle(i).normal(hb), dist - t)),
        None if pr.include_working || !pr.ray_miss_check => Ok((n, 0.0)),
        None => Err(()),
    }
}

fn encode(v: f32) -> u8 {
    ((v.clamp(-1.0, 1.0) * 0.5 + 0.5) * 255.0).round() as u8
}

fn background(img: &mut RgbaImage, bg: [u8; 3]) {
    for p in img.pixels_mut() {
        *p = Rgba([bg[0], bg[1], bg[2], 0]);
    }
}

/// A NormalsMap: per covered texel the normal (the mesh's own, or projected
/// from the high-resolution objects) in `opts.space`, mapped to 0–255
/// (tangent space: red along U, green along V, blue out of the surface).
/// `height_alpha` = (min, max) puts the HeightMap into the alpha channel.
/// Returns the image and the range of heights the rays found (the Buffer
/// min / max Height), None without hits.
pub fn bake_normals(low: &[Triangle], raster: &Raster, proj: Option<&Projection>, opts: &NormalOptions, height_alpha: Option<(f32, f32)>, bg: [u8; 3]) -> (RgbaImage, Option<(f32, f32)>) {
    let mut img = RgbaImage::new(raster.width, raster.height);
    background(&mut img, bg);
    let mut range: Option<(f32, f32)> = None;
    for y in 0..raster.height {
        for x in 0..raster.width {
            let Some((ti, b)) = raster.get(x, y) else { continue };
            let t = &low[ti as usize];
            let (n, h) = match probe(t, b, proj) {
                Ok(v) => v,
                Err(()) => {
                    let c = proj.map_or([255, 0, 0], |p| p.miss_color);
                    img.put_pixel(x, y, Rgba([c[0], c[1], c[2], 255]));
                    continue;
                }
            };
            if proj.is_some() {
                range = Some(range.map_or((h, h), |(a, c)| (a.min(h), c.max(h))));
            }
            let v = match opts.space {
                NormalSpace::Tangent => {
                    let nn = t.normal(b);
                    let (tu, tv) = t.uv_axes().unwrap_or_else(|| (nn.any_orthonormal_vector(), nn.cross(nn.any_orthonormal_vector())));
                    let tan = (tu - nn * nn.dot(tu)).normalize_or(nn.any_orthonormal_vector());
                    let sign = if nn.cross(tan).dot(tv) < 0.0 { -1.0 } else { 1.0 };
                    let bit = nn.cross(tan) * sign;
                    Vec3::new(n.dot(tan) * if opts.red_flip { -1.0 } else { 1.0 }, n.dot(bit) * if opts.green_flip { 1.0 } else { -1.0 }, n.dot(nn))
                }
                NormalSpace::World | NormalSpace::Screen | NormalSpace::Local => {
                    let m = opts.basis * n;
                    Vec3::new(m.x * if opts.red_flip { -1.0 } else { 1.0 }, m.y * if opts.green_flip { -1.0 } else { 1.0 }, m.z)
                }
            };
            let a = height_alpha.map_or(255, |(lo, hi)| grey(h, lo, hi));
            img.put_pixel(x, y, Rgba([encode(v.x), encode(v.y), encode(v.z), a]));
        }
    }
    (img, range)
}

fn grey(h: f32, lo: f32, hi: f32) -> u8 {
    let t = if (hi - lo).abs() > 1e-12 { (h - lo) / (hi - lo) } else { 0.0 };
    (t.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A HeightMap: the height of the projected surface above the working
/// model, `min_height` black to `max_height` white (0 without projection).
pub fn bake_height(low: &[Triangle], raster: &Raster, proj: Option<&Projection>, min_height: f32, max_height: f32, bg: [u8; 3]) -> RgbaImage {
    let mut img = RgbaImage::new(raster.width, raster.height);
    background(&mut img, bg);
    for y in 0..raster.height {
        for x in 0..raster.width {
            let Some((ti, b)) = raster.get(x, y) else { continue };
            let g = match probe(&low[ti as usize], b, proj) {
                Ok((_, h)) => grey(h, min_height, max_height),
                Err(()) => 0,
            };
            img.put_pixel(x, y, Rgba([g, g, g, 255]));
        }
    }
    img
}

/// An AlphaMap: the opacity (0–1) of each texel's material, as grey and alpha.
pub fn bake_alpha(low: &[Triangle], raster: &Raster, opacity: impl Fn(u16) -> f32, bg: [u8; 3]) -> RgbaImage {
    let mut img = RgbaImage::new(raster.width, raster.height);
    background(&mut img, bg);
    for y in 0..raster.height {
        for x in 0..raster.width {
            let Some((ti, _)) = raster.get(x, y) else { continue };
            let g = (opacity(low[ti as usize].material_id).clamp(0.0, 1.0) * 255.0).round() as u8;
            img.put_pixel(x, y, Rgba([g, g, g, g]));
        }
    }
    img
}

/// The covered texels of a GPU-rendered map: alpha above 0.
pub fn alpha_coverage(img: &RgbaImage) -> Vec<bool> {
    img.pixels().map(|p| p.0[3] > 0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Mat4;

    fn mapped_plane(size: f32) -> Mesh {
        let mut m = loom_mesh::plane_segmented(size, size, 2, 2);
        loom_mesh::uvmap::auto_map(&mut m, loom_mesh::uvmap::AutoMap::Planar, 1);
        m
    }

    #[test]
    fn a_flat_plane_bakes_the_flat_tangent_normal() {
        let tris = triangles(&mapped_plane(10.0), 1);
        let raster = rasterize(&tris, 32, 32);
        let covered = raster.coverage().iter().filter(|c| **c).count();
        assert!(covered > 32 * 32 * 9 / 10, "the plane fills its map: {covered}");
        let (img, range) = bake_normals(&tris, &raster, None, &NormalOptions::default(), None, [0; 3]);
        assert_eq!(range, None);
        for (i, p) in img.pixels().enumerate() {
            if raster.texels[i].is_some() {
                assert_eq!(p.0, [128, 128, 255, 255], "texel {i}: (0.5, 0.5, 1)");
            }
        }
    }

    #[test]
    fn projection_takes_the_high_objects_normals_and_heights() {
        let low = triangles(&mapped_plane(10.0), 1);
        let raster = rasterize(&low, 16, 16);
        // A higher plane 0.5 above: flat normals, height 0.5.
        let mut high = loom_mesh::plane_segmented(12.0, 12.0, 1, 1);
        let _ = loom_mesh::edit_ops::transform_mesh(&mut high, Mat4::from_translation(Vec3::new(0.0, 0.0, 0.5)));
        let bvh = Bvh::new(triangles(&high, 1));
        let proj = Projection { high: &bvh, use_cage: true, push: 5.0, offset: 10.0, closest: false, match_material_id: false, include_working: false, ray_miss_check: true, miss_color: [255, 0, 0] };
        let (img, range) = bake_normals(&low, &raster, Some(&proj), &NormalOptions::default(), None, [0; 3]);
        let (lo, hi) = range.expect("rays hit");
        assert!((lo - 0.5).abs() < 1e-3 && (hi - 0.5).abs() < 1e-3, "{lo} {hi}");
        assert_eq!(img.get_pixel(8, 8).0, [128, 128, 255, 255]);
        let h = bake_height(&low, &raster, Some(&proj), 0.0, 1.0, [0; 3]);
        assert!((h.get_pixel(8, 8).0[0] as i32 - 128).abs() <= 1, "{:?}", h.get_pixel(8, 8));
        // Tilted 30 degrees about Y: the normal leans along U.
        let mut tilted = loom_mesh::plane_segmented(12.0, 12.0, 1, 1);
        let _ = loom_mesh::edit_ops::transform_mesh(&mut tilted, Mat4::from_translation(Vec3::new(0.0, 0.0, 0.5)) * Mat4::from_rotation_y(30f32.to_radians()));
        let bvh = Bvh::new(triangles(&tilted, 1));
        let proj = Projection { high: &bvh, ..proj };
        let (img, _) = bake_normals(&low, &raster, Some(&proj), &NormalOptions::default(), None, [0; 3]);
        let c = img.get_pixel(8, 8).0;
        assert!((c[0] as i32 - 128).abs() > 50, "red leans: {c:?}");
        assert!((c[1] as i32 - 128).abs() <= 2 && (c[2] as i32 - 238).abs() <= 2, "{c:?}");
        // Out of reach of the cage: the ray miss colour.
        let proj = Projection { push: 0.1, ..proj };
        let (img, _) = bake_normals(&low, &raster, Some(&proj), &NormalOptions::default(), None, [0; 3]);
        assert_eq!(img.get_pixel(1, 8).0, [255, 0, 0, 255]);
    }

    #[test]
    fn padding_grows_the_islands_by_whole_texels() {
        let mut img = RgbaImage::new(5, 5);
        let mut covered = vec![false; 25];
        img.put_pixel(2, 2, Rgba([200, 100, 50, 255]));
        covered[12] = true;
        dilate(&mut img, &mut covered, 1);
        assert_eq!(covered.iter().filter(|c| **c).count(), 9);
        assert_eq!(img.get_pixel(1, 1).0, [200, 100, 50, 255]);
        assert_eq!(img.get_pixel(0, 0).0[3], 0, "one ring only");
        dilate(&mut img, &mut covered, 2);
        assert!(covered.iter().all(|c| *c));
    }

    #[test]
    fn alpha_bakes_each_materials_opacity() {
        let mut m = mapped_plane(10.0);
        for (i, f) in m.faces_mut().iter_mut().enumerate() {
            f.material_id = (i % 2) as u16;
        }
        let tris = triangles(&m, 1);
        let raster = rasterize(&tris, 16, 16);
        let img = bake_alpha(&tris, &raster, |id| if id == 0 { 1.0 } else { 0.5 }, [0; 3]);
        let values: std::collections::BTreeSet<u8> = img.pixels().filter(|p| p.0[3] > 0).map(|p| p.0[0]).collect();
        assert_eq!(values.into_iter().collect::<Vec<_>>(), vec![128, 255]);
    }
}
