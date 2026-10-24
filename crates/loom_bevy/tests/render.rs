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
    r.sync(&[SceneObject { id: 1, mesh: &mesh, wire_color: [220, 30, 30], material: None }]);
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
    r.sync(&[SceneObject { id: 1, mesh: &moved, wire_color: [220, 30, 30], material: None }]);
    let img = r.render_image(64, 64, &camera).expect("render");
    assert_eq!(&img.get_pixel(32, 32).0[..3], &[0, 0, 0], "the box moved out of view");
}

#[test]
fn debug_viewport_path() {
    let Some((adapter, device, queue)) = gpu() else { return };
    let mut r = BevyRenderer::new(adapter, device, queue);
    let mesh = loom_mesh::box_primitive(Vec3::splat(5.0));
    let mut plane = loom_mesh::box_primitive(Vec3::new(60.0, 60.0, 0.5));
    let _ = loom_mesh::edit_ops::translate_mesh(&mut plane, Vec3::new(0.0, 0.0, -5.5));
    r.sync(&[SceneObject { id: 1, mesh: &mesh, wire_color: [220, 30, 30], material: None }, SceneObject { id: 2, mesh: &plane, wire_color: [30, 200, 30], material: None }]);
    let eye = Vec3::new(-180.0, -240.0, 160.0);
    let camera = ViewCamera { view: Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Z), projection: Mat4::perspective_rh(45f32.to_radians(), 1.6, 0.33, 330000.0) };
    for _ in 0..5 {
        r.set_view(3, [504, 316], &camera, [0.0; 4], true);
        r.update();
    }
    r.debug_read(3).unwrap().save("/tmp/claude-1000/-home-b1s-dev-github-3dloom/133d9ffa-5cf9-4f2c-8165-69b12a73953d/scratchpad/dbg_view.png").unwrap();
    let img = r.render_image(504, 316, &camera).unwrap();
    img.save("/tmp/claude-1000/-home-b1s-dev-github-3dloom/133d9ffa-5cf9-4f2c-8165-69b12a73953d/scratchpad/dbg_render.png").unwrap();
}
