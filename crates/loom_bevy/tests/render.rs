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
    r.sync(&[SceneObject { id: 1, mesh: &mesh, wire_color: [220, 30, 30], surfaces: None, renderable: true, flags: Default::default() }]);
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
    r.sync(&[SceneObject { id: 1, mesh: &moved, wire_color: [220, 30, 30], surfaces: None, renderable: true, flags: Default::default() }]);
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
    r.sync(&[SceneObject { id: 1, mesh: &mesh, wire_color: [220, 30, 30], surfaces: None, renderable: true, flags: Default::default() }]);
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

/// Renders `camera` with `options` in `n` x `n` buckets, as the UI does.
fn bucketed(r: &mut BevyRenderer, camera: &ViewCamera, (w, h): (u32, u32), options: loom_bevy::RenderOptions, n: u32) -> image::RgbaImage {
    r.begin_render(options);
    let mut out = image::RgbaImage::new(w, h);
    let (tw, th) = (w / n, h / n);
    for by in 0..n {
        for bx in 0..n {
            let sub = [(bx * tw) as f32 / w as f32, (by * th) as f32 / h as f32, ((bx + 1) * tw) as f32 / w as f32, ((by + 1) * th) as f32 / h as f32];
            for _ in 0..if bx + by == 0 { 4 } else { 1 } {
                r.set_render_bucket(camera, w as f32 / h as f32, sub, [tw, th]).unwrap();
                r.update();
            }
            image::imageops::replace(&mut out, &r.read_render().unwrap(), (bx * tw) as i64, (by * th) as i64);
        }
    }
    r.end_render();
    out
}

/// A red spot light straight down onto a white plane: the scene light
/// replaces the default key light and lights only inside its cone (also
/// in a bucketed render); a box under it shadows the plane unless it is
/// excluded from shadow casting or Render Setup turns shadows off; the
/// Environment's ambient colour adds to the scene lights.
#[test]
fn a_red_spot_lights_a_plane_only_inside_its_cone() {
    let Some((adapter, device, queue)) = gpu() else {
        eprintln!("a_red_spot_lights_a_plane_only_inside_its_cone: no GPU adapter, skipped");
        return;
    };
    use loom_bevy::RenderOptions;
    use loom_scene::light::{default_lights, Emitter, Exclusion};
    let mut r = BevyRenderer::new(adapter, device, queue);
    let plane = loom_mesh::plane_segmented(400.0, 400.0, 1, 1);
    let mut blocker = loom_mesh::box_primitive(Vec3::splat(5.0));
    let _ = loom_mesh::edit_ops::translate_mesh(&mut blocker, Vec3::new(0.0, 0.0, 50.0));
    let objects = |with_box: bool| {
        let mut v = vec![SceneObject { id: 1, mesh: &plane, wire_color: [255, 255, 255], surfaces: None, renderable: true, flags: Default::default() }];
        if with_box {
            v.push(SceneObject { id: 2, mesh: &blocker, wire_color: [255, 255, 255], surfaces: None, renderable: true, flags: Default::default() });
        }
        v
    };
    let mut spot = default_lights(1, Mat4::IDENTITY)[0].clone();
    spot.id = loom_scene::NodeId(10);
    spot.emitter = Emitter::Spot { hotspot: 30.0, falloff: 40.0 };
    spot.position = Vec3::new(0.0, 0.0, 100.0);
    spot.direction = Vec3::NEG_Z;
    spot.color = Vec3::new(1.0, 0.0, 0.0);
    spot.shadows = true;
    // From the side, low enough to see the ground under the box.
    let eye = Vec3::new(150.0, 0.0, 60.0);
    let size = (160u32, 120u32);
    let camera = ViewCamera { view: Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Z), projection: Mat4::perspective_rh(50f32.to_radians(), size.0 as f32 / size.1 as f32, 1.0, 2000.0) };
    let pixel = |p: Vec3| {
        let c = camera.projection * camera.view * p.extend(1.0);
        let n = c.truncate() / c.w;
        (((n.x + 1.0) * 0.5 * size.0 as f32) as u32, ((1.0 - n.y) * 0.5 * size.1 as f32) as u32)
    };
    let at = |img: &image::RgbaImage, p: Vec3| {
        let (x, y) = pixel(p);
        img.get_pixel(x, y).0
    };
    // No ambient light: what the spot lights is all there is.
    let ambient = |c: [f32; 3]| loom_bevy::environment::EnvSettings { ambient: c, ..Default::default() };
    r.set_environment(&ambient([0.0; 3]));
    let dark = RenderOptions::default;
    let (inside_p, outside_p) = (Vec3::new(0.0, 10.0, 0.0), Vec3::new(-20.0, 100.0, 0.0));

    r.sync(&objects(false));
    r.sync_lights(std::slice::from_ref(&spot));
    assert!(!r.default_lighting(), "a scene light replaces the default lighting");
    for buckets in [1, 4] {
        let img = bucketed(&mut r, &camera, size, dark(), buckets);
        let (inside, outside) = (at(&img, inside_p), at(&img, outside_p));
        assert!(inside[0] > 80 && inside[1] < 30 && inside[2] < 30, "{buckets} buckets: red inside the cone: {inside:?}");
        assert!(outside[0] < 15 && outside[1] < 15 && outside[2] < 15, "{buckets} buckets: dark outside the cone: {outside:?}");
    }
    // The Environment's ambient colour adds to the scene lights.
    r.set_environment(&ambient([0.0, 0.0, 1.0]));
    let img = bucketed(&mut r, &camera, size, RenderOptions::default(), 1);
    let outside = at(&img, outside_p);
    assert!(outside[2] > 40 && outside[0] < 15, "a blue ambient light outside the cone: {outside:?}");
    r.set_environment(&ambient([0.0; 3]));

    // A box above the centre shadows it ...
    r.sync(&objects(true));
    r.sync_lights(std::slice::from_ref(&spot));
    let under = at(&bucketed(&mut r, &camera, size, dark(), 1), Vec3::ZERO);
    assert!(under[0] < 40, "the box shadows the centre: {under:?}");
    // ... unless Render Setup turns shadows off ...
    let under = at(&bucketed(&mut r, &camera, size, RenderOptions { shadows: false, ..dark() }, 1), Vec3::ZERO);
    assert!(under[0] > 80, "no shadows with Render Setup shadows off: {under:?}");
    // ... or the spot excludes the box from shadow casting.
    spot.exclusion = Exclusion { include: false, scope: 1, nodes: vec![loom_scene::NodeId(2)] };
    r.sync_lights(std::slice::from_ref(&spot));
    let under = at(&bucketed(&mut r, &camera, size, dark(), 1), Vec3::ZERO);
    assert!(under[0] > 80, "an excluded box casts no shadow: {under:?}");

    // No lights again: the default lighting comes back.
    r.sync_lights(&[]);
    assert!(r.default_lighting());
    let c = at(&bucketed(&mut r, &camera, size, dark(), 1), outside_p);
    assert!(c[0] > 60 && c[1] > 60 && c[2] > 60, "the default key light lights the plane white: {c:?}");
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
        SceneObject { id: 1, mesh: &mesh, wire_color: [220, 30, 30], surfaces: None, renderable: true, flags: Default::default() },
        SceneObject { id: 2, mesh: &floor, wire_color: [200, 200, 200], surfaces: None, renderable: true, flags: Default::default() },
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
        SceneObject { id: 1, mesh: &left, wire_color: [230, 20, 20], surfaces: None, renderable: true, flags: Default::default() },
        SceneObject { id: 2, mesh: &right, wire_color: [20, 230, 20], surfaces: None, renderable: false, flags: Default::default() },
    ]);
    // A hidden blue box in the middle.
    r.sync_hidden(&[SceneObject { id: 3, mesh: &centre, wire_color: [20, 20, 230], surfaces: None, renderable: true, flags: Default::default() }]);
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
        SceneObject { id: 1, mesh: &left, wire_color: [230, 20, 20], surfaces: None, renderable: true, flags: Default::default() },
        SceneObject { id: 4, mesh: &centre, wire_color: [20, 20, 230], surfaces: None, renderable: true, flags: Default::default() },
    ]);
    r.sync_hidden(&[]);
    let selected = shot(&mut r, loom_bevy::RenderOptions { selected_only: Some(vec![4]), ..Default::default() });
    assert!(count(&selected, 2) > 50 && count(&selected, 0) == 0, "Selected renders only the selection");
    // The Environment's background colour comes with alpha 0.
    r.set_environment(&loom_bevy::environment::EnvSettings { background: [0.0, 0.0, 1.0], ..Default::default() });
    let bg = shot(&mut r, Default::default());
    assert_eq!(bg.get_pixel(0, 0).0, [0, 0, 255, 0]);
}

/// Object Properties > Rendering Control: Cast / Receive Shadows, Visible
/// to Camera (the shadow stays) and Renderable (the shadow goes too).
#[test]
fn rendering_control_flags_reach_the_render() {
    let Some((adapter, device, queue)) = gpu() else {
        eprintln!("rendering_control_flags_reach_the_render: no GPU adapter, skipped");
        return;
    };
    let mut r = BevyRenderer::new(adapter, device, queue);
    let floor = loom_mesh::box_primitive(Vec3::new(40.0, 40.0, 0.5));
    let cube = {
        let mut m = loom_mesh::box_primitive(Vec3::splat(3.0));
        let _ = loom_mesh::edit_ops::translate_mesh(&mut m, Vec3::new(0.0, 0.0, 8.0));
        m
    };
    let camera = ViewCamera { view: Mat4::look_at_rh(Vec3::new(0.0, -1.0, 90.0), Vec3::ZERO, Vec3::Y), projection: Mat4::perspective_rh(45f32.to_radians(), 1.0, 0.1, 1000.0) };
    let mut shot = |cube_flags: loom_bevy::ObjectFlags, cube_renderable: bool, floor_flags: loom_bevy::ObjectFlags| {
        r.sync(&[
            SceneObject { id: 1, mesh: &cube, wire_color: [230, 20, 20], surfaces: None, renderable: cube_renderable, flags: cube_flags },
            SceneObject { id: 2, mesh: &floor, wire_color: [200, 200, 200], surfaces: None, renderable: true, flags: floor_flags },
        ]);
        r.render_image(128, 128, &camera).unwrap()
    };
    let red = |img: &image::RgbaImage| img.pixels().filter(|p| p.0[0] > 60 && p.0[0] as u32 > 2 * p.0[1] as u32).count();
    // Floor pixels (grey) much darker than the lit floor: the shadow.
    let shadow = |img: &image::RgbaImage| {
        let grey = |p: &&image::Rgba<u8>| p.0[3] > 0 && (p.0[0] as i32 - p.0[1] as i32).abs() < 12 && (p.0[1] as i32 - p.0[2] as i32).abs() < 12;
        let lit = img.pixels().filter(grey).map(|p| p.0[0]).max().unwrap_or(0) as f32;
        img.pixels().filter(grey).filter(|p| (p.0[0] as f32) < lit * 0.6).count()
    };
    let on = loom_bevy::ObjectFlags::default();
    let plain = shot(on, true, on);
    assert!(red(&plain) > 100, "the cube renders");
    assert!(shadow(&plain) > 100, "the cube shadows the floor: {}", shadow(&plain));
    let no_cast = shot(loom_bevy::ObjectFlags { cast_shadows: false, ..on }, true, on);
    assert!(red(&no_cast) > 100);
    assert!(shadow(&no_cast) < 20, "Cast Shadows off: no shadow ({})", shadow(&no_cast));
    let no_receive = shot(on, true, loom_bevy::ObjectFlags { receive_shadows: false, ..on });
    assert!(shadow(&no_receive) < 20, "Receive Shadows off on the floor: no shadow ({})", shadow(&no_receive));
    let unseen = shot(loom_bevy::ObjectFlags { visible_to_camera: false, ..on }, true, on);
    assert_eq!(red(&unseen), 0, "Visible to Camera off: the cube is not rendered");
    assert!(shadow(&unseen) > 100, "... but its shadow is ({})", shadow(&unseen));
    let unrenderable = shot(on, false, on);
    assert_eq!(red(&unrenderable), 0);
    assert!(shadow(&unrenderable) < 20, "Renderable off: no shadow either ({})", shadow(&unrenderable));
    let invisible = shot(loom_bevy::ObjectFlags { visibility: 0.0, ..on }, true, on);
    assert_eq!(red(&invisible), 0, "Visibility 0 leaves the cube out");
}
