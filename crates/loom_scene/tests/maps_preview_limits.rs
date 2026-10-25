use loom_scene::{
    maps::{self, Image, Images},
    material::MapType,
    Map,
};
use std::sync::Arc;
fn source() -> (Map, Images) {
    let map = Map::bitmap("preview-source.png", 100.);
    let mut images = Images::new();
    images.insert(
        map.path.clone(),
        Some(Arc::new(Image {
            width: 128,
            height: 128,
            rgba: vec![255; 128 * 128 * 4],
        })),
    );
    (map, images)
}
#[test]
fn placed_image_is_baked_within_preview_limit_before_allocation() {
    let (mut map, images) = source();
    map.crop.apply = true;
    map.crop.place = true;
    map.crop.w = 0.05;
    map.crop.h = 0.05;
    let result = maps::bake_with_limit(&map, &images, 128).unwrap();
    assert_eq!(
        (result.width, result.height, result.rgba.len()),
        (128, 128, 128 * 128 * 4)
    );
}
#[test]
fn nested_procedural_map_obeys_preview_limit() {
    let (bitmap, images) = source();
    let mut nested = Map::new(MapType::Mix, 100.);
    nested.sub_maps[0] = Some(Box::new(bitmap));
    let mut outer = Map::new(MapType::Mix, 100.);
    outer.sub_maps[0] = Some(Box::new(nested));
    let image = maps::bake_with_limit(&outer, &images, 64).unwrap();
    assert_eq!(
        (image.width, image.height, image.rgba.len()),
        (64, 64, 64 * 64 * 4)
    );
}
#[test]
fn zero_preview_budget_produces_no_bitmap_or_procedural_output() {
    let (map, images) = source();
    assert!(maps::bake_with_limit(&map, &images, 0).is_none());
    assert!(maps::bake_with_limit(&Map::new(MapType::Checker, 100.), &Images::new(), 0).is_none());
}
#[test]
fn legacy_bake_matches_its_original_limit() {
    let (map, images) = source();
    assert_eq!(
        maps::bake(&map, &images),
        maps::bake_with_limit(&map, &images, 2048)
    );
}
