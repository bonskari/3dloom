//! The other material types on a real GPU: Double Sided backs, Top/Bottom
//! bands, Matte/Shadow showing only shadows, Ink 'n Paint's flat bands.
//! Skips (passes with a message) when the machine has no GPU adapter.

use glam::{Mat4, Vec3};
use loom_bevy::{BevyRenderer, SceneObject, ViewCamera};
use loom_scene::material::{surfaces, MaterialKind, StandardMaterial, Surfaces};
use loom_scene::material_types::{DoubleSided, TopBottom};
use loom_scene::{MaterialId, MaterialLibrary};

fn gpu() -> Option<(wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    if !loom_bevy::supported(&adapter) {
        return None;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&loom_bevy::device_descriptor(&adapter), None)).ok()?;
    Some((adapter, device, queue))
}

fn coloured(c: Vec3) -> StandardMaterial {
    let mut m = StandardMaterial::new("c");
    m.diffuse.set(c);
    m.ambient.set(c);
    m
}

/// A library: 1 red, 2 blue, 3 the material under test.
fn lib(top: StandardMaterial) -> MaterialLibrary {
    [(MaterialId(1), coloured(Vec3::X)), (MaterialId(2), coloured(Vec3::Z)), (MaterialId(3), top)].into_iter().collect()
}

fn camera(eye: Vec3, up: Vec3) -> ViewCamera {
    ViewCamera { view: Mat4::look_at_rh(eye, Vec3::ZERO, up), projection: Mat4::perspective_rh(40f32.to_radians(), 1.0, 0.5, 1000.0) }
}

/// One frame of view 5 (`size` square, cleared to `clear`, no MSAA) after a few updates.
fn shot(r: &mut BevyRenderer, cam: &ViewCamera, size: u32, clear: [f32; 4]) -> image::RgbaImage {
    for _ in 0..4 {
        r.set_view(5, [size, size], cam, clear, false);
        r.update();
    }
    r.read_view(5).unwrap()
}

fn object<'a>(mesh: &'a loom_mesh::Mesh, s: Option<Surfaces>, id: u64) -> SceneObject<'a> {
    SceneObject { id, mesh, wire_color: [200, 200, 200], surfaces: s, renderable: true, flags: Default::default() }
}

#[test]
fn double_sided_shows_its_back_material_from_behind_and_top_bottom_splits_a_box() {
    let Some((adapter, device, queue)) = gpu() else {
        eprintln!("double_sided...: no GPU adapter, skipped");
        return;
    };
    let mut r = BevyRenderer::new(adapter, device, queue);
    let mut ds = StandardMaterial::new("ds");
    ds.kind = MaterialKind::DoubleSided(Box::new(DoubleSided { materials: [Some(MaterialId(1)), Some(MaterialId(2))], enabled: [true; 2], translucency: 0.0 }));
    let l = lib(ds);
    let plane = loom_mesh::plane_segmented(20.0, 20.0, 1, 1);
    r.sync(&[object(&plane, surfaces(&l, MaterialId(3)), 1)]);
    let above = shot(&mut r, &camera(Vec3::new(0.0, -1.0, 40.0), Vec3::Y), 64, [0.0, 0.0, 0.0, 1.0]);
    let below = shot(&mut r, &camera(Vec3::new(0.0, -1.0, -40.0), Vec3::Y), 64, [0.0, 0.0, 0.0, 1.0]);
    let (a, b) = (above.get_pixel(32, 32).0, below.get_pixel(32, 32).0);
    assert!(a[0] > a[2] + 20, "from above the facing material (red): {a:?}");
    assert!(b[2] > b[0] + 5, "from below the back material (blue): {b:?}");

    let mut tb = StandardMaterial::new("tb");
    tb.kind = MaterialKind::TopBottom(Box::new(TopBottom { materials: [Some(MaterialId(1)), Some(MaterialId(2))], enabled: [true; 2], local: false, blend: 0.0, position: 75.0 }));
    let l = lib(tb);
    let cube = loom_mesh::box_primitive(Vec3::splat(5.0));
    r.sync(&[object(&cube, surfaces(&l, MaterialId(3)), 2)]);
    let top = shot(&mut r, &camera(Vec3::new(0.0, -1.0, 40.0), Vec3::Y), 64, [0.0, 0.0, 0.0, 1.0]).get_pixel(32, 32).0;
    let side = shot(&mut r, &camera(Vec3::new(0.0, -40.0, 0.0), Vec3::Z), 64, [0.0, 0.0, 0.0, 1.0]).get_pixel(32, 32).0;
    assert!(top[0] > top[2] + 20, "the top face is the top material: {top:?}");
    assert!(side[2] > side[0] + 5, "at Position 75 the sides are the bottom material: {side:?}");
}

#[test]
fn matte_shadow_shows_only_the_shadows_and_ink_n_paint_paints_flat_bands() {
    let Some((adapter, device, queue)) = gpu() else {
        eprintln!("matte_shadow...: no GPU adapter, skipped");
        return;
    };
    let mut r = BevyRenderer::new(adapter, device, queue);
    let mut matte = StandardMaterial::new("matte");
    matte.kind = MaterialKind::MatteShadow(Box::default());
    let l = lib(matte);
    let plane = loom_mesh::plane_segmented(200.0, 200.0, 1, 1);
    let mut blocker = loom_mesh::box_primitive(Vec3::splat(6.0));
    let _ = loom_mesh::edit_ops::translate_mesh(&mut blocker, Vec3::new(0.0, 0.0, 12.0));
    let grey = [0.5, 0.5, 0.5, 1.0];
    let cam = camera(Vec3::new(0.0, -1.0, 150.0), Vec3::Y);
    let dark = |img: &image::RgbaImage| img.pixels().filter(|p| p.0[0] < 90 && p.0[1] < 90 && p.0[2] < 90).count();
    // Without a caster the matte plane is invisible over the grey background.
    r.sync(&[object(&plane, surfaces(&l, MaterialId(3)), 1)]);
    let empty = shot(&mut r, &cam, 96, grey);
    let bg = empty.get_pixel(2, 2).0;
    let c = empty.get_pixel(48, 48).0;
    assert!((c[0] as i32 - bg[0] as i32).abs() <= 3, "the matte plane shows the background: {c:?} vs {bg:?}");
    // A box above it casts a dark shadow on it.
    r.sync(&[object(&plane, surfaces(&l, MaterialId(3)), 1), object(&blocker, None, 2)]);
    let shadowed = shot(&mut r, &cam, 96, grey);
    assert!(dark(&shadowed) > 40, "the shadow shows on the matte: {} dark pixels", dark(&shadowed));

    // Ink 'n Paint: a sphere in a few flat colours (two paint bands and the ink).
    let mut ink = StandardMaterial::new("ink");
    ink.kind = MaterialKind::InkPaint(Box::default());
    let l = lib(ink);
    let sphere = loom_mesh::sphere_segmented(10.0, 32, 0.0);
    r.sync(&[object(&sphere, surfaces(&l, MaterialId(3)), 3)]);
    let img = shot(&mut r, &camera(Vec3::new(-10.0, -40.0, 20.0), Vec3::Z), 96, [0.0, 0.0, 0.0, 1.0]);
    let mut colours: Vec<[u8; 3]> = img.pixels().filter(|p| p.0[0] > 0 || p.0[1] > 0 || p.0[2] > 0).map(|p| [p.0[0], p.0[1], p.0[2]]).collect();
    let covered = colours.len();
    colours.sort();
    colours.dedup();
    assert!(covered > 500, "the sphere shows: {covered}");
    assert!(colours.len() <= 4, "flat bands, not smooth shading: {} colours {:?}", colours.len(), &colours[..colours.len().min(8)]);
}
