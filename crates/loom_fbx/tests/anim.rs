//! FBX animation: the first anim stack read as per-frame local transforms,
//! and written back as an AnimationStack with linear curves (phase 8).

use glam::Vec3;
use loom_fbx::{FbxAnim, FbxDocument, FbxKind, FbxNode, FbxTake, WriteOpts};

const RIG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sitting_idle.fbx");

#[test]
fn reads_the_mixamo_take() {
    let doc = loom_fbx::read(&std::fs::read(RIG).unwrap(), 0.01).unwrap();
    let take = doc.anim.as_ref().expect("Sitting Idle has an animation stack");
    assert!(take.fps > 0.0);
    assert!(take.end > take.start + 10, "{}..{}", take.start, take.end);
    assert!(take.tracks.len() > 20, "{} animated nodes", take.tracks.len());
    let (i, a) = &take.tracks[0];
    assert_eq!(a.frames.len(), a.rotation_deg.len());
    assert_eq!(a.frames.len() as i32, take.end - take.start + 1);
    assert!(*i < doc.nodes.len());
}

#[test]
fn keyed_node_round_trips() {
    let mut n = FbxNode::new("Mover", FbxKind::Empty);
    n.translation = Vec3::new(1.0, 2.0, 3.0);
    let frames: Vec<f32> = (0..=20).map(|f| f as f32).collect();
    let anim = FbxAnim {
        translation: frames.iter().map(|f| Vec3::new(*f, 0.0, 2.0 * *f)).collect(),
        rotation_deg: frames.iter().map(|f| Vec3::new(0.0, 0.0, 9.0 * *f)).collect(),
        scale: frames.iter().map(|f| Vec3::splat(1.0 + 0.05 * *f)).collect(),
        frames,
    };
    let doc = FbxDocument { nodes: vec![n], unit_metres: 0.01, anim: Some(FbxTake { name: "Take 001".into(), fps: 30.0, start: 0, end: 20, tracks: vec![(0, anim.clone())] }), layers: Vec::new() };
    let bytes = loom_fbx::write(&doc, &WriteOpts::default());
    let back = loom_fbx::read(&bytes, 0.01).unwrap();
    let take = back.anim.expect("the take survives");
    assert_eq!((take.start, take.end), (0, 20));
    assert!((take.fps - 30.0).abs() < 1e-3);
    let (_, a) = &take.tracks[0];
    for f in [0usize, 7, 20] {
        assert!(a.translation[f].distance(anim.translation[f]) < 1e-3, "frame {f}: {:?}", a.translation[f]);
        assert!((a.rotation_deg[f].z - anim.rotation_deg[f].z).abs() < 0.01, "frame {f}: {:?}", a.rotation_deg[f]);
        assert!(a.scale[f].distance(anim.scale[f]) < 1e-3);
    }
}
