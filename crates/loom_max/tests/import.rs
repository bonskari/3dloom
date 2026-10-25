//! .max import. The real-scene tests read sample .max files from
//! $LOOM_TEST_ASSETS (files under <LOOM_TEST_ASSETS>/Models/; they are not in
//! the repository) and skip without it.

use std::path::Path;

fn sample(rel: &str) -> Option<Vec<u8>> {
    let Some(dir) = std::env::var_os("LOOM_TEST_ASSETS") else {
        eprintln!("skipped: LOOM_TEST_ASSETS is not set");
        return None;
    };
    let p = Path::new(&dir).join("Models").join(rel);
    match std::fs::read(&p) {
        Ok(b) => Some(b),
        Err(_) => {
            eprintln!("skipped: {} is not available", p.display());
            None
        }
    }
}

#[test]
fn max_import_rejects_garbage() {
    for bad in [&b""[..], b"garbage", &[0u8; 4096], b"\x89PNG\r\n\x1a\n0000000000000000"] {
        assert!(loom_max::read(bad).is_err());
    }
    // A compound-file signature with nothing behind it.
    let mut hdr = vec![0u8; 512];
    hdr[..8].copy_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
    assert!(loom_max::read(&hdr).is_err());
    let Some(full) = sample("Cone/TrafficCone.max") else { return };
    // Every truncation and a run of byte flips: an error or a document, never a panic.
    for cut in (0..full.len()).step_by(997) {
        let _ = loom_max::read(&full[..cut]);
    }
    let mut seed = 0x1234_5678u32;
    for _ in 0..200 {
        let mut b = full.clone();
        for _ in 0..16 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let i = seed as usize % b.len();
            b[i] ^= (seed >> 8) as u8 | 1;
        }
        let _ = loom_max::read(&b);
    }
}

#[test]
fn imports_editable_poly_with_uvs_and_material_ids() {
    let Some(b) = sample("Cone/TrafficCone.max") else { return };
    let doc = loom_max::read(&b).unwrap();
    assert_eq!(doc.nodes.len(), 1, "{:?}", doc.nodes.iter().map(|n| &n.name).collect::<Vec<_>>());
    let n = &doc.nodes[0];
    assert_eq!(n.name, "Box001");
    assert_eq!(n.parent, None);
    let loom_max::MaxKind::Mesh(m) = &n.kind else { panic!("{:?}", n.kind) };
    assert_eq!(m.face_count(), 196);
    assert!(m.vertex_count() >= 192);
    assert!(m.map(1).is_some(), "UV channel 1");
    let ids: std::collections::BTreeSet<u16> = (0..m.face_count() as u32).map(|f| m.faces()[f as usize].material_id).collect();
    assert!(ids.len() > 1, "more than one material id: {ids:?}");
    assert!(ids.iter().all(|i| *i >= 1));
    // Its pivot was moved off the geometry in the source scene: the object offset carries it.
    let off = n.offset.w_axis.truncate();
    assert!((off - glam::Vec3::new(-159.38, -25.37, 0.92)).length() < 0.05, "{off}");
}

#[test]
fn imports_nodes_and_hierarchy() {
    let Some(b) = sample("Truck/Turck.max") else { return };
    let doc = loom_max::read(&b).unwrap();
    assert!(doc.nodes.len() > 200, "{}", doc.nodes.len());
    let kids = doc.nodes.iter().filter(|n| n.parent.is_some()).count();
    assert!(kids > 100, "a hierarchy: {kids} linked nodes");
    let count = |f: &dyn Fn(&loom_max::MaxKind) -> bool| doc.nodes.iter().filter(|n| f(&n.kind)).count();
    assert!(count(&|k| matches!(k, loom_max::MaxKind::Box { .. })) >= 50);
    assert!(count(&|k| matches!(k, loom_max::MaxKind::Cylinder { .. })) >= 20);
    assert!(count(&|k| matches!(k, loom_max::MaxKind::Mesh(_))) >= 100, "Editable Mesh objects");
    assert!(count(&|k| matches!(k, loom_max::MaxKind::Dummy { .. })) >= 30);
    // Cylinder parameters as the file stores them: radius, height, sides.
    let cyl = doc.nodes.iter().find_map(|n| match n.kind { loom_max::MaxKind::Cylinder { radius, height, sides, .. } => Some((radius, height, sides)), _ => None }).unwrap();
    assert!(cyl.0 > 0.0 && cyl.1 != 0.0 && cyl.2 >= 3, "{cyl:?}");
    // Every node of every sample reads without an error.
    for f in ["Cone/TrafficCone.max", "Core_Lightning/Unit78.max", "Giganticore/Giganticore.max", "Mine/Obstacle.max", "Monster/Monster_HEad.max", "Monster/Monster_hand.max", "Pedestal/Pedestal.max", "Striker/Murmur.max", "Striker/StrikerBoy.max", "Truck/Model_Truck.max"] {
        let Some(b) = sample(f) else { continue };
        let d = loom_max::read(&b).unwrap_or_else(|e| panic!("{f}: {e}"));
        assert!(!d.nodes.is_empty(), "{f}");
        let broken: Vec<&String> = d.warnings.iter().filter(|w| w.contains("Editable") && !w.contains("modifier stack")).collect();
        assert!(broken.is_empty(), "{f}: {broken:?}");
    }
}

/// A compound file whose FAT points every chain back at itself, with 64 KiB
/// sectors and 109 FAT sectors: chains that loop must fail at once, not
/// read gigabytes.
#[test]
fn looping_sector_chains_are_errors_not_memory_blowups() {
    let sector = 1usize << 16;
    let mut f = vec![0u8; sector * 2];
    f[..8].copy_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
    f[24..26].copy_from_slice(&0x3Eu16.to_le_bytes());
    f[26..28].copy_from_slice(&4u16.to_le_bytes());
    f[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes());
    f[30..32].copy_from_slice(&16u16.to_le_bytes()); // sector shift
    f[32..34].copy_from_slice(&6u16.to_le_bytes());
    f[44..48].copy_from_slice(&109u32.to_le_bytes()); // FAT sectors
    f[48..52].copy_from_slice(&0u32.to_le_bytes()); // directory at sector 0
    f[56..60].copy_from_slice(&4096u32.to_le_bytes());
    f[60..64].copy_from_slice(&0u32.to_le_bytes()); // mini FAT at sector 0 too
    f[68..72].copy_from_slice(&0u32.to_le_bytes()); // DIFAT continues at sector 0: a loop
    for i in 0..109 {
        f[76 + i * 4..80 + i * 4].copy_from_slice(&0u32.to_le_bytes()); // every FAT sector is sector 0
    }
    // Sector 0 (at offset 65536) is all zeros: FAT entry 0 = 0, a self loop.
    assert!(loom_max::read(&f).is_err());
    // The same without the DIFAT loop: only the FAT chain loops.
    f[68..72].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes());
    assert!(loom_max::read(&f).is_err());
}

fn node<'a>(doc: &'a loom_max::MaxDoc, name: &str) -> &'a loom_max::MaxNode {
    doc.nodes.iter().find(|n| n.name == name).unwrap_or_else(|| panic!("no node {name}"))
}

fn param(m: &loom_max::MaxModifier, name: &str) -> f64 {
    m.params.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("{} has no {name}: {:?}", m.class, m.params)).1
}

fn classes(n: &loom_max::MaxNode) -> Vec<&str> {
    n.modifiers.iter().map(|m| m.class).collect()
}

#[test]
fn modifier_stacks_come_bottom_first_with_their_parameters() {
    let Some(b) = sample("Monster/Monster_HEad.max") else { return };
    let doc = loom_max::read(&b).unwrap();
    // The Symmetry sits on the half (its stored input bounds end at x = 0.2), the FFD and TurboSmooth above.
    let g = node(&doc, "Group01");
    assert_eq!(classes(g), ["Symmetry", "FFD 4x4x4", "TurboSmooth"]);
    let sym = &g.modifiers[0];
    assert_eq!((param(sym, "Mirror Axis"), param(sym, "Flip"), param(sym, "Slice Along Mirror"), param(sym, "Weld Seam")), (0.0, 1.0, 1.0, 1.0));
    assert!((param(sym, "Threshold") - 0.1).abs() < 1e-6);
    // Its context runs from object space to the pivot offset at the time: the gizmo comes back.
    let gz = sym.gizmo.unwrap().w_axis.truncate();
    assert!((gz - glam::Vec3::new(0.0, 0.39, 78.91)).length() < 0.02, "{gz}");
    assert_eq!(param(&g.modifiers[2], "Iterations"), 1.0);
    let lat = g.modifiers[1].lattice.as_ref().unwrap();
    assert_eq!((lat.dims, lat.points.len()), ([4, 4, 4], 64));
    assert!((lat.lo - glam::Vec3::new(-6.89, -14.38, -10.85)).length() < 0.02, "{}", lat.lo);
    // Control point (x 0, y 0, z 1) of the file (z runs fastest there) is index 16 here (x fastest).
    assert!((lat.points[16] - glam::Vec3::new(0.0, 0.11, 0.21)).length() < 0.01, "{}", lat.points[16]);
    // A Symmetry gizmo moved off the centre.
    let s = &node(&doc, "Sphere01").modifiers[0];
    assert!((s.gizmo.unwrap().w_axis.truncate() - glam::Vec3::new(-2.197, -1.8825, 5.1721)).length() < 1e-3);
    // An object Loom cannot read takes its modifiers' place in the report.
    let Some(b) = sample("Extra/ChampionsLegends.max") else { return };
    let doc = loom_max::read(&b).unwrap();
    assert!(node(&doc, "Text01").modifiers.is_empty());
    assert!(doc.warnings.iter().any(|w| w.contains("Text01") && w.contains("modifiers are not imported")), "{:?}", doc.warnings);
}

#[test]
fn deformers_maps_and_shapes_keep_their_settings() {
    let Some(b) = sample("Truck/Turck.max") else { return };
    let doc = loom_max::read(&b).unwrap();
    // Bend with its Limits: the stored bounds above it (x to 42.38, z to 16.56) are a 90° bend of
    // the first 20.98 of a 50-tall cylinder, so 5 is the upper limit and 4 the lower.
    let c = node(&doc, "Cylinder29");
    assert_eq!(classes(c), ["Bend"]);
    let bend = &c.modifiers[0];
    assert_eq!((param(bend, "Angle"), param(bend, "Direction"), param(bend, "Bend Axis"), param(bend, "Limit Effect"), param(bend, "Lower Limit")), (90.0, 0.0, 2.0, 1.0, 0.0));
    assert!((param(bend, "Upper Limit") - 20.98).abs() < 0.01);
    assert!(doc.warnings.iter().any(|w| w.starts_with("Cylinder29:") && w.contains("Edit Mesh is not imported")), "{:?}", doc.warnings);
    // UVW Map: cylindrical on a radius-4, 30-tall cylinder, alignment Z, channel 1.
    let uvw = &node(&doc, "Cylinder28").modifiers[0];
    assert_eq!((uvw.class, param(uvw, "Mapping"), param(uvw, "Alignment"), param(uvw, "Channel")), ("UVW Map", 1.0, 2.0, 1.0));
    assert!((param(uvw, "Length") - 8.008).abs() < 1e-3 && (param(uvw, "Height") - 30.03).abs() < 1e-3);
    assert!((uvw.gizmo.unwrap().w_axis.truncate() - glam::Vec3::new(0.0, 0.0, 15.0)).length() < 1e-3);
    // Box mapping on a flat 90 × 18.8 box: length is Y.
    let uvw = &node(&doc, "Box51").modifiers[0];
    assert_eq!(param(uvw, "Mapping"), 4.0);
    assert!((param(uvw, "Length") - 90.09).abs() < 0.01 && (param(uvw, "Width") - 18.78).abs() < 0.01);
    // Smooth: the threshold is stored in radians.
    let sm = &node(&doc, "Box53").modifiers[0];
    assert_eq!((param(sm, "Auto Smooth"), param(sm, "Smoothing Groups")), (0.0, 16.0));
    assert!((param(sm, "Threshold") - 30.0).abs() < 1e-3);
    // An editable spline with an Extrude.
    let circle = node(&doc, "Circle03");
    let loom_max::MaxKind::Shape(s) = &circle.kind else { panic!("{:?}", circle.kind) };
    assert_eq!((s.splines.len(), s.splines[0].knots.len(), s.splines[0].closed, s.steps), (1, 10, true, 10));
    assert_eq!(s.splines[0].knots[0].kind, loom_max::KnotType::BezierCorner);
    assert_eq!(classes(circle), ["Extrude"]);
    assert_eq!(param(&circle.modifiers[0], "Amount"), 5.0);
    // Lines with a Lathe whose axis is the object's Y, 0.59 off the pivot: the knots and the
    // object offset move so the axis goes through the pivot.
    let lathed: Vec<&loom_max::MaxNode> = doc.nodes.iter().filter(|n| classes(n) == ["Lathe"]).collect();
    assert!(lathed.len() >= 5, "{}", lathed.len());
    let n = lathed[0];
    let loom_max::MaxKind::Shape(s) = &n.kind else { panic!() };
    assert_eq!((s.splines[0].knots.len(), s.splines[0].closed), (5, false));
    let l = &n.modifiers[0];
    assert_eq!((param(l, "Degrees"), param(l, "Segments"), param(l, "Direction"), param(l, "Align")), (360.0, 20.0, 1.0, 3.0));
    assert!(l.gizmo.is_none());
    assert!((s.splines[0].knots[0].pos - glam::Vec3::new(0.72 - 0.59, 3.32, 0.0)).length() < 0.01, "{}", s.splines[0].knots[0].pos);
}

#[test]
fn unwrap_keeps_its_texture_vertices_and_xform_its_gizmo() {
    let Some(b) = sample("Core_Lightning/Unit89.max") else { return };
    let doc = loom_max::read(&b).unwrap();
    let n = node(&doc, "unit89_body1");
    assert_eq!(classes(n), ["Unwrap UVW", "XForm"]);
    let uv = n.modifiers[0].uvs.as_ref().unwrap();
    assert_eq!((uv.verts.len(), uv.faces.len()), (226, 192));
    // The first polygon of the Editable Poly below is the triangle 5, 4, 12.
    assert_eq!(uv.faces[0], (vec![198, 28, 142], vec![5, 4, 12]));
    let g = n.modifiers[1].gizmo.unwrap();
    assert!((g.x_axis.truncate().length() - 0.5).abs() < 1e-4, "{g}");
}

#[test]
fn materials_layers_lights_and_cameras() {
    let Some(b) = sample("Cone/TrafficCone.max") else { return };
    let doc = loom_max::read(&b).unwrap();
    let m = node(&doc, "Box001").material.unwrap();
    let loom_max::MaxMaterial::MultiSub { subs, .. } = &doc.materials[m] else { panic!("{:?}", doc.materials[m]) };
    assert_eq!(subs.iter().map(|s| s.id).collect::<Vec<_>>(), [1, 2]);
    let loom_max::MaxMaterial::Standard(red) = &doc.materials[subs[0].material.unwrap()] else { panic!() };
    assert_eq!(red.name, "_traffic_conecone_red");
    assert!((glam::Vec3::from(red.diffuse) - glam::Vec3::new(0.992, 0.259, 0.008)).length() < 2e-3);
    assert!((red.glossiness.unwrap() - 10.0).abs() < 1e-4 && (red.soften.unwrap() - 0.1).abs() < 1e-6 && (red.opacity.unwrap() - 100.0).abs() < 1e-4, "{red:?}");
    assert_eq!(doc.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["0"]);
    assert!(doc.nodes.iter().all(|n| n.layer == Some(0)));
    // A bitmap on the Diffuse channel names its file.
    if let Some(b) = sample("Extra/Arena_01.max") {
        let doc = loom_max::read(&b).unwrap();
        let loom_max::MaxMaterial::Standard(s) = &doc.materials[node(&doc, "Cylinder01").material.unwrap()] else { panic!() };
        let diffuse = s.maps.iter().find(|(ch, _)| *ch == 1).unwrap();
        assert!(diffuse.1.ends_with("BaseTexture_01.png"), "{}", diffuse.1);
    }
    // Turck's glass is 40 % opaque.
    if let Some(b) = sample("Truck/Turck.max") {
        let doc = loom_max::read(&b).unwrap();
        let glass = doc.materials.iter().find_map(|m| match m { loom_max::MaxMaterial::Standard(s) if s.name == "glass" => Some(s), _ => None }).unwrap();
        assert!((glass.opacity.unwrap() - 40.0).abs() < 1e-3);
    }
    let Some(b) = sample("Monster/Monster_HEad.max") else { return };
    let doc = loom_max::read(&b).unwrap();
    let d = node(&doc, "Direct02");
    let loom_max::MaxKind::Light { kind, params } = &d.kind else { panic!("{:?}", d.kind) };
    assert_eq!(*kind, "target_direct");
    let p = |n: &str| params.iter().find(|(k, _)| *k == n).unwrap().1;
    assert!((p("Hotspot") - 109.22).abs() < 1e-3 && (p("Falloff") - 114.3).abs() < 1e-3 && p("Multiplier") == 1.0);
    assert_eq!(d.look_at.map(|t| doc.nodes[t].name.as_str()), Some("Direct02.Target"));
    assert!(matches!(node(&doc, "Direct02.Target").kind, loom_max::MaxKind::Target));
    assert!((d.local.w_axis.truncate() - glam::Vec3::new(177.233, 118.559, 95.045)).length() < 1e-2);
    let loom_max::MaxKind::Camera { params } = &node(&doc, "Camera01").kind else { panic!() };
    let p = |n: &str| params.iter().find(|(k, _)| *k == n).unwrap().1;
    assert!((p("FOV") - 36.0).abs() < 1e-3 && (p("Near Clip") - 2.54).abs() < 1e-4 && p("Far Clip") == 2540.0);
}
