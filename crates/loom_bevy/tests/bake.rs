//! Render To Texture on a real GPU: a plane baked in its texture space, lit
//! by a spot light. Skips (passes with a message) without a GPU adapter.

use glam::{Mat4, Vec3};
use loom_bevy::bake::{BakeLook, BakeObject, BakeOptions};
use loom_bevy::{BevyRenderer, SceneObject};

fn gpu() -> Option<(wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
    loom_bevy::test_watchdog(std::time::Duration::from_secs(600));
    // No GL backend: an EGL context torn down while another test thread uses or
    // destroys its Vulkan device deadlocked inside the NVIDIA driver (itx stacks:
    // eglDestroyContext, vkDestroyDevice and a libvulkan mutex all waiting).
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends: wgpu::Backends::PRIMARY, ..Default::default() });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    if !loom_bevy::supported(&adapter) {
        return None;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&loom_bevy::device_descriptor(&adapter), None)).ok()?;
    Some((adapter, device, queue))
}

fn luminance(img: &image::RgbaImage, x: u32, y: u32) -> u32 {
    let p = img.get_pixel(x, y).0;
    p[0] as u32 + p[1] as u32 + p[2] as u32
}

/// A 200 x 200 plane mapped on channel 3 under a spot light at x = 50: its
/// LightingMap is bright under the light and dark away from it; a box over
/// the spot's centre shows in the ShadowsMap.
#[test]
fn a_lighting_map_is_brighter_under_the_spot_light() {
    let Some((adapter, device, queue)) = gpu() else {
        eprintln!("a_lighting_map_is_brighter_under_the_spot_light: no GPU adapter, skipped");
        return;
    };
    use loom_scene::light::{default_lights, Emitter};
    let mut r = BevyRenderer::new(adapter, device, queue);
    let mut plane = loom_mesh::plane_segmented(200.0, 200.0, 4, 4);
    loom_mesh::uvmap::auto_map(&mut plane, loom_mesh::uvmap::AutoMap::Planar, 3);
    let mut blocker = loom_mesh::box_primitive(Vec3::splat(4.0));
    let _ = loom_mesh::edit_ops::translate_mesh(&mut blocker, Vec3::new(50.0, 0.0, 50.0));
    let objects = |with_box: bool| {
        let mut v = vec![SceneObject { transform: Mat4::IDENTITY, id: 1, mesh: &plane, wire_color: [255, 255, 255], surfaces: None, renderable: true, flags: Default::default() }];
        if with_box {
            v.push(SceneObject { transform: Mat4::IDENTITY, id: 2, mesh: &blocker, wire_color: [255, 255, 255], surfaces: None, renderable: true, flags: Default::default() });
        }
        v
    };
    let mut spot = default_lights(1, Mat4::IDENTITY)[0].clone();
    spot.id = loom_scene::NodeId(10);
    spot.emitter = Emitter::Spot { hotspot: 30.0, falloff: 40.0 };
    spot.position = Vec3::new(50.0, 0.0, 100.0);
    spot.direction = Vec3::NEG_Z;
    spot.shadows = true;
    r.sync(&objects(false));
    r.sync_lights(std::slice::from_ref(&spot));
    let object = BakeObject { mesh: &plane, channel: 3, surfaces: None, wire_color: [255, 255, 255] };
    let options = BakeOptions { look: BakeLook::Lighting, shadows: true, direct: true, indirect: true, size: [64, 64], background: [0.0; 3] };
    let img = r.bake(&object, &options).expect("bake");
    assert_eq!(img.dimensions(), (64, 64));
    // Texel x 48 is world x 51.6 (under the spot), texel 16 world x -48.4.
    let (under, away) = (luminance(&img, 48, 32), luminance(&img, 16, 32));
    assert!(under > away + 120, "under the spot {under}, away {away}");
    // Texel 54 (world x 70.3) is still in the light pool.
    assert!(luminance(&img, 54, 32) > away + 60, "texel 54 lit: {}", luminance(&img, 54, 32));
    assert!(img.pixels().all(|p| p.0[3] == 255), "the plane fills its map");
    // Without the spot (the default key light lights everything evenly), no difference.
    r.sync_lights(&[]);
    let even = r.bake(&object, &options).expect("bake");
    let (a, b) = (luminance(&even, 48, 32), luminance(&even, 16, 32));
    assert!(a.abs_diff(b) < 12, "flat lighting {a} vs {b}");
    // Shadows: the box over the spot's centre shadows the middle of the light pool.
    r.sync(&objects(true));
    r.sync_lights(std::slice::from_ref(&spot));
    let shadows = r.bake(&object, &BakeOptions { look: BakeLook::Shadows, ..options }).expect("bake");
    let (shadowed, lit, unlit) = (shadows.get_pixel(48, 32).0[0], shadows.get_pixel(54, 32).0[0], shadows.get_pixel(10, 32).0[0]);
    assert!(shadowed < 80, "under the box: {shadowed}");
    assert!(lit > 200, "lit beside the box's shadow: {lit}");
    assert_eq!(unlit, 255, "outside the light there is no shadow");
}
