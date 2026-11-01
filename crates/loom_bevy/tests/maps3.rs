//! Material component maps on a real GPU: a Glossiness map sharpens the
//! highlight as the spinner would. Skips when the machine has no GPU adapter.

use glam::{Mat4, Vec3};
use loom_bevy::{BevyRenderer, RenderOptions, SceneObject, ViewCamera};
use loom_scene::light::{default_lights, Emitter};
use loom_scene::material::{Map, MapType, Surface, Surfaces};
use loom_scene::StandardMaterial;

fn gpu() -> Option<(wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    if !loom_bevy::supported(&adapter) {
        return None;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&loom_bevy::device_descriptor(&adapter), None)).ok()?;
    Some((adapter, device, queue))
}

/// A grey plane's brightest pixel under a point light beside the camera.
fn peak(r: &mut BevyRenderer, s: Surface) -> u8 {
    let mut plane = loom_mesh::plane_segmented(200.0, 200.0, 1, 1);
    loom_mesh::uvmap::auto_map(&mut plane, loom_mesh::uvmap::AutoMap::Planar, 1);
    r.sync(&[SceneObject { id: 1, mesh: &plane, wire_color: [200, 200, 200], surfaces: Some(Surfaces::Single(Box::new(s))), renderable: true, flags: Default::default() }]);
    let mut l = default_lights(1, Mat4::IDENTITY)[0].clone();
    l.id = loom_scene::NodeId(10);
    l.emitter = Emitter::Point;
    l.position = Vec3::new(0.0, -30.0, 120.0);
    l.color = Vec3::ONE;
    l.shadows = false;
    r.sync_lights(&[l]);
    let cam = ViewCamera { view: Mat4::look_at_rh(Vec3::new(0.0, 30.0, 120.0), Vec3::ZERO, Vec3::Y), projection: Mat4::perspective_rh(50f32.to_radians(), 1.0, 1.0, 1000.0) };
    let img = r.render_with(128, 128, &cam, RenderOptions { ambient: [0.0; 3], ..RenderOptions::default() }).unwrap();
    img.pixels().map(|p| p.0[0].max(p.0[1]).max(p.0[2])).max().unwrap()
}

fn shiny(gloss: f32, map: Option<f32>) -> Surface {
    let mut m = StandardMaterial::new("m");
    m.diffuse.set(Vec3::splat(0.2));
    m.specular_level.set(150.0);
    m.glossiness.set(gloss);
    if let Some(grey) = map {
        let mut g = Map::new(MapType::Checker, 100.0);
        g.colors = [Vec3::splat(grey), Vec3::splat(grey), Vec3::ONE];
        m.glossiness_map = Some(g);
    }
    Surface::from_standard(&m)
}

#[test]
fn a_glossiness_map_sets_the_highlight_like_the_spinner() {
    let Some((adapter, device, queue)) = gpu() else {
        eprintln!("glossiness map: no GPU adapter, skipped");
        return;
    };
    let mut r = BevyRenderer::new(adapter, device, queue);
    let dull = peak(&mut r, shiny(5.0, None));
    let sharp = peak(&mut r, shiny(80.0, None));
    let mapped_sharp = peak(&mut r, shiny(5.0, Some(0.8)));
    let mapped_dull = peak(&mut r, shiny(80.0, Some(0.05)));
    eprintln!("dull {dull} sharp {sharp} mapped_sharp {mapped_sharp} mapped_dull {mapped_dull}");
    assert!(sharp > dull + 30, "the spinner sharpens the highlight: {dull} vs {sharp}");
    assert!(mapped_sharp.abs_diff(sharp) < 12, "a white-ish map at Amount 100 is Glossiness 80: {mapped_sharp} vs {sharp}");
    assert!(mapped_dull.abs_diff(dull) < 12, "a dark map at Amount 100 is Glossiness 5: {mapped_dull} vs {dull}");
}
