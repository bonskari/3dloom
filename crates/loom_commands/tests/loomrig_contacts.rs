use glam::{Quat, Vec3};
use loom_commands::{execute, AppState, Command};
use loom_scene::loomrig::{HumanoidMapping, LimbMapping};
use loom_scene::loomrig_contact::{ContactKey, ContactReference, ContactSpace, ContactTrack};
use loom_scene::{Node, NodeId, ObjectKind};

fn rigged() -> (AppState, NodeId, NodeId) {
    let mut s = AppState::new();
    let (root,end)=add_rig(&mut s);
    (s,root,end)
}
fn add_rig(s:&mut AppState) -> (NodeId,NodeId) {
    let root = s.scene.add_node(Node::new("root", ObjectKind::bone(1.)));
    let mut ids = vec![];
    for i in 0..3 {
        let mut n = Node::new("joint", ObjectKind::bone(10.));
        n.parent = Some(if i == 0 { root } else { ids[i - 1] });
        if i > 0 {
            n.transform.position.set(Vec3::X * 10.);
        }
        ids.push(s.scene.add_node(n));
    }
    let m = HumanoidMapping {
        root,
        pelvis: ids[0],
        spine: vec![],
        head: ids[2],
        limbs: vec![LimbMapping {
            name: "Arm".into(),
            start: ids[0],
            middle: ids[1],
            end: ids[2],
        }],
    };
    execute(
        s,
        Command::CreateLoomRig {
            root: root.0,
            mapping: Some(m),
        },
    )
    .unwrap();
    (root, ids[2])
}
fn reference(space: ContactSpace, position: Vec3) -> ContactReference {
    ContactReference {
        space,
        position,
        rotation: Quat::from_rotation_x(0.3),
        pole_position: Vec3::new(0., 0., 20.),
    }
}
fn key(time: f32, reference: u64, blend: f32) -> ContactKey {
    ContactKey {
        time,
        reference,
        blend,
    }
}

#[test]
fn set_keys_preserves_authored_contact_keys_and_target_edits_as_one_undo() {
    let (mut s, _, _) = rigged();
    let rig = s.scene.loom_rigs[0].id;
    let limb = s.scene.loom_rigs[0].limbs[0].clone();
    execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
    execute(&mut s, Command::CreateLoomRigContactKey {
        rig, limb: limb.mapping.name.clone(), time: 10., preset: "planted".into(),
    }).unwrap();
    let before = s.scene.loom_rigs.clone();
    execute(&mut s, Command::SetKeyMode).unwrap();
    let target = s.scene.world_matrix(limb.goal).w_axis.truncate() + Vec3::Y;
    execute(&mut s, Command::SetTransform {
        id: limb.goal.0, position: Some(target.to_array()), rotation_deg: None, scale: None,
    }).unwrap();
    let posed = s.scene.loom_rigs.clone();
    let position = s.set_key_buffer.as_ref().unwrap().position;
    execute(&mut s, Command::SetKeys).unwrap();
    assert_eq!(s.scene.loom_rigs, posed, "Set Keys must preserve explicit contact authoring");
    assert_eq!(s.undo.position(), position + 1);
    execute(&mut s, Command::SetKeyMode).unwrap();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, before);
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, posed);
}

#[test]
fn contact_pose_buffer_respects_ik_filter_and_explicit_discard() {
    for discard in [false, true] {
        let (mut s, _, _) = rigged();
        let rig = s.scene.loom_rigs[0].id;
        execute(&mut s, Command::CreateLoomRigContactKey {
            rig, limb: "Arm".into(), time: 0., preset: "planted".into(),
        }).unwrap();
        let before = s.scene.loom_rigs.clone();
        let goal = s.scene.loom_rigs[0].limbs[0].goal;
        execute(&mut s, Command::SetKeyMode).unwrap();
        let target = s.scene.world_matrix(goal).w_axis.truncate() + Vec3::Y;
        execute(&mut s, Command::SetTransform {
            id: goal.0, position: Some(target.to_array()), rotation_deg: None, scale: None,
        }).unwrap();
        assert_ne!(s.scene.loom_rigs, before);
        if discard {
            execute(&mut s, Command::ClearSetKeyModeBuffer).unwrap();
        } else {
            s.key_filters.ik_parameters = false;
            execute(&mut s, Command::SetKeys).unwrap();
        }
        assert_eq!(s.scene.loom_rigs, before);
    }
}
fn point(s: &AppState, id: NodeId, time: f32) -> Vec3 {
    s.scene.at_time(time, || {
        s.scene.world_matrix(id).transform_point3(Vec3::ZERO)
    })
}

#[test]
fn rig_pose_group_move_delete_and_undo_preserve_source_and_solved_pose() {
    let (mut s,_,_)=rigged();
    execute(&mut s,Command::SetLoomRigContactPreset {rig:1,limb:"Arm".into(),time:0.,preset:"free".into()}).unwrap();
    execute(&mut s,Command::KeyLoomRigPose {rig:1}).unwrap();
    execute(&mut s,Command::SetFrame {frame:10}).unwrap();
    execute(&mut s,Command::SetKeyMode).unwrap();
    let control=s.scene.loom_rigs[0].bindings[1].control;
    execute(&mut s,Command::SetTransform {id:control.0,position:None,rotation_deg:Some([0.,0.,12.]),scale:None}).unwrap();
    execute(&mut s,Command::KeyLoomRigPose {rig:1}).unwrap();
    assert_eq!(s.scene.loom_rigs[0].pose_frames.times(),&[0.,10.]);
    let original_rigs=s.scene.loom_rigs.clone();
    let original_nodes:Vec<_>=s.scene.nodes().map(|node|(node.id,node.transform.clone())).collect();
    let sources:Vec<_>=s.scene.nodes().filter(|node|node.object.is_bone()).map(|node|(node.id,node.transform.clone(),s.scene.world_matrix(node.id))).collect();
    let history=s.undo.position();
    execute(&mut s,Command::MoveLoomRigPoseKey {rig:1,from:10.,to:25.5}).unwrap();
    assert_eq!(s.undo.position(),history+1);
    let rig=&s.scene.loom_rigs[0];
    assert_eq!(rig.pose_frames.times(),&[0.,25.5]);
    assert!(rig.limbs[0].contacts.keys().iter().any(|key|key.time == 25.5));
    assert!(rig.limbs[0].pose_matches.keys().iter().any(|key|key.time == 25.5));
    for binding in &rig.bindings {
        for param in [&s.scene.node(binding.control).unwrap().transform.position,&s.scene.node(binding.control).unwrap().transform.rotation,&s.scene.node(binding.control).unwrap().transform.scale] {
            assert!(param.keys().unwrap().tracks.iter().all(|track|track.key_at(10.).is_none() && track.key_at(25.5).is_some()));
        }
    }
    s.scene.at_time(25.5,|| {for (id,transform,world) in &sources {assert_eq!(&s.scene.node(*id).unwrap().transform,transform);assert!(s.scene.world_matrix(*id).abs_diff_eq(*world,0.001));}});
    execute(&mut s,Command::DeleteLoomRigPoseKey {rig:1,time:25.5}).unwrap();
    assert_eq!(s.scene.loom_rigs[0].pose_frames.times(),&[0.]);
    assert!(!s.scene.loom_rigs[0].limbs[0].contacts.keys().iter().any(|key|key.time == 25.5));
    assert!(!s.scene.loom_rigs[0].limbs[0].pose_matches.keys().iter().any(|key|key.time == 25.5));
    execute(&mut s,Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs[0].pose_frames.times(),&[0.,25.5]);
    execute(&mut s,Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs,original_rigs);
    for (id,transform) in original_nodes {assert_eq!(s.scene.node(id).unwrap().transform,transform);}
}

#[test]
fn rig_pose_group_edit_rejects_collisions_locks_and_invalid_frames_atomically() {
    for case in 0..7 {
        let (mut s,_,_)=rigged();
        execute(&mut s,Command::KeyLoomRigPose {rig:1}).unwrap();
        let control=s.scene.loom_rigs[0].bindings[1].control;
        if case == 0 {
            execute(&mut s,Command::SetFrame {frame:20}).unwrap();
            execute(&mut s,Command::KeyLoomRigPose {rig:1}).unwrap();
        } else if case == 1 {
            loom_scene::anim::with(20.,false,||s.scene.node_mut(control).unwrap().transform.rotation.key_now());
        } else if case == 2 {
            let track=loom_scene::tracks::node_tracks(s.scene.node(control).unwrap()).into_iter().find(|track|track.pointer == "/rotation").unwrap();
            s.scene.track_data.set_flags(track.key(),loom_scene::tracks::TrackFlags {locked:true,..Default::default()});
        }
        execute(&mut s,Command::SetFrame {frame:10}).unwrap();execute(&mut s,Command::SetKeyMode).unwrap();
        execute(&mut s,Command::SetTransform {id:control.0,position:None,rotation_deg:Some([5.,0.,0.]),scale:None}).unwrap();
        let nodes:Vec<_>=s.scene.nodes().map(|node|(node.id,node.transform.clone())).collect();
        let rigs=s.scene.loom_rigs.clone();let history=s.undo.position();let baseline=s.set_key_buffer.as_ref().unwrap().position;
        assert!(execute(&mut s,Command::MoveLoomRigPoseKey {rig:1,from:if case == 3 {99.} else {0.},to:match case {4=>f32::NAN,5=>1e30,6=>-1e30,_=>20.}}).is_err());
        assert_eq!(s.scene.loom_rigs,rigs);assert_eq!(s.undo.position(),history);assert_eq!(s.set_key_buffer.as_ref().unwrap().position,baseline);
        for (id,transform) in nodes {assert_eq!(s.scene.node(id).unwrap().transform,transform);}
    }
}

#[test]
fn rig_pose_frames_survive_mapping_and_clear_when_all_rig_animation_is_frozen() {
    let (mut s,_,_)=rigged();
    execute(&mut s,Command::KeyLoomRigPose {rig:1}).unwrap();
    let mapping=s.scene.loom_rigs[0].mapping.clone();
    execute(&mut s,Command::SetLoomRigMapping {rig:1,mapping}).unwrap();
    assert_eq!(s.scene.loom_rigs[0].pose_frames.times(),&[0.]);
    let rig=s.scene.loom_rigs[0].clone();
    s.selection=rig.bindings.iter().map(|binding|binding.control).chain([rig.master]).chain(rig.limbs.iter().flat_map(|limb|[limb.goal,limb.pole])).collect();
    execute(&mut s,Command::DeleteSelectedAnimation).unwrap();
    assert!(s.scene.loom_rigs[0].pose_frames.is_empty());
    execute(&mut s,Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs[0].pose_frames.times(),&[0.]);
    execute(&mut s,Command::NewSceneKeep {keep:"objects_and_hierarchy".into()}).unwrap();
    assert!(s.scene.loom_rigs[0].pose_frames.is_empty());
}

#[test]
fn rig_pose_freeze_retains_only_markers_with_remaining_locked_channel_keys() {
    let (mut s,_,_)=rigged();
    for frame in [0,10] {execute(&mut s,Command::SetFrame {frame}).unwrap();execute(&mut s,Command::KeyLoomRigPose {rig:1}).unwrap();}
    let rig=s.scene.loom_rigs[0].clone();let control=rig.bindings[1].control;
    for track in &mut s.scene.node_mut(control).unwrap().transform.rotation.ensure_keys().unwrap().tracks {track.keys.retain(|key|key.time == 0.);}
    let path=loom_scene::tracks::node_tracks(s.scene.node(control).unwrap()).into_iter().find(|path|path.pointer == "/rotation").unwrap();
    s.scene.track_data.set_flags(path.key(),loom_scene::tracks::TrackFlags {locked:true,..Default::default()});
    s.selection=rig.bindings.iter().map(|binding|binding.control).chain([rig.master]).chain(rig.limbs.iter().flat_map(|limb|[limb.goal,limb.pole])).collect();
    execute(&mut s,Command::DeleteSelectedAnimation).unwrap();
    assert_eq!(s.scene.loom_rigs[0].pose_frames.times(),&[0.]);
    assert!(s.scene.node(control).unwrap().transform.rotation.keys().unwrap().tracks.iter().all(|track|track.key_at(0.).is_some()));
    execute(&mut s,Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs[0].pose_frames.times(),&[0.,10.]);
}

#[test]
fn rig_pose_key_validation_keeps_a_live_buffer_on_error() {
    for protected in [false,true] {
        let (mut s,_,_)=rigged();
        let control=s.scene.loom_rigs[0].bindings[1].control;
        if protected {
            let path=loom_scene::tracks::node_tracks(s.scene.node(control).unwrap()).into_iter().find(|path|path.pointer == "/rotation").unwrap();
            s.scene.track_data.set_flags(path.key(),loom_scene::tracks::TrackFlags {locked:true,..Default::default()});
        } else {
            s.scene.loom_rigs[0].limbs[0].blend.controller=loom_scene::param::Controller::Noise(Default::default());
        }
        s.scene.bump_version();
        execute(&mut s,Command::SetFrame {frame:10}).unwrap();execute(&mut s,Command::SetKeyMode).unwrap();
        execute(&mut s,Command::SetTransform {id:control.0,position:None,rotation_deg:Some([5.,0.,0.]),scale:None}).unwrap();
        let nodes:Vec<_>=s.scene.nodes().map(|node|(node.id,node.transform.clone())).collect();
        let rigs=s.scene.loom_rigs.clone();let history=s.undo.position();let baseline=s.set_key_buffer.as_ref().unwrap().position;
        assert!(execute(&mut s,Command::KeyLoomRigPose {rig:1}).is_err());
        assert_eq!(s.undo.position(),history);assert_eq!(s.set_key_buffer.as_ref().unwrap().position,baseline);
        assert_eq!(s.scene.loom_rigs,rigs);
        for (id,transform) in nodes {assert_eq!(s.scene.node(id).unwrap().transform,transform);}
        execute(&mut s,Command::ClearSetKeyModeBuffer).unwrap();
        assert!(s.scene.node(control).unwrap().transform.rotation.get().abs_diff_eq(Vec3::ZERO,1e-6));
    }
}

#[test]
fn rig_pose_key_rejects_xref_controls_without_losing_pending_pose() {
    let (mut s,_,_)=rigged();
    let control=s.scene.loom_rigs[0].bindings[1].control;
    let referenced=s.scene.loom_rigs[0].master;
    s.scene.xrefs.push(loom_scene::XrefRecord {
        path:"referenced-rig.loom".into(),scene:true,automatic:false,
        nodes:vec![("Master".into(),referenced)],
    });
    execute(&mut s,Command::SetKeyMode).unwrap();
    execute(&mut s,Command::SetTransform {id:control.0,position:None,rotation_deg:Some([5.,0.,0.]),scale:None}).unwrap();
    let nodes:Vec<_>=s.scene.nodes().map(|node|(node.id,node.transform.clone())).collect();
    let rigs=s.scene.loom_rigs.clone();
    let history=s.undo.position();let baseline=s.set_key_buffer.as_ref().unwrap().position;
    let error=execute(&mut s,Command::KeyLoomRigPose {rig:1}).unwrap_err();
    assert!(error.contains("XRef Scene"),"{error}");
    assert_eq!(s.undo.position(),history);
    assert_eq!(s.set_key_buffer.as_ref().unwrap().position,baseline);
    assert_eq!(s.scene.loom_rigs,rigs);
    for (id,transform) in nodes {assert_eq!(s.scene.node(id).unwrap().transform,transform);}
    execute(&mut s,Command::ClearSetKeyModeBuffer).unwrap();
    assert!(s.scene.node(control).unwrap().transform.rotation.get().abs_diff_eq(Vec3::ZERO,1e-6));
}

#[test]
fn rig_pose_key_is_scoped_but_commits_other_pending_set_key_edits() {
    for buffered in [false,true] {
        let (mut s,_,_)=rigged();add_rig(&mut s);
        let other=s.scene.loom_rigs[1].clone();
        let control=other.bindings[1].control;
        let original=s.scene.node(control).unwrap().transform.clone();
        execute(&mut s,Command::SetFrame {frame:10}).unwrap();
        if buffered {
            execute(&mut s,Command::SetKeyMode).unwrap();
            execute(&mut s,Command::SetTransform {id:control.0,position:None,rotation_deg:Some([0.,0.,7.]),scale:None}).unwrap();
        }
        execute(&mut s,Command::KeyLoomRigPose {rig:1}).unwrap();
        assert_eq!(s.scene.loom_rigs[1],other);
        if buffered {
            execute(&mut s,Command::SetFrame {frame:20}).unwrap();
            assert!(s.scene.node(control).unwrap().transform.rotation.get_at(10.).abs_diff_eq(Vec3::new(0.,0.,7.),1e-6));
            assert!(s.scene.node(control).unwrap().transform.rotation.keys().is_some());
        } else {assert_eq!(s.scene.node(control).unwrap().transform,original);}
    }
}

#[test]
fn rig_pose_key_captures_owned_channels_without_rewriting_source_bones() {
    let (mut s,_,_) = rigged();
    let rig = s.scene.loom_rigs[0].clone();
    let sources:Vec<_> = s.scene.nodes().filter(|node|node.object.is_bone()).map(|node|(node.id,node.transform.clone())).collect();
    let control = rig.bindings.iter().find(|binding|binding.bone == rig.mapping.pelvis).unwrap().control;
    execute(&mut s,Command::SetTransform {id:control.0,position:None,rotation_deg:Some([0.,0.,15.]),scale:None}).unwrap();
    execute(&mut s,Command::SetFrame {frame:10}).unwrap();
    let pose:Vec<_> = sources.iter().map(|(id,_)|(*id,s.scene.world_matrix(*id))).collect();
    let before = s.scene.loom_rigs.clone();
    let nodes:Vec<_> = s.scene.nodes().map(|node|(node.id,node.transform.clone())).collect();
    let undo = s.undo.position();
    execute(&mut s,Command::KeyLoomRigPose {rig:rig.id}).unwrap();
    assert_eq!(s.undo.position(),undo+1);
    for binding in &rig.bindings {
        let transform = &s.scene.node(binding.control).unwrap().transform;
        for param in [&transform.position,&transform.rotation,&transform.scale] {
            assert!(param.keys().unwrap().tracks.iter().all(|track|track.key_at(10.).is_some()));
        }
    }
    for limb in &s.scene.loom_rigs[0].limbs {
        assert!(limb.blend.keys().unwrap().tracks[0].key_at(10.).is_some());
        assert!(limb.pose_offsets[..3].iter().all(|param|param.keys().unwrap().tracks.iter().all(|track|track.key_at(10.).is_some())));
    }
    for (id,expected) in &pose {assert!(s.scene.world_matrix(*id).abs_diff_eq(*expected,0.001));}
    for (id,transform) in sources {assert_eq!(s.scene.node(id).unwrap().transform,transform);}
    let path=std::env::temp_dir().join(format!("loom-full-rig-pose-{}.loom",std::process::id()));
    execute(&mut s,Command::SaveFileAsCopy {path:Some(path.to_string_lossy().into_owned())}).unwrap();
    let mut reopened=AppState::new();
    execute(&mut reopened,Command::OpenFile {path:Some(path.to_string_lossy().into_owned())}).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(reopened.scene.loom_rigs,s.scene.loom_rigs);
    reopened.scene.at_time(10.,|| {for (id,expected) in &pose {assert!(reopened.scene.world_matrix(*id).abs_diff_eq(*expected,0.001));}});
    execute(&mut s,Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs,before);
    for (id,transform) in nodes {assert_eq!(s.scene.node(id).unwrap().transform,transform);}
}

#[test]
fn rig_pose_key_commits_a_buffered_contact_and_fk_pose_as_one_key_step() {
    let (mut s,_,_) = rigged();
    execute(&mut s,Command::SetLoomRigContactPreset {rig:1,limb:"Arm".into(),time:0.,preset:"planted".into()}).unwrap();
    execute(&mut s,Command::KeyLoomRigPose {rig:1}).unwrap();
    execute(&mut s,Command::SetFrame {frame:10}).unwrap();
    execute(&mut s,Command::SetKeyMode).unwrap();
    let rig = s.scene.loom_rigs[0].clone();
    let control = rig.bindings.iter().find(|binding|binding.bone == rig.mapping.pelvis).unwrap().control;
    execute(&mut s,Command::SetTransform {id:control.0,position:None,rotation_deg:Some([5.,0.,0.]),scale:None}).unwrap();
    execute(&mut s,Command::SetTransform {id:rig.limbs[0].goal.0,position:Some([12.,8.,0.]),rotation_deg:None,scale:None}).unwrap();
    let pose = [rig.limbs[0].mapping.start,rig.limbs[0].mapping.middle,rig.limbs[0].mapping.end].map(|id|(id,s.scene.world_matrix(id)));
    let start = s.set_key_buffer.as_ref().unwrap().position;
    let sources:Vec<_> = s.scene.nodes().filter(|node|node.object.is_bone()).map(|node|(node.id,node.transform.clone())).collect();
    execute(&mut s,Command::KeyLoomRigPose {rig:1}).unwrap();
    assert_eq!(s.undo.position(),start+1);
    let rotation = &s.scene.node(control).unwrap().transform.rotation;
    assert!(rotation.get_at(0.).abs_diff_eq(Vec3::ZERO,1e-6));
    assert!(rotation.get_at(10.).abs_diff_eq(Vec3::new(5.,0.,0.),1e-6));
    assert!(s.scene.loom_rigs[0].limbs[0].contacts.keys().iter().any(|key|key.time == 10.));
    for (id,expected) in pose {assert!(s.scene.world_matrix(id).abs_diff_eq(expected,0.001));}
    for (id,transform) in sources {assert_eq!(s.scene.node(id).unwrap().transform,transform);}
    execute(&mut s,Command::SetFrame {frame:20}).unwrap();
    assert!(s.scene.node(control).unwrap().transform.rotation.get_at(20.).abs_diff_eq(Vec3::new(5.,0.,0.),1e-6));
}

#[test]
fn explicit_contact_presets_replace_one_key_and_keying_preserves_target_and_source() {
    let (mut s,root,_) = rigged();
    for (time,preset) in [(0.,"planted"),(10.,"sliding")] {
        execute(&mut s,Command::SetLoomRigContactPreset {rig:1,limb:"Arm".into(),time,preset:preset.into()}).unwrap();
        if time == 0. {
            let goal = s.scene.loom_rigs[0].limbs[0].goal;
            execute(&mut s,Command::SetTransform {id:goal.0,position:Some([12.,8.,0.]),rotation_deg:None,scale:None}).unwrap();
        }
    }
    execute(&mut s,Command::SetFrame {frame:10}).unwrap();
    let limb = s.scene.loom_rigs[0].limbs[0].clone();
    let ids = [limb.mapping.start,limb.mapping.middle,limb.mapping.end];
    let pose = ids.map(|id|s.scene.world_matrix(id));
    let nodes:Vec<_> = s.scene.nodes().map(|n|(n.id,n.transform.clone())).collect();
    let before = s.scene.loom_rigs.clone();
    let undo = s.undo.position();
    execute(&mut s,Command::SetLoomRigContactPreset {rig:1,limb:"Arm".into(),time:10.,preset:"free".into()}).unwrap();
    assert_eq!(s.undo.position(),undo+1);
    assert_eq!(s.scene.loom_rigs[0].limbs[0].contacts.keys()[0],before[0].limbs[0].contacts.keys()[0]);
    assert_eq!(s.scene.loom_rigs[0].limbs[0].contacts.keys()[1].blend,0.);
    for (id,expected) in ids.iter().zip(pose) {assert!(s.scene.world_matrix(*id).abs_diff_eq(expected,0.001));}
    let free = s.scene.loom_rigs.clone();
    execute(&mut s,Command::SetLoomRigContactPreset {rig:1,limb:"Arm".into(),time:10.,preset:"planted".into()}).unwrap();
    for (id,expected) in ids.iter().zip(pose) {assert!(s.scene.world_matrix(*id).abs_diff_eq(expected,0.001));}
    execute(&mut s,Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs,free);
    execute(&mut s,Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs,before);
    assert!(execute(&mut s,Command::SetLoomRigContactPreset {rig:1,limb:"Arm".into(),time:10.,preset:"unknown".into()}).is_err());
    assert_eq!(s.scene.loom_rigs,before);
    assert!(execute(&mut s,Command::CreateLoomRigContactKey {rig:1,limb:"Arm".into(),time:10.,preset:"free".into()}).is_err());
    assert_eq!(s.scene.loom_rigs,before);
    let target = s.scene.at_time(5.,||s.scene.contact_control(limb.goal).unwrap().unwrap().sample);
    execute(&mut s,Command::KeyLoomRigContact {rig:1,limb:"Arm".into(),time:5.}).unwrap();
    let after = s.scene.loom_rigs[0].limbs[0].contacts.sample(5.,s.scene.world_matrix(root)).unwrap().unwrap();
    assert!(after.position.abs_diff_eq(target.position,0.001));
    assert!(after.pole_position.abs_diff_eq(target.pole_position,0.001));
    assert!(after.rotation.dot(target.rotation).abs()>1.-1e-5);
    assert!((after.blend-target.blend).abs()<1e-6);
    assert_eq!(s.current_frame,10);
    assert_eq!(s.scene.time(),10.);
    for (id,transform) in nodes {assert_eq!(s.scene.node(id).unwrap().transform,transform);}
    execute(&mut s,Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs,before);
    assert!(execute(&mut s,Command::KeyLoomRigContact {rig:1,limb:"Arm".into(),time:-1.}).is_err());
    assert_eq!(s.scene.loom_rigs,before);
}

#[test]
fn auto_key_contact_weight_before_a_future_key_keeps_constant_fallback() {
    let (mut s,_,_) = rigged();
    let mut track = ContactTrack::constant(reference(ContactSpace::World,Vec3::new(12.,8.,0.)),1.).unwrap();
    track.insert_key(key(20.,1,0.8)).unwrap();
    s.scene.loom_rigs[0].limbs[0].contacts = track;
    s.scene.bump_version();
    execute(&mut s,Command::SetFrame {frame:10}).unwrap();
    execute(&mut s,Command::AutoKeyModeToggle).unwrap();
    execute(&mut s,Command::SetLoomRigLimb {rig:1,limb:"Arm".into(),solver:"limb".into(),blend:0.4}).unwrap();
    let track = &s.scene.loom_rigs[0].limbs[0].contacts;
    assert_eq!(track.keys(),&[key(0.,1,1.),key(10.,1,0.4),key(20.,1,0.8)]);
    assert_eq!(track.sample_blend(-5.).unwrap(),Some(1.));
    assert!((track.sample_blend(5.).unwrap().unwrap()-0.7).abs()<1e-6);
}

#[test]
fn active_contact_solver_switch_preserves_twisted_limb_pose_and_anchor() {
    let (mut s,_,_) = rigged();
    let limb = s.scene.loom_rigs[0].limbs[0].clone();
    s.scene.node_mut(limb.mapping.start).unwrap().transform.rotation.set(Vec3::new(25.,0.,0.));
    s.scene.node_mut(limb.mapping.middle).unwrap().transform.rotation.set(Vec3::new(-15.,0.,0.));
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::constant(reference(ContactSpace::World,Vec3::new(12.,8.,0.)),1.).unwrap();
    s.scene.bump_version();
    let ids = [limb.mapping.start,limb.mapping.middle,limb.mapping.end];
    let visible = ids.map(|id|s.scene.world_matrix(id));
    let contacts = s.scene.loom_rigs[0].limbs[0].contacts.clone();
    execute(&mut s,Command::SetLoomRigLimb {rig:1,limb:"Arm".into(),solver:"hi".into(),blend:1.}).unwrap();
    assert_eq!(s.scene.loom_rigs[0].limbs[0].contacts,contacts);
    for (id,expected) in ids.iter().zip(visible) {
        let actual = s.scene.world_matrix(*id);
        assert!(actual.abs_diff_eq(expected,0.003),"solver switch changed {id:?}: expected {expected:?}; actual {actual:?}");
    }
}

#[test]
fn auto_key_limb_weight_adds_current_contact_key_without_rewriting_other_keys() {
    let (mut s,root,_) = rigged();
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [(1,reference(ContactSpace::World,Vec3::new(12.,8.,0.)))].into(),
        vec![key(0.,1,1.),key(20.,1,1.)],
    ).unwrap();
    s.scene.bump_version();
    execute(&mut s,Command::SetFrame {frame:10}).unwrap();
    execute(&mut s,Command::AutoKeyModeToggle).unwrap();
    execute(&mut s,Command::SetLoomRigLimb {rig:1,limb:"Arm".into(),solver:"limb".into(),blend:0.4}).unwrap();
    let track = &s.scene.loom_rigs[0].limbs[0].contacts;
    assert_eq!(track.keys(),&[key(0.,1,1.),key(10.,1,0.4),key(20.,1,1.)]);
    assert!((track.sample(10.,s.scene.world_matrix(root)).unwrap().unwrap().blend-0.4).abs()<1e-6);
}

#[test]
fn auto_key_contact_weight_retains_interpolated_targets_and_constant_baselines() {
    for (constant,next_space) in [(true,ContactSpace::Body),(false,ContactSpace::Body),(false,ContactSpace::World)] {
        let (mut s,root,_) = rigged();
        s.scene.node_mut(root).unwrap().transform.scale.set(Vec3::new(-2.,3.,0.5));
        let pose = reference(ContactSpace::Body,Vec3::new(12.,8.,0.));
        s.scene.loom_rigs[0].limbs[0].contacts = if constant {
            ContactTrack::constant(pose,1.).unwrap()
        } else {
            ContactTrack::new([(1,pose),(2,reference(next_space,Vec3::new(8.,12.,1.)))].into(),vec![key(0.,1,1.),key(20.,2,0.8)]).unwrap()
        };
        s.scene.bump_version();
        execute(&mut s,Command::SetFrame {frame:10}).unwrap();
        let original = s.scene.loom_rigs[0].limbs[0].contacts.clone();
        let target = original.sample(10.,s.scene.world_matrix(root)).unwrap().unwrap();
        execute(&mut s,Command::AutoKeyModeToggle).unwrap();
        execute(&mut s,Command::SetLoomRigLimb {rig:1,limb:"Arm".into(),solver:"limb".into(),blend:0.4}).unwrap();
        let track = &s.scene.loom_rigs[0].limbs[0].contacts;
        let after = track.sample(10.,s.scene.world_matrix(root)).unwrap().unwrap();
        assert!(after.position.abs_diff_eq(target.position,0.001));
        assert!(after.pole_position.abs_diff_eq(target.pole_position,0.001));
        assert!(after.rotation.dot(target.rotation).abs()>1.-1e-5);
        assert!((after.blend-0.4).abs()<1e-6);
        for (id,reference) in original.references() {assert_eq!(&track.references()[id],reference);}
        if constant {
            assert_eq!(track.sample_blend(0.).unwrap(),Some(1.));
            assert!((track.sample_blend(5.).unwrap().unwrap()-0.7).abs()<1e-6);
        } else {
            assert_eq!(track.keys()[0],original.keys()[0]);
            assert_eq!(track.keys()[2],original.keys()[1]);
        }
    }
}

#[test]
fn limb_settings_edit_effective_contact_weight_and_release_fk_without_source_edits() {
    for constant in [false,true] {
        for blend in [0.3,0.] {
            let (mut s,root,_) = rigged();
            let pose = reference(ContactSpace::World,Vec3::new(12.,8.,0.));
            s.scene.loom_rigs[0].limbs[0].contacts = if constant {
                ContactTrack::constant(pose,1.).unwrap()
            } else {
                ContactTrack::new([(1,pose)].into(),vec![key(0.,1,1.),key(20.,1,1.)]).unwrap()
            };
            s.scene.bump_version();
            execute(&mut s,Command::SetFrame {frame:10}).unwrap();
            let limb = s.scene.loom_rigs[0].limbs[0].clone();
            let ids = [limb.mapping.start,limb.mapping.middle,limb.mapping.end];
            let visible = ids.map(|id|s.scene.world_matrix(id));
            let nodes:Vec<_> = s.scene.nodes().map(|n|(n.id,n.transform.clone())).collect();
            let before = s.scene.loom_rigs.clone();
            let undo = s.undo.position();
            execute(&mut s,Command::SetLoomRigLimb {rig:1,limb:"Arm".into(),solver:"limb".into(),blend}).unwrap();
            let track = &s.scene.loom_rigs[0].limbs[0].contacts;
            let sampled = track.sample(10.,s.scene.world_matrix(root)).unwrap().unwrap();
            assert!((sampled.blend-blend).abs()<1e-6,"settings must change the evaluated IK weight");
            assert_eq!(track.references(),limb.contacts.references(),"weight edits must retain contact anchors");
            for (id,transform) in nodes {assert_eq!(s.scene.node(id).unwrap().transform,transform);}
            if blend == 0. {
                for (id,expected) in ids.iter().zip(visible) {
                    assert!(s.scene.world_matrix(*id).abs_diff_eq(expected,0.001),"release changed {id:?}");
                }
                if constant {assert!(s.scene.loom_rigs[0].limbs[0].pose_matches.keys().is_empty());}
            }
            assert_eq!(s.undo.position(),undo+1);
            execute(&mut s,Command::UndoSceneOperation).unwrap();
            assert_eq!(s.scene.loom_rigs,before);
        }
    }
}

#[test]
fn creating_free_contact_preserves_the_solved_pose_and_source_tracks() {
    for scale in [Vec3::ONE,Vec3::new(2.,3.,0.5),Vec3::new(-2.,3.,0.5)] {
        let (mut s, root, _) = rigged();
        let mut parent = Node::new("affine parent",ObjectKind::dummy(1.));
        parent.transform.scale.set(scale);
        parent.transform.rotation.set(Vec3::new(0.,0.,17.));
        let parent = s.scene.add_node(parent);
        s.scene.node_mut(root).unwrap().parent = Some(parent);
        s.scene.node_mut(root).unwrap().transform.rotation.set(Vec3::new(0.,0.,23.));
        let limb = s.scene.loom_rigs[0].limbs[0].clone();
        let target = s.scene.world_matrix(root).transform_point3(Vec3::new(12.,8.,0.));
        s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
            [(1, reference(ContactSpace::World, target))].into(),
            vec![key(0., 1, 1.)],
        ).unwrap();
        s.scene.bump_version();
        execute(&mut s, Command::SetFrame { frame: 10 }).unwrap();
        let ids = [limb.mapping.start, limb.mapping.middle, limb.mapping.end];
        let before: Vec<_> = ids.iter().map(|id|s.scene.world_matrix(*id)).collect();
        let source: Vec<_> = s.scene.nodes().map(|n|(n.id,n.transform.clone())).collect();
        let before_rigs = s.scene.loom_rigs.clone();
        execute(&mut s, Command::CreateLoomRigContactKey {
            rig: 1, limb: "Arm".into(), time: 10., preset: "free".into(),
        }).unwrap();
        for (id, expected) in ids.iter().zip(&before) {
            assert!(s.scene.world_matrix(*id).abs_diff_eq(*expected, 0.001), "Free changed {id:?}");
        }
        for (id, transform) in source {
            assert_eq!(s.scene.node(id).unwrap().transform, transform);
        }
        assert!(s.scene.contact_errors().is_empty());
        let authored = s.scene.loom_rigs.clone();
        execute(&mut s, Command::UndoSceneOperation).unwrap();
        assert_eq!(s.scene.loom_rigs,before_rigs);
        execute(&mut s, Command::RedoSceneOperation).unwrap();
        assert_eq!(s.scene.loom_rigs,authored);
        execute(&mut s, Command::MoveLoomRigContactKey {rig:1,limb:"Arm".into(),from:10.,to:20.}).unwrap();
        assert!(s.scene.loom_rigs[0].limbs[0].pose_matches.keys().iter().any(|key|key.time == 20.));
        s.scene.at_time(20.,|| {
            for (id,expected) in ids.iter().zip(&before) {
                assert!(s.scene.world_matrix(*id).abs_diff_eq(*expected,0.001),"moved Free changed {id:?}");
            }
        });
        execute(&mut s, Command::UndoSceneOperation).unwrap();
        assert_eq!(s.scene.loom_rigs,authored);
        execute(&mut s, Command::DeleteLoomRigContactKey {rig:1,limb:"Arm".into(),time:10.}).unwrap();
        assert!(!s.scene.loom_rigs[0].limbs[0].pose_matches.keys().iter().any(|key|key.time == 10.));
        execute(&mut s, Command::UndoSceneOperation).unwrap();
        assert_eq!(s.scene.loom_rigs,authored);
        let path = std::env::temp_dir().join(format!("loom-free-match-{}-{}.loom",std::process::id(),scale.x.to_bits()));
        execute(&mut s,Command::SaveFileAsCopy {path:Some(path.to_string_lossy().into_owned())}).unwrap();
        let mut reopened = AppState::new();
        execute(&mut reopened,Command::OpenFile {path:Some(path.to_string_lossy().into_owned())}).unwrap();
        std::fs::remove_file(path).unwrap();
        reopened.scene.at_time(10.,|| {
            for (id,expected) in ids.iter().zip(&before) {
                assert!(reopened.scene.world_matrix(*id).abs_diff_eq(*expected,0.001));
            }
        });
        s.selection = [limb.goal].into();
        execute(&mut s,Command::DeleteSelectedAnimation).unwrap();
        assert!(s.scene.loom_rigs[0].limbs[0].pose_matches.keys().is_empty());
        let reads = loom_scene::anim::animated_reads();
        for time in [-10.,0.,9.5,50.] {
            s.scene.at_time(time,|| {
                for (id,expected) in ids.iter().zip(&before) {
                    assert!(s.scene.world_matrix(*id).abs_diff_eq(*expected,0.001));
                }
            });
        }
        assert_eq!(loom_scene::anim::animated_reads(),reads);
    }
}

#[test]
fn world_contact_holds_body_contact_follows_root_and_free_releases() {
    for space in [ContactSpace::World, ContactSpace::Body] {
        let (mut s, root, end) = rigged();
        let target = Vec3::new(12., 8., 0.);
        let tangents = (
            loom_scene::anim::TangentType::Linear,
            loom_scene::anim::TangentType::Linear,
        );
        let keys = s
            .scene
            .node_mut(root)
            .unwrap()
            .transform
            .position
            .ensure_keys()
            .unwrap();
        keys.tracks[1].interp = loom_scene::anim::Interp::Linear;
        keys.tracks[1].set_key(0., 0., tangents);
        keys.tracks[1].set_key(40., 2., tangents);
        let bones: Vec<_> = s
            .scene
            .nodes()
            .filter(|n| n.object.is_bone())
            .cloned()
            .collect();
        let contacts = ContactTrack::new(
            [
                (7, reference(space.clone(), target)),
                (9, reference(ContactSpace::Body, target)),
            ]
            .into(),
            vec![key(10., 7, 1.), key(20., 7, 1.), key(30., 9, 0.)],
        )
        .unwrap();
        s.scene.loom_rigs[0].limbs[0].contacts = contacts;
        // Changing rig metadata directly is fixture setup, before the first evaluation.
        for time in [0., 10., 15.5, 20., 30., 10., 1., 19.25] {
            let actual = point(&s, end, time);
            let root_y = time * 0.05;
            let expected = if time < 10. || time >= 30. {
                Vec3::new(20., root_y, 0.)
            } else {
                target
                    + if space == ContactSpace::Body {
                        Vec3::Y * root_y
                    } else {
                        Vec3::ZERO
                    }
            };
            assert!(
                actual.distance(expected) < 0.001,
                "{space:?} at {time}: {actual:?}, expected {expected:?}"
            );
        }
        for bone in bones {
            assert_eq!(s.scene.node(bone.id).unwrap(), &bone);
        }
    }
}

#[test]
fn contact_targets_slide_between_independent_references_at_fractional_times() {
    let (mut s, _, end) = rigged();
    let a = Vec3::new(12., 8., 0.);
    let b = Vec3::new(8., 12., 0.);
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [
            (7, reference(ContactSpace::World, a)),
            (9, reference(ContactSpace::World, b)),
        ]
        .into(),
        vec![key(10., 7, 1.), key(20., 9, 1.)],
    )
    .unwrap();
    for time in [0_f32, 10., 15.5, 20., 12.25, 1., 25.] {
        let expected = if time < 10. {
            Vec3::X * 20.
        } else {
            a.lerp(b, ((time - 10.) / 10.).clamp(0., 1.))
        };
        let actual = point(&s, end, time);
        assert!(
            actual.distance(expected) < 0.001,
            "at {time}: {actual:?} != {expected:?}"
        );
    }
}

#[test]
fn captured_contact_has_no_pop_and_preserves_stretch_under_affine_parents() {
    use glam::{Mat3, Mat4};
    for (sign, author) in [(1., false), (-1., false), (1., true), (-1., true)] {
        let (mut s, root, end) = rigged();
        let limb = s.scene.loom_rigs[0].limbs[0].clone();
        let mut parent = Node::new("scaled parent", ObjectKind::dummy(1.));
        parent.transform.scale.set(Vec3::new(sign * 2., 3., 1.));
        parent
            .transform
            .set_rotation_quat(Quat::from_rotation_z(0.3));
        let parent = s.scene.add_node(parent);
        s.scene.node_mut(root).unwrap().parent = Some(parent);
        s.scene
            .node_mut(root)
            .unwrap()
            .transform
            .set_rotation_quat(Quat::from_rotation_z(0.4));
        s.scene
            .node_mut(limb.mapping.start)
            .unwrap()
            .transform
            .set_rotation_quat(Quat::from_rotation_z(0.4));
        s.scene
            .node_mut(limb.mapping.middle)
            .unwrap()
            .transform
            .set_rotation_quat(Quat::from_rotation_z(-0.8));
        let ids = [root, limb.mapping.start, limb.mapping.middle, end];
        let original: Vec<_> = ids.iter().map(|id| s.scene.world_matrix(*id)).collect();
        let target = original[3].transform_point3(Vec3::ZERO);
        let middle = original[2].transform_point3(Vec3::ZERO);
        let captured = ContactReference {
            space: ContactSpace::World,
            position: target,
            rotation: Quat::from_rotation_z(0.3),
            pole_position: middle + Vec3::Z * 20.,
        };
        // Use the existing bend plane, not an arbitrary new elbow direction.
        let start = original[1].transform_point3(Vec3::ZERO);
        let axis = (target - start).normalize();
        let bend = middle - start - axis * (middle - start).dot(axis);
        let captured = ContactReference {
            pole_position: middle + bend * 3.,
            ..captured
        };
        let reference = if author {
            execute(
                &mut s,
                Command::CreateLoomRigContactKey {
                    rig: 1,
                    limb: "Arm".into(),
                    time: 0.,
                    preset: "planted".into(),
                },
            )
            .unwrap();
            s.scene.loom_rigs[0].limbs[0].contacts.keys()[0].reference
        } else {
            s.scene.loom_rigs[0].limbs[0].contacts =
                ContactTrack::new([(7, captured)].into(), vec![key(0., 7, 1.)]).unwrap();
            s.scene.bump_version();
            7
        };
        for (id, expected) in ids.iter().zip(&original) {
            let actual = s.scene.world_matrix(*id);
            assert!(
                actual.abs_diff_eq(*expected, 0.002),
                "sign{sign} node{id:?}: {actual:?} != {expected:?}"
            );
        }
        let mut edited = s.scene.loom_rigs[0].limbs[0].contacts.references()[&reference].clone();
        edited.rotation = Quat::from_rotation_z(0.7);
        s.scene.loom_rigs[0].limbs[0]
            .contacts
            .update_reference(reference, edited)
            .unwrap();
        s.scene.bump_version();
        let rotated = s.scene.world_matrix(end);
        let expected = Mat4::from_quat(Quat::from_rotation_z(0.4)) * original[3];
        for axis in 0..3 {
            assert!(
                rotated.col(axis).abs_diff_eq(expected.col(axis), 0.002),
                "sign{sign} axis{axis}: {rotated:?} != {expected:?}"
            );
        }
        assert!(rotated.w_axis.abs_diff_eq(original[3].w_axis, 0.002));
        let a = Mat3::from_mat4(original[3]);
        let b = Mat3::from_mat4(rotated);
        assert!((a.transpose() * a).abs_diff_eq(b.transpose() * b, 0.005));
    }
}

#[test]
fn invalid_contact_frames_are_reported_and_recover_after_the_frame_is_fixed() {
    let (mut s, root, end) = rigged();
    s.scene
        .node_mut(root)
        .unwrap()
        .transform
        .scale
        .set(Vec3::new(0., 1., 1.));
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [(7, reference(ContactSpace::Body, Vec3::new(12., 8., 0.)))].into(),
        vec![key(0., 7, 1.)],
    )
    .unwrap();
    assert!(s.scene.world_matrix(end).is_finite());
    let errors = s.scene.contact_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].rig, 1);
    assert_eq!(errors[0].limb, "Arm");
    assert!(errors[0].message.contains("invertible"));
    s.scene
        .node_mut(root)
        .unwrap()
        .transform
        .scale
        .set(Vec3::ONE);
    assert!(s.scene.contact_errors().is_empty());
    assert!(point(&s, end, 0.).distance(Vec3::new(12., 8., 0.)) < 0.001);
}

#[test]
fn contact_blend_interpolates_and_master_motion_does_not_move_a_world_contact() {
    let (mut s, _, end) = rigged();
    let limb = s.scene.loom_rigs[0].limbs[0].clone();
    let target = Vec3::new(12., 8., 0.);
    let reference = reference(ContactSpace::World, target);
    s.scene
        .node_mut(limb.goal)
        .unwrap()
        .transform
        .position
        .set(target);
    s.scene
        .node_mut(limb.goal)
        .unwrap()
        .transform
        .set_rotation_quat(reference.rotation);
    s.scene
        .node_mut(limb.pole)
        .unwrap()
        .transform
        .position
        .set(reference.pole_position);
    // The contact track, rather than the legacy blend, owns authored contact IK/FK.
    s.scene.loom_rigs[0].limbs[0].blend.set(1.);
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [(7, reference.clone())].into(),
        vec![key(10., 7, 0.), key(20., 7, 1.)],
    )
    .unwrap();
    s.scene.bump_version();
    // Interpolate between FK and the roll-preserving contact solution, not
    // the legacy solver's canonical axial roll.
    let ik = s.scene.at_time(20., || {
        [limb.mapping.start, limb.mapping.middle]
            .map(|id| s.scene.local_matrix(id).to_scale_rotation_translation().1)
    });
    let q0 = s
        .scene
        .fk_local_matrix(limb.mapping.start)
        .to_scale_rotation_translation()
        .1
        .slerp(ik[0], 0.5);
    let q1 = s
        .scene
        .fk_local_matrix(limb.mapping.middle)
        .to_scale_rotation_translation()
        .1
        .slerp(ik[1], 0.5);
    let parent =
        glam::Mat4::from_quat(q0) * glam::Mat4::from_rotation_translation(q1, Vec3::X * 10.);
    let expected = glam::Mat4::from_rotation_translation(
        (q0 * q1).slerp(reference.rotation, 0.5),
        parent.transform_point3(Vec3::X * 10.),
    );
    let half = s.scene.at_time(15., || s.scene.world_matrix(end));
    assert!(
        half.abs_diff_eq(expected, 0.001),
        "{half:?} != {expected:?}"
    );
    assert!(point(&s, end, 10.).abs_diff_eq(Vec3::X * 20., 0.001));
    let master = s.scene.loom_rigs[0].master;
    s.scene
        .node_mut(master)
        .unwrap()
        .transform
        .position
        .set(Vec3::Y * 2.);
    assert!(point(&s, end, 20.).distance(target) < 0.001);
    assert_eq!(s.scene.loom_rigs[0].limbs[0].blend.get(), 1.);
}

#[test]
fn contact_authoring_captures_solved_pose_and_edits_are_atomic_and_undoable() {
    let (mut s, root, end) = rigged();
    let limb = s.scene.loom_rigs[0].limbs[0].clone();
    s.scene
        .node_mut(limb.mapping.start)
        .unwrap()
        .transform
        .set_rotation_quat(Quat::from_euler(glam::EulerRot::XYZ, 0.25, 0.1, 0.4));
    s.scene
        .node_mut(limb.mapping.middle)
        .unwrap()
        .transform
        .set_rotation_quat(Quat::from_euler(glam::EulerRot::XYZ, -0.15, 0.2, -0.8));
    let source: Vec<_> = s.scene.nodes().cloned().collect();
    let before = s.scene.loom_rigs.clone();
    let undo = s.undo.position();
    let ids = [root, limb.mapping.start, limb.mapping.middle, end];
    let visible: Vec<_> = s.scene.at_time(10., || {
        ids.iter().map(|id| s.scene.world_matrix(*id)).collect()
    });
    execute(
        &mut s,
        Command::CreateLoomRigContactKey {
            rig: 1,
            limb: "Arm".into(),
            time: 10.,
            preset: "planted".into(),
        },
    )
    .unwrap();
    assert_eq!(s.undo.position(), undo + 1);
    assert_eq!(s.scene.time(), 0.);
    for (id, expected) in ids.into_iter().zip(visible) {
        let actual = s.scene.at_time(10., || s.scene.world_matrix(id));
        assert!(
            actual.abs_diff_eq(expected, 0.001),
            "{id:?}: {actual:?} != {expected:?}"
        );
    }
    let created = s.scene.loom_rigs.clone();
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, before);
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, created);
    for (time, preset) in [(20., "planted"), (30., "sliding"), (40., "free")] {
        execute(
            &mut s,
            Command::CreateLoomRigContactKey {
                rig: 1,
                limb: "Arm".into(),
                time,
                preset: preset.into(),
            },
        )
        .unwrap();
    }
    let track = &s.scene.loom_rigs[0].limbs[0].contacts;
    assert_eq!(track.keys()[0].reference, track.keys()[1].reference);
    assert_ne!(track.keys()[1].reference, track.keys()[2].reference);
    assert_eq!(track.keys()[3].blend, 0.);
    assert_eq!(
        track.references()[&track.keys()[3].reference].space,
        ContactSpace::Body
    );
    let reference = track.keys()[0].reference;
    let mut pose = track.references()[&reference].clone();
    pose.position.y += 1.;
    execute(
        &mut s,
        Command::EditLoomRigContactReference {
            rig: 1,
            limb: "Arm".into(),
            reference,
            pose: pose.clone(),
        },
    )
    .unwrap();
    assert_eq!(
        s.scene.loom_rigs[0].limbs[0].contacts.references()[&reference],
        pose
    );
    let mut changed = s.scene.loom_rigs[0].limbs[0].contacts.keys()[2].clone();
    changed.blend = 0.7;
    execute(
        &mut s,
        Command::EditLoomRigContactKey {
            rig: 1,
            limb: "Arm".into(),
            key: changed.clone(),
        },
    )
    .unwrap();
    assert_eq!(s.scene.loom_rigs[0].limbs[0].contacts.keys()[2], changed);
    execute(
        &mut s,
        Command::MoveLoomRigContactKey {
            rig: 1,
            limb: "Arm".into(),
            from: 20.,
            to: 25.,
        },
    )
    .unwrap();
    let snapshot = s.scene.loom_rigs.clone();
    let position = s.undo.position();
    assert!(execute(
        &mut s,
        Command::MoveLoomRigContactKey {
            rig: 1,
            limb: "Arm".into(),
            from: 10.,
            to: 25.
        }
    )
    .is_err());
    assert_eq!(s.scene.loom_rigs, snapshot);
    assert_eq!(s.undo.position(), position);
    execute(
        &mut s,
        Command::DeleteLoomRigContactKey {
            rig: 1,
            limb: "Arm".into(),
            time: 10.,
        },
    )
    .unwrap();
    assert!(s.scene.loom_rigs[0].limbs[0]
        .contacts
        .references()
        .contains_key(&reference));
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, snapshot);
    assert_eq!(s.scene.nodes().cloned().collect::<Vec<_>>(), source);
}

#[test]
fn invalid_contact_authoring_leaves_the_scene_and_undo_unchanged() {
    let (mut s, _, _) = rigged();
    let before = serde_json::to_value(&s.scene).unwrap();
    let undo = s.undo.position();
    for (rig, time, preset) in [
        (1, f32::NAN, "planted"),
        (1, f32::INFINITY, "planted"),
        (99, 0., "planted"),
        (1, 0., "invalid"),
    ] {
        assert!(execute(
            &mut s,
            Command::CreateLoomRigContactKey {
                rig,
                limb: "Arm".into(),
                time,
                preset: preset.into()
            }
        )
        .is_err());
        assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
        assert_eq!(s.undo.position(), undo);
    }
}

#[test]
fn contact_commands_publish_typed_schemas_and_roundtrip_through_command_json() {
    let (mut s, _, _) = rigged();
    let create = Command::CreateLoomRigContactKey {
        rig: 1,
        limb: "Arm".into(),
        time: 10.,
        preset: "planted".into(),
    };
    let decoded: Command = serde_json::from_value(serde_json::to_value(create).unwrap()).unwrap();
    execute(&mut s, decoded).unwrap();
    let registry = loom_commands::registry();
    let key = registry
        .iter()
        .find(|entry| entry.name == "Edit LoomRig Contact Key")
        .unwrap();
    assert_eq!(
        key.args_schema["properties"]["key"]["required"],
        serde_json::json!(["time", "reference", "blend"])
    );
    let reference = registry
        .iter()
        .find(|entry| entry.name == "Edit LoomRig Contact Reference")
        .unwrap();
    assert_eq!(
        reference.args_schema["properties"]["pose"]["properties"]["space"]["enum"],
        serde_json::json!(["body", "world"])
    );
    assert_eq!(
        reference.args_schema["properties"]["pose"]["properties"]["rotation"]["minItems"],
        4
    );
    let reference_id = s.scene.loom_rigs[0].limbs[0].contacts.keys()[0].reference;
    let pose = s.scene.loom_rigs[0].limbs[0].contacts.references()[&reference_id].clone();
    let edit = Command::EditLoomRigContactReference {
        rig: 1,
        limb: "Arm".into(),
        reference: reference_id,
        pose,
    };
    let undo = s.undo.position();
    let decoded: Command = serde_json::from_value(serde_json::to_value(edit).unwrap()).unwrap();
    execute(&mut s, decoded).unwrap();
    assert_eq!(
        s.undo.position(),
        undo,
        "an unchanged reference must not add an undo step"
    );
}

#[test]
fn new_plant_after_free_captures_a_new_stance_and_duplicate_creation_is_atomic() {
    let (mut s, _, end) = rigged();
    for (time, preset) in [(10., "planted"), (20., "free")] {
        execute(
            &mut s,
            Command::CreateLoomRigContactKey {
                rig: 1,
                limb: "Arm".into(),
                time,
                preset: preset.into(),
            },
        )
        .unwrap();
    }
    let master = s.scene.loom_rigs[0].master;
    s.scene
        .node_mut(master)
        .unwrap()
        .transform
        .position
        .set(Vec3::Y * 2.);
    let expected = point(&s, end, 30.);
    execute(
        &mut s,
        Command::CreateLoomRigContactKey {
            rig: 1,
            limb: "Arm".into(),
            time: 30.,
            preset: "planted".into(),
        },
    )
    .unwrap();
    let track = &s.scene.loom_rigs[0].limbs[0].contacts;
    assert_ne!(track.keys()[0].reference, track.keys()[2].reference);
    assert!(point(&s, end, 30.).distance(expected) < 0.001);
    let before = s.scene.loom_rigs.clone();
    let undo = s.undo.position();
    assert!(execute(
        &mut s,
        Command::CreateLoomRigContactKey {
            rig: 1,
            limb: "Arm".into(),
            time: 30.,
            preset: "sliding".into()
        }
    )
    .is_err());
    assert_eq!(s.scene.loom_rigs, before);
    assert_eq!(s.undo.position(), undo);
    let reference = before[0].limbs[0].contacts.keys()[0].reference;
    let mut pose = before[0].limbs[0].contacts.references()[&reference].clone();
    pose.rotation = Quat::from_xyzw(0., 0., 0., 0.);
    assert!(execute(
        &mut s,
        Command::EditLoomRigContactReference {
            rig: 1,
            limb: "Arm".into(),
            reference,
            pose
        }
    )
    .is_err());
    assert_eq!(s.scene.loom_rigs, before);
    assert_eq!(s.undo.position(), undo);
}

#[test]
#[ignore = "requires LOOM_RAY_CONTACT_SOURCE production scene"]
fn production_ray_contact_capture_preserves_source_data_and_solved_pose() {
    use std::hash::Hasher;
    struct Fingerprint(std::collections::hash_map::DefaultHasher);
    impl std::io::Write for Fingerprint {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.write(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    fn source(s: &AppState) -> u64 {
        let mut out = Fingerprint(Default::default());
        for node in s.scene.nodes() {
            serde_json::to_writer(&mut out, node).unwrap();
        }
        serde_json::to_writer(&mut out, &s.scene.materials).unwrap();
        out.0.finish()
    }
    let path = std::env::var("LOOM_RAY_CONTACT_SOURCE").unwrap();
    let mut s = AppState::new();
    execute(&mut s, Command::OpenFile { path: Some(path) }).unwrap();
    execute(&mut s, Command::SetFrame { frame: 39 }).unwrap();
    // Capture the complete production rig in isolation; never save over the source.
    {
        let mut keyed = AppState::new();
        keyed.scene = s.scene.clone();
        execute(&mut keyed, Command::SetFrame {frame:39}).unwrap();
        let rig = keyed.scene.loom_rigs[0].clone();
        let mut owned:std::collections::BTreeSet<_> = rig.bindings.iter().map(|binding|binding.control).collect();
        owned.insert(rig.master);
        for limb in &rig.limbs {owned.extend([limb.goal,limb.pole]);}
        let untouched = |state:&AppState| {
            let mut out = Fingerprint(Default::default());
            for node in state.scene.nodes().filter(|node|!owned.contains(&node.id)) {
                serde_json::to_writer(&mut out,node).unwrap();
            }
            serde_json::to_writer(&mut out,&state.scene.materials).unwrap();
            out.0.finish()
        };
        let fingerprint = untouched(&keyed);
        let pose:Vec<_> = keyed.scene.nodes().filter(|node|!owned.contains(&node.id)).map(|node|(node.id,keyed.scene.world_matrix(node.id))).collect();
        let metadata = keyed.scene.loom_rigs.clone();
        let controls:Vec<_> = owned.iter().map(|id|(*id,keyed.scene.node(*id).unwrap().transform.clone())).collect();
        let history = keyed.undo.position();
        let timer = std::time::Instant::now();
        execute(&mut keyed,Command::KeyLoomRigPose {rig:rig.id}).unwrap();
        println!("Ray full-rig pose capture latency: {:?}",timer.elapsed());
        assert_eq!(keyed.undo.position(),history+1);
        assert_eq!(untouched(&keyed),fingerprint,"full-rig capture changed source data, skin or materials");
        for (id,expected) in pose {assert!(keyed.scene.world_matrix(id).abs_diff_eq(expected,0.003),"full-rig capture changed {id:?}");}
        for binding in &rig.bindings {
            assert!(keyed.scene.node(binding.control).unwrap().transform.rotation.keys().unwrap().tracks.iter().all(|track|track.key_at(39.).is_some()));
        }
        let captured=keyed.scene.loom_rigs.clone();
        let captured_controls:Vec<_>=owned.iter().map(|id|(*id,keyed.scene.node(*id).unwrap().transform.clone())).collect();
        execute(&mut keyed,Command::MoveLoomRigPoseKey {rig:rig.id,from:39.,to:777.25}).unwrap();
        assert_eq!(untouched(&keyed),fingerprint,"moving a rig pose changed source data, skin or materials");
        assert_eq!(keyed.scene.loom_rigs[0].pose_frames.times(),&[777.25]);
        for binding in &rig.bindings {
            assert!(keyed.scene.node(binding.control).unwrap().transform.rotation.keys().unwrap().tracks.iter().all(|track|track.key_at(39.).is_none() && track.key_at(777.25).is_some()));
        }
        execute(&mut keyed,Command::UndoSceneOperation).unwrap();
        assert_eq!(keyed.scene.loom_rigs,captured);
        for (id,transform) in &captured_controls {assert_eq!(&keyed.scene.node(*id).unwrap().transform,transform);}
        execute(&mut keyed,Command::DeleteLoomRigPoseKey {rig:rig.id,time:39.}).unwrap();
        assert!(keyed.scene.loom_rigs[0].pose_frames.is_empty());
        assert_eq!(untouched(&keyed),fingerprint,"deleting a rig pose changed source data, skin or materials");
        execute(&mut keyed,Command::UndoSceneOperation).unwrap();
        assert_eq!(keyed.scene.loom_rigs,captured);
        for (id,transform) in &captured_controls {assert_eq!(&keyed.scene.node(*id).unwrap().transform,transform);}
        execute(&mut keyed,Command::UndoSceneOperation).unwrap();
        assert_eq!(keyed.scene.loom_rigs,metadata);
        for (id,transform) in controls {assert_eq!(keyed.scene.node(id).unwrap().transform,transform);}
        assert_eq!(untouched(&keyed),fingerprint);
    }
    let before = source(&s);
    let rig = s.scene.loom_rigs[0].id;
    for name in ["Left Leg", "Right Leg", "Left Arm", "Right Arm"] {
        let limb = s.scene.loom_rigs[0]
            .limbs
            .iter()
            .find(|l| l.mapping.name == name)
            .unwrap()
            .clone();
        let ids = [limb.mapping.start, limb.mapping.middle, limb.mapping.end];
        let expected: Vec<_> = ids.iter().map(|id| s.scene.world_matrix(*id)).collect();
        execute(
            &mut s,
            Command::CreateLoomRigContactKey {
                rig,
                limb: name.into(),
                time: 39.,
                preset: "planted".into(),
            },
        )
        .unwrap();
        for (id, expected) in ids.into_iter().zip(expected) {
            let actual = s.scene.world_matrix(id);
            assert!(
                actual.abs_diff_eq(expected, 0.003),
                "{name} {id:?} solver {:?}: capture changed solved pose; expected {expected:?}; actual {actual:?}", limb.solver
            );
        }
        assert!(s.scene.contact_errors().is_empty());
        assert_eq!(
            source(&s),
            before,
            "capture changed a source node, Skin data or materials"
        );
        let contacts = s.scene.loom_rigs.clone();
        if name == "Left Leg" {
            let mut preview = AppState::new();
            preview.scene = s.scene.clone();
            execute(&mut preview,Command::SetFrame {frame:39}).unwrap();
            preview.selection = [limb.goal,limb.pole].into();
            preview.undo.set_limit(2);
            execute(&mut preview,Command::SetKeyMode).unwrap();
            let start = preview.scene.world_matrix(limb.mapping.start).w_axis.truncate();
            let end = preview.scene.world_matrix(limb.mapping.end).w_axis.truncate();
            for _ in 0..6 {
                execute(&mut preview,Command::MoveSelection {
                    delta:((start-end)*0.001).to_array(),system:Some(loom_commands::CoordSystem::World),
                }).unwrap();
            }
            execute(&mut preview,Command::ClearSetKeyModeBuffer).unwrap();
            assert_eq!(preview.scene.loom_rigs,contacts,"Ray pose rollback lost rig metadata beyond the Undo limit");
            assert_eq!(source(&preview),before,"Ray pose rollback changed source nodes or materials");
            for id in ids {assert!(preview.scene.world_matrix(id).abs_diff_eq(s.scene.world_matrix(id),0.003));}
            assert!(preview.undo.is_saved());
        }
        let goal = s.scene.world_matrix(limb.goal).w_axis.truncate();
        let pole = s.scene.world_matrix(limb.pole).w_axis.truncate();
        let start = s.scene.world_matrix(limb.mapping.start).w_axis.truncate();
        let delta = (start - goal) * 0.001;
        let selection = s.selection.clone();
        s.selection = [limb.goal, limb.pole].into();
        let timer = std::time::Instant::now();
        execute(&mut s, Command::MoveSelection { delta: delta.to_array(), system: Some(loom_commands::CoordSystem::World) }).unwrap();
        println!("{name} goal+pole command latency: {:?}", timer.elapsed());
        assert!(s.scene.world_matrix(limb.goal).w_axis.truncate().distance(goal + delta) < 0.003);
        assert!(s.scene.world_matrix(limb.pole).w_axis.truncate().distance(pole + delta) < 0.003);
        assert!(s.scene.world_matrix(limb.mapping.end).w_axis.truncate().distance(goal + delta) < 0.003);
        assert_eq!(source(&s), before, "target editing changed source nodes or materials");
        execute(&mut s, Command::UndoSceneOperation).unwrap();
        assert_eq!(s.scene.loom_rigs, contacts);
        let release_pose = ids.map(|id|s.scene.world_matrix(id));
        execute(&mut s,Command::SetLoomRigLimb {
            rig,limb:name.into(),solver:if limb.solver == loom_scene::ik::IkSolver::HI {"hi"} else {"limb"}.into(),blend:0.,
        }).unwrap();
        for (id,expected) in ids.iter().zip(release_pose) {
            assert!(s.scene.world_matrix(*id).abs_diff_eq(expected,0.003),"{name} settings release changed {id:?}");
        }
        assert_eq!(source(&s),before,"settings changed source nodes or materials");
        execute(&mut s,Command::UndoSceneOperation).unwrap();
        assert_eq!(s.scene.loom_rigs,contacts);
        let preset_pose = ids.map(|id|s.scene.world_matrix(id));
        execute(&mut s,Command::SetLoomRigContactPreset {rig,limb:name.into(),time:39.,preset:"free".into()}).unwrap();
        for (id,expected) in ids.iter().zip(preset_pose) {
            assert!(s.scene.world_matrix(*id).abs_diff_eq(expected,0.003),"{name} preset replacement changed {id:?}");
        }
        assert_eq!(source(&s),before,"preset replacement changed source nodes or materials");
        execute(&mut s,Command::UndoSceneOperation).unwrap();
        assert_eq!(s.scene.loom_rigs,contacts);
        let keyed_pose = s.scene.at_time(40.,||ids.map(|id|s.scene.world_matrix(id)));
        execute(&mut s,Command::KeyLoomRigContact {rig,limb:name.into(),time:40.}).unwrap();
        s.scene.at_time(40.,|| {
            for (id,expected) in ids.iter().zip(keyed_pose) {
                assert!(s.scene.world_matrix(*id).abs_diff_eq(expected,0.003),"{name} contact key changed {id:?}");
            }
        });
        assert_eq!(source(&s),before,"contact key changed source nodes or materials");
        execute(&mut s,Command::UndoSceneOperation).unwrap();
        assert_eq!(s.scene.loom_rigs,contacts);
        let free_pose = s.scene.at_time(40.,||ids.map(|id|s.scene.world_matrix(id)));
        execute(&mut s, Command::CreateLoomRigContactKey {
            rig,limb:name.into(),time:40.,preset:"free".into(),
        }).unwrap();
        s.scene.at_time(40.,|| {
            for (id,expected) in ids.iter().zip(free_pose) {
                assert!(s.scene.world_matrix(*id).abs_diff_eq(expected,0.003),"{name} Free changed {id:?}");
            }
        });
        assert_eq!(source(&s),before,"Free matching changed source nodes or materials");
        execute(&mut s, Command::UndoSceneOperation).unwrap();
        assert_eq!(s.scene.loom_rigs,contacts);
        s.selection = selection;
        execute(&mut s, Command::UndoSceneOperation).unwrap();
        assert_eq!(source(&s), before);
    }
    for name in ["Left Leg", "Right Leg", "Left Arm", "Right Arm"] {
        execute(
            &mut s,
            Command::CreateLoomRigContactKey {
                rig,
                limb: name.into(),
                time: 39.,
                preset: "planted".into(),
            },
        )
        .unwrap();
    }
    let expected: Vec<_> = s
        .scene
        .nodes()
        .map(|node| (node.id, s.scene.world_matrix(node.id)))
        .collect();
    let materials = serde_json::to_value(&s.scene.materials).unwrap();
    execute(
        &mut s,
        Command::NewSceneKeep {
            keep: "objects_and_hierarchy".into(),
        },
    )
    .unwrap();
    assert!(s.scene.loom_rigs[0]
        .limbs
        .iter()
        .all(|limb| limb.contacts.keys().is_empty() && limb.contacts.constant_state().is_some()));
    assert_eq!(serde_json::to_value(&s.scene.materials).unwrap(), materials);
    for time in [0., 60., 149.] {
        s.scene.at_time(time, || {
            for (id, world) in &expected {
                assert!(
                    s.scene.world_matrix(*id).abs_diff_eq(*world, 0.003),
                    "freeze changed {id:?} at {time}"
                );
            }
        });
    }
    assert!(s.scene.contact_errors().is_empty());
}

#[test]
fn new_keep_freezes_contacts_at_current_pose_without_retaining_contact_animation() {
    for (frame, scale) in [0, 10, 15, 30].into_iter().flat_map(|frame| {
        [Vec3::ONE, Vec3::new(2., 3., 1.), Vec3::new(-2., 3., 1.)].map(|scale| (frame, scale))
    }) {
        let (mut s, root, end) = rigged();
        let mut parent = Node::new("affine parent", ObjectKind::dummy(1.));
        parent.transform.scale.set(scale);
        parent
            .transform
            .set_rotation_quat(Quat::from_rotation_z(0.3));
        let parent = s.scene.add_node(parent);
        s.scene.node_mut(root).unwrap().parent = Some(parent);
        s.scene
            .node_mut(root)
            .unwrap()
            .transform
            .set_rotation_quat(Quat::from_rotation_z(0.4));
        s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
            [
                (1, reference(ContactSpace::World, Vec3::new(12., 8., 0.))),
                (2, reference(ContactSpace::Body, Vec3::new(8., 12., 0.))),
            ]
            .into(),
            vec![key(10., 1, 1.), key(20., 2, 0.5)],
        )
        .unwrap();
        s.scene.bump_version();
        execute(&mut s, Command::SetFrame { frame }).unwrap();
        let expected = s.scene.world_matrix(end);
        execute(
            &mut s,
            Command::NewSceneKeep {
                keep: "objects_and_hierarchy".into(),
            },
        )
        .unwrap();
        assert!(s.scene.loom_rigs[0].limbs[0].contacts.keys().is_empty());
        let path = std::env::temp_dir().join(format!(
            "loom-contact-freeze-{}-{frame}-{}.loom",
            std::process::id(),
            scale.x.to_bits()
        ));
        execute(
            &mut s,
            Command::SaveFileAsCopy {
                path: Some(path.to_string_lossy().into_owned()),
            },
        )
        .unwrap();
        let mut reopened = AppState::new();
        execute(
            &mut reopened,
            Command::OpenFile {
                path: Some(path.to_string_lossy().into_owned()),
            },
        )
        .unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(reopened
            .scene
            .world_matrix(end)
            .abs_diff_eq(expected, 0.001));
        let reads = loom_scene::anim::animated_reads();
        for time in [-100., 0., 10., 15., 50.] {
            let actual = s.scene.at_time(time, || s.scene.world_matrix(end));
            assert!(
                actual.abs_diff_eq(expected, 0.001),
                "frame{frame},time{time}: {actual:?} != {expected:?}"
            );
        }
        assert_eq!(
            loom_scene::anim::animated_reads(),
            reads,
            "frozen Scene retained contact time dependence"
        );
    }
}

#[test]
fn deleting_selected_control_animation_freezes_only_its_limb_in_one_undo_step() {
    let (mut s, _, end) = rigged();
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [
            (1, reference(ContactSpace::World, Vec3::new(12., 8., 0.))),
            (2, reference(ContactSpace::Body, Vec3::new(8., 12., 0.))),
        ]
        .into(),
        vec![key(10., 1, 1.), key(20., 2, 0.5)],
    )
    .unwrap();
    s.scene.bump_version();
    let root = s.scene.loom_rigs[0].mapping.root;
    let mut joints = Vec::new();
    for i in 0..3 {
        let mut node = Node::new("other limb", ObjectKind::bone(10.));
        node.parent = Some(if i == 0 { root } else { joints[i - 1] });
        node.transform
            .position
            .set(if i == 0 { Vec3::Y * 30. } else { Vec3::X * 10. });
        joints.push(s.scene.add_node(node));
    }
    let mut mapping = s.scene.loom_rigs[0].mapping.clone();
    mapping.limbs.push(LimbMapping {
        name: "Other".into(),
        start: joints[0],
        middle: joints[1],
        end: joints[2],
    });
    execute(&mut s, Command::SetLoomRigMapping { rig: 1, mapping }).unwrap();
    s.scene.loom_rigs[0].limbs[1].contacts = s.scene.loom_rigs[0].limbs[0].contacts.clone();
    let doc = loom_commands::file::Document {
        format: loom_commands::file::FORMAT,
        app: "3dloom".into(),
        scene: s.scene.clone(),
        settings: loom_commands::file::SceneSettings::from_state(&s),
    };
    loom_commands::file::merge_map(&mut s, &doc, None, &[], "Other rig").unwrap();
    execute(&mut s, Command::SetFrame { frame: 15 }).unwrap();
    let goal = s.scene.loom_rigs[0].limbs[0].goal;
    let linear = (
        loom_scene::anim::TangentType::Linear,
        loom_scene::anim::TangentType::Linear,
    );
    let keys = s
        .scene
        .node_mut(goal)
        .unwrap()
        .transform
        .position
        .ensure_keys()
        .unwrap();
    keys.tracks[0].set_key(0., 0., linear);
    keys.tracks[0].set_key(30., 10., linear);
    let expected = s.scene.world_matrix(end);
    let rigs = s.scene.loom_rigs.clone();
    let node = s.scene.node(goal).unwrap().clone();
    let undo = s.undo.position();
    s.selection = [goal].into();
    execute(&mut s, Command::DeleteSelectedAnimation).unwrap();
    assert_eq!(s.undo.position(), undo + 1);
    assert!(s.scene.loom_rigs[0].limbs[0].contacts.keys().is_empty());
    assert!(s
        .scene
        .node(goal)
        .unwrap()
        .transform
        .position
        .keys()
        .is_none());
    assert_eq!(s.scene.loom_rigs[1], rigs[1]);
    assert_eq!(s.scene.loom_rigs[0].limbs[1], rigs[0].limbs[1]);
    for time in [0., 15., 30.] {
        assert!(s
            .scene
            .at_time(time, || s.scene.world_matrix(end))
            .abs_diff_eq(expected, 0.001));
    }
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, rigs);
    assert_eq!(s.scene.node(goal).unwrap(), &node);
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert!(s.scene.loom_rigs[0].limbs[0].contacts.keys().is_empty());
}

#[test]
fn invalid_contact_freeze_is_atomic_even_when_selected_node_tracks_would_change() {
    let (mut s, root, _) = rigged();
    let goal = s.scene.loom_rigs[0].limbs[0].goal;
    let linear = (
        loom_scene::anim::TangentType::Linear,
        loom_scene::anim::TangentType::Linear,
    );
    s.scene
        .node_mut(goal)
        .unwrap()
        .transform
        .position
        .ensure_keys()
        .unwrap()
        .tracks[0]
        .set_key(0., 2., linear);
    s.scene
        .node_mut(root)
        .unwrap()
        .transform
        .scale
        .set(Vec3::new(0., 1., 1.));
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [(1, reference(ContactSpace::Body, Vec3::X))].into(),
        vec![key(0., 1, 1.)],
    )
    .unwrap();
    s.scene.bump_version();
    s.selection = [goal].into();
    let before = serde_json::to_value(&s.scene).unwrap();
    let undo = s.undo.position();
    assert!(execute(&mut s, Command::DeleteSelectedAnimation).is_err());
    assert_eq!(serde_json::to_value(&s.scene).unwrap(), before);
    assert_eq!(s.undo.position(), undo);
}

#[test]
fn ik_cache_drops_old_time_dependence_and_recovers_it_for_new_native_animation() {
    let (mut s, _, end) = rigged();
    let master = s.scene.loom_rigs[0].master;
    let animate = |s: &mut AppState| {
        let keys = s
            .scene
            .node_mut(master)
            .unwrap()
            .transform
            .position
            .ensure_keys()
            .unwrap();
        keys.tracks[0].interp = loom_scene::anim::Interp::Linear;
        let linear = (
            loom_scene::anim::TangentType::Linear,
            loom_scene::anim::TangentType::Linear,
        );
        keys.tracks[0].set_key(0., 0., linear);
        keys.tracks[0].set_key(10., 2., linear);
    };
    animate(&mut s);
    assert!(point(&s, end, 0.).abs_diff_eq(Vec3::X * 20., 0.001));
    assert!(point(&s, end, 10.).abs_diff_eq(Vec3::X * 22., 0.001));
    s.scene.node_mut(master).unwrap().transform.position = loom_scene::Param::new(Vec3::X);
    let reads = loom_scene::anim::animated_reads();
    for time in [0., 10., 5.] {
        assert!(point(&s, end, time).abs_diff_eq(Vec3::X * 21., 0.001));
    }
    assert_eq!(
        loom_scene::anim::animated_reads(),
        reads,
        "removed animation kept the IK cache time-dependent"
    );
    animate(&mut s);
    assert!(point(&s, end, 0.).abs_diff_eq(Vec3::X * 20., 0.001));
    assert!(point(&s, end, 10.).abs_diff_eq(Vec3::X * 22., 0.001));
    assert!(loom_scene::anim::animated_reads() > reads);
}

#[test]
fn deleting_body_contact_animation_keeps_body_inheritance_and_source_motion() {
    let (mut s, root, end) = rigged();
    let linear = (
        loom_scene::anim::TangentType::Linear,
        loom_scene::anim::TangentType::Linear,
    );
    let keys = s
        .scene
        .node_mut(root)
        .unwrap()
        .transform
        .position
        .ensure_keys()
        .unwrap();
    keys.tracks[1].interp = loom_scene::anim::Interp::Linear;
    keys.tracks[1].set_key(0., 0., linear);
    keys.tracks[1].set_key(40., 2., linear);
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [
            (1, reference(ContactSpace::Body, Vec3::new(12., 8., 0.))),
            (2, reference(ContactSpace::Body, Vec3::new(8., 12., 0.))),
        ]
        .into(),
        vec![key(10., 1, 1.), key(20., 2, 1.)],
    )
    .unwrap();
    s.scene.bump_version();
    execute(&mut s, Command::SetFrame { frame: 15 }).unwrap();
    let expected = s.scene.world_matrix(end).w_axis.truncate();
    let root_before = s.scene.node(root).unwrap().transform.clone();
    s.selection = [s.scene.loom_rigs[0].limbs[0].goal].into();
    execute(&mut s, Command::DeleteSelectedAnimation).unwrap();
    assert_eq!(s.scene.node(root).unwrap().transform, root_before);
    for time in [0., 15., 30.] {
        let target = expected + Vec3::Y * ((time - 15.) * 0.05);
        assert!(
            point(&s, end, time).distance(target) < 0.001,
            "body inheritance lost at {time}"
        );
    }
}

#[test]
fn contact_controller_world_poses_follow_the_effective_sample_without_changing_raw_helpers() {
    let (mut s, root, _) = rigged();
    s.scene
        .node_mut(root)
        .unwrap()
        .transform
        .position
        .set(Vec3::Y * 2.);
    let limb = s.scene.loom_rigs[0].limbs[0].clone();
    let raw = s.scene.node(limb.goal).unwrap().transform.clone();
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [
            (1, reference(ContactSpace::World, Vec3::new(12., 8., 0.))),
            (2, reference(ContactSpace::Body, Vec3::new(8., 12., 0.))),
        ]
        .into(),
        vec![key(10., 1, 1.), key(20., 2, 1.)],
    )
    .unwrap();
    s.scene.bump_version();
    for time in [0., 10., 15., 20., 12.5] {
        s.scene.at_time(time, || {
            let sample = s.scene.loom_rigs[0].limbs[0]
                .contacts
                .sample(time, s.scene.world_matrix(root))
                .unwrap();
            let goal = s.scene.world_matrix(limb.goal);
            let pole = s.scene.world_matrix(limb.pole);
            if let Some(sample) = sample {
                assert!(goal.w_axis.truncate().distance(sample.position) < 0.001);
                assert!(goal
                    .to_scale_rotation_translation()
                    .1
                    .abs_diff_eq(sample.rotation, 0.001));
                assert!(pole.w_axis.truncate().distance(sample.pole_position) < 0.001);
            } else {
                assert!(goal.abs_diff_eq(raw.matrix(), 0.001));
            }
        });
    }
    assert_eq!(s.scene.node(limb.goal).unwrap().transform, raw);
}

#[test]
fn set_transform_edits_contact_goals_and_poles_with_atomic_undo_and_keeps_source_tracks() {
    let (mut s, root, end) = rigged();
    s.scene
        .node_mut(root)
        .unwrap()
        .transform
        .position
        .set(Vec3::Y * 2.);
    let limb = s.scene.loom_rigs[0].limbs[0].clone();
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [
            (1, reference(ContactSpace::World, Vec3::new(12., 8., 0.))),
            (2, reference(ContactSpace::Body, Vec3::new(8., 12., 0.))),
        ]
        .into(),
        vec![key(10., 1, 1.), key(20., 2, 1.)],
    )
    .unwrap();
    s.scene.bump_version();
    execute(&mut s, Command::SetFrame { frame: 15 }).unwrap();
    let source: Vec<_> = s.scene.nodes().cloned().collect();
    let before = s.scene.loom_rigs.clone();
    let undo = s.undo.position();
    let sample = before[0].limbs[0]
        .contacts
        .sample(15., s.scene.world_matrix(root))
        .unwrap()
        .unwrap();
    let position = sample.position + Vec3::new(1., -2., 0.5);
    let rotation = Quat::from_rotation_y(0.4) * sample.rotation;
    let (x, y, z) = rotation.to_euler(glam::EulerRot::XYZ);
    execute(
        &mut s,
        Command::SetTransform {
            id: limb.goal.0,
            position: Some(position.to_array()),
            rotation_deg: Some([x.to_degrees(), y.to_degrees(), z.to_degrees()]),
            scale: None,
        },
    )
    .unwrap();
    assert_eq!(s.undo.position(), undo + 1);
    assert!(
        s.scene
            .world_matrix(limb.goal)
            .w_axis
            .truncate()
            .distance(position)
            < 0.001
    );
    assert!(
        s.scene
            .world_matrix(end)
            .w_axis
            .truncate()
            .distance(position)
            < 0.001
    );
    assert_eq!(s.scene.nodes().cloned().collect::<Vec<_>>(), source);
    let edited = s.scene.loom_rigs.clone();
    assert_ne!(edited, before);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, before);
    execute(&mut s, Command::RedoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, edited);
    let goal = s.scene.world_matrix(limb.goal);
    let middle = s.scene.world_matrix(limb.mapping.middle);
    let pole = sample.pole_position + Vec3::new(7., -5., -8.);
    execute(
        &mut s,
        Command::SetTransform {
            id: limb.pole.0,
            position: Some(pole.to_array()),
            rotation_deg: None,
            scale: None,
        },
    )
    .unwrap();
    assert!(
        s.scene
            .world_matrix(limb.pole)
            .w_axis
            .truncate()
            .distance(pole)
            < 0.001
    );
    assert!(s.scene.world_matrix(limb.goal).abs_diff_eq(goal, 0.001));
    assert!(
        s.scene
            .world_matrix(end)
            .w_axis
            .truncate()
            .distance(position)
            < 0.001
    );
    assert!(!s
        .scene
        .world_matrix(limb.mapping.middle)
        .abs_diff_eq(middle, 0.001));
    assert_eq!(s.scene.nodes().cloned().collect::<Vec<_>>(), source);
    let before = s.scene.loom_rigs.clone();
    let undo = s.undo.position();
    assert!(execute(
        &mut s,
        Command::SetTransform {
            id: limb.goal.0,
            position: Some([f32::NAN, 0., 0.]),
            rotation_deg: None,
            scale: None
        }
    )
    .is_err());
    assert_eq!(s.scene.loom_rigs, before);
    assert_eq!(s.undo.position(), undo);
}

#[test]
fn parented_contact_transform_uses_local_fields_without_losing_affine_body_orientation() {
    for sign in [1., -1.] {
        let (mut s, root, end) = rigged();
        let limb = s.scene.loom_rigs[0].limbs[0].clone();
        let mut parent = Node::new("body frame", ObjectKind::dummy(1.));
        parent.transform.scale.set(Vec3::new(sign * 2., 3., 1.));
        parent
            .transform
            .set_rotation_quat(Quat::from_rotation_z(0.4));
        parent.transform.position.set(Vec3::new(3., 4., 5.));
        let parent = s.scene.add_node(parent);
        s.scene.node_mut(root).unwrap().parent = Some(parent);
        s.scene.node_mut(limb.goal).unwrap().parent = Some(parent);
        s.scene.node_mut(limb.pole).unwrap().parent = Some(parent);
        s.scene.loom_rigs[0].limbs[0].contacts =
            ContactTrack::constant(reference(ContactSpace::Body, Vec3::new(12., 8., 0.)), 1.)
                .unwrap();
        s.scene.bump_version();
        let raw = s.scene.node(limb.goal).unwrap().transform.clone();
        let frame = s.scene.world_matrix(parent);
        let position = Vec3::new(1., 2., 0.);
        let rotation = Quat::from_euler(
            glam::EulerRot::XYZ,
            10_f32.to_radians(),
            20_f32.to_radians(),
            30_f32.to_radians(),
        );
        let expected_rotation =
            loom_scene::loomrig_contact::affine_rotation(frame * glam::Mat4::from_quat(rotation))
                .unwrap();
        execute(
            &mut s,
            Command::SetTransform {
                id: limb.goal.0,
                position: Some(position.to_array()),
                rotation_deg: Some([10., 20., 30.]),
                scale: None,
            },
        )
        .unwrap();
        let goal = s.scene.world_matrix(limb.goal);
        assert!(
            goal.w_axis
                .truncate()
                .distance(frame.transform_point3(position))
                < 0.001
        );
        let actual_rotation = goal.to_scale_rotation_translation().1;
        assert!(
            actual_rotation.abs_diff_eq(expected_rotation, 0.001)
                || actual_rotation.abs_diff_eq(-expected_rotation, 0.001)
        );
        assert!(
            s.scene
                .world_matrix(end)
                .w_axis
                .truncate()
                .distance(goal.w_axis.truncate())
                < 0.003
        );
        assert_eq!(s.scene.node(limb.goal).unwrap().transform, raw);
        let before = s.scene.loom_rigs.clone();
        let foot = s.scene.world_matrix(end);
        execute(
            &mut s,
            Command::SetTransform {
                id: limb.goal.0,
                position: None,
                rotation_deg: None,
                scale: Some([2., 3., 4.]),
            },
        )
        .unwrap();
        assert_eq!(s.scene.loom_rigs, before);
        assert_eq!(
            s.scene.node(limb.goal).unwrap().transform.scale.get(),
            Vec3::new(2., 3., 4.)
        );
        assert!(s.scene.world_matrix(end).abs_diff_eq(foot, 0.003));
        let pole = Vec3::new(4., 1., 6.);
        execute(
            &mut s,
            Command::SetTransform {
                id: limb.pole.0,
                position: Some(pole.to_array()),
                rotation_deg: None,
                scale: None,
            },
        )
        .unwrap();
        assert!(
            s.scene
                .world_matrix(limb.pole)
                .w_axis
                .truncate()
                .distance(frame.transform_point3(pole))
                < 0.001
        );
    }
}

#[test]
fn cross_rig_contact_dependency_cycle_is_rejected_without_recursion() {
    let (mut s, root, _) = rigged();
    let goal = s.scene.loom_rigs[0].limbs[0].goal;
    let other_root = s.scene.add_node(Node::new("other root", ObjectKind::bone(1.)));
    let mut other_goal_node = s.scene.node(goal).unwrap().clone();
    other_goal_node.id = NodeId(0);
    let other_goal = s.scene.add_node(other_goal_node);
    assert_ne!(goal, other_goal);
    let mut other = s.scene.loom_rigs[0].clone();
    other.id = 2;
    other.mapping.root = other_root;
    other.limbs[0].goal = other_goal;
    s.scene.loom_rigs.push(other);
    for rig in &mut s.scene.loom_rigs {
        rig.limbs[0].contacts = ContactTrack::constant(
            reference(ContactSpace::World, Vec3::new(12., 8., 0.)), 1.,
        ).unwrap();
    }
    s.scene.node_mut(root).unwrap().parent = Some(other_goal);
    s.scene.node_mut(other_root).unwrap().parent = Some(goal);
    s.scene.bump_version();
    assert!(s.scene.contact_control(goal).is_err());
    assert!(s.scene.contact_control(other_goal).is_err());
    assert!(s.scene.world_matrix(goal).is_finite());
    assert!(s.scene.contact_errors().iter().any(|error|error.message.contains("cyclic")));
    s.scene.node_mut(other_root).unwrap().parent = None;
    s.scene.bump_version();
    assert!(s.scene.contact_control(goal).unwrap().is_some());
    assert!(s.scene.contact_control(other_goal).unwrap().is_some());
}

#[test]
fn contact_controller_dependency_cycle_reports_error_without_recursive_world_queries() {
    let (mut s, root, _) = rigged();
    let goal = s.scene.loom_rigs[0].limbs[0].goal;
    s.scene.node_mut(root).unwrap().parent = Some(goal);
    s.scene.loom_rigs[0].limbs[0].contacts =
        ContactTrack::constant(reference(ContactSpace::World, Vec3::new(12., 8., 0.)), 1.).unwrap();
    s.scene.bump_version();
    assert!(s.scene.contact_control(goal).is_err());
    assert!(s.scene.world_matrix(goal).is_finite());
    assert!(s
        .scene
        .contact_errors()
        .iter()
        .any(|error| error.message.contains("ancestor")));
    let before = s.scene.loom_rigs.clone();
    let undo = s.undo.position();
    assert!(execute(
        &mut s,
        Command::SetTransform {
            id: goal.0,
            position: Some([1., 2., 3.]),
            rotation_deg: None,
            scale: None
        }
    )
    .is_err());
    assert_eq!(s.scene.loom_rigs, before);
    assert_eq!(s.undo.position(), undo);
}

#[test]
fn selection_transforms_move_goal_and_pole_in_one_contact_edit_and_respect_locks() {
    let (mut s, _, end) = rigged();
    let limb = s.scene.loom_rigs[0].limbs[0].clone();
    s.scene.loom_rigs[0].limbs[0].contacts = ContactTrack::new(
        [
            (1, reference(ContactSpace::World, Vec3::new(12., 8., 0.))),
            (2, reference(ContactSpace::World, Vec3::new(8., 12., 0.))),
        ]
        .into(),
        vec![key(10., 1, 1.), key(20., 2, 1.)],
    )
    .unwrap();
    s.scene.bump_version();
    execute(&mut s, Command::SetFrame { frame: 15 }).unwrap();
    let goal = s.scene.world_matrix(limb.goal).w_axis.truncate();
    let pole = s.scene.world_matrix(limb.pole).w_axis.truncate();
    let nodes: Vec<_> = s.scene.nodes().cloned().collect();
    let rigs = s.scene.loom_rigs.clone();
    let undo = s.undo.position();
    s.selection = [limb.goal, limb.pole].into();
    let delta = Vec3::new(0.5, -1., 2.);
    execute(
        &mut s,
        Command::MoveSelection {
            delta: delta.to_array(),
            system: Some(loom_commands::CoordSystem::World),
        },
    )
    .unwrap();
    assert_eq!(s.undo.position(), undo + 1);
    assert!(
        s.scene
            .world_matrix(limb.goal)
            .w_axis
            .truncate()
            .distance(goal + delta)
            < 0.001
    );
    assert!(
        s.scene
            .world_matrix(limb.pole)
            .w_axis
            .truncate()
            .distance(pole + delta)
            < 0.001
    );
    assert!(
        s.scene
            .world_matrix(end)
            .w_axis
            .truncate()
            .distance(goal + delta)
            < 0.001
    );
    assert_eq!(s.scene.nodes().cloned().collect::<Vec<_>>(), nodes);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, rigs);
    s.selection = [limb.goal].into();
    s.scene.node_mut(limb.goal).unwrap().locks.move_axes[0] = true;
    execute(
        &mut s,
        Command::MoveSelection {
            delta: [2., 1., 0.],
            system: Some(loom_commands::CoordSystem::World),
        },
    )
    .unwrap();
    assert!(
        s.scene
            .world_matrix(limb.goal)
            .w_axis
            .truncate()
            .distance(goal + Vec3::Y)
            < 0.001
    );
}

#[test]
fn transform_type_in_reads_and_edits_the_effective_contact_rotation() {
    let (mut s, _, _) = rigged();
    let limb = s.scene.loom_rigs[0].limbs[0].clone();
    s.scene.loom_rigs[0].limbs[0].contacts =
        ContactTrack::constant(reference(ContactSpace::World, Vec3::new(12., 8., 0.)), 1.).unwrap();
    s.scene.bump_version();
    s.selection = [limb.goal].into();
    s.active_tool = loom_commands::Tool::Rotate;
    s.coord_system = loom_commands::CoordSystem::World;
    assert!((loom_commands::type_in_values(&s).x - 0.3_f32.to_degrees()).abs() < 0.001);
    let before = s.scene.loom_rigs.clone();
    let node = s.scene.node(limb.goal).unwrap().clone();
    let undo = s.undo.position();
    execute(
        &mut s,
        Command::TransformTypeIn {
            absolute: true,
            values: [15., 25., 35.],
        },
    )
    .unwrap();
    assert!(loom_commands::type_in_values(&s).abs_diff_eq(Vec3::new(15., 25., 35.), 0.001));
    assert_eq!(s.undo.position(), undo + 1);
    assert_eq!(s.scene.node(limb.goal).unwrap(), &node);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, before);
}

#[test]
fn moving_master_and_body_contact_together_does_not_apply_the_motion_twice() {
    let (mut s, _, end) = rigged();
    let rig = s.scene.loom_rigs[0].clone();
    let goal = rig.limbs[0].goal;
    s.scene.loom_rigs[0].limbs[0].contacts =
        ContactTrack::constant(reference(ContactSpace::Body, Vec3::new(12., 8., 0.)), 1.).unwrap();
    s.scene.bump_version();
    let before = s.scene.world_matrix(goal).w_axis.truncate();
    let rigs = s.scene.loom_rigs.clone();
    let node = s.scene.node(rig.master).unwrap().transform.clone();
    let undo = s.undo.position();
    s.selection = [rig.master, goal].into();
    let delta = Vec3::new(1., 2., 0.);
    execute(
        &mut s,
        Command::MoveSelection {
            delta: delta.to_array(),
            system: Some(loom_commands::CoordSystem::World),
        },
    )
    .unwrap();
    assert!(
        s.scene
            .world_matrix(goal)
            .w_axis
            .truncate()
            .distance(before + delta)
            < 0.001
    );
    assert!(
        s.scene
            .world_matrix(end)
            .w_axis
            .truncate()
            .distance(before + delta)
            < 0.001
    );
    assert_eq!(s.undo.position(), undo + 1);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(s.scene.loom_rigs, rigs);
    assert_eq!(s.scene.node(rig.master).unwrap().transform, node);
}

#[test]
fn world_type_in_rotation_remains_exact_under_a_mirrored_controller_parent() {
    let (mut s, _, _) = rigged();
    let goal = s.scene.loom_rigs[0].limbs[0].goal;
    let mut parent = Node::new("controller parent", ObjectKind::dummy(1.));
    parent.transform.scale.set(Vec3::new(-2., 3., 1.));
    parent
        .transform
        .set_rotation_quat(Quat::from_rotation_z(0.4));
    let parent = s.scene.add_node(parent);
    s.scene.node_mut(goal).unwrap().parent = Some(parent);
    s.scene.loom_rigs[0].limbs[0].contacts =
        ContactTrack::constant(reference(ContactSpace::World, Vec3::new(12., 8., 0.)), 1.).unwrap();
    s.scene.bump_version();
    s.selection = [goal].into();
    s.active_tool = loom_commands::Tool::Rotate;
    s.coord_system = loom_commands::CoordSystem::World;
    execute(
        &mut s,
        Command::TransformTypeIn {
            absolute: true,
            values: [15., 25., 35.],
        },
    )
    .unwrap();
    assert!(loom_commands::type_in_values(&s).abs_diff_eq(Vec3::new(15., 25., 35.), 0.001));
}
