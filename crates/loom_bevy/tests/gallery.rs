//! The render quality gallery: a few test scenes rendered at 1280x720 the
//! way Render does, written as PNGs to `LOOM_GALLERY_DIR`.
//! Run with `cargo test -p loom_bevy --test gallery -- --ignored`.

use glam::{Mat4, Vec3};
use loom_bevy::{BevyRenderer, RenderOptions, SceneObject, ViewCamera};
use loom_scene::light::{default_lights, Emitter, LightSample};
use loom_scene::material::{Map, MapType, Surface, Surfaces};
use loom_scene::StandardMaterial;

const W: u32 = 1280;
const H: u32 = 720;

fn gpu() -> Option<(wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    if !loom_bevy::supported(&adapter) {
        return None;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&loom_bevy::device_descriptor(&adapter), None)).ok()?;
    Some((adapter, device, queue))
}

fn moved(mut m: loom_mesh::Mesh, by: Vec3) -> loom_mesh::Mesh {
    let _ = loom_mesh::edit_ops::translate_mesh(&mut m, by);
    m
}

fn mapped(mut m: loom_mesh::Mesh, kind: loom_mesh::uvmap::AutoMap) -> loom_mesh::Mesh {
    loom_mesh::uvmap::auto_map(&mut m, kind, 1);
    m
}

/// A Standard material: diffuse (sRGB), Specular Level, Glossiness, Opacity %.
fn surface(diffuse: [f32; 3], level: f32, gloss: f32, opacity: f32) -> Surface {
    let mut m = StandardMaterial::new("m");
    m.diffuse.set(Vec3::from(diffuse));
    m.specular_level.set(level);
    m.glossiness.set(gloss);
    m.opacity.set(opacity);
    Surface::from_standard(&m)
}

fn light(id: u64, emitter: Emitter, position: Vec3, target: Vec3, color: Vec3) -> LightSample {
    let mut s = default_lights(1, Mat4::IDENTITY)[0].clone();
    s.id = loom_scene::NodeId(id);
    s.emitter = emitter;
    s.position = position;
    s.direction = (target - position).normalize();
    s.target_distance = (target - position).length();
    s.color = color;
    s.shadows = true;
    s.map_size = 2048;
    s
}

fn camera(eye: Vec3, target: Vec3, fov: f32) -> ViewCamera {
    ViewCamera { view: Mat4::look_at_rh(eye, target, Vec3::Z), projection: Mat4::perspective_rh(fov.to_radians(), W as f32 / H as f32, 1.0, 5000.0) }
}

/// Renders the frame as the Rendered Frame Window does at this size: whole, every pass.
fn frame(r: &mut BevyRenderer, cam: &ViewCamera, options: RenderOptions) -> image::RgbaImage {
    let t = std::time::Instant::now();
    let img = r.render_with(W, H, cam, options).unwrap();
    eprintln!("rendered in {:.2} s", t.elapsed().as_secs_f32());
    img
}

fn object<'a>(id: u64, mesh: &'a loom_mesh::Mesh, s: Surface) -> SceneObject<'a> {
    SceneObject { id, mesh, transform: Mat4::IDENTITY, wire_color: [200, 200, 200], surfaces: Some(Surfaces::Single(Box::new(s))), renderable: true, flags: Default::default() }
}

#[test]
#[ignore = "writes the quality gallery to LOOM_GALLERY_DIR"]
fn render_gallery() {
    let Ok(dir) = std::env::var("LOOM_GALLERY_DIR") else {
        eprintln!("render_gallery: LOOM_GALLERY_DIR not set");
        return;
    };
    let Some((adapter, device, queue)) = gpu() else {
        eprintln!("render_gallery: no GPU adapter, skipped");
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let mut r = BevyRenderer::new(adapter, device, queue);
    let save = |name: &str, img: &image::RgbaImage| {
        // Over the background as the Rendered Frame Window shows it.
        let path = format!("{dir}/{name}.png");
        image::DynamicImage::ImageRgba8(img.clone()).to_rgb8().save(&path).unwrap();
        eprintln!("wrote {path}");
    };
    let options = RenderOptions::default();

    // 1. Teapot on a plane, a spot light and an omni.
    let teapot = loom_mesh::teapot_primitive(25.0, 8, Default::default());
    let plane = mapped(loom_mesh::plane_segmented(400.0, 400.0, 1, 1), loom_mesh::uvmap::AutoMap::Planar);
    r.sync(&[object(1, &teapot, surface([0.7, 0.08, 0.05], 90.0, 45.0, 100.0)), object(2, &plane, surface([0.75, 0.75, 0.72], 0.0, 10.0, 100.0))]);
    let mut spot = light(10, Emitter::Spot { hotspot: 35.0, falloff: 55.0 }, Vec3::new(90.0, -110.0, 160.0), Vec3::new(0.0, 0.0, 10.0), Vec3::splat(1.0));
    spot.source_radius = 6.0;
    let mut omni = light(11, Emitter::Point, Vec3::new(-120.0, 60.0, 80.0), Vec3::ZERO, Vec3::new(0.25, 0.3, 0.4));
    omni.shadows = false;
    r.sync_lights(&[spot, omni]);
    let img = frame(&mut r, &camera(Vec3::new(-95.0, -120.0, 70.0), Vec3::new(0.0, 0.0, 15.0), 40.0), options.clone());
    save("teapot", &img);

    // 2. A room lit through a window: sun and sky.
    let (w, d, h) = (200.0, 160.0, 120.0);
    let slab = |c: Vec3, half: Vec3| moved(loom_mesh::box_primitive(half), c);
    // The +Y wall has a window opening at x -40..20, z 30..90.
    let wy = d / 2.0 + 2.0;
    let room = [
        slab(Vec3::new(0.0, 0.0, -2.0), Vec3::new(w / 2.0 + 4.0, d / 2.0 + 4.0, 2.0)),
        slab(Vec3::new(0.0, 0.0, h + 2.0), Vec3::new(w / 2.0 + 4.0, d / 2.0 + 4.0, 2.0)),
        slab(Vec3::new(-w / 2.0 - 2.0, 0.0, h / 2.0), Vec3::new(2.0, d / 2.0, h / 2.0)),
        slab(Vec3::new(w / 2.0 + 2.0, 0.0, h / 2.0), Vec3::new(2.0, d / 2.0, h / 2.0)),
        slab(Vec3::new(0.0, -d / 2.0 - 2.0, h / 2.0), Vec3::new(w / 2.0, 2.0, h / 2.0)),
        slab(Vec3::new(-70.0, wy, h / 2.0), Vec3::new(30.0, 2.0, h / 2.0)),
        slab(Vec3::new(60.0, wy, h / 2.0), Vec3::new(40.0, 2.0, h / 2.0)),
        slab(Vec3::new(-10.0, wy, 15.0), Vec3::new(30.0, 2.0, 15.0)),
        slab(Vec3::new(-10.0, wy, 105.0), Vec3::new(30.0, 2.0, 15.0)),
    ];
    let table = slab(Vec3::new(-10.0, 10.0, 18.0), Vec3::new(30.0, 20.0, 18.0));
    let ball = moved(loom_mesh::sphere_segmented(14.0, 32, 0.0), Vec3::new(45.0, -20.0, 14.0));
    let wall = surface([0.8, 0.78, 0.74], 0.0, 10.0, 100.0);
    let mut objects: Vec<SceneObject> = room.iter().enumerate().map(|(i, m)| object(20 + i as u64, m, wall.clone())).collect();
    objects.push(object(40, &table, surface([0.45, 0.3, 0.18], 40.0, 35.0, 100.0)));
    objects.push(object(41, &ball, surface([0.2, 0.35, 0.7], 100.0, 60.0, 100.0)));
    r.sync(&objects);
    let sun = light(30, Emitter::Directional, Vec3::new(-150.0, 400.0, 350.0), Vec3::new(-20.0, 0.0, 0.0), Vec3::new(1.0, 0.95, 0.85) * 1.6);
    let sky = light(31, Emitter::Ambient, Vec3::ZERO, Vec3::NEG_Z, Vec3::new(0.12, 0.14, 0.18));
    r.sync_lights(&[sun, sky]);
    let img = frame(&mut r, &camera(Vec3::new(70.0, -70.0, 75.0), Vec3::new(-20.0, 30.0, 40.0), 70.0), RenderOptions { ambient: [0.0; 3], ..options.clone() });
    save("room", &img);

    // 3. Materials: glossy plastic, metal-like, glass, textured with bump.
    let sphere = |x: f32| mapped(moved(loom_mesh::sphere_segmented(18.0, 48, 0.0), Vec3::new(x, 0.0, 18.0)), loom_mesh::uvmap::AutoMap::Spherical);
    let (s1, s2, s3, s4) = (sphere(-75.0), sphere(-25.0), sphere(25.0), sphere(75.0));
    let mut checker = Map::new(MapType::Checker, 100.0);
    checker.coords.tiling = [6.0, 3.0];
    checker.colors = [Vec3::new(0.9, 0.85, 0.7), Vec3::new(0.15, 0.25, 0.45), Vec3::ONE];
    let mut bump = Map::new(MapType::Noise, 60.0);
    bump.params.size = 8.0;
    let mut textured = surface([0.5, 0.5, 0.5], 60.0, 40.0, 100.0);
    textured.diffuse_map = Some(checker.clone());
    textured.bump_map = Some(bump);
    let mut floor = surface([0.5, 0.5, 0.5], 30.0, 30.0, 100.0);
    let mut floor_checker = checker;
    floor_checker.coords.tiling = [8.0, 8.0];
    floor_checker.colors = [Vec3::splat(0.75), Vec3::splat(0.35), Vec3::ONE];
    floor.diffuse_map = Some(floor_checker);
    r.sync(&[
        object(50, &s1, surface([0.8, 0.1, 0.1], 120.0, 70.0, 100.0)),
        object(51, &s2, surface([0.08, 0.08, 0.09], 300.0, 85.0, 100.0)),
        object(52, &s3, surface([0.6, 0.8, 0.75], 150.0, 90.0, 20.0)),
        object(53, &s4, textured),
        object(54, &plane, floor),
    ]);
    let mut key = light(60, Emitter::Spot { hotspot: 40.0, falloff: 70.0 }, Vec3::new(120.0, -160.0, 200.0), Vec3::new(0.0, 0.0, 10.0), Vec3::splat(1.1));
    key.source_radius = 10.0;
    let mut fill = light(61, Emitter::Point, Vec3::new(-150.0, -40.0, 90.0), Vec3::ZERO, Vec3::splat(0.3));
    fill.shadows = false;
    let sky = light(62, Emitter::Ambient, Vec3::ZERO, Vec3::NEG_Z, Vec3::splat(0.12));
    r.sync_lights(&[key, fill, sky]);
    let img = frame(&mut r, &camera(Vec3::new(0.0, -190.0, 75.0), Vec3::new(0.0, 0.0, 15.0), 45.0), RenderOptions { ambient: [0.0; 3], ..options });
    save("materials", &img);
}
