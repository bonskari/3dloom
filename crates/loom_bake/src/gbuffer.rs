//! What a render's camera sees, ray traced on the CPU per pixel: the depth
//! of the nearest surface (the Z Depth element) and the polygon edges a
//! Force Wireframe render draws, each found through the scene's `Bvh`.

use crate::{triangles, Bvh, Triangle};
use glam::{Mat4, Vec3, Vec4};
use loom_mesh::Mesh;

/// A triangle of a polygon: which of its edges p0-p1, p1-p2, p2-p0 are the
/// polygon's own (not the diagonals the triangulation added).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeTri {
    pub tri: Triangle,
    pub edges: [bool; 3],
}

/// `mesh`'s visible faces fan-triangulated (as `triangles` does), with their polygon edges.
pub fn edge_triangles(mesh: &Mesh) -> Vec<EdgeTri> {
    let tris = triangles(mesh, 1);
    let mut flags = Vec::with_capacity(tris.len());
    for (fi, face) in mesh.faces().iter().enumerate() {
        if face.hidden {
            continue;
        }
        let Some(corners) = mesh.face_corners(loom_mesh::FaceId(fi as u32)) else { continue };
        let n = corners.len();
        if n < 3 {
            continue;
        }
        for i in 1..n - 1 {
            let vis = |k: usize| mesh.edge_visible(corners[k]);
            flags.push([i == 1 && vis(0), vis(i), i == n - 2 && vis(n - 1)]);
        }
    }
    tris.into_iter().zip(flags).map(|(tri, edges)| EdgeTri { tri, edges }).collect()
}

/// The camera and the part of its frame an image covers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub view: Mat4,
    pub clip_from_view: Mat4,
    /// The image's rectangle in the camera's frame: u0, v0, u1, v1 (fractions, v down).
    pub rect: [f32; 4],
    pub size: [u32; 2],
}

impl View {
    /// Where image position (x, y) in pixels lies in clip space x, y.
    fn ndc(&self, x: f32, y: f32) -> (f32, f32) {
        let [u0, v0, u1, v1] = self.rect;
        let u = u0 + x / self.size[0] as f32 * (u1 - u0);
        let v = v0 + y / self.size[1] as f32 * (v1 - v0);
        (u * 2.0 - 1.0, 1.0 - v * 2.0)
    }

    /// The ray through image position (x, y): origin on the near plane,
    /// unit direction and the distance to the far plane.
    pub fn ray(&self, world_from_clip: Mat4, x: f32, y: f32) -> (Vec3, Vec3, f32) {
        let (nx, ny) = self.ndc(x, y);
        let p = |z: f32| {
            let h = world_from_clip * Vec4::new(nx, ny, z, 1.0);
            h.truncate() / h.w
        };
        let (near, far) = (p(0.0), p(1.0));
        let d = far - near;
        (near, d.normalize_or_zero(), d.length())
    }

    /// Image position in pixels of world point `p` (None behind the camera).
    pub fn project(&self, clip_from_world: Mat4, p: Vec3) -> Option<[f32; 2]> {
        let c = clip_from_world * p.extend(1.0);
        if c.w <= 1e-6 {
            return None;
        }
        let (nx, ny) = (c.x / c.w, c.y / c.w);
        let [u0, v0, u1, v1] = self.rect;
        let (u, v) = ((nx + 1.0) * 0.5, (1.0 - ny) * 0.5);
        Some([(u - u0) / (u1 - u0) * self.size[0] as f32, (v - v0) / (v1 - v0) * self.size[1] as f32])
    }
}

/// Runs `f` for every pixel on all cores, row by row.
pub fn per_pixel<T: Send + Clone>(size: [u32; 2], empty: T, f: impl Fn(u32, u32) -> T + Sync) -> Vec<T> {
    let (w, h) = (size[0] as usize, size[1] as usize);
    let mut out = vec![empty; w * h];
    if w == 0 || h == 0 {
        return out;
    }
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(h);
    let rows = h.div_ceil(threads.max(1)).max(1);
    std::thread::scope(|s| {
        for (k, chunk) in out.chunks_mut(rows * w).enumerate() {
            let f = &f;
            s.spawn(move || {
                for (j, px) in chunk.iter_mut().enumerate() {
                    *px = f((j % w) as u32, (k * rows + j / w) as u32);
                }
            });
        }
    });
    out
}

/// Per pixel centre: the camera depth (distance along the view direction)
/// of the nearest surface, None where the ray hits nothing.
pub fn depth(scene: &Bvh, view: &View) -> Vec<Option<f32>> {
    let world_from_clip = (view.clip_from_view * view.view).inverse();
    per_pixel(view.size, None, |x, y| {
        let (o, d, far) = view.ray(world_from_clip, x as f32 + 0.5, y as f32 + 0.5);
        let (t, _, _) = scene.hits(o, d, 0.0, far).into_iter().min_by(|a, b| a.0.total_cmp(&b.0))?;
        Some(-(view.view * (o + d * t).extend(1.0)).z)
    })
}

/// Per pixel centre: the nearest surface's camera depth, triangle index and
/// world normal (the G-buffer), None where the ray hits nothing.
pub fn surfaces(scene: &Bvh, view: &View) -> Vec<Option<(f32, u32, Vec3)>> {
    let world_from_clip = (view.clip_from_view * view.view).inverse();
    per_pixel(view.size, None, |x, y| {
        let (o, d, far) = view.ray(world_from_clip, x as f32 + 0.5, y as f32 + 0.5);
        let (t, tri, b) = scene.nearest(o, d, 0.0, far, |_| true)?;
        let n = scene.triangle(tri).normal(b).normalize_or_zero();
        Some((-(view.view * (o + d * t).extend(1.0)).z, tri, n))
    })
}

/// Z Depth element: depth `z_min` white to `z_max` black (and beyond),
/// nothing hit black with alpha 0.
pub fn depth_image(depth: &[Option<f32>], size: [u32; 2], z_min: f32, z_max: f32) -> image::RgbaImage {
    let mut img = image::RgbaImage::new(size[0], size[1]);
    for (p, d) in img.pixels_mut().zip(depth) {
        p.0 = match d {
            Some(z) => {
                let t = ((z - z_min) / (z_max - z_min).max(1e-6)).clamp(0.0, 1.0);
                let g = ((1.0 - t) * 255.0).round() as u8;
                [g, g, g, 255]
            }
            None => [0, 0, 0, 0],
        };
    }
    img
}

/// Force Wireframe: per pixel, how much of it (0-1, over `ss` x `ss`
/// samples) lies on a polygon edge `thickness` pixels wide, and the summed
/// colour of those samples. Each sample's ray passes through the faces
/// (wire surfaces are open) and stops at the first edge it meets; faces
/// turned away from the camera have no wires unless `two_sided`.
pub fn wire_coverage(scene: &Bvh, tris: &[EdgeTri], view: &View, thickness: f32, ss: u32, two_sided: bool, color: impl Fn(u32) -> [f32; 3] + Sync) -> Vec<(f32, [f32; 3])> {
    let ss = ss.max(1);
    let world_from_clip = (view.clip_from_view * view.view).inverse();
    let clip_from_world = view.clip_from_view * view.view;
    let half = thickness.max(0.1) * 0.5;
    per_pixel(view.size, (0.0, [0.0; 3]), |x, y| {
        let (mut hits, mut sum) = (0u32, [0.0f32; 3]);
        for sy in 0..ss {
            for sx in 0..ss {
                let (px, py) = (x as f32 + (sx as f32 + 0.5) / ss as f32, y as f32 + (sy as f32 + 0.5) / ss as f32);
                let (o, d, far) = view.ray(world_from_clip, px, py);
                let mut list = scene.hits(o, d, 0.0, far);
                list.sort_by(|a, b| a.0.total_cmp(&b.0));
                let on_wire = list.into_iter().find(|(_, i, b)| {
                    let t = &tris[*i as usize];
                    if !two_sided && t.tri.face_normal().dot(d) > 0.0 {
                        return false;
                    }
                    edge_distance(view, clip_from_world, t, *b).is_some_and(|e| e <= half)
                });
                if let Some((_, i, _)) = on_wire {
                    let c = color(i);
                    hits += 1;
                    for k in 0..3 {
                        sum[k] += c[k];
                    }
                }
            }
        }
        let n = (ss * ss) as f32;
        (hits as f32 / n, sum.map(|v| v / n))
    })
}

/// Distance in pixels from the point at barycentric `b` of `t` to its
/// nearest polygon edge, on the image (None: no edge, or behind the camera).
fn edge_distance(view: &View, clip_from_world: Mat4, t: &EdgeTri, b: Vec3) -> Option<f32> {
    let s = [view.project(clip_from_world, t.tri.p[0])?, view.project(clip_from_world, t.tri.p[1])?, view.project(clip_from_world, t.tri.p[2])?];
    let area2 = ((s[1][0] - s[0][0]) * (s[2][1] - s[0][1]) - (s[2][0] - s[0][0]) * (s[1][1] - s[0][1])).abs();
    if area2 < 1e-9 {
        return None;
    }
    let mut best: Option<f32> = None;
    // The edge opposite corner k joins corners k+1 and k+2: edges[(k + 1) % 3].
    for k in 0..3 {
        if !t.edges[(k + 1) % 3] {
            continue;
        }
        let (a, c) = (s[(k + 1) % 3], s[(k + 2) % 3]);
        let len = ((c[0] - a[0]).powi(2) + (c[1] - a[1]).powi(2)).sqrt();
        if len < 1e-6 {
            continue;
        }
        let dist = b[k].max(0.0) * area2 / len;
        best = Some(best.map_or(dist, |v: f32| v.min(dist)));
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(z: f32, half: f32) -> Mesh {
        loom_mesh::standard_primitives::build_merged(&[[[-half, -half], [half, -half], [half, half], [-half, half]].iter().map(|p| Vec3::new(p[0], p[1], z)).collect()])
    }

    fn front_view(size: u32) -> View {
        // Looking down -Z from z = 10 at the origin, 20 units across.
        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, Vec3::Y);
        View { view, clip_from_view: Mat4::orthographic_rh(-10.0, 10.0, -10.0, 10.0, 0.1, 100.0), rect: [0.0, 0.0, 1.0, 1.0], size: [size, size] }
    }

    #[test]
    fn a_quad_has_four_wire_edges_not_its_diagonal() {
        let t = edge_triangles(&quad(0.0, 5.0));
        assert_eq!(t.len(), 2);
        assert_eq!(t.iter().map(|e| e.edges.iter().filter(|x| **x).count()).sum::<usize>(), 4);
    }

    #[test]
    fn depth_finds_the_nearest_surface() {
        let tris: Vec<Triangle> = edge_triangles(&quad(0.0, 5.0)).into_iter().chain(edge_triangles(&quad(4.0, 2.0))).map(|e| e.tri).collect();
        let d = depth(&Bvh::new(tris), &front_view(20));
        assert!((d[10 * 20 + 10].unwrap() - 6.0).abs() < 1e-3, "the near quad in the middle: {:?}", d[210]);
        assert!((d[6 * 20 + 6].unwrap() - 10.0).abs() < 1e-3, "the far quad around it");
        assert_eq!(d[0], None);
        let img = depth_image(&d, [20, 20], 6.0, 10.0);
        assert_eq!((img.get_pixel(10, 10).0, img.get_pixel(6, 6).0, img.get_pixel(0, 0).0), ([255, 255, 255, 255], [0, 0, 0, 255], [0, 0, 0, 0]));
    }

    #[test]
    fn wires_lie_on_the_polygon_edges_and_show_what_is_behind() {
        // A big quad behind a small one: the small one's faces do not hide the big one's edges.
        let mut tris = edge_triangles(&quad(0.0, 9.0));
        tris.extend(edge_triangles(&quad(4.0, 5.0)));
        let bvh = Bvh::new(tris.iter().map(|e| e.tri).collect());
        let cov = wire_coverage(&bvh, &tris, &front_view(40), 1.0, 2, false, |i| if i < 2 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] });
        let at = |x: usize, y: usize| cov[y * 40 + x];
        // The small quad's edge x = -5 is at pixel 10; the big one's x = -9 at pixel 2.
        assert!(at(10, 20).0 > 0.4 && at(10, 20).1[1] > 0.4, "small quad edge: {:?}", at(10, 20));
        assert!(at(2, 20).0 > 0.4 && at(2, 20).1[0] > 0.4, "big quad edge: {:?}", at(2, 20));
        assert_eq!(at(20, 20).0, 0.0, "no wire across the faces or the diagonal");
        assert_eq!(at(6, 20).0, 0.0);
        // Seen from behind, one-sided faces have no wires; two-sided ones do.
        let mut back = front_view(40);
        back.view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, -10.0), Vec3::ZERO, Vec3::Y);
        let one = wire_coverage(&bvh, &tris, &back, 1.0, 1, false, |_| [1.0; 3]);
        assert!(one.iter().all(|c| c.0 == 0.0));
        let two = wire_coverage(&bvh, &tris, &back, 1.0, 1, true, |_| [1.0; 3]);
        assert!(two.iter().any(|c| c.0 > 0.0));
    }
}
