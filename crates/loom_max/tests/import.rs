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
