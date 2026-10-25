use super::*;
use loom_scene::material::{Map, MaterialId, NavStep, StandardMaterial};
use std::time::{Duration, Instant};

fn image(name: &str, side: u32) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("loom-editor-preview-{}-{name}.png", std::process::id()));
    image::RgbaImage::from_pixel(side, side, image::Rgba([180, 40, 70, 255])).save(&path).unwrap();
    path
}
fn material(h: &mut Harness, slot: usize, path: &std::path::Path) {
    let id = MaterialId(slot as u32 + 1);
    let mut m = StandardMaterial::new(&format!("Preview {slot}"));
    m.diffuse_map = Some(Map::bitmap(path.to_string_lossy().into_owned(), 100.0));
    h.app.state.scene.materials.insert(id, m);
    h.app.state.scene.material_editor.slots[slot] = Some(id);
    h.app.state.material_editor_open = true;
}
fn wait(h: &mut Harness, mut done: impl FnMut(&Harness) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(h) {
        assert!(Instant::now() < deadline, "material image preview did not complete");
        h.frames(1);
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn material_sample_waits_for_actual_pixels_and_refreshes_after_completion() {
    let path = image("sample", 32);
    let mut h = Harness::new();
    material(&mut h, 0, &path);
    h.frames(1);
    let key = egui::Id::new(("material_preview", 0usize));
    assert!(h.app.material_previews.is_pending(path.to_str().unwrap()));
    assert!(h.ctx.data(|d| d.get_temp::<(String, egui::TextureHandle)>(key)).is_none(), "pending pixels must not become a cached black/fake map");
    wait(&mut h, |h| h.ctx.data(|d| d.get_temp::<(String, egui::TextureHandle)>(key)).is_some());
    let generation = h.app.material_preview_generation;
    assert!(generation > 0);
    h.app.state.scene.materials.get_mut(&MaterialId(1)).unwrap().diffuse_map.as_mut().unwrap().path = path.with_extension("missing").to_string_lossy().into_owned();
    h.frames(1);
    assert!(h.ctx.data(|d| d.get_temp::<(String, egui::TextureHandle)>(key)).is_none());
    wait(&mut h, |h| h.text_rect("Image error", SCREEN).is_some());
    std::fs::remove_file(path).unwrap();
}
#[test]
fn cropping_8k_source_is_async_bounded_and_keeps_normalized_region_and_source() {
    let path = image("crop8k", 8192);
    let bytes = std::fs::read(&path).unwrap();
    let mut h = Harness::new();
    material(&mut h, 0, &path);
    let crop = &mut h.app.state.scene.materials.get_mut(&MaterialId(1)).unwrap().diffuse_map.as_mut().unwrap().crop;
    (crop.u, crop.v, crop.w, crop.h) = (0.2, 0.1, 0.4, 0.5);
    let original = crop.clone();
    h.app.state.scene.material_editor.path = vec![NavStep::Map("Diffuse Color".into())];
    h.app.state.dialog = Some(loom_commands::Dialog::SpecifyCropping);
    h.frames(1);
    let key = egui::Id::new("cropping_image");
    assert!(h.ctx.data(|d| d.get_temp::<Option<egui::TextureHandle>>(key)).is_none(), "pending must never be permanently cached as None");
    wait(&mut h, |h| h.ctx.data(|d| d.get_temp::<(String, egui::TextureHandle)>(key)).is_some());
    let (_, texture) = h.ctx.data(|d| d.get_temp::<(String, egui::TextureHandle)>(key)).unwrap();
    assert!(texture.size().into_iter().all(|side| side <= 512));
    assert_eq!(h.app.state.scene.materials[&MaterialId(1)].diffuse_map.as_ref().unwrap().crop, original);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn editor_input_previews_share_quarter_viewport_budget_and_release_unused_paths() {
    let mut h = Harness::new();
    h.app.state.prefs.viewport_texture_budget_mb = 16;
    let paths: Vec<_> = (0..24).map(|slot| image(&format!("budget{slot}"), 32)).collect();
    for (slot, path) in paths.iter().enumerate() { material(&mut h, slot, path); }
    h.frames(1);
    assert!(h.app.material_preview_side <= 209, "24 previews share 4MiB, got {}", h.app.material_preview_side);
    wait(&mut h, |h| h.app.material_previews.pending_count() == 0);
    // Keep the same tier across eviction so a policy reset cannot mask stale paths.
    h.app.state.prefs.viewport_texture_max_size = 128;
    h.frames(1);
    wait(&mut h, |h| h.app.material_previews.pending_count() == 0);
    for slot in 1..24 { h.app.state.scene.material_editor.slots[slot] = None; }
    h.frames(1);
    assert!(h.app.material_previews.request(paths[1].to_str().unwrap(), h.app.material_preview_side, h.app.state.bitmap_generation).is_none(), "unused ready image must be evicted");
    for path in paths { std::fs::remove_file(path).unwrap(); }
}
