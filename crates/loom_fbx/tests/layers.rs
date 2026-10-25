//! Display layers: written as CollectionExclusive objects, read back by ufbx.

use glam::Vec3;
use loom_fbx::{FbxDocument, FbxKind, FbxLayer, FbxNode, WriteOpts};

fn mesh_node(name: &str) -> FbxNode {
    FbxNode::new(name, FbxKind::Mesh { mesh: loom_mesh::box_segmented(10.0, 10.0, 10.0, [1, 1, 1]), materials: vec![] })
}

#[test]
fn display_layers_round_trip() {
    let doc = FbxDocument {
        nodes: vec![mesh_node("Crate"), mesh_node("Wall"), mesh_node("Barrel")],
        unit_metres: 0.01,
        anim: None,
        layers: vec![
            FbxLayer { name: "Props".into(), color: Vec3::new(1.0, 0.5, 0.0), visible: true, frozen: false, nodes: vec![0, 2] },
            FbxLayer { name: "Walls".into(), color: Vec3::new(0.0, 0.0, 1.0), visible: false, frozen: true, nodes: vec![1] },
        ],
        ..Default::default()
    };
    let bytes = loom_fbx::write(&doc, &WriteOpts::default());
    let scene = ufbx::load_memory(&bytes, ufbx::LoadOpts { strict: true, ..Default::default() }).expect("ufbx strict load");
    assert!(scene.metadata.warnings.is_empty(), "{:?}", scene.metadata.warnings.iter().map(|w| w.description.to_string()).collect::<Vec<_>>());
    assert_eq!(scene.display_layers.len(), 2);
    let back = loom_fbx::read(&bytes, 0.01).unwrap();
    let name_of = |i: usize| back.nodes[i].name.clone();
    let props = back.layers.iter().find(|l| l.name == "Props").expect("Props layer");
    let mut members: Vec<String> = props.nodes.iter().map(|i| name_of(*i)).collect();
    members.sort();
    assert_eq!(members, ["Barrel", "Crate"]);
    assert!(props.visible && !props.frozen);
    assert!((props.color - Vec3::new(1.0, 0.5, 0.0)).length() < 1e-4);
    let walls = back.layers.iter().find(|l| l.name == "Walls").expect("Walls layer");
    assert_eq!(walls.nodes.iter().map(|i| name_of(*i)).collect::<Vec<_>>(), ["Wall"]);
    assert!(!walls.visible && walls.frozen);
}
