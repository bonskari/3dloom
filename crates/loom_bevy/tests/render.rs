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
    r.sync(&[SceneObject { id: 1, mesh: &mesh, wire_color: [220, 30, 30], surfaces: None }]);
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
    r.sync(&[SceneObject { id: 1, mesh: &moved, wire_color: [220, 30, 30], surfaces: None }]);
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
    r.sync(&[SceneObject { id: 1, mesh: &mesh, wire_color: [220, 30, 30], surfaces: None }]);
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
