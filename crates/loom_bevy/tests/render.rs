//! Headless render on a real GPU: a red box lit by the key light.
//! Skips (passes with a message) when the machine has no GPU adapter.

use glam::{Mat4, Vec3};
use loom_bevy::{BevyRenderer, SceneObject, ViewCamera};

fn gpu() -> Option<(wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    if !loom_bevy::supported(&adapter) {
        return None;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&loom_bevy::device_descriptor(&adapter), None)).ok()?;
    Some((adapter, device, queue))
}

#[test]
fn renders_a_lit_red_box() {
    let Some((adapter, device, queue)) = gpu() else {
        eprintln!("renders_a_lit_red_box: no GPU adapter, skipped");
        return;
    };
    let mut r = BevyRenderer::new(adapter, device, queue);
    let mesh = loom_mesh::box_primitive(Vec3::splat(5.0));
    r.sync(&[SceneObject { id: 1, mesh: &mesh, wire_color: [220, 30, 30], surfaces: None, renderable: true }]);
    let eye = Vec3::new(-18.0, -24.0, 16.0);
    let camera = ViewCamera { view: Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Z), projection: Mat4::perspective_rh(45f32.to_radians(), 1.0, 0.1, 1000.0) };
    let img = r.render_image(128, 128, &camera).expect("render");
    assert_eq!(img.dimensions(), (128, 128));
    let c = img.get_pixel(64, 64).0;
    assert!(c[0] > 60 && c[0] as u32 > 2 * c[1] as u32 && c[0] as u32 > 2 * c[2] as u32, "centre not red: {c:?}");
    // The background stays black.
    assert_eq!(&img.get_pixel(2, 2).0[..3], &[0, 0, 0]);
    // Faces turned to the key light are brighter than those turned away.
    let brightest = img.pixels().map(|p| p.0[0]).max().unwrap();
    let darkest_red = img.pixels().filter(|p| p.0[0] > 0).map(|p| p.0[0]).min().unwrap();
    assert!(brightest as i32 - darkest_red as i32 > 30, "no lighting contrast: {brightest} vs {darkest_red}");

    // A second render after a scene change reuses the app.
    let moved = {
        let mut m = loom_mesh::box_primitive(Vec3::splat(5.0));
        let _ = loom_mesh::edit_ops::translate_mesh(&mut m, Vec3::new(200.0, 0.0, 0.0));
        m
    };
    r.sync(&[SceneObject { id: 1, mesh: &moved, wire_color: [220, 30, 30], surfaces: None, renderable: true }]);
    let img = r.render_image(64, 64, &camera).expect("render");
    assert_eq!(&img.get_pixel(32, 32).0[..3], &[0, 0, 0], "the box moved out of view");
}

/// A viewport view keeps following its camera: after the first frame the
/// camera moves out and the whole teapot comes into view.
#[test]
fn a_view_follows_its_camera() {
    let Some((adapter, device, queue)) = gpu() else {
        eprintln!("a_view_follows_its_camera: no GPU adapter, skipped");
        return;
    };
    let mut r = BevyRenderer::new(adapter, device, queue);
    let mesh = loom_mesh::teapot_primitive(12.5, 4, Default::default());
    r.sync(&[SceneObject { id: 1, mesh: &mesh, wire_color: [220, 30, 30], surfaces: None, renderable: true }]);
    let target = Vec3::new(0.0, 0.0, 8.0);
    let at = |eye: Vec3| ViewCamera { view: Mat4::look_at_rh(eye, target, Vec3::Z), projection: Mat4::perspective_rh(45f32.to_radians(), 1.6, 0.05, 5000.0) };
    let close = at(Vec3::new(400.0, 0.0, 8.0));
    let framed = at(Vec3::new(-55.0, -55.0, 70.0));
    let red = |img: &image::RgbaImage| img.pixels().filter(|p| p.0[3] > 0 && p.0[0] > 2 * p.0[1]).count();
    for _ in 0..3 {
        r.set_view(3, [160, 100], &close, [0.0; 4], true);
        r.update();
    }
    let far = red(&r.read_view(3).unwrap());
    for _ in 0..3 {
        r.set_view(3, [160, 100], &framed, [0.0; 4], true);
        r.update();
    }
    let img = r.read_view(3).unwrap();
    let near = red(&img);
    assert!(near > far * 4 && near > 500, "the moved camera was not used: {far} -> {near} red pixels");
    // Transparent background around the object.
    assert_eq!(img.get_pixel(1, 1).0[3], 0);
}

fn render_buckets(r: &mut BevyRenderer, camera: &ViewCamera, size: u32, tile: u32) -> image::RgbaImage {
    r.begin_render(Default::default());
    let mut out = image::RgbaImage::new(size, size);
    let n = size / tile;
    for by in 0..n {
        for bx in 0..n {
            let f = |p: u32| p as f32 / size as f32;
            let sub = [f(bx * tile), f(by * tile), f((bx + 1) * tile), f((by + 1) * tile)];
            // Settle frames for the first bucket only, as the UI does: later
            // buckets render in the very update that set them.
            for _ in 0..if bx + by == 0 { 3 } else { 1 } {
                r.set_render_bucket(camera, 1.0, sub, [tile, tile]).unwrap();
                r.update();
            }
            let img = r.read_render().unwrap();
            image::imageops::replace(&mut out, &img, (bx * tile) as i64, (by * tile) as i64);
        }
    }
    r.end_render();
    out
}

/// Buckets of a frame join into the image a single full-frame render makes.
#[test]
fn buckets_join_into_the_full_frame() {
    let Some((adapter, device, queue)) = gpu() else {
        eprintln!("buckets_join_into_the_full_frame: no GPU adapter, skipped");
        return;
    };
    let mut r = BevyRenderer::new(adapter, device, queue);
    let mesh = loom_mesh::teapot_primitive(10.0, 4, Default::default());
    let floor = loom_mesh::box_primitive(Vec3::new(40.0, 40.0, 0.5));
    r.sync(&[
        SceneObject { id: 1, mesh: &mesh, wire_color: [220, 30, 30], surfaces: None, renderable: true },
        SceneObject { id: 2, mesh: &floor, wire_color: [200, 200, 200], surfaces: None, renderable: true },
    ]);
    let camera = ViewCamera { view: Mat4::look_at_rh(Vec3::new(-40.0, -50.0, 40.0), Vec3::new(0.0, 0.0, 5.0), Vec3::Z), projection: Mat4::perspective_rh(45f32.to_radians(), 1.0, 0.1, 1000.0) };
    let whole = r.render_image(128, 128, &camera).unwrap();
    let joined = render_buckets(&mut r, &camera, 128, 64);
    let bad = whole.pixels().zip(joined.pixels()).filter(|(a, b)| (0..4).any(|i| (a.0[i] as i32 - b.0[i] as i32).abs() > 24)).count();
    assert!(bad < 128 * 128 / 100, "{bad} pixels differ between the buckets and the full frame");
    assert!(joined.pixels().filter(|p| p.0[3] > 0 && p.0[0] as u32 > 2 * p.0[1] as u32).count() > 300, "the teapot shows");
}

/// Selected renders only the selection; helpers never render; hidden
/// objects only with Render Hidden Geometry.
#[test]
fn render_layers_pick_what_a_render_shows() {
    let Some((adapter, device, queue)) = gpu() else {
        eprintln!("render_layers_pick_what_a_render_shows: no GPU adapter, skipped");
        return;
    };
    let mut r = BevyRenderer::new(adapter, device, queue);
    let at = |x: f32, half: f32| {
        let mut m = loom_mesh::box_primitive(Vec3::splat(half));
        let _ = loom_mesh::edit_ops::translate_mesh(&mut m, Vec3::new(x, 0.0, 0.0));
        m
    };
    let (left, right, centre) = (at(-8.0, 3.0), at(8.0, 3.0), at(0.0, 2.0));
    // Red left box (renderable), green right box (a helper: viewports only).
    r.sync(&[
        SceneObject { id: 1, mesh: &left, wire_color: [230, 20, 20], surfaces: None, renderable: true },
        SceneObject { id: 2, mesh: &right, wire_color: [20, 230, 20], surfaces: None, renderable: false },
    ]);
    // A hidden blue box in the middle.
    r.sync_hidden(&[SceneObject { id: 3, mesh: &centre, wire_color: [20, 20, 230], surfaces: None, renderable: true }]);
    let camera = ViewCamera { view: Mat4::look_at_rh(Vec3::new(0.0, -40.0, 0.0), Vec3::ZERO, Vec3::Z), projection: Mat4::perspective_rh(45f32.to_radians(), 2.0, 0.1, 1000.0) };
    let count = |img: &image::RgbaImage, ch: usize| img.pixels().filter(|p| p.0[3] > 0 && p.0[ch] as u32 > 2 * p.0[(ch + 1) % 3] as u32 && p.0[ch] as u32 > 2 * p.0[(ch + 2) % 3] as u32).count();
    let shot = |r: &mut BevyRenderer, o: loom_bevy::RenderOptions| {
        r.begin_render(o);
        for _ in 0..4 {
            r.set_render_bucket(&camera, 2.0, [0.0, 0.0, 1.0, 1.0], [128, 64]).unwrap();
            r.update();
        }
        let img = r.read_render().unwrap();
        r.end_render();
        img
    };
    let plain = shot(&mut r, Default::default());
    assert!(count(&plain, 0) > 50, "the red box renders");
    assert_eq!(count(&plain, 1), 0, "a helper does not render");
    assert_eq!(count(&plain, 2), 0, "hidden objects do not render");
    let hidden = shot(&mut r, loom_bevy::RenderOptions { hidden: true, ..Default::default() });
    assert!(count(&hidden, 2) > 50, "Render Hidden Geometry shows the hidden box");
    r.sync(&[
        SceneObject { id: 1, mesh: &left, wire_color: [230, 20, 20], surfaces: None, renderable: true },
        SceneObject { id: 4, mesh: &centre, wire_color: [20, 20, 230], surfaces: None, renderable: true },
    ]);
    r.sync_hidden(&[]);
    let selected = shot(&mut r, loom_bevy::RenderOptions { selected_only: Some(vec![4]), ..Default::default() });
    assert!(count(&selected, 2) > 50 && count(&selected, 0) == 0, "Selected renders only the selection");
    // The background colour comes with alpha 0.
    let bg = shot(&mut r, loom_bevy::RenderOptions { background: [0.0, 0.0, 1.0], ..Default::default() });
    assert_eq!(bg.get_pixel(0, 0).0, [0, 0, 255, 0]);
}
