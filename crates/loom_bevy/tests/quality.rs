//! Render quality on a real GPU: ambient occlusion darkens corners, area
//! shadows widen with the light's Source Radius, antialiasing smooths
//! edges, glossy floors reflect. Skips (passes with a message) without a GPU.

use glam::{Mat4, Vec3};
use loom_bevy::{BevyRenderer, RenderOptions, SceneObject, ViewCamera};
use loom_scene::light::{default_lights, Emitter, LightSample};
use loom_scene::material::{Surface, Surfaces};
use loom_scene::render::BevyRenderSettings;

fn gpu() -> Option<(wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    if !loom_bevy::supported(&adapter) {
        return None;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&loom_bevy::device_descriptor(&adapter), None)).ok()?;
    Some((adapter, device, queue))
}

/// Render options with only what a test turns on.
fn options(f: impl FnOnce(&mut BevyRenderSettings)) -> RenderOptions {
    let mut q = BevyRenderSettings { ambient_occlusion: false, reflections: false, sky_lighting: false, area_shadows: false, passes: 1, ..Default::default() };
    f(&mut q);
    RenderOptions { ambient: [0.0; 3], quality: q, ..Default::default() }
}

fn moved(mut m: loom_mesh::Mesh, by: Vec3) -> loom_mesh::Mesh {
    let _ = loom_mesh::edit_ops::translate_mesh(&mut m, by);
    m
}

fn surface(diffuse: [f32; 3], level: f32, gloss: f32, self_illum: f32) -> Surface {
    let mut m = loom_scene::StandardMaterial::new("m");
    m.diffuse.set(Vec3::from(diffuse));
    m.specular_level.set(level);
    m.glossiness.set(gloss);
    m.self_illum.set(self_illum);
    Surface::from_standard(&m)
}

fn object(id: u64, mesh: &loom_mesh::Mesh, s: Surface) -> SceneObject<'_> {
    SceneObject { id, mesh, wire_color: [200, 200, 200], surfaces: Some(Surfaces::Single(Box::new(s))), renderable: true, flags: Default::default() }
}

fn light(id: u64, emitter: Emitter, position: Vec3, target: Vec3, color: Vec3) -> LightSample {
    let mut s = default_lights(1, Mat4::IDENTITY)[0].clone();
    s.id = loom_scene::NodeId(id);
    s.emitter = emitter;
    s.position = position;
    s.direction = (target - position).normalize();
    s.target_distance = (target - position).length();
    s.color = color;
    s
}

struct Shot {
    camera: ViewCamera,
    size: (u32, u32),
}

impl Shot {
    fn new(eye: Vec3, target: Vec3, size: (u32, u32)) -> Self {
        let up = if (target - eye).normalize().z.abs() > 0.99 { Vec3::Y } else { Vec3::Z };
        Self { camera: ViewCamera { view: Mat4::look_at_rh(eye, target, up), projection: Mat4::perspective_rh(45f32.to_radians(), size.0 as f32 / size.1 as f32, 1.0, 2000.0) }, size }
    }

    fn pixel(&self, p: Vec3) -> (u32, u32) {
        let c = self.camera.projection * self.camera.view * p.extend(1.0);
        let n = c.truncate() / c.w;
        (((n.x + 1.0) * 0.5 * self.size.0 as f32) as u32, ((1.0 - n.y) * 0.5 * self.size.1 as f32) as u32)
    }

    fn render(&self, r: &mut BevyRenderer, o: RenderOptions) -> image::RgbaImage {
        r.render_with(self.size.0, self.size.1, &self.camera, o).expect("render")
    }
}

fn luma(p: [u8; 4]) -> f32 {
    0.3 * p[0] as f32 + 0.59 * p[1] as f32 + 0.11 * p[2] as f32
}

/// Mean luma of the 5x5 pixels around `at`.
fn around(img: &image::RgbaImage, at: (u32, u32)) -> f32 {
    let mut sum = 0.0;
    for dy in -2..=2i32 {
        for dx in -2..=2i32 {
            sum += luma(img.get_pixel((at.0 as i32 + dx) as u32, (at.1 as i32 + dy) as u32).0);
        }
    }
    sum / 25.0
}

/// Ambient light only: with Ambient Occlusion the floor right at the foot
/// of a box darkens; the open floor away from it stays as it was.
#[test]
fn ambient_occlusion_darkens_the_corner_at_a_box() {
    let Some((adapter, device, queue)) = gpu() else {
        eprintln!("ambient_occlusion_darkens_the_corner_at_a_box: no GPU adapter, skipped");
        return;
    };
    let mut r = BevyRenderer::new(adapter, device, queue);
    let floor = loom_mesh::plane_segmented(300.0, 300.0, 1, 1);
    let cube = moved(loom_mesh::box_primitive(Vec3::splat(10.0)), Vec3::new(0.0, 0.0, 10.0));
    let white = surface([0.8, 0.8, 0.8], 0.0, 10.0, 0.0);
    r.sync(&[object(1, &floor, white.clone()), object(2, &cube, white)]);
    r.sync_lights(&[light(5, Emitter::Ambient, Vec3::ZERO, Vec3::NEG_Z, Vec3::splat(0.6))]);
    let shot = Shot::new(Vec3::new(60.0, -60.0, 50.0), Vec3::new(0.0, 0.0, 5.0), (256, 192));
    let (corner, open) = (shot.pixel(Vec3::new(11.0, -3.0, 0.0)), shot.pixel(Vec3::new(45.0, -50.0, 0.0)));
    let off = shot.render(&mut r, options(|_| {}));
    let on = shot.render(&mut r, options(|q| {
        q.ambient_occlusion = true;
        q.ao_radius = 10.0;
        q.passes = 4;
    }));
    let (c_off, c_on, o_off, o_on) = (around(&off, corner), around(&on, corner), around(&off, open), around(&on, open));
    eprintln!("corner {c_off} -> {c_on}, open floor {o_off} -> {o_on}");
    assert!(c_off > 60.0, "the ambient light lights the floor: {c_off}");
    assert!(c_on < c_off * 0.85, "AO darkens the corner: {c_off} -> {c_on}");
    assert!((o_on - o_off).abs() < o_off * 0.06, "the open floor keeps its light: {o_off} -> {o_on}");
}

/// Pixels across the shadow edge that are neither lit nor fully shadowed.
fn penumbra(img: &image::RgbaImage, shot: &Shot, from: Vec3, to: Vec3, lit: f32) -> usize {
    let (a, b) = (shot.pixel(from), shot.pixel(to));
    let n = (b.0 as i32 - a.0 as i32).abs().max((b.1 as i32 - a.1 as i32).abs()) as usize;
    (0..=n)
        .filter(|i| {
            let t = *i as f32 / n as f32;
            let (x, y) = (a.0 as f32 + (b.0 as f32 - a.0 as f32) * t, a.1 as f32 + (b.1 as f32 - a.1 as f32) * t);
            let l = luma(img.get_pixel(x as u32, y as u32).0);
            l > lit * 0.15 && l < lit * 0.85
        })
        .count()
}

/// A spot over a box: the shadow's soft edge widens with the light's
/// Source Radius when Area Shadows add up the passes.
#[test]
fn area_shadows_soften_with_the_source_radius() {
    let Some((adapter, device, queue)) = gpu() else {
        eprintln!("area_shadows_soften_with_the_source_radius: no GPU adapter, skipped");
        return;
    };
    let mut r = BevyRenderer::new(adapter, device, queue);
    let floor = loom_mesh::plane_segmented(400.0, 400.0, 1, 1);
    let cube = moved(loom_mesh::box_primitive(Vec3::splat(10.0)), Vec3::new(0.0, 0.0, 40.0));
    let white = surface([0.8, 0.8, 0.8], 0.0, 10.0, 0.0);
    r.sync(&[object(1, &floor, white.clone()), object(2, &cube, white)]);
    let mut spot = light(5, Emitter::Spot { hotspot: 90.0, falloff: 100.0 }, Vec3::new(0.0, 0.0, 100.0), Vec3::ZERO, Vec3::ONE);
    spot.shadows = true;
    spot.map_size = 2048;
    let shot = Shot::new(Vec3::new(0.0, -1.0, 160.0), Vec3::ZERO, (256, 256));
    // From inside the shadow out across its right edge to the lit floor.
    let (from, to, lit_at) = (Vec3::ZERO, Vec3::new(45.0, 0.0, 0.0), Vec3::new(45.0, 0.0, 0.0));
    let mut widths = Vec::new();
    for radius in [0.0, 12.0] {
        spot.source_radius = radius;
        r.sync_lights(std::slice::from_ref(&spot));
        let img = shot.render(&mut r, options(|q| {
            q.area_shadows = true;
            q.passes = 24;
        }));
        let lit = around(&img, shot.pixel(lit_at));
        assert!(lit > 80.0, "the floor is lit: {lit}");
        assert!(luma(img.get_pixel(shot.pixel(Vec3::ZERO).0, shot.pixel(Vec3::ZERO).1).0) < lit * 0.3, "radius {radius}: shadow under the box");
        widths.push(penumbra(&img, &shot, from, to, lit));
    }
    eprintln!("penumbra pixels: radius 0 {}, radius 12 {}", widths[0], widths[1]);
    assert!(widths[1] >= widths[0] * 3 && widths[1] >= 6, "a larger Source Radius widens the penumbra: {widths:?}");
}

/// A white slab's slanted edge over the black background: antialiasing
/// brings its coverage much closer to a heavily supersampled reference.
#[test]
fn antialiasing_brings_edges_close_to_the_reference() {
    let Some((adapter, device, queue)) = gpu() else {
        eprintln!("antialiasing_brings_edges_close_to_the_reference: no GPU adapter, skipped");
        return;
    };
    let mut r = BevyRenderer::new(adapter, device, queue);
    let slab = {
        let mut m = loom_mesh::plane_segmented(60.0, 60.0, 1, 1);
        loom_mesh::edit_ops::transform_mesh(&mut m, Mat4::from_rotation_z(7f32.to_radians())).unwrap();
        m
    };
    r.sync(&[object(1, &slab, surface([1.0, 1.0, 1.0], 0.0, 10.0, 100.0))]);
    let shot = Shot::new(Vec3::new(0.0, -0.01, 90.0), Vec3::ZERO, (128, 128));
    let coverage = |img: &image::RgbaImage| img.pixels().map(|p| p.0[3] as f32 / 255.0).collect::<Vec<f32>>();
    let reference = coverage(&shot.render(&mut r, options(|q| {
        q.supersampling = 4;
        q.passes = 16;
    })));
    let error = |o: RenderOptions, r: &mut BevyRenderer| -> f32 {
        let c = coverage(&shot.render(r, o));
        c.iter().zip(&reference).map(|(a, b)| (a - b).abs()).sum::<f32>()
    };
    let aliased = error(options(|q| q.antialiasing = false), &mut r);
    let smooth = error(options(|q| q.passes = 16), &mut r);
    eprintln!("edge coverage error: aliased {aliased}, antialiased {smooth}");
    assert!(aliased > 5.0, "the slab's edges show: {aliased}");
    assert!(smooth < aliased * 0.4, "antialiasing: {aliased} -> {smooth}");
}

/// A self-illuminated red box over a glossy black floor: with Reflections
/// the floor shows it where its mirror image falls.
#[test]
fn reflections_show_on_a_glossy_floor() {
    let Some((adapter, device, queue)) = gpu() else {
        eprintln!("reflections_show_on_a_glossy_floor: no GPU adapter, skipped");
        return;
    };
    let mut r = BevyRenderer::new(adapter, device, queue);
    let floor = loom_mesh::plane_segmented(400.0, 400.0, 1, 1);
    let cube = moved(loom_mesh::box_primitive(Vec3::new(10.0, 10.0, 8.0)), Vec3::new(0.0, 0.0, 12.0));
    r.sync(&[object(1, &floor, surface([0.02, 0.02, 0.02], 999.0, 90.0, 0.0)), object(2, &cube, surface([1.0, 0.1, 0.05], 0.0, 10.0, 100.0))]);
    let shot = Shot::new(Vec3::new(0.0, -90.0, 14.0), Vec3::new(0.0, 0.0, 4.0), (192, 128));
    // The mirror image of the box's middle, below the floor.
    let mirror = shot.pixel(Vec3::new(0.0, -10.0, -10.0));
    let red = |img: &image::RgbaImage| {
        let p = img.get_pixel(mirror.0, mirror.1).0;
        p[0] as f32 - p[2] as f32
    };
    let without = red(&shot.render(&mut r, options(|_| {})));
    let with = red(&shot.render(&mut r, options(|q| q.reflections = true)));
    eprintln!("floor redness at the mirror image: {without} -> {with}");
    assert!(with > without + 25.0, "the floor reflects the red box: {without} -> {with}");
}
