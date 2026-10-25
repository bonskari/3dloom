//! The Environment on a real GPU: background colour and map, global
//! lighting, exposure control, fog, volume light and Depth of Field in
//! renders and Realistic viewports. Skips when the machine has no GPU.

use glam::{Mat4, Vec3};
use loom_bevy::environment::{DepthOfField, EnvSettings, Exposure, Fog, ViewEnv, VolumeLight};
use loom_bevy::{BevyRenderer, RenderOptions, SceneObject, ViewCamera};
use loom_scene::environment::{EnvMapping, ExposureKind, LayerFalloff};

fn gpu() -> Option<BevyRenderer> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    if !loom_bevy::supported(&adapter) {
        return None;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&loom_bevy::device_descriptor(&adapter), None)).ok()?;
    Some(BevyRenderer::new(adapter, device, queue))
}

macro_rules! renderer {
    ($name:literal) => {
        match gpu() {
            Some(r) => r,
            None => {
                eprintln!("{}: no GPU adapter, skipped", $name);
                return;
            }
        }
    };
}

fn moved(mut m: loom_mesh::Mesh, by: Vec3) -> loom_mesh::Mesh {
    let _ = loom_mesh::edit_ops::translate_mesh(&mut m, by);
    m
}

fn object(id: u64, mesh: &loom_mesh::Mesh) -> SceneObject<'_> {
    SceneObject { id, mesh, wire_color: [230, 230, 230], surfaces: None, renderable: true, flags: Default::default() }
}

/// Looks along +Y from y = -100 at the height of the boxes' centres.
fn camera(aspect: f32) -> ViewCamera {
    ViewCamera { view: Mat4::look_at_rh(Vec3::new(0.0, -100.0, 5.0), Vec3::new(0.0, 0.0, 5.0), Vec3::Z), projection: Mat4::perspective_rh(40f32.to_radians(), aspect, 0.5, 5000.0) }
}

/// Renders `camera` in `n` x 1 buckets, as the UI does.
fn render(r: &mut BevyRenderer, cam: &ViewCamera, (w, h): (u32, u32), options: RenderOptions, n: u32) -> image::RgbaImage {
    r.begin_render(options);
    let mut out = image::RgbaImage::new(w, h);
    let tw = w / n;
    for b in 0..n {
        let sub = [(b * tw) as f32 / w as f32, 0.0, ((b + 1) * tw) as f32 / w as f32, 1.0];
        for _ in 0..if b == 0 { 5 } else { 2 } {
            r.set_render_bucket(cam, w as f32 / h as f32, sub, [tw, h]).unwrap();
            r.update();
        }
        image::imageops::replace(&mut out, &r.read_render().unwrap(), (b * tw) as i64, 0);
    }
    r.end_render();
    out
}

/// Pixel of world point `p` in a `w` x `h` image of `cam`.
fn pixel(cam: &ViewCamera, (w, h): (u32, u32), p: Vec3) -> (u32, u32) {
    let c = cam.projection * cam.view * p.extend(1.0);
    let n = c.truncate() / c.w;
    (((n.x * 0.5 + 0.5) * w as f32) as u32, ((0.5 - n.y * 0.5) * h as f32) as u32)
}

fn luma(p: [u8; 4]) -> f32 {
    0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
}

fn env(f: impl FnOnce(&mut EnvSettings)) -> EnvSettings {
    let mut e = EnvSettings::default();
    f(&mut e);
    e
}

/// Standard fog in black darkens the far box, not the near one; Render
/// Setup's Atmospherics off leaves it out; a Realistic viewport shows it.
#[test]
fn fog_darkens_distant_objects() {
    let mut r = renderer!("fog_darkens_distant_objects");
    let near = moved(loom_mesh::box_primitive(Vec3::splat(10.0)), Vec3::new(-15.0, -60.0, 0.0));
    let far = moved(loom_mesh::box_primitive(Vec3::splat(40.0)), Vec3::new(40.0, 150.0, -15.0));
    r.sync(&[object(1, &near), object(2, &far)]);
    let size = (160, 100);
    let cam = camera(1.6);
    let (pn, pf) = (pixel(&cam, size, Vec3::new(-15.0, -65.0, 5.0)), pixel(&cam, size, Vec3::new(40.0, 130.0, 5.0)));
    let at = |img: &image::RgbaImage, p: (u32, u32)| luma(img.get_pixel(p.0, p.1).0);
    let plain = render(&mut r, &cam, size, RenderOptions::default(), 1);
    assert!(at(&plain, pn) > 60.0 && at(&plain, pf) > 60.0, "both boxes lit: {} {}", at(&plain, pn), at(&plain, pf));

    let fog = Fog::Standard { color: [0.0; 3], near: 0.0, far: 1.0, exponential: false, fog_background: true };
    r.set_environment(&env(|e| e.fogs = vec![fog]));
    let view = ViewEnv { ranges: Some([0.0, 300.0]), focus: None };
    let fogged = render(&mut r, &cam, size, RenderOptions { view, ..Default::default() }, 2);
    let (n, f) = (at(&fogged, pn) / at(&plain, pn), at(&fogged, pf) / at(&plain, pf));
    assert!(n > 0.65, "the near box keeps most of its light: {n}");
    // The fog mixes in linear light, so a density of about 0.75 leaves about
    // half the sRGB value.
    assert!(f < 0.65 && f < n - 0.25, "the far box fades into the fog: near {n}, far {f}");

    let off = render(&mut r, &cam, size, RenderOptions { view, atmospherics: false, ..Default::default() }, 1);
    assert!((at(&off, pf) - at(&plain, pf)).abs() < 6.0, "Atmospherics off: no fog");

    // White fog with Fog Background fills the background (still alpha 0).
    let white = Fog::Standard { color: [1.0; 3], near: 0.0, far: 0.5, exponential: false, fog_background: true };
    r.set_environment(&env(|e| e.fogs = vec![white]));
    let img = render(&mut r, &cam, size, RenderOptions { view, ..Default::default() }, 1);
    let bg = img.get_pixel(2, 2).0;
    assert!(bg[3] == 0 && bg[0] > 150 && bg[0] < 220, "half-dense fog over the black background: {bg:?}");

    // The Realistic viewport shows the fog too, over a transparent background.
    r.set_environment(&env(|e| e.fogs = vec![fog]));
    r.set_view_environment(0, view);
    for _ in 0..4 {
        r.set_view(0, [size.0, size.1], &cam, [0.0; 4], true);
        r.update();
    }
    let vp = r.read_view(0).unwrap();
    assert!(at(&vp, pf) < at(&plain, pf) * 0.6, "the viewport's far box is fogged");
    assert_eq!(vp.get_pixel(2, 2).0[3], 0, "viewports keep their own background");
}

/// Layered fog fills its layer: a low box is fogged, a high one is not.
#[test]
fn layered_fog_stays_in_its_layer() {
    let mut r = renderer!("layered_fog_stays_in_its_layer");
    let low = moved(loom_mesh::box_primitive(Vec3::splat(8.0)), Vec3::new(-12.0, 0.0, -10.0));
    let high = moved(loom_mesh::box_primitive(Vec3::splat(8.0)), Vec3::new(12.0, 0.0, 14.0));
    r.sync(&[object(1, &low), object(2, &high)]);
    let size = (160, 100);
    let cam = camera(1.6);
    let (pl, ph) = (pixel(&cam, size, Vec3::new(-12.0, -4.0, -6.0)), pixel(&cam, size, Vec3::new(12.0, -4.0, 18.0)));
    let plain = render(&mut r, &cam, size, RenderOptions::default(), 1);
    let layer = Fog::Layered { color: [0.0; 3], bottom: -30.0, top: 0.0, density: 1.0, falloff: LayerFalloff::None, fog_background: false };
    r.set_environment(&env(|e| e.fogs = vec![layer]));
    let img = render(&mut r, &cam, size, RenderOptions::default(), 1);
    let ratio = |p: (u32, u32)| luma(img.get_pixel(p.0, p.1).0) / luma(plain.get_pixel(p.0, p.1).0).max(1.0);
    assert!(ratio(pl) < 0.5, "the box in the layer is fogged: {}", ratio(pl));
    assert!(ratio(ph) > 0.9, "the box above it is not: {}", ratio(ph));
}

/// The background colour and a screen-mapped map render behind the scene
/// with alpha 0, across buckets; a spherical map puts the sky up.
#[test]
fn background_colour_and_map_in_render() {
    let mut r = renderer!("background_colour_and_map_in_render");
    let b = loom_mesh::box_primitive(Vec3::splat(10.0));
    r.sync(&[object(1, &b)]);
    let size = (160, 100);
    let cam = camera(1.6);
    r.set_environment(&env(|e| e.background = [0.0, 0.5, 1.0]));
    let img = render(&mut r, &cam, size, RenderOptions::default(), 2);
    let bg = img.get_pixel(150, 5).0;
    assert!(bg[3] == 0 && bg[0] < 3 && (bg[1] as i32 - 128).abs() < 3 && bg[2] > 250, "sRGB (0, 0.5, 1), alpha 0: {bg:?}");
    let (cx, cy) = pixel(&cam, size, Vec3::new(0.0, -5.0, 5.0));
    assert_eq!(img.get_pixel(cx, cy).0[3], 255, "the box covers the background");

    let dir = std::env::temp_dir().join(format!("loom-env-map-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Screen: left half red, right half green.
    let screen = dir.join("screen.png");
    image::RgbaImage::from_fn(64, 32, |x, _| if x < 32 { image::Rgba([255, 0, 0, 255]) } else { image::Rgba([0, 255, 0, 255]) }).save(&screen).unwrap();
    r.set_environment(&env(|e| e.map = Some((screen.to_string_lossy().into(), EnvMapping::Screen))));
    let img = render(&mut r, &cam, size, RenderOptions::default(), 2);
    let (l, rt) = (img.get_pixel(10, 5).0, img.get_pixel(150, 95).0);
    assert!(l[0] > 240 && l[1] < 10 && l[3] == 0, "left of the frame is the map's left: {l:?}");
    assert!(rt[1] > 240 && rt[0] < 10 && rt[3] == 0, "the second bucket shows the map's right: {rt:?}");

    // Spherical: the upper half of the map is blue sky, the lower brown ground.
    let sphere = dir.join("sphere.png");
    image::RgbaImage::from_fn(64, 32, |_, y| if y < 16 { image::Rgba([0, 0, 255, 255]) } else { image::Rgba([120, 60, 0, 255]) }).save(&sphere).unwrap();
    r.set_environment(&env(|e| e.map = Some((sphere.to_string_lossy().into(), EnvMapping::Spherical))));
    let img = render(&mut r, &cam, size, RenderOptions::default(), 1);
    let (up, down) = (img.get_pixel(80, 3).0, img.get_pixel(80, 97).0);
    assert!(up[2] > 240 && up[0] < 10, "above the horizon: sky {up:?}");
    assert!(down[0] > 100 && down[2] < 10, "below it: ground {down:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Global Lighting Level multiplies the lights, Tint colours them.
#[test]
fn tint_and_level_change_the_lights() {
    let mut r = renderer!("tint_and_level_change_the_lights");
    let b = loom_mesh::box_primitive(Vec3::splat(10.0));
    r.sync(&[object(1, &b)]);
    let size = (120, 80);
    let cam = ViewCamera { view: Mat4::look_at_rh(Vec3::new(-25.0, -35.0, 30.0), Vec3::new(0.0, 0.0, 5.0), Vec3::Z), projection: Mat4::perspective_rh(40f32.to_radians(), 1.5, 0.5, 5000.0) };
    let (x, y) = pixel(&cam, size, Vec3::new(0.0, 0.0, 10.0));
    // No ambient light, so only the key light shows.
    let shot = |r: &mut BevyRenderer, f: &dyn Fn(&mut EnvSettings)| {
        r.set_environment(&env(|e| {
            e.ambient = [0.0; 3];
            f(e)
        }));
        render(r, &cam, size, RenderOptions::default(), 1).get_pixel(x, y).0
    };
    let base = shot(&mut r, &|_| {});
    let dim = shot(&mut r, &|e| e.level = 0.25);
    let red = shot(&mut r, &|e| e.tint = [1.0, 0.0, 0.0]);
    assert!(luma(dim) < luma(base) * 0.7, "Level 0.25 dims the top: {base:?} -> {dim:?}");
    assert!(red[0] > 40 && red[1] < 5 && red[2] < 5, "a red tint leaves only red: {red:?}");
}

/// Exposure control changes the brightness; Automatic meters the image,
/// so a dimly lit scene comes out near a brightly lit one.
#[test]
fn exposure_changes_brightness() {
    let mut r = renderer!("exposure_changes_brightness");
    let b = loom_mesh::box_primitive(Vec3::splat(10.0));
    r.sync(&[object(1, &b)]);
    let size = (120, 80);
    let cam = ViewCamera { view: Mat4::look_at_rh(Vec3::new(-25.0, -35.0, 30.0), Vec3::new(0.0, 0.0, 5.0), Vec3::Z), projection: Mat4::perspective_rh(40f32.to_radians(), 1.5, 0.5, 5000.0) };
    let mean = |img: &image::RgbaImage| {
        let v: Vec<f32> = img.pixels().filter(|p| p.0[3] == 255).map(|p| luma(p.0)).collect();
        v.iter().sum::<f32>() / v.len().max(1) as f32
    };
    let x = |kind, brightness, exposure_value| Exposure { kind, process_background: false, brightness, contrast: 50.0, exposure_value, mid_tones: 1.0 };
    let shot = |r: &mut BevyRenderer, level: f32, exposure: Option<Exposure>| {
        r.set_environment(&env(|e| {
            e.level = level;
            e.ambient = [0.2; 3];
            e.exposure = exposure;
        }));
        mean(&render(r, &cam, size, RenderOptions::default(), 1))
    };
    let lin = shot(&mut r, 1.0, Some(x(ExposureKind::Linear, 50.0, 0.0)));
    let lin_up = shot(&mut r, 1.0, Some(x(ExposureKind::Linear, 50.0, 2.0)));
    let lin_down = shot(&mut r, 1.0, Some(x(ExposureKind::Linear, 25.0, 0.0)));
    assert!(lin_up > lin + 25.0, "Exposure Value +2 brightens: {lin} -> {lin_up}");
    assert!(lin_down < lin - 15.0, "Brightness 25 darkens: {lin} -> {lin_down}");
    let log = shot(&mut r, 1.0, Some(x(ExposureKind::Logarithmic, 50.0, 0.0)));
    let log_up = shot(&mut r, 1.0, Some(x(ExposureKind::Logarithmic, 150.0, 0.0)));
    assert!(log_up > log + 25.0, "Logarithmic Brightness 150 brightens: {log} -> {log_up}");

    let none_bright = shot(&mut r, 1.0, None);
    let none_dim = shot(&mut r, 0.1, None);
    let auto_bright = shot(&mut r, 1.0, Some(x(ExposureKind::Automatic, 50.0, 0.0)));
    let auto_dim = shot(&mut r, 0.1, Some(x(ExposureKind::Automatic, 50.0, 0.0)));
    assert!(none_dim < none_bright * 0.6, "without exposure control dim light is dark: {none_bright} vs {none_dim}");
    assert!((auto_dim - auto_bright).abs() < (none_bright - none_dim) * 0.5, "Automatic meters both towards mid grey: {auto_bright} vs {auto_dim}");
}

/// Process Background off keeps the background colour exactly; on, the
/// exposure control brightens it with the scene.
#[test]
fn process_background_puts_the_background_under_exposure() {
    let mut r = renderer!("process_background_puts_the_background_under_exposure");
    let b = loom_mesh::box_primitive(Vec3::splat(10.0));
    r.sync(&[object(1, &b)]);
    let cam = camera(1.5);
    let up = Exposure { kind: ExposureKind::Linear, process_background: false, brightness: 50.0, contrast: 50.0, exposure_value: 3.0, mid_tones: 1.0 };
    r.set_environment(&env(|e| {
        e.background = [0.2, 0.2, 0.2];
        e.exposure = Some(up);
    }));
    let kept = render(&mut r, &cam, (120, 80), RenderOptions::default(), 1).get_pixel(2, 2).0;
    assert!((kept[0] as i32 - 51).abs() <= 2 && kept[3] == 0, "the background keeps its colour: {kept:?}");
    r.set_environment(&env(|e| {
        e.background = [0.2, 0.2, 0.2];
        e.exposure = Some(Exposure { process_background: true, ..up });
    }));
    let processed = render(&mut r, &cam, (120, 80), RenderOptions::default(), 1).get_pixel(2, 2).0;
    assert!(processed[0] > kept[0] + 40, "processed, the background brightens too: {processed:?}");
}

/// Depth of Field blurs what is past the focal limit, not what is in focus.
#[test]
fn depth_of_field_blurs_out_of_focus_edges() {
    let mut r = renderer!("depth_of_field_blurs_out_of_focus_edges");
    let near = moved(loom_mesh::box_primitive(Vec3::splat(10.0)), Vec3::new(-12.0, -50.0, 0.0));
    let far = moved(loom_mesh::box_primitive(Vec3::splat(30.0)), Vec3::new(30.0, 150.0, -10.0));
    r.sync(&[object(1, &near), object(2, &far)]);
    let size = (200, 120);
    let cam = camera(200.0 / 120.0);
    // Soft pixels (neither background nor a box face) across a box's side edge.
    let soft = |img: &image::RgbaImage, y: u32, x0: u32, x1: u32| (x0..x1).filter(|x| (8..247).contains(&img.get_pixel(*x, y).0[3])).count();
    let dof = DepthOfField { focal_node: None, horiz_loss: 8.0, vert_loss: 8.0, focal_range: 10.0, focal_limit: 60.0 };
    let (ny, nx) = { let (x, y) = pixel(&cam, size, Vec3::new(-7.0, -55.0, 5.0)); (y, x) };
    let (fy, fx) = { let (x, y) = pixel(&cam, size, Vec3::new(45.0, 135.0, 5.0)); (y, x) };
    let sharp = render(&mut r, &cam, size, RenderOptions::default(), 1);
    r.set_environment(&env(|e| e.depth_of_field = Some(dof)));
    let view = ViewEnv { ranges: None, focus: Some(50.0) };
    let blurred = render(&mut r, &cam, size, RenderOptions { view, ..Default::default() }, 1);
    let (near_sharp, near_dof) = (soft(&sharp, ny, nx.saturating_sub(12), nx + 12), soft(&blurred, ny, nx.saturating_sub(12), nx + 12));
    let (far_sharp, far_dof) = (soft(&sharp, fy, fx.saturating_sub(12), fx + 12), soft(&blurred, fy, fx.saturating_sub(12), fx + 12));
    assert!(near_dof <= near_sharp + 1, "the box in focus stays sharp: {near_sharp} -> {near_dof}");
    assert!(far_dof >= far_sharp + 4, "the far box's edge blurs: {far_sharp} -> {far_dof}");
    let off = render(&mut r, &cam, size, RenderOptions { view, effects: false, ..Default::default() }, 1);
    assert!(soft(&off, fy, fx.saturating_sub(12), fx + 12) <= far_sharp + 1, "Effects off: no blur");
}

/// The default environment (black ambient) with no scene lights: the
/// default key and fill lights leave no face of a box black; a face the
/// key light misses is lit, more dimly, by the fill.
#[test]
fn default_lighting_lights_every_side() {
    let mut r = renderer!("default_lighting_lights_every_side");
    let b = loom_mesh::box_primitive(Vec3::splat(10.0));
    r.sync(&[object(1, &b)]);
    r.sync_lights(&[]);
    assert!(r.default_lighting());
    let size = (120, 120);
    // From -X, a little below: the -X face (away from the key light) and the -Y face.
    let cam = ViewCamera { view: Mat4::look_at_rh(Vec3::new(-40.0, -40.0, 2.0), Vec3::new(0.0, 0.0, 5.0), Vec3::Z), projection: Mat4::perspective_rh(40f32.to_radians(), 1.0, 0.5, 5000.0) };
    let img = render(&mut r, &cam, size, RenderOptions::default(), 1);
    let at = |p: Vec3| {
        let (x, y) = pixel(&cam, size, p);
        img.get_pixel(x, y).0
    };
    let (fill_side, key_side) = (at(Vec3::new(-5.0, 2.0, 5.0)), at(Vec3::new(2.0, -5.0, 5.0)));
    assert!(fill_side[3] == 255 && luma(fill_side) > 25.0, "the fill light lights the side the key misses: {fill_side:?}");
    assert!(luma(key_side) > luma(fill_side), "the key side is brighter: {key_side:?} vs {fill_side:?}");
}

/// With scene lights (no default lights) and the default black ambient,
/// what no light reaches renders black.
#[test]
fn scene_lit_renders_leave_unlit_areas_black() {
    let mut r = renderer!("scene_lit_renders_leave_unlit_areas_black");
    let plane = loom_mesh::plane_segmented(400.0, 400.0, 1, 1);
    r.sync(&[SceneObject { id: 1, mesh: &plane, wire_color: [255, 255, 255], surfaces: None, renderable: true, flags: Default::default() }]);
    let mut spot = loom_scene::light::default_lights(1, Mat4::IDENTITY)[0].clone();
    spot.id = loom_scene::NodeId(7);
    spot.position = Vec3::new(0.0, 0.0, 100.0);
    spot.direction = Vec3::new(0.0, 0.0, -1.0);
    spot.emitter = loom_scene::light::Emitter::Spot { hotspot: 20.0, falloff: 30.0 };
    r.sync_lights(std::slice::from_ref(&spot));
    assert!(!r.default_lighting());
    let cam = ViewCamera { view: Mat4::look_at_rh(Vec3::new(0.0, -1.0, 300.0), Vec3::ZERO, Vec3::Y), projection: Mat4::perspective_rh(60f32.to_radians(), 1.0, 0.5, 5000.0) };
    let size = (100, 100);
    let img = render(&mut r, &cam, size, RenderOptions::default(), 1);
    let at = |p: Vec3| {
        let (x, y) = pixel(&cam, size, p);
        img.get_pixel(x, y).0
    };
    let (inside, outside) = (at(Vec3::ZERO), at(Vec3::new(150.0, 150.0, 0.0)));
    assert!(luma(inside) > 80.0, "lit inside the cone: {inside:?}");
    assert_eq!((outside[0], outside[1], outside[2], outside[3]), (0, 0, 0, 255), "black outside it");
}

/// Volume Fog without gizmos fills the scene (not the space between the
/// camera and it): a box behind another is fogged about the same seen from
/// near or far away.
#[test]
fn volume_fog_fills_the_scene() {
    let mut r = renderer!("volume_fog_fills_the_scene");
    let front = moved(loom_mesh::box_primitive(Vec3::splat(10.0)), Vec3::new(-20.0, -40.0, 0.0));
    let back = moved(loom_mesh::box_primitive(Vec3::splat(10.0)), Vec3::new(20.0, 40.0, 0.0));
    r.sync(&[object(1, &front), object(2, &back)]);
    let size = (160, 100);
    let at_distance = |d: f32| ViewCamera { view: Mat4::look_at_rh(Vec3::new(0.0, -d, 5.0), Vec3::new(0.0, 0.0, 5.0), Vec3::Z), projection: Mat4::perspective_rh((40f32 * 100.0 / d).to_radians(), 1.6, 0.5, 50000.0) };
    let ratio = |r: &mut BevyRenderer, d: f32| {
        let cam = at_distance(d);
        let p = pixel(&cam, size, Vec3::new(20.0, 35.0, 5.0));
        r.set_environment(&EnvSettings::default());
        let plain = luma(render(r, &cam, size, RenderOptions::default(), 1).get_pixel(p.0, p.1).0);
        r.set_environment(&env(|e| e.fogs = vec![Fog::Volume { color: [0.0; 3], density: 10.0, fog_background: true }]));
        luma(render(r, &cam, size, RenderOptions::default(), 1).get_pixel(p.0, p.1).0) / plain.max(1.0)
    };
    let (near, far) = (ratio(&mut r, 100.0), ratio(&mut r, 1000.0));
    assert!(near < 0.8, "the back box is fogged: {near}");
    assert!((near - far).abs() < 0.1, "the same from ten times as far: {near} vs {far}");
}

/// Volume Light: a spot light's cone glows in the air.
#[test]
fn volume_light_glows_in_the_air() {
    let mut r = renderer!("volume_light_glows_in_the_air");
    let floor = loom_mesh::plane_segmented(400.0, 400.0, 1, 1);
    r.sync(&[object(1, &floor)]);
    let mut spot = loom_scene::light::default_lights(1, Mat4::IDENTITY)[0].clone();
    spot.id = loom_scene::NodeId(7);
    spot.position = Vec3::new(0.0, 0.0, 80.0);
    spot.direction = Vec3::new(0.0, 0.0, -1.0);
    spot.emitter = loom_scene::light::Emitter::Spot { hotspot: 20.0, falloff: 30.0 };
    spot.color = Vec3::splat(2.0);
    spot.shadows = true;
    r.sync_lights(std::slice::from_ref(&spot));
    let cam = ViewCamera { view: Mat4::look_at_rh(Vec3::new(0.0, -200.0, 40.0), Vec3::new(0.0, 0.0, 40.0), Vec3::Z), projection: Mat4::perspective_rh(50f32.to_radians(), 1.5, 0.5, 5000.0) };
    let size = (150, 100);
    let (x, y) = pixel(&cam, size, Vec3::new(0.0, 0.0, 50.0));
    let air = |img: &image::RgbaImage| luma(img.get_pixel(x, y).0);
    r.set_environment(&env(|e| e.ambient = [0.0; 3]));
    let clear = render(&mut r, &cam, size, RenderOptions::default(), 1);
    r.set_environment(&env(|e| {
        e.ambient = [0.0; 3];
        e.volume_lights = vec![VolumeLight { lights: vec![7], color: [1.0; 3], density: 5.0 }];
    }));
    let foggy = render(&mut r, &cam, size, RenderOptions::default(), 1);
    assert!(air(&foggy) > air(&clear) + 10.0, "the cone glows: {} -> {}", air(&clear), air(&foggy));
}
