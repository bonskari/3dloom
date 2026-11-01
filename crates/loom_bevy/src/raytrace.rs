//! Ray tracing on the CPU for offline renders, through a `Bvh` of the
//! render's triangles: the reflections and refractions of the traced
//! materials and maps (Raytrace material and map, Reflect/Refract, Flat
//! Mirror, Thin Wall Refraction) composited into the frame Bevy rendered,
//! the Light Tracer's gathered light added to it, and the screen motion of
//! what each pixel sees (Image Motion Blur, the Velocity element).
//!
//! Units are the reference's: a light of multiplier 1 lights a white
//! surface it faces to white; colours are linear.

use glam::{Mat4, Vec3};
use loom_bake::gbuffer::{per_pixel, View};
use loom_bake::{Bvh, Triangle};
use loom_scene::light::{Emitter, LightSample};
use loom_scene::material::{TraceLook, TraceSource};

/// The light a CPU pass shades with: the lights (skylights apart), the
/// ambient light and the skylights' colour.
#[derive(Debug, Clone, Default)]
pub struct Lighting {
    pub lights: Vec<LightSample>,
    pub ambient: Vec3,
    pub sky: Vec3,
}

/// A surface as the tracer shades it.
#[derive(Debug, Clone)]
pub struct TraceSurface {
    /// Linear diffuse colour.
    pub diffuse: Vec3,
    /// Linear self-illumination (it glows this much whatever the light).
    pub glow: Vec3,
    pub trace: Option<TraceLook>,
}

/// One object of the traced scene.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraceObject {
    pub id: u64,
    /// The camera sees it (primary rays).
    pub camera: bool,
    /// Reflections and refractions see it (Visible to Reflection/Refraction,
    /// not in the Raytracer's exclude list).
    pub rays: bool,
    pub cast_shadows: bool,
    pub receive_shadows: bool,
}

/// The scene the CPU passes trace.
pub struct TraceScene {
    pub bvh: Bvh,
    /// Per triangle: its object and surface (indices into the lists).
    pub tris: Vec<(u32, u32)>,
    pub objects: Vec<TraceObject>,
    pub surfaces: Vec<TraceSurface>,
    pub lighting: Lighting,
    /// Linear background colour (what a ray that leaves the scene sees).
    pub background: Vec3,
    /// The Environment Map and its mapping (`with_environment`).
    pub environment: Option<(std::sync::Arc<image::RgbaImage>, loom_scene::environment::EnvMapping)>,
}

/// The Raytracer Global Parameters and renderer options a trace uses.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceSettings {
    pub max_depth: u32,
    pub cutoff: f32,
    /// Linear colour at Maximum Depth (None: the background).
    pub max_depth_color: Option<Vec3>,
    /// Global Ray Antialiaser: its rays per sample and the blur (degrees)
    /// of Raytrace reflections and refractions.
    pub antialias: bool,
    pub samples: u32,
    pub blur: f32,
    pub self_reflect: bool,
    /// Auto Reflect/Refract and Mirrors (off: those maps are left out) and their Rendering Iterations.
    pub auto_reflect: bool,
    pub reflect_iterations: u32,
    /// The lights cast shadows.
    pub shadows: bool,
    /// Samples per pixel side of the primary rays.
    pub pixel_samples: u32,
}

impl Default for TraceSettings {
    fn default() -> Self {
        Self { max_depth: 9, cutoff: 0.05, max_depth_color: None, antialias: false, samples: 8, blur: 0.0, self_reflect: true, auto_reflect: true, reflect_iterations: 1, shadows: true, pixel_samples: 1 }
    }
}

/// Per output pixel, what the traced reflections and refractions do to it:
/// the share of the rendered colour that stays, and the traced colour added
/// (reflection and refraction apart, for their render elements).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TracedPixel {
    pub keep: Vec3,
    pub reflection: Vec3,
    pub refraction: Vec3,
    /// Share (0-1) of the pixel's samples that met a traced surface.
    pub coverage: f32,
}

/// A small random number stream (xorshift) per pixel and sample.
struct Rng(u32);

impl Rng {
    fn new(a: u32, b: u32, c: u32) -> Self {
        let mut s = a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B) ^ c.wrapping_mul(0xC2B2_AE35) ^ 0x2718_2818;
        s ^= s >> 16;
        s = s.wrapping_mul(0x7FEB_352D);
        s ^= s >> 15;
        Rng(s.max(1))
    }

    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// A direction within `half_angle` (radians) of unit `axis`: uniform over the
/// cone's solid angle for `cosine` false, else cosine weighted about the axis.
fn cone(axis: Vec3, half_angle: f32, cosine: bool, rng: &mut Rng) -> Vec3 {
    if half_angle <= 0.0 {
        return axis;
    }
    let (u, v) = (rng.next(), rng.next());
    let s = half_angle.min(std::f32::consts::FRAC_PI_2).sin();
    let (sin_t, cos_t) = if cosine {
        // sin²θ uniform in [0, sin²α]: cosine weighted inside the cone.
        let s2 = u * s * s;
        (s2.sqrt(), (1.0 - s2).max(0.0).sqrt())
    } else {
        let cos_a = half_angle.cos();
        let c = 1.0 - u * (1.0 - cos_a);
        ((1.0 - c * c).max(0.0).sqrt(), c)
    };
    let phi = v * std::f32::consts::TAU;
    let a = axis.any_orthonormal_vector();
    let b = axis.cross(a);
    (a * (sin_t * phi.cos()) + b * (sin_t * phi.sin()) + axis * cos_t).normalize_or(axis)
}

fn reflect(d: Vec3, n: Vec3) -> Vec3 {
    d - n * (2.0 * d.dot(n))
}

/// Refraction of `d` at normal `n` (facing against `d`) with `eta` = n1 / n2;
/// None at total internal reflection.
fn refract(d: Vec3, n: Vec3, eta: f32) -> Option<Vec3> {
    let cos_i = -d.dot(n);
    let k = 1.0 - eta * eta * (1.0 - cos_i * cos_i);
    (k >= 0.0).then(|| (d * eta + n * (eta * cos_i - k.sqrt())).normalize_or(d))
}

/// Schlick's Fresnel reflectance at incidence cosine `cos` for `ior`.
fn schlick(cos: f32, ior: f32) -> f32 {
    let r0 = ((ior - 1.0) / (ior + 1.0)).powi(2);
    r0 + (1.0 - r0) * (1.0 - cos.clamp(0.0, 1.0)).powi(5)
}

/// A hit: where, the triangle, the shading normal facing the ray, and
/// whether the ray came from the triangle's back.
struct Hit {
    p: Vec3,
    tri: u32,
    n: Vec3,
    /// Geometric normal, facing the ray.
    ng: Vec3,
    inside: bool,
    t: f32,
}

impl TraceScene {
    /// The scene from `triangles` with, per triangle, its object and surface.
    pub fn new(triangles: Vec<Triangle>, tris: Vec<(u32, u32)>, objects: Vec<TraceObject>, surfaces: Vec<TraceSurface>, lighting: Lighting, background: Vec3) -> Self {
        Self { bvh: Bvh::new(triangles), tris, objects, surfaces, lighting, background, environment: None }
    }

    /// The Environment Map that rays leaving the scene see (Spherical,
    /// Cylindrical or Shrink-Wrap; a Screen map is the background colour to them).
    pub fn with_environment(mut self, map: std::sync::Arc<image::RgbaImage>, mapping: loom_scene::environment::EnvMapping) -> Self {
        if mapping != loom_scene::environment::EnvMapping::Screen {
            self.environment = Some((map, mapping));
        }
        self
    }

    /// What a ray leaving the scene along `d` sees.
    fn sky(&self, d: Vec3) -> Vec3 {
        let Some((img, mapping)) = &self.environment else { return self.background };
        use loom_scene::environment::EnvMapping;
        use std::f32::consts::PI;
        let d = d.normalize_or_zero();
        let az = d.y.atan2(d.x);
        let uv = match mapping {
            EnvMapping::Spherical => [0.5 - az / (2.0 * PI), 0.5 - d.z.clamp(-1.0, 1.0).asin() / PI],
            EnvMapping::Cylindrical => [0.5 - az / (2.0 * PI), 0.5 - (d.z / d.truncate().length().max(1e-4)).clamp(-1.0, 1.0) * 0.5],
            _ => {
                let a = d.z.clamp(-1.0, 1.0).acos() / PI;
                [0.5 + 0.5 * a * az.cos(), 0.5 - 0.5 * a * az.sin()]
            }
        };
        let (w, h) = img.dimensions();
        if w == 0 || h == 0 {
            return self.background;
        }
        let x = ((uv[0].rem_euclid(1.0) * w as f32) as u32).min(w - 1);
        let y = ((uv[1].clamp(0.0, 1.0) * h as f32) as u32).min(h - 1);
        let p = img.get_pixel(x, y).0;
        let lin = |c: u8| crate::environment::srgb_to_linear(c as f32 / 255.0);
        Vec3::new(lin(p[0]), lin(p[1]), lin(p[2]))
    }

    fn object(&self, tri: u32) -> &TraceObject {
        &self.objects[self.tris[tri as usize].0 as usize]
    }

    fn surface(&self, tri: u32) -> &TraceSurface {
        &self.surfaces[self.tris[tri as usize].1 as usize]
    }

    /// Whether any surface traces anything with `s` (so a render needs the pass).
    pub fn traces(&self, s: &TraceSettings) -> bool {
        self.surfaces.iter().any(|f| f.trace.as_ref().is_some_and(|t| usable(t, s).is_some()))
    }

    fn hit(&self, o: Vec3, d: Vec3, t0: f32, keep: impl Fn(u32) -> bool) -> Option<Hit> {
        let (t, tri, b) = self.bvh.nearest(o, d, t0, f32::MAX, keep)?;
        let tr = self.bvh.triangle(tri);
        let ng0 = tr.face_normal();
        let inside = ng0.dot(d) > 0.0;
        let ng = if inside { -ng0 } else { ng0 };
        let mut n = tr.normal(b).normalize_or(ng0);
        if n.dot(ng) < 0.0 {
            n = -n;
        }
        Some(Hit { p: o + d * t, tri, n, ng, inside, t })
    }

    /// The camera's nearest visible surface along a primary ray.
    fn primary(&self, o: Vec3, d: Vec3, far: f32) -> Option<Hit> {
        self.hit(o, d, 0.0, |i| self.object(i).camera).filter(|h| h.t <= far)
    }

    /// Light reaching `p` (normal `n`) from the lights, shadowed by casting
    /// objects when the receiver takes shadows: irradiance in the reference's units.
    fn direct(&self, p: Vec3, n: Vec3, receive: bool, shadows: bool) -> Vec3 {
        let eps = 1e-3 * (1.0 + p.abs().max_element());
        let mut e = Vec3::ZERO;
        for l in &self.lighting.lights {
            let (to, dist) = match l.emitter {
                Emitter::Directional => (-l.direction, f32::MAX),
                Emitter::Ambient => continue,
                _ => {
                    let v = l.position - p;
                    let d = v.length();
                    if d <= 1e-6 {
                        continue;
                    }
                    (v / d, d)
                }
            };
            let cos = n.dot(to);
            if cos <= 0.0 {
                continue;
            }
            let k = l.falloff(if dist == f32::MAX { 0.0 } else { dist }, (-to).dot(l.direction));
            if k <= 0.0 {
                continue;
            }
            let mut c = l.color * (k * cos);
            if shadows && receive && l.shadows {
                let far = if dist == f32::MAX { f32::MAX } else { dist - eps };
                if self.bvh.any(p + n * eps, to, 0.0, far, |i| self.object(i).cast_shadows) {
                    c *= l.shadow_transmission;
                }
            }
            e += c;
        }
        e
    }

    /// What a ray from `o` along `d` sees, `depth` reflections / refractions
    /// deep (up to `limit`), its share of the pixel being `weight`; `from`
    /// is the object it leaves (Self Reflect/Refract off: not seen again by a reflection).
    #[allow(clippy::too_many_arguments)]
    fn trace(&self, o: Vec3, d: Vec3, depth: u32, limit: u32, weight: f32, skip: Option<u32>, s: &TraceSettings, rng: &mut Rng) -> Vec3 {
        if depth > limit {
            return s.max_depth_color.unwrap_or(self.background);
        }
        if weight < s.cutoff {
            return Vec3::ZERO;
        }
        let eps = 1e-4 * (1.0 + o.abs().max_element());
        let Some(h) = self.hit(o, d, eps, |i| {
            let (obj, _) = self.tris[i as usize];
            self.objects[obj as usize].rays && skip != Some(obj)
        }) else {
            return self.sky(d);
        };
        self.shade(&h, d, depth, limit, weight, s, rng)
    }

    /// The colour of a hit seen along `d`: lit diffuse, glow, and its own
    /// traced reflection and refraction.
    #[allow(clippy::too_many_arguments)]
    fn shade(&self, h: &Hit, d: Vec3, depth: u32, limit: u32, weight: f32, s: &TraceSettings, rng: &mut Rng) -> Vec3 {
        let surf = self.surface(h.tri);
        let obj = self.object(h.tri);
        let lit = self.direct(h.p, h.n, obj.receive_shadows, s.shadows) + self.lighting.ambient + self.lighting.sky;
        let base = surf.diffuse * lit + surf.glow;
        match surf.trace.as_ref().and_then(|t| usable(t, s)) {
            Some(t) => {
                let (keep, refl, refr) = self.traced(h, d, &t, depth, limit, weight, s, rng);
                base * keep + refl + refr
            }
            None => base,
        }
    }

    /// At hit `h` of a traced surface: the share of its own shading that
    /// stays, and the reflected and refracted colour it adds.
    #[allow(clippy::too_many_arguments)]
    fn traced(&self, h: &Hit, d: Vec3, t: &TraceLook, depth: u32, limit: u32, weight: f32, s: &TraceSettings, rng: &mut Rng) -> (Vec3, Vec3, Vec3) {
        let obj_index = self.tris[h.tri as usize].0;
        let eps = 1e-3 * (1.0 + h.p.abs().max_element());
        let cos_i = (-d).dot(h.n).clamp(0.0, 1.0);
        let mut keep = Vec3::ONE;
        let (mut refl, mut refr) = (Vec3::ZERO, Vec3::ZERO);
        // Rays per lookup: the blur's samples, one when sharp.
        let blur_of = |source: TraceSource, own: f32| match source {
            TraceSource::Raytrace => {
                if s.antialias {
                    s.blur
                } else {
                    0.0
                }
            }
            _ => own,
        };
        let next_limit = |source: TraceSource| if source == TraceSource::ReflectRefract { limit.min(depth + s.reflect_iterations.max(1)) } else { limit };
        let mut r = t.reflect;
        if t.fresnel {
            r *= schlick(cos_i, t.ior);
        }
        // Refraction first: what it passes is what the reflection then covers.
        if t.refract != Vec3::ZERO {
            let blur = blur_of(t.refract_source, t.refract_blur).to_radians();
            let n_rays = if blur > 0.0 { s.samples.max(1) } else { 1 };
            let w = weight * t.refract.max_element();
            let mut sum = Vec3::ZERO;
            for _ in 0..n_rays {
                let c = match t.refract_source {
                    TraceSource::ThinWall => {
                        // Through a thin wall: the ray goes on parallel, offset by the wall's thickness.
                        let bent = refract(d, h.n, 1.0 / t.ior.max(1e-3)).unwrap_or(d);
                        let cos_t = (-bent).dot(h.n).abs().max(0.05);
                        let o = h.p + bent * (t.thickness / cos_t) - h.ng * eps;
                        let dir = cone(d, blur, false, rng);
                        self.trace(o, dir, depth + 1, next_limit(t.refract_source), w, None, s, rng)
                    }
                    _ => {
                        let eta = if h.inside { t.ior } else { 1.0 / t.ior.max(1e-3) };
                        match refract(d, h.n, eta) {
                            Some(dir) => {
                                let dir = cone(dir, blur, false, rng);
                                self.trace(h.p - h.ng * eps, dir, depth + 1, next_limit(t.refract_source), w, None, s, rng)
                            }
                            // Total internal reflection: the light stays inside.
                            None => {
                                let dir = cone(reflect(d, h.n), blur, false, rng);
                                self.trace(h.p + h.ng * eps, dir, depth + 1, next_limit(t.refract_source), w, None, s, rng)
                            }
                        }
                    }
                };
                sum += c;
            }
            refr = t.refract * (sum / n_rays as f32);
            keep *= Vec3::ONE - t.refract;
        }
        if r != Vec3::ZERO {
            let blur = blur_of(t.reflect_source, t.reflect_blur).to_radians();
            let n_rays = if blur > 0.0 { s.samples.max(1) } else { 1 };
            // Flat Mirror reflects about the face's own plane.
            let n = if t.reflect_source == TraceSource::FlatMirror { h.ng } else { h.n };
            let ideal = reflect(d, n);
            let w = weight * r.max_element();
            let skip = (!s.self_reflect).then_some(obj_index);
            let mut sum = Vec3::ZERO;
            for _ in 0..n_rays {
                let mut dir = cone(ideal, blur, false, rng);
                if dir.dot(h.ng) <= 0.0 {
                    dir = ideal;
                }
                sum += self.trace(h.p + h.ng * eps, dir, depth + 1, next_limit(t.reflect_source), w, skip, s, rng);
            }
            refl = r * (sum / n_rays as f32);
            if !t.reflect_adds {
                // The Raytrace material's Reflect replaces that share of the rest.
                keep *= Vec3::ONE - r;
                refr *= Vec3::ONE - r;
            }
        }
        (keep, refl, refr)
    }

    /// Reflections and refractions per output pixel of `view`: what stays of
    /// the rendered colour and what the tracer adds (only pixels whose
    /// samples meet a traced surface change).
    pub fn reflections(&self, view: &View, s: &TraceSettings) -> Vec<TracedPixel> {
        let world_from_clip = (view.clip_from_view * view.view).inverse();
        let ss = s.pixel_samples.max(1);
        per_pixel(view.size, TracedPixel { keep: Vec3::ONE, ..Default::default() }, |x, y| {
            let mut px = TracedPixel::default();
            let mut hits = 0u32;
            for k in 0..ss * ss {
                let (sx, sy) = (k % ss, k / ss);
                let mut rng = Rng::new(x, y, k);
                let (o, d, far) = view.ray(world_from_clip, x as f32 + (sx as f32 + 0.5) / ss as f32, y as f32 + (sy as f32 + 0.5) / ss as f32);
                let traced = self.primary(o, d, far).and_then(|h| {
                    let t = self.surface(h.tri).trace.as_ref().and_then(|t| usable(t, s))?;
                    Some(self.traced(&h, d, &t, 0, s.max_depth, 1.0, s, &mut rng))
                });
                match traced {
                    Some((keep, refl, refr)) => {
                        px.keep += keep;
                        px.reflection += refl;
                        px.refraction += refr;
                        hits += 1;
                    }
                    None => px.keep += Vec3::ONE,
                }
            }
            let n = (ss * ss) as f32;
            TracedPixel { keep: px.keep / n, reflection: px.reflection / n, refraction: px.refraction / n, coverage: hits as f32 / n }
        })
    }

    /// The Light Tracer's light per output pixel of `view`: per pixel the
    /// diffuse colour of what the camera sees and the light gathered there
    /// (None where it sees nothing), before the multipliers and filter.
    pub fn gather(&self, view: &View, t: &TracerSettings) -> Vec<Option<(Vec3, Vec3)>> {
        let world_from_clip = (view.clip_from_view * view.view).inverse();
        per_pixel(view.size, None, |x, y| {
            let (o, d, far) = view.ray(world_from_clip, x as f32 + 0.5, y as f32 + 0.5);
            let h = self.primary(o, d, far)?;
            let mut rng = Rng::new(x, y, 7919);
            let albedo = self.surface(h.tri).diffuse;
            Some((albedo, self.gathered(h.p, h.n, h.ng, t.bounces, t.rays.max(1), t, &mut rng)))
        })
    }

    /// The light arriving at `p` (normal `n`) over the cone around it:
    /// the skylights where rays leave the scene and, with bounces left, the
    /// light the surfaces they meet reflect.
    #[allow(clippy::too_many_arguments)]
    fn gathered(&self, p: Vec3, n: Vec3, ng: Vec3, bounces: u32, rays: u32, t: &TracerSettings, rng: &mut Rng) -> Vec3 {
        let half = t.cone_angle.clamp(33.0, 90.0).to_radians();
        let bias = t.ray_bias.max(1e-4);
        let mut sum = Vec3::ZERO;
        for _ in 0..rays {
            let mut dir = cone(n, half, true, rng);
            if dir.dot(ng) <= 0.0 {
                dir = n;
            }
            let hit = self.hit(p + ng * bias, dir, 0.0, |i| self.object(i).rays);
            sum += match hit {
                None => t.sky,
                Some(h) if bounces > 0 => {
                    let surf = self.surface(h.tri);
                    let grey = Vec3::splat((surf.diffuse.x + surf.diffuse.y + surf.diffuse.z) / 3.0);
                    let albedo = (grey + (surf.diffuse - grey) * t.color_bleed).max(Vec3::ZERO);
                    let mut e = self.direct(h.p, h.n, self.object(h.tri).receive_shadows, t.shadows);
                    if bounces > 1 {
                        e += self.gathered(h.p, h.n, h.ng, bounces - 1, (rays / 8).max(4), t, rng);
                    }
                    albedo * e * t.object_multiplier + surf.glow
                }
                Some(_) => Vec3::ZERO,
            };
        }
        sum / rays as f32
    }

    /// Ray-traced shadows of light `l` per output pixel of `view` (`ss`²
    /// samples per pixel): the share of the light that reaches what the
    /// camera sees, 1 unshadowed (and where nothing is seen, or the light
    /// does not reach anyway), traced to `shadow.samples` points over the
    /// light's emitting disk of `shadow.radius` and through its shadow
    /// colour and density.
    pub fn shadows(&self, view: &View, l: &LightSample, shadow: loom_scene::light::TracedShadow, ss: u32) -> Vec<Vec3> {
        let world_from_clip = (view.clip_from_view * view.view).inverse();
        let ss = ss.max(1);
        let samples = shadow.samples.max(1);
        per_pixel(view.size, Vec3::ONE, |x, y| {
            let mut sum = Vec3::ZERO;
            for k in 0..ss * ss {
                let (sx, sy) = (k % ss, k / ss);
                let (o, d, far) = view.ray(world_from_clip, x as f32 + (sx as f32 + 0.5) / ss as f32, y as f32 + (sy as f32 + 0.5) / ss as f32);
                let Some(h) = self.primary(o, d, far) else {
                    sum += Vec3::ONE;
                    continue;
                };
                if !self.object(h.tri).receive_shadows {
                    sum += Vec3::ONE;
                    continue;
                }
                let eps = 1e-3 * (1.0 + h.p.abs().max_element());
                let p = h.p + h.ng * eps;
                let mut rng = Rng::new(x, y, k + 31);
                let mut lit = 0.0f32;
                for i in 0..samples {
                    // A point of the emitting disk, facing the shaded point.
                    let (to_light, dist) = match l.emitter {
                        Emitter::Directional => {
                            let axis = -l.direction;
                            let spread = if shadow.radius > 0.0 { (shadow.radius / l.target_distance.max(1e-3)).atan() } else { 0.0 };
                            (cone(axis, spread, false, &mut rng), f32::MAX)
                        }
                        _ => {
                            let c = l.position - p;
                            let mut q = l.position;
                            if shadow.radius > 0.0 {
                                let axis = c.normalize_or(Vec3::Z);
                                let (a, b) = (axis.any_orthonormal_vector(), axis.cross(axis.any_orthonormal_vector()));
                                // Spread over the disk (golden angle spiral, turned per pixel).
                                let r = shadow.radius * ((i as f32 + 0.5) / samples as f32).sqrt();
                                let t = i as f32 * 2.399_963 + rng.next() * std::f32::consts::TAU;
                                q += (a * t.cos() + b * t.sin()) * r;
                            }
                            let v = q - p;
                            let len = v.length();
                            (v / len.max(1e-6), len)
                        }
                    };
                    if h.n.dot(to_light) <= 0.0 {
                        continue;
                    }
                    let far = if dist == f32::MAX { f32::MAX } else { dist - eps };
                    let blocked = self.bvh.any(p, to_light, 0.0, far, |t| {
                        let o = self.object(t);
                        o.cast_shadows && !l.exclusion.excludes(loom_scene::NodeId(o.id), false)
                    });
                    if !blocked {
                        lit += 1.0;
                    }
                }
                // Points facing away from the light get none of it anyway.
                let facing = l.emitter == Emitter::Directional && h.n.dot(-l.direction) <= 0.0;
                let open = if facing { 1.0 } else { lit / samples as f32 };
                sum += Vec3::splat(open) + l.shadow_transmission * (1.0 - open);
            }
            sum / (ss * ss) as f32
        })
    }

    /// The nearest surface the camera sees through each output pixel's centre:
    /// (object index, world point), None where it sees nothing.
    pub fn visible(&self, view: &View) -> Vec<Option<(u32, Vec3)>> {
        let world_from_clip = (view.clip_from_view * view.view).inverse();
        per_pixel(view.size, None, |x, y| {
            let (o, d, far) = view.ray(world_from_clip, x as f32 + 0.5, y as f32 + 0.5);
            let h = self.primary(o, d, far)?;
            Some((self.tris[h.tri as usize].0, h.p))
        })
    }
}

/// The traced look as the settings let it render (None: nothing traced):
/// Auto Reflect/Refract and Mirrors off leaves the Reflect/Refract and
/// Flat Mirror maps out.
fn usable(t: &TraceLook, s: &TraceSettings) -> Option<TraceLook> {
    let mut t = t.clone();
    let auto = |src: TraceSource| matches!(src, TraceSource::ReflectRefract | TraceSource::FlatMirror);
    if !s.auto_reflect && auto(t.reflect_source) {
        t.reflect = Vec3::ZERO;
    }
    if !s.auto_reflect && auto(t.refract_source) {
        t.refract = Vec3::ZERO;
    }
    (t.reflect != Vec3::ZERO || t.refract != Vec3::ZERO).then_some(t)
}

/// The Light Tracer's parameters a gather uses.
#[derive(Debug, Clone, PartialEq)]
pub struct TracerSettings {
    pub rays: u32,
    pub bounces: u32,
    pub cone_angle: f32,
    pub ray_bias: f32,
    pub color_bleed: f32,
    pub object_multiplier: f32,
    /// Linear sky colour the rays that leave the scene gather (zero: none).
    pub sky: Vec3,
    pub shadows: bool,
}

/// The Light Tracer's light blurred over `radius` pixels (Filter Size):
/// each pixel averages the gathered light of the pixels within the radius
/// that see something.
pub fn filter_gathered(g: &[Option<(Vec3, Vec3)>], size: [u32; 2], radius: f32) -> Vec<Option<(Vec3, Vec3)>> {
    let r = radius.max(0.0).floor() as i64;
    if r < 1 {
        return g.to_vec();
    }
    let (w, h) = (size[0] as i64, size[1] as i64);
    per_pixel(size, None, |x, y| {
        let (albedo, _) = g[(y as i64 * w + x as i64) as usize]?;
        let (mut sum, mut n) = (Vec3::ZERO, 0.0f32);
        for dy in -r..=r {
            for dx in -r..=r {
                let (px, py) = (x as i64 + dx, y as i64 + dy);
                if px < 0 || py < 0 || px >= w || py >= h || (dx * dx + dy * dy) as f32 > radius * radius {
                    continue;
                }
                if let Some((_, l)) = g[(py * w + px) as usize] {
                    sum += l;
                    n += 1.0;
                }
            }
        }
        Some((albedo, sum / n.max(1.0)))
    })
}

/// Linear value of an sRGB byte.
pub fn to_linear(c: u8) -> f32 {
    crate::bake::srgb_to_linear(c)
}

/// sRGB byte of a linear value (0-1).
pub fn to_srgb(c: f32) -> u8 {
    crate::bake::linear_to_srgb(c)
}

/// Screen motion per output pixel (pixels over the shutter) of what the
/// camera sees: `visible` from `TraceScene::visible`, `motion[object]` the
/// world transform taking a point at the frame to where it is at the
/// shutter's start and end (None: it does not blur), projected by `view`.
pub fn velocities(visible: &[Option<(u32, Vec3)>], motion: &[Option<(Mat4, Mat4)>], view: &View) -> Vec<Option<[f32; 2]>> {
    let clip_from_world = view.clip_from_view * view.view;
    visible
        .iter()
        .map(|v| {
            let (obj, p) = (*v)?;
            let (a, b) = motion.get(obj as usize).copied().flatten()?;
            let p0 = view.project(clip_from_world, a.transform_point3(p))?;
            let p1 = view.project(clip_from_world, b.transform_point3(p))?;
            Some([p1[0] - p0[0], p1[1] - p0[1]])
        })
        .collect()
}

/// Image Motion Blur: each pixel of a moving object smeared along its
/// motion `vel` (pixels over the shutter, centred on the pixel) over
/// `behind`, the frame without those objects; pixels with no motion keep
/// `img`. Colours add up in linear light; alpha with them.
pub fn smear(img: &image::RgbaImage, behind: &image::RgbaImage, vel: &[Option<[f32; 2]>]) -> image::RgbaImage {
    let (w, h) = img.dimensions();
    let n = (w * h) as usize;
    // Per pixel: premultiplied linear RGB, alpha, and the coverage of the smeared objects.
    let mut acc = vec![[0.0f32; 5]; n];
    let lin = |p: [u8; 4]| [to_linear(p[0]), to_linear(p[1]), to_linear(p[2]), p[3] as f32 / 255.0];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let Some(v) = vel[i] else { continue };
            let c = lin(img.get_pixel(x, y).0);
            let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
            let taps = (len.ceil() as u32 + 1).clamp(1, 512);
            let wt = 1.0 / taps as f32;
            for k in 0..taps {
                let f = if taps == 1 { 0.0 } else { k as f32 / (taps - 1) as f32 - 0.5 };
                let (px, py) = ((x as f32 + 0.5 + v[0] * f).floor(), (y as f32 + 0.5 + v[1] * f).floor());
                if px < 0.0 || py < 0.0 || px >= w as f32 || py >= h as f32 {
                    continue;
                }
                let a = &mut acc[(py as u32 * w + px as u32) as usize];
                for (ak, ck) in a.iter_mut().zip(&c[..3]) {
                    *ak += ck * c[3] * wt;
                }
                a[3] += c[3] * wt;
                a[4] += wt;
            }
        }
    }
    image::RgbaImage::from_fn(w, h, |x, y| {
        let i = (y * w + x) as usize;
        let a = acc[i];
        // Where no smeared object reaches, the frame stays as rendered.
        if a[4] <= 0.0 && vel[i].is_none() {
            return *img.get_pixel(x, y);
        }
        let cover = a[4].min(1.0);
        let b = lin(behind.get_pixel(x, y).0);
        let alpha = (a[3] + b[3] * (1.0 - cover)).clamp(0.0, 1.0);
        let rgb: [u8; 3] = std::array::from_fn(|k| {
            let premul = a[k] + b[k] * b[3] * (1.0 - cover);
            to_srgb(if alpha > 1e-6 { premul / alpha } else { b[k] })
        });
        image::Rgba([rgb[0], rgb[1], rgb[2], (alpha * 255.0).round() as u8])
    })
}

/// Image Motion Blur on the environment: each background pixel (`vel`
/// Some) becomes the average of the background along its motion.
pub fn smear_background(img: &image::RgbaImage, vel: &[Option<[f32; 2]>]) -> image::RgbaImage {
    let (w, h) = img.dimensions();
    image::RgbaImage::from_fn(w, h, |x, y| {
        let own = *img.get_pixel(x, y);
        let Some(v) = vel[(y * w + x) as usize] else { return own };
        let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
        let taps = (len.ceil() as u32 + 1).clamp(1, 512);
        let (mut sum, mut n) = (Vec3::ZERO, 0.0f32);
        for k in 0..taps {
            let f = if taps == 1 { 0.0 } else { k as f32 / (taps - 1) as f32 - 0.5 };
            let (px, py) = ((x as f32 + 0.5 + v[0] * f).floor(), (y as f32 + 0.5 + v[1] * f).floor());
            if px < 0.0 || py < 0.0 || px >= w as f32 || py >= h as f32 {
                continue;
            }
            let p = img.get_pixel(px as u32, py as u32).0;
            // Only the background takes part (objects stay sharp in front of it).
            if p[3] == 0 {
                sum += Vec3::new(to_linear(p[0]), to_linear(p[1]), to_linear(p[2]));
                n += 1.0;
            }
        }
        if n == 0.0 {
            return own;
        }
        let c = sum / n;
        image::Rgba([to_srgb(c.x), to_srgb(c.y), to_srgb(c.z), own.0[3]])
    })
}

/// The Velocity element: screen motion as colour, red the horizontal and
/// green the vertical motion (0.5 none, 0 and 1 the largest either way),
/// alpha where something moves or is seen.
pub fn velocity_image(vel: &[Option<[f32; 2]>], seen: &[bool], size: [u32; 2]) -> image::RgbaImage {
    let max = vel.iter().flatten().map(|v| v[0].abs().max(v[1].abs())).fold(0.0f32, f32::max).max(1e-6);
    image::RgbaImage::from_fn(size[0], size[1], |x, y| {
        let i = (y * size[0] + x) as usize;
        let v = vel[i].unwrap_or([0.0; 2]);
        let c = |m: f32| ((0.5 + 0.5 * m / max).clamp(0.0, 1.0) * 255.0).round() as u8;
        image::Rgba([c(v[0]), c(v[1]), 0, if seen[i] { 255 } else { 0 }])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;

    fn quad(c: Vec3, u: Vec3, v: Vec3) -> Vec<Triangle> {
        let (a, b, cc, d) = (c - u - v, c + u - v, c + u + v, c - u + v);
        let n = u.cross(v).normalize();
        let t = |p: [Vec3; 3]| Triangle { p, n: [n; 3], uv: [Vec2::ZERO; 3], material_id: 1 };
        vec![t([a, b, cc]), t([a, cc, d])]
    }

    fn object(id: u64) -> TraceObject {
        TraceObject { id, camera: true, rays: true, cast_shadows: true, receive_shadows: true }
    }

    fn plain(c: Vec3) -> TraceSurface {
        TraceSurface { diffuse: c, glow: Vec3::ZERO, trace: None }
    }

    fn mirror() -> TraceLook {
        TraceLook { reflect: Vec3::ONE, reflect_source: TraceSource::Raytrace, reflect_adds: false, reflect_blur: 0.0, refract: Vec3::ZERO, refract_source: TraceSource::Raytrace, refract_blur: 0.0, ior: 1.5, thickness: 0.0, fresnel: false }
    }

    /// Looking down -Z from z = 10, 20 units across, orthographic.
    fn front(size: u32) -> View {
        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, Vec3::Y);
        View { view, clip_from_view: Mat4::orthographic_rh(-10.0, 10.0, -10.0, 10.0, 0.1, 100.0), rect: [0.0, 0.0, 1.0, 1.0], size: [size, size] }
    }

    fn lit(ambient: f32) -> Lighting {
        Lighting { lights: Vec::new(), ambient: Vec3::splat(ambient), sky: Vec3::ZERO }
    }

    /// A mirror tilted 45° facing the camera and a red wall beside it: the
    /// mirror's pixels take the wall's colour; elsewhere nothing changes.
    #[test]
    fn a_mirror_reflects_what_is_beside_it() {
        let s2 = std::f32::consts::FRAC_1_SQRT_2;
        // Mirror at the origin facing (+X, +Z)/√2: the camera's -Z rays go to +X.
        let mut tris = quad(Vec3::ZERO, Vec3::new(s2 * 4.0, 0.0, -s2 * 4.0), Vec3::new(0.0, 4.0, 0.0));
        let mut info = vec![(0, 0); 2];
        // A red wall at x = 30 facing -X (outside the camera's view).
        tris.extend(quad(Vec3::new(30.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 20.0), Vec3::new(0.0, 20.0, 0.0)));
        info.extend([(1, 1); 2]);
        let mut m = plain(Vec3::splat(0.5));
        m.trace = Some(mirror());
        let scene = TraceScene::new(tris, info, vec![object(1), object(2)], vec![m, plain(Vec3::new(1.0, 0.0, 0.0))], lit(1.0), Vec3::ZERO);
        let px = scene.reflections(&front(20), &TraceSettings::default());
        let centre = px[10 * 20 + 10];
        assert!(centre.reflection.x > 0.9 && centre.reflection.y < 0.05, "the red wall in the mirror: {centre:?}");
        assert!(centre.keep.x < 0.05, "the mirror replaces its own colour: {centre:?}");
        assert_eq!(px[0].keep, Vec3::ONE, "off the mirror nothing changes");
        // Maximum Depth 0: the mirror returns the max depth colour.
        let s = TraceSettings { max_depth: 0, max_depth_color: Some(Vec3::new(0.0, 0.0, 1.0)), ..Default::default() };
        let c = scene.reflections(&front(20), &s)[210];
        assert!(c.reflection.z > 0.9 && c.reflection.x < 0.05, "{c:?}");
        // Excluded from the rays (Visible to Reflection/Refraction off): the background.
        let mut excluded = scene;
        excluded.objects[1].rays = false;
        let c = excluded.reflections(&front(20), &TraceSettings::default())[210];
        assert_eq!(c.reflection, Vec3::ZERO);
    }

    /// A mirror facing the camera reflects the zenith: with a Spherical
    /// Environment Map that is the map's top row, not the background colour.
    #[test]
    fn reflections_leaving_the_scene_see_the_environment_map() {
        let tris = quad(Vec3::ZERO, Vec3::new(4.0, 0.0, 0.0), Vec3::new(0.0, 4.0, 0.0));
        let mut m = plain(Vec3::splat(0.5));
        m.trace = Some(mirror());
        let scene = || TraceScene::new(tris.clone(), vec![(0, 0); 2], vec![object(1)], vec![m.clone()], lit(1.0), Vec3::ZERO);
        let plain_bg = scene().reflections(&front(20), &TraceSettings::default())[210].reflection;
        assert_eq!(plain_bg, Vec3::ZERO, "no map: the background colour");
        let img = image::RgbaImage::from_fn(8, 8, |_, y| if y < 4 { image::Rgba([0, 255, 0, 255]) } else { image::Rgba([0, 0, 255, 255]) });
        let env = scene().with_environment(std::sync::Arc::new(img.clone()), loom_scene::environment::EnvMapping::Spherical);
        let c = env.reflections(&front(20), &TraceSettings::default())[210].reflection;
        assert!(c.y > 0.9 && c.z < 0.05, "the zenith: the map's top (green): {c:?}");
        let screen = scene().with_environment(std::sync::Arc::new(img), loom_scene::environment::EnvMapping::Screen);
        assert_eq!(screen.reflections(&front(20), &TraceSettings::default())[210].reflection, Vec3::ZERO, "a Screen map is not seen by rays");
    }

    /// A glass slab tilted in front of a stripe shifts it sideways by its
    /// IOR; IOR 1 leaves it where it is.
    #[test]
    fn refraction_bends_the_view_through_glass() {
        // The stripe: a white bar at x 0..1 on a black wall at z = -10.
        let mut tris = quad(Vec3::new(0.5, 0.0, -10.0), Vec3::new(0.5, 0.0, 0.0), Vec3::new(0.0, 10.0, 0.0));
        let mut info = vec![(0, 0); 2];
        // A slab between z 2 and 4, tilted 40° about Y: its faces as two quads.
        let r = Mat4::from_rotation_y(40f32.to_radians());
        for (z, flip) in [(5.0, 1.0), (1.0, -1.0)] {
            let c = r.transform_point3(Vec3::new(0.0, 0.0, z - 3.0)) + Vec3::new(0.0, 0.0, 3.0);
            let u = r.transform_vector3(Vec3::new(8.0 * flip, 0.0, 0.0));
            tris.extend(quad(c, u, Vec3::new(0.0, 8.0, 0.0)));
            info.extend([(1, 1); 2]);
        }
        let glass = |ior: f32| {
            let mut g = plain(Vec3::ZERO);
            g.trace = Some(TraceLook { reflect: Vec3::ZERO, refract: Vec3::ONE, ior, ..mirror() });
            g
        };
        let stripe_x = |ior: f32| {
            let scene = TraceScene::new(tris.clone(), info.clone(), vec![object(1), object(2)], vec![plain(Vec3::ONE), glass(ior)], lit(1.0), Vec3::ZERO);
            let px = scene.reflections(&front(80), &TraceSettings::default());
            let row: Vec<f32> = (0..80).map(|x| px[40 * 80 + x].refraction.x).collect();
            let bright: Vec<usize> = (0..80).filter(|x| row[*x] > 0.5).collect();
            assert!(!bright.is_empty(), "the stripe is seen through the glass at IOR {ior}: {row:?}");
            bright.iter().sum::<usize>() as f32 / bright.len() as f32
        };
        let (straight, bent) = (stripe_x(1.0), stripe_x(1.5));
        assert!((straight - 41.0).abs() <= 1.5, "IOR 1 leaves the stripe at x 0..1: {straight}");
        assert!((bent - straight).abs() >= 2.0, "IOR 1.5 shifts it: {bent} against {straight}");
    }

    /// The Light Tracer: a floor beside a lit white wall gathers the wall's
    /// light with a bounce, none without; the sky reaches an open floor.
    #[test]
    fn light_tracer_gathers_bounced_light_and_sky() {
        // Floor at z = 0 (facing +Z) seen from above; a wall at x = 6 facing -X.
        let mut tris = quad(Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0), Vec3::new(0.0, 10.0, 0.0));
        let mut info = vec![(0, 0); 2];
        tris.extend(quad(Vec3::new(6.0, 0.0, 5.0), Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 10.0, 0.0)));
        info.extend([(1, 1); 2]);
        // A light shining along +X onto the wall only (the floor faces away from it).
        let mut l = loom_scene::light::default_lights(1, Mat4::IDENTITY).remove(0);
        l.direction = Vec3::X;
        l.color = Vec3::ONE;
        let lighting = Lighting { lights: vec![l], ambient: Vec3::ZERO, sky: Vec3::ZERO };
        let mut wall_obj = object(2);
        wall_obj.camera = false;
        let scene = TraceScene::new(tris, info, vec![object(1), wall_obj], vec![plain(Vec3::ONE), plain(Vec3::ONE)], lighting, Vec3::ZERO);
        let view = View { view: Mat4::look_at_rh(Vec3::new(0.0, 0.0, 30.0), Vec3::ZERO, Vec3::Y), ..front(20) };
        let t = TracerSettings { rays: 64, bounces: 1, cone_angle: 88.0, ray_bias: 0.01, color_bleed: 1.0, object_multiplier: 1.0, sky: Vec3::ZERO, shadows: true };
        let near_wall = |g: &[Option<(Vec3, Vec3)>]| g[10 * 20 + 15].unwrap().1.x;
        let bounced = scene.gather(&view, &t);
        assert!(near_wall(&bounced) > 0.05, "light from the wall: {:?}", bounced[215]);
        let none = scene.gather(&view, &TracerSettings { bounces: 0, ..t.clone() });
        assert_eq!(near_wall(&none), 0.0, "no bounce, no sky: dark");
        let sky = scene.gather(&view, &TracerSettings { bounces: 0, sky: Vec3::ONE, ..t });
        let (open, by_wall) = (sky[10 * 20 + 2].unwrap().1.x, near_wall(&sky));
        assert!(open > 0.8 && by_wall < open - 0.2, "the wall hides part of the sky: {open} {by_wall}");
    }

    /// A point light above a floor with a square between: its traced shadow
    /// is dark under the square and open beside it; over a disk the edge softens.
    #[test]
    fn traced_shadows_are_sharp_from_a_point_and_soft_from_a_disk() {
        let mut tris = quad(Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0), Vec3::new(0.0, 10.0, 0.0));
        let mut info = vec![(0, 0); 2];
        tris.extend(quad(Vec3::new(0.0, 0.0, 5.0), Vec3::new(2.0, 0.0, 0.0), Vec3::new(0.0, 2.0, 0.0)));
        info.extend([(1, 0); 2]);
        let mut blocker = object(2);
        blocker.camera = false;
        let scene = TraceScene::new(tris, info, vec![object(1), blocker], vec![plain(Vec3::ONE)], lit(0.0), Vec3::ZERO);
        let mut l = loom_scene::light::default_lights(1, Mat4::IDENTITY).remove(0);
        l.emitter = Emitter::Point;
        l.position = Vec3::new(0.0, 0.0, 10.0);
        l.direction = Vec3::NEG_Z;
        let view = View { view: Mat4::look_at_rh(Vec3::new(0.0, 0.0, 30.0), Vec3::ZERO, Vec3::Y), ..front(40) };
        let sharp = scene.shadows(&view, &l, loom_scene::light::TracedShadow { radius: 0.0, samples: 1 }, 1);
        // The square (±2 at z 5) shadows ±4 on the floor: pixels 16..24 of 40 over ±10.
        assert_eq!(sharp[20 * 40 + 20], Vec3::ZERO, "under the square");
        assert_eq!(sharp[20 * 40 + 2], Vec3::ONE, "beside it");
        let soft = scene.shadows(&view, &l, loom_scene::light::TracedShadow { radius: 3.0, samples: 64 }, 1);
        let partial = soft.iter().filter(|v| v.x > 0.05 && v.x < 0.95).count();
        let sharp_partial = sharp.iter().filter(|v| v.x > 0.05 && v.x < 0.95).count();
        assert!(partial > sharp_partial + 20, "a disk light's penumbra: {partial} against {sharp_partial}");
        // Shadow colour / density: the shadow lets part of the light through.
        l.shadow_transmission = Vec3::splat(0.25);
        let through = scene.shadows(&view, &l, loom_scene::light::TracedShadow { radius: 0.0, samples: 1 }, 1);
        assert_eq!(through[20 * 40 + 20], Vec3::splat(0.25));
    }

    #[test]
    fn smear_spreads_a_moving_pixel_along_its_motion() {
        let mut img = image::RgbaImage::from_pixel(20, 3, image::Rgba([0, 0, 0, 0]));
        img.put_pixel(10, 1, image::Rgba([255, 255, 255, 255]));
        let behind = image::RgbaImage::from_pixel(20, 3, image::Rgba([0, 0, 0, 0]));
        let mut vel = vec![None; 60];
        vel[30] = Some([8.0, 0.0]);
        let out = smear(&img, &behind, &vel);
        let lit: Vec<u32> = (0..20).filter(|x| out.get_pixel(*x, 1).0[3] > 0).collect();
        assert!(lit.len() >= 8 && lit.contains(&6) && lit.contains(&14), "{lit:?}");
        assert_eq!(out.get_pixel(10, 0).0, [0, 0, 0, 0]);
        let v = velocity_image(&vel, &vel.iter().map(|v| v.is_some()).collect::<Vec<_>>(), [20, 3]);
        assert_eq!(v.get_pixel(10, 1).0, [255, 128, 0, 255]);
        assert_eq!(v.get_pixel(0, 0).0[3], 0);
    }
}
