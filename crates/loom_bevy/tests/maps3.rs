//! Material component maps on a real GPU: a Glossiness map sharpens the
//! highlight as the spinner would. Skips when the machine has no GPU adapter.

use glam::{Mat4, Vec3};
use loom_bevy::{BevyRenderer, RenderOptions, SceneObject, ViewCamera};
use loom_scene::light::{default_lights, Emitter};
use loom_scene::material::{Map, MapType, Surface, Surfaces};
use loom_scene::StandardMaterial;

fn gpu() -> Option<(wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
    loom_bevy::test_gpu()
}

/// A black plane's highlight under a point light beside the camera: its
/// peak and how many pixels are over half of it (its width).
fn peak(r: &mut BevyRenderer, s: Surface) -> (u8, usize) {
    let mut plane = loom_mesh::plane_segmented(200.0, 200.0, 1, 1);
    loom_mesh::uvmap::auto_map(&mut plane, loom_mesh::uvmap::AutoMap::Planar, 1);
    r.sync(&[SceneObject { id: 1, mesh: &plane, wire_color: [200, 200, 200], surfaces: Some(Surfaces::Single(Box::new(s))), renderable: true, flags: Default::default() }]);
    let mut l = default_lights(1, Mat4::IDENTITY)[0].clone();
    l.id = loom_scene::NodeId(10);
    l.emitter = Emitter::Point;
    l.position = Vec3::new(0.0, -30.0, 120.0);
    l.color = Vec3::splat(0.5);
    l.shadows = false;
    r.sync_lights(&[l]);
    let cam = ViewCamera { view: Mat4::look_at_rh(Vec3::new(0.0, 30.0, 120.0), Vec3::ZERO, Vec3::Y), projection: Mat4::perspective_rh(50f32.to_radians(), 1.0, 1.0, 1000.0) };
    let img = r.render_with(128, 128, &cam, RenderOptions { ambient: [0.0; 3], ..RenderOptions::default() }).unwrap();
    let v: Vec<u8> = img.pixels().map(|p| p.0[0].max(p.0[1]).max(p.0[2])).collect();
    let top = *v.iter().max().unwrap();
    (top, v.iter().filter(|x| **x > top / 2).count())
}

fn shiny(gloss: f32, map: Option<f32>) -> Surface {
    let mut m = StandardMaterial::new("m");
    m.diffuse.set(Vec3::ZERO);
    m.ambient.set(Vec3::ZERO);
    m.specular_level.set(40.0);
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
    eprintln!("dull {dull:?} sharp {sharp:?} mapped_sharp {mapped_sharp:?} mapped_dull {mapped_dull:?}");
    assert!(sharp.1 * 3 < dull.1, "the spinner narrows the highlight: {dull:?} vs {sharp:?}");
    let near = |a: (u8, usize), b: (u8, usize)| a.0.abs_diff(b.0) < 8 && a.1.abs_diff(b.1) * 10 <= b.1.max(10);
    assert!(near(mapped_sharp, sharp), "a light map at Amount 100 is Glossiness 80: {mapped_sharp:?} vs {sharp:?}");
    assert!(near(mapped_dull, dull), "a dark map at Amount 100 is Glossiness 5: {mapped_dull:?} vs {dull:?}");
}

/// A plane seen at a grazing angle, lit from above.
fn grazing(r: &mut BevyRenderer, s: Surface) -> image::RgbaImage {
    let mut plane = loom_mesh::plane_segmented(200.0, 200.0, 1, 1);
    loom_mesh::uvmap::auto_map(&mut plane, loom_mesh::uvmap::AutoMap::Planar, 1);
    r.sync(&[SceneObject { id: 2, mesh: &plane, wire_color: [200, 200, 200], surfaces: Some(Surfaces::Single(Box::new(s))), renderable: true, flags: Default::default() }]);
    let mut l = default_lights(1, Mat4::IDENTITY)[0].clone();
    l.id = loom_scene::NodeId(11);
    l.emitter = Emitter::Point;
    l.position = Vec3::new(0.0, 0.0, 200.0);
    l.color = Vec3::ONE;
    l.shadows = false;
    r.sync_lights(&[l]);
    let cam = ViewCamera { view: Mat4::look_at_rh(Vec3::new(0.0, -150.0, 60.0), Vec3::ZERO, Vec3::Z), projection: Mat4::perspective_rh(50f32.to_radians(), 1.0, 1.0, 1000.0) };
    r.render_with(128, 128, &cam, RenderOptions { ambient: [0.0; 3], ..RenderOptions::default() }).unwrap()
}

#[test]
fn a_displacement_map_shifts_the_texture_by_its_heights() {
    let Some((adapter, device, queue)) = gpu() else {
        eprintln!("displacement map: no GPU adapter, skipped");
        return;
    };
    let mut r = BevyRenderer::new(adapter, device, queue);
    let checker = |tiles: f32| {
        let mut c = Map::new(MapType::Checker, 100.0);
        c.coords.tiling = [tiles, tiles];
        c
    };
    let mut m = StandardMaterial::new("m");
    m.diffuse_map = Some(checker(8.0));
    let flat = grazing(&mut r, Surface::from_standard(&m));
    let mut d = m.clone();
    d.displacement_map = Some(checker(8.0));
    let displaced = grazing(&mut r, Surface::from_standard(&d));
    d.displacement_map.as_mut().unwrap().amount = 0.0;
    let zero = grazing(&mut r, Surface::from_standard(&d));
    let diff = |a: &image::RgbaImage, b: &image::RgbaImage| a.pixels().zip(b.pixels()).filter(|(p, q)| p.0[0].abs_diff(q.0[0]) > 60).count();
    eprintln!("displaced differs at {} px, Amount 0 at {} px", diff(&flat, &displaced), diff(&flat, &zero));
    assert!(diff(&flat, &displaced) > 300, "the raised squares shift the checker");
    assert!(diff(&flat, &zero) < 30, "Amount 0 displaces nothing");
}

#[test]
fn an_unlocked_ambient_map_darkens_the_ambient_light() {
    let Some((adapter, device, queue)) = gpu() else {
        eprintln!("ambient map: no GPU adapter, skipped");
        return;
    };
    let mut r = BevyRenderer::new(adapter, device, queue);
    let mut plane = loom_mesh::plane_segmented(200.0, 200.0, 1, 1);
    loom_mesh::uvmap::auto_map(&mut plane, loom_mesh::uvmap::AutoMap::Planar, 1);
    let cam = ViewCamera { view: Mat4::look_at_rh(Vec3::new(0.0, -1.0, 150.0), Vec3::ZERO, Vec3::Y), projection: Mat4::perspective_rh(50f32.to_radians(), 1.0, 1.0, 1000.0) };
    // One black light: no default key and fill, the ambient light alone.
    let mut l = default_lights(1, Mat4::IDENTITY)[0].clone();
    l.id = loom_scene::NodeId(12);
    l.color = Vec3::ZERO;
    l.shadows = false;
    r.sync_lights(&[l]);
    let mut shot = |m: &StandardMaterial| {
        r.sync(&[SceneObject { id: 3, mesh: &plane, wire_color: [200, 200, 200], surfaces: Some(Surfaces::Single(Box::new(Surface::from_standard(m)))), renderable: true, flags: Default::default() }]);
        r.render_with(32, 32, &cam, RenderOptions { ambient: [1.0; 3], ..RenderOptions::default() }).unwrap().get_pixel(16, 16).0[1]
    };
    let mut m = StandardMaterial::new("m");
    m.diffuse.set(Vec3::splat(0.8));
    m.ambient.set(Vec3::splat(0.8));
    let plain = shot(&m);
    let mut dark = Map::new(MapType::Checker, 100.0);
    dark.colors = [Vec3::splat(0.1), Vec3::splat(0.1), Vec3::ONE];
    m.ambient_map = Some(dark);
    let locked = shot(&m);
    m.ambient_map_locked = false;
    let mapped = shot(&m);
    eprintln!("plain {plain} locked {locked} mapped {mapped}");
    assert!(plain > 30, "ambient light shows: {plain}");
    assert_eq!(locked, plain, "locked: the Diffuse map serves, the Ambient one is unused");
    assert!((mapped as f32) < plain as f32 * 0.5, "a dark Ambient map darkens the ambient light: {plain} -> {mapped}");
}
