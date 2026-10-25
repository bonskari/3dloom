use super::*;

#[test]
fn imported_animation_can_be_dragged_as_one_clip_and_undone_once() {
    let mut h=Harness::new();
    let path=std::env::temp_dir().join(format!("loom-clip-ui-{}.bvh",std::process::id()));
    std::fs::write(&path,"HIERARCHY ROOT Root { OFFSET 0 0 0 CHANNELS 6 Xposition Yposition Zposition Zrotation Yrotation Xrotation JOINT Head { OFFSET 0 10 0 CHANNELS 3 Zrotation Yrotation Xrotation End Site { OFFSET 0 5 0 } } } MOTION Frames: 2 Frame Time: 0.0333333333 0 0 0 0 0 0 0 0 0 10 0 0 0 0 0 0 0 0").unwrap();
    h.cmd(Command::ImportFile{path:Some(path.to_string_lossy().into_owned())});
    h.app.state.frame_range=(0,100);
    h.frames(3);
    let clip=h.app.state.scene.animation_clips[0].clone();
    let rect=h.find(egui::accesskit::Role::Button,&clip.name);
    h.drag(rect.center(),rect.center()+egui::vec2(80.0,0.0));
    assert!(h.app.state.scene.animation_clips[0].start>0.0,"whole clip moves on the timeline");
    let moved=h.app.state.scene.animation_clips[0].start;
    let keys=h.app.state.scene.node(clip.nodes[0]).unwrap().transform.position.keys().unwrap();
    assert_eq!(keys.tracks[0].keys[0].time,moved);
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.animation_clips[0].start,0.0);
    assert_eq!(h.app.state.scene.nodes().count(),2,"one undo keeps the imported skeleton");
    std::fs::remove_file(path).unwrap();
}
