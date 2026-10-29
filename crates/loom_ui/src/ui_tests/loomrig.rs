use super::*;
use loom_scene::{Node, ObjectKind};

fn character(h: &mut Harness) -> loom_scene::NodeId {
    fn bone(h: &mut Harness, name: &str, parent: Option<loom_scene::NodeId>) -> loom_scene::NodeId {
        let mut n = Node::new(name, ObjectKind::bone(10.0));
        n.parent = parent;
        n.transform.position.set(Vec3::new(0., 0., 10.));
        h.app.state.scene.add_node(n)
    }
    let root = bone(h, "root", None);
    let pelvis = bone(h, "pelvis", Some(root));
    let mut parent = pelvis;
    for i in 1..=5 {
        parent = bone(h, &format!("spine_{i:02}"), Some(parent));
    }
    bone(h, "head", Some(parent));
    for side in ["l", "r"] {
        let a = bone(h, &format!("upperarm_{side}"), Some(parent));
        let b = bone(h, &format!("lowerarm_{side}"), Some(a));
        bone(h, &format!("hand_{side}"), Some(b));
        let a = bone(h, &format!("thigh_{side}"), Some(pelvis));
        let b = bone(h, &format!("calf_{side}"), Some(a));
        bone(h, &format!("foot_{side}"), Some(b));
    }
    h.app.state.selection = [root].into_iter().collect();
    root
}

#[test]
fn motion_panel_builds_real_loomrig_and_selects_animatable_controls() {
    let mut h = Harness::new();
    h.screen.max.y = 1800.;
    let root = character(&mut h);
    h.app.clip_timeline.open=true;h.app.clip_timeline.tab=2;
    h.frames(3);
    assert!(h.has(egui::accesskit::Role::Button, "Create LoomRig"));
    h.click_text_anywhere("Create LoomRig");
    h.frames(3);
    assert_eq!(h.app.state.scene.loom_rigs.len(), 1);
    let rig = h.app.state.scene.loom_rigs[0].clone();
    assert_eq!(rig.mapping.root, root);
    h.click_text_anywhere("Select Master");
    h.frames(2);
    assert!(h.app.state.selection.contains(&rig.master));
    assert!(h.has(egui::accesskit::Role::Button, "Select Goal"));
    h.click_text_anywhere("Select Goal");
    h.frames(2);
    assert!(h.app.state.selection.contains(&rig.limbs[0].goal));
    assert!(h.has(egui::accesskit::Role::Label, "IK / FK"));
}

#[test]
fn motion_panel_applies_source_clip_to_existing_character() {
    let mut h = Harness::new();
    h.screen.max.y = 2000.;
    let root = character(&mut h);
    let path = std::env::temp_dir().join(format!("loom-retarget-ui-{}.bvh", std::process::id()));
    std::fs::write(&path,"HIERARCHY ROOT Hips { OFFSET 0 100 0 CHANNELS 6 Xposition Yposition Zposition Zrotation Yrotation Xrotation JOINT Head { OFFSET 0 20 0 CHANNELS 3 Zrotation Yrotation Xrotation End Site { OFFSET 0 5 0 } } } MOTION Frames: 2 Frame Time: 0.05 0 100 0 0 0 0 0 0 0 0 98 2 20 0 0 10 0 0").unwrap();
    h.cmd(Command::ImportAnimationClip {
        path: Some(path.to_string_lossy().into_owned()),
    });
    h.cmd(Command::CreateLoomRig {
        root: root.0,
        mapping: None,
    });
    h.app.clip_timeline.open=true;h.app.clip_timeline.tab=2;
    h.frames(3);
    assert!(h.has(egui::accesskit::Role::Button, "Apply Motion to Character"));
    h.click_text_anywhere("Apply Motion to Character");
    h.frames(3);
    assert_eq!(
        h.app.state.scene.animation_clips.len(),
        2,
        "source retained and new character clip added: {}",
        h.app.state.status_prompt
    );
    assert!(h.app.state.scene.animation_clips[1]
        .nodes
        .iter()
        .all(|id| *id == root || h.app.state.scene.ancestors(*id).contains(&root)));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn rig_timeline_moves_one_ik_fk_key_with_one_undo() {
    let mut h = Harness::new();
    let root = character(&mut h);
    h.cmd(Command::CreateLoomRig {
        root: root.0,
        mapping: None,
    });
    for (time, value) in [(0., 0.), (20., 1.)] {
        h.cmd(Command::SetLoomRigBlendKey {
            rig: 1,
            limb: "Left Arm".into(),
            time,
            value: Some(value),
        });
    }
    h.app.state.frame_range = (0, 100);
    h.frames(3);
    assert!(!h.has(egui::accesskit::Role::Button,"Left Arm IK/FK @ 20"));
    h.click_text_anywhere("Animation");h.click_text_anywhere("Animation Editor...");h.frames(3);
    let rect = h.find(egui::accesskit::Role::Button, "Left Arm IK/FK @ 20");
    h.drag(rect.center(), rect.center() + egui::vec2(75., 0.));
    let time = h.app.state.scene.loom_rigs[0].limbs[0]
        .blend
        .keys()
        .unwrap()
        .tracks[0]
        .keys[1]
        .time;
    assert!(time > 20.);
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(
        h.app.state.scene.loom_rigs[0].limbs[0]
            .blend
            .keys()
            .unwrap()
            .tracks[0]
            .keys[1]
            .time,
        20.
    );
}

#[test]
fn second_character_can_be_rigged_and_chain_rows_are_editable() {
    let mut h=Harness::new();h.screen.max.y=3000.;let first=character(&mut h);
    h.cmd(Command::CreateLoomRig{root:first.0,mapping:None});
    let second=character(&mut h);h.app.clip_timeline.open=true;h.app.clip_timeline.tab=2;h.frames(3);
    h.click_text_anywhere("Create Character Rig");h.frames(2);
    assert!(h.has(egui::accesskit::Role::Button,"Create LoomRig"));
    h.click_text_anywhere("Characterization");h.frames(2);
    assert!(h.has(egui::accesskit::Role::Button,"Add limb chain"));
    h.click_text_anywhere("Add limb chain");h.frames(2);
    assert_eq!(h.widgets.iter().filter(|(role,text,_)|*role==egui::accesskit::Role::Button&&text=="Remove chain").count(),5);
    let remove=h.widgets.iter().filter(|(role,text,_)|*role==egui::accesskit::Role::Button&&text=="Remove chain").last().unwrap().2;
    h.click(remove.center());h.frames(2);
    h.click_text_anywhere("Create LoomRig");h.frames(2);
    assert_eq!(h.app.state.scene.loom_rigs.len(),2);
    assert_eq!(h.app.state.scene.loom_rigs[1].mapping.root,second);
}

#[test]
fn contact_editor_presets_are_separate_and_capture_replace_and_key_real_contacts() {
    let mut h = Harness::new();
    h.screen.max.y = 1800.;
    let (state,mapping) = crate::loomrig_overlay::tests::rig();
    h.app.state = state;
    h.frames(3);
    assert!(!h.has(egui::accesskit::Role::Button,"Planted"));
    h.app.clip_timeline.open = true;
    h.app.clip_timeline.tab = 2;
    h.frames(3);
    h.click_text_anywhere("Planted");
    assert_eq!(h.app.state.scene.loom_rigs[0].limbs[0].contacts.keys()[0].blend,1.);
    h.cmd(Command::SetFrame {frame:10});
    h.click_text_anywhere("Sliding");
    let limb = h.app.state.scene.loom_rigs[0].limbs[0].clone();
    assert_ne!(limb.contacts.keys()[0].reference,limb.contacts.keys()[1].reference);
    let ids = [mapping.limbs[0].start,mapping.limbs[0].middle,mapping.limbs[0].end];
    let pose = ids.map(|id|h.app.state.scene.world_matrix(id));
    let before = h.app.state.scene.loom_rigs.clone();
    let undo = h.app.state.undo.position();
    h.click_text_anywhere("Free");
    assert_eq!(h.app.state.scene.loom_rigs[0].limbs[0].contacts.keys().len(),2);
    assert_eq!(h.app.state.scene.loom_rigs[0].limbs[0].contacts.keys()[1].blend,0.);
    assert_eq!(h.app.state.undo.position(),undo+1);
    for (id,expected) in ids.iter().zip(pose) {assert!(h.app.state.scene.world_matrix(*id).abs_diff_eq(expected,0.001));}
    h.click_text_anywhere("Contact Anchors");
    assert!(!h.text_rects("Body").is_empty(),"the inspector must initially select this frame's active Free anchor");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.loom_rigs,before);
    h.cmd(Command::SetFrame {frame:5});
    assert!(!h.has(egui::accesskit::Role::Button,"Key IK / FK"));
    h.click_text_anywhere("Key Contact");
    assert!(h.app.state.scene.loom_rigs[0].limbs[0].contacts.keys().iter().any(|key|key.time == 5.));
    h.app.clip_timeline.open = false;
    h.frames(3);
    assert!(!h.has(egui::accesskit::Role::Button,"Planted"));
}

#[test]
fn contact_editor_changes_anchor_space_without_moving_affine_world_pose() {
    let mut h = Harness::new();
    h.screen.max.y = 1800.;
    let (state,mapping) = crate::loomrig_overlay::tests::rig();
    h.app.state=state;
    let root = h.app.state.scene.node_mut(mapping.root).unwrap();
    root.transform.scale.set(Vec3::new(-2.,3.,0.5));
    root.transform.rotation.set(Vec3::new(0.,0.,23.));
    root.transform.position.set(Vec3::Y*3.);
    h.app.state.scene.bump_version();
    h.cmd(Command::SetLoomRigContactPreset {rig:1,limb:"Arm".into(),time:0.,preset:"planted".into()});
    let goal = h.app.state.scene.loom_rigs[0].limbs[0].goal;
    let before = h.app.state.scene.world_matrix(goal);
    let pole = h.app.state.scene.loom_rigs[0].limbs[0].pole;
    let before_pole = h.app.state.scene.world_matrix(pole);
    let ids = [mapping.limbs[0].start,mapping.limbs[0].middle,mapping.limbs[0].end];
    let bones = ids.map(|id|h.app.state.scene.world_matrix(id));
    let metadata = h.app.state.scene.loom_rigs.clone();
    h.app.clip_timeline.open=true;
    h.app.clip_timeline.tab=2;
    h.frames(3);
    h.click_text_anywhere("Contact Anchors");
    let world = *h.text_rects("World").last().expect("anchor reference space combo");
    h.click(world.center());
    h.click_text_anywhere("Body");
    let limb = &h.app.state.scene.loom_rigs[0].limbs[0];
    let reference = limb.contacts.keys()[0].reference;
    assert_eq!(limb.contacts.references()[&reference].space,loom_scene::loomrig_contact::ContactSpace::Body);
    assert!(h.app.state.scene.world_matrix(goal).abs_diff_eq(before,0.001));
    assert!(h.app.state.scene.world_matrix(pole).abs_diff_eq(before_pole,0.001));
    for (id,expected) in ids.iter().zip(bones) {assert!(h.app.state.scene.world_matrix(*id).abs_diff_eq(expected,0.003));}
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.loom_rigs,metadata);
}

#[test]
fn contact_timeline_moves_and_deletes_contact_with_matching_pose_in_one_undo() {
    let mut h = Harness::new();
    let (state,_) = crate::loomrig_overlay::tests::rig();
    h.app.state = state;
    for (time,preset) in [(0.,"planted"),(20.,"free")] {
        h.cmd(Command::CreateLoomRigContactKey {rig:1,limb:"Arm".into(),time,preset:preset.into()});
    }
    let before = h.app.state.scene.loom_rigs.clone();
    h.app.state.frame_range = (0,100);
    h.app.clip_timeline.open = true;
    h.app.clip_timeline.tab = 0;
    h.frames(3);
    let marker = h.find(egui::accesskit::Role::Button,"Arm Contact @ 20");
    let undo = h.app.state.undo.position();
    h.drag(marker.center(),marker.center()+egui::vec2(75.,0.));
    let limb = &h.app.state.scene.loom_rigs[0].limbs[0];
    let moved = limb.contacts.keys()[1].time;
    assert!(moved>20.);
    assert!(limb.pose_matches.keys().iter().any(|key|key.time == moved));
    assert_eq!(h.app.state.undo.position(),undo+1);
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.loom_rigs,before);
    let marker = h.find(egui::accesskit::Role::Button,"Arm Contact @ 20");
    let occupied = h.find(egui::accesskit::Role::Button,"Arm Contact @ 0");
    let undo = h.app.state.undo.position();
    h.drag(marker.center(),occupied.center());
    assert_eq!(h.app.state.scene.loom_rigs,before,"collision must retain both contact and pose keys");
    assert_eq!(h.app.state.undo.position(),undo);
    let marker = h.find(egui::accesskit::Role::Button,"Arm Contact @ 20");
    right_click(&mut h,marker.center());
    h.click_text_anywhere("Delete Contact Key");
    assert_eq!(h.app.state.scene.loom_rigs[0].limbs[0].contacts.keys().len(),1);
    assert!(!h.app.state.scene.loom_rigs[0].limbs[0].pose_matches.keys().iter().any(|key|key.time == 20.));
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.loom_rigs,before);
}

#[test]
fn animation_editor_displays_the_effective_contact_ik_weight() {
    let mut h = Harness::new();
    h.screen.max.y = 1800.;
    let (state,_) = crate::loomrig_overlay::tests::rig();
    h.app.state = state;
    h.app.state.scene.loom_rigs[0].limbs[0].contacts = loom_scene::loomrig_contact::ContactTrack::constant(
        loom_scene::loomrig_contact::ContactReference {
            space:loom_scene::loomrig_contact::ContactSpace::World,
            position:Vec3::new(12.,8.,0.),rotation:glam::Quat::IDENTITY,pole_position:Vec3::Z*20.,
        },0.73,
    ).unwrap();
    h.app.state.scene.bump_version();
    h.app.clip_timeline.open = true;
    h.app.clip_timeline.tab = 2;
    h.frames(3);
    assert!(!h.text_rects("0.73").is_empty(),"IK weight must show the active contact's value, not the base FK weight");
    let before = h.app.state.scene.loom_rigs.clone();
    let slider = h.find(egui::accesskit::Role::Slider,"IK weight");
    h.click(egui::pos2(slider.left()+slider.width()*0.2,slider.center().y));
    let weight = h.app.state.scene.loom_rigs[0].limbs[0].contacts.sample_blend(0.).unwrap().unwrap();
    assert!((weight-0.73).abs()>0.1,"pointer interaction must change the effective contact weight");
    assert_eq!(h.app.state.scene.loom_rigs[0].limbs[0].blend.get(),0.,"contact editing must retain the base curve");
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.loom_rigs,before);
}

#[test]
fn contact_controller_pointer_drag_updates_target_and_keeps_source_tracks_with_one_undo() {
    for pole in [false, true] {
        let mut h = Harness::new();
        let (state, mapping) = crate::loomrig_overlay::tests::rig();
        h.app.state = state;
        let goal = h.app.state.scene.loom_rigs[0].limbs[0].goal;
        let control = if pole {h.app.state.scene.loom_rigs[0].limbs[0].pole} else {goal};
        let end = mapping.limbs[0].end;
        h.cmd(Command::CreateLoomRigContactKey {
            rig: 1, limb: "Arm".into(), time: 0., preset: "planted".into(),
        });
        h.cmd(Command::SetTransform {
            id: goal.0, position: Some([12., 8., 0.]), rotation_deg: None, scale: None,
        });
        h.app.state.selection = [control].into();
        h.app.state.mesh_dirty = true;
        h.app.sync_after_command();
        framed(&mut h);
        h.cmd(Command::SelectAndMove);
        let source: Vec<_> = h.app.state.scene.nodes().map(|n|(n.id,n.transform.clone())).collect();
        let before = h.app.state.scene.loom_rigs.clone();
        let before_end = h.app.state.scene.world_matrix(end).w_axis.truncate();
        let middle = mapping.limbs[0].middle;
        let before_middle = h.app.state.scene.world_matrix(middle).w_axis.truncate();
        let undo = h.app.state.undo.position();
        let origin = h.screen_of(h.app.state.scene.world_matrix(control).w_axis.truncate());
        h.drag(origin, origin + egui::vec2(15., -12.));
        assert_ne!(h.app.state.scene.loom_rigs, before, "pointer input must edit the contact reference");
        assert_eq!(h.app.state.undo.position(), undo + 1, "one gesture must be one Undo");
        let target = h.app.state.scene.world_matrix(goal).w_axis.truncate();
        let actual = h.app.state.scene.world_matrix(end).w_axis.truncate();
        if pole {
            assert!(actual.distance(before_end) < 0.001, "pole editing must preserve the hand target");
            assert!(h.app.state.scene.world_matrix(middle).w_axis.truncate().distance(before_middle) > 0.001);
        } else {
            assert!(actual.distance(before_end) > 0.01);
        }
        assert!(actual.distance(target) < 0.001, "solved hand must follow the displayed target");
        for (id, transform) in &source {
            assert_eq!(&h.app.state.scene.node(*id).unwrap().transform, transform);
        }
        h.cmd(Command::UndoSceneOperation);
        assert_eq!(h.app.state.scene.loom_rigs, before);
        assert!(h.app.state.scene.world_matrix(end).w_axis.truncate().distance(before_end) < 0.001);
    }
}

#[test]
fn limb_proxy_pointer_rotate_drives_skin_once_and_undo_restores_authored_offset() {
    let mut h = Harness::new();
    let (state, mapping) = crate::loomrig_overlay::tests::rig();
    h.app.state = state;
    let bone = mapping.pelvis;
    let control = h.app.state.scene.loom_rigs[0].bindings.iter().find(|b| b.bone == bone).unwrap().control;
    h.app.state.scene.node_mut(mapping.root).unwrap().transform.scale.set(Vec3::new(-2., 3., 0.5));
    h.app.state.scene.node_mut(control).unwrap().transform.rotation.set(Vec3::new(20., 0., 35.));
    let a = h.app.state.scene.world_matrix(bone).transform_point3(Vec3::ZERO);
    let b = h.app.state.scene.world_matrix(mapping.limbs[0].middle).transform_point3(Vec3::ZERO);
    let mut skin = Node::new("skinned arm surface", ObjectKind::box_object(2., 2., 2.));
    skin.transform.position.set(a.lerp(b, 0.5) - Vec3::Z);
    let skin = h.app.state.scene.add_node(skin);
    let mut binding = loom_scene::skin::SkinData::bind(&h.app.state.scene, skin, &[bone]);
    for row in &mut binding.weights { *row = vec![(0, 1.)]; }
    h.app.state.scene.node_mut(skin).unwrap().modifiers.push(loom_scene::ModifierSlot::new(loom_scene::Modifier::Skin(Box::new(binding))));
    h.app.state.selection.clear();
    h.app.state.mesh_dirty = true;
    h.app.sync_after_command();
    framed(&mut h);
    h.cmd(Command::SelectObject);
    let proxy = h.app.rig_controls.iter().find(|p| p.control == control).unwrap();
    let centre = proxy.vertices.iter().copied().sum::<Vec3>() / 8.;
    let p = h.screen_of(centre);
    h.click(p);
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), vec![control], "the actual pointer event must select the rig input, not a source bone or skin");
    let source = h.app.state.scene.node(bone).unwrap().transform.clone();
    let before_control = h.app.state.scene.node(control).unwrap().transform.clone();
    let before_world = h.app.state.scene.world_matrix(bone);
    let before_skin = serde_json::to_value(&h.app.state.scene.node(skin).unwrap().modifiers).unwrap();
    let positions = |h: &Harness| {
        let index = h.app.world_meshes().iter().position(|(id, _)| *id == skin).unwrap();
        h.app.drawn_mesh(index).vertices().iter().map(|v| v.position).collect::<Vec<_>>()
    };
    let before = positions(&h);
    h.cmd(Command::SelectAndRotate);
    let viewport = h.app.state.active_viewport;
    let rect = h.app.viewport_rects[viewport];
    let camera = h.app.camera_for(viewport);
    let projection = crate::gizmo::Projection::new(camera.view_matrix(), camera.projection_matrix(rect.width() / rect.height()), rect);
    let layout = crate::gizmo::layout_for(&h.app, viewport, rect.height());
    let origin = projection.screen(layout.origin).unwrap();
    let start = (0..360).step_by(5).flat_map(|angle| (10..150).step_by(2).map(move |radius| (angle as f32, radius as f32)))
        .map(|(angle, radius)| origin + egui::vec2(angle.to_radians().cos(), angle.to_radians().sin()) * radius)
        .find(|p| rect.shrink(10.).contains(*p) && matches!(crate::gizmo::hit(&h.app, &layout, &projection, *p), Some(crate::gizmo::Handle::Axis(_))))
        .expect("visible rotate ring");
    h.drag(start, start + (start - origin).normalized().rot90() * 25.);
    let after_world = h.app.state.scene.world_matrix(bone);
    assert_ne!(h.app.state.scene.node(control).unwrap().transform, before_control);
    assert!(!after_world.abs_diff_eq(before_world, 1e-4));
    assert_eq!(h.app.state.scene.node(bone).unwrap().transform, source, "source bone controllers must not be baked or edited");
    assert!(h.app.state.scene.world_matrix(control).abs_diff_eq(after_world, 1e-4));
    let delta = after_world * before_world.inverse();
    let after = positions(&h);
    assert!(after.iter().zip(&before).any(|(a, b)| a.distance(*b) > 1e-4));
    for (a, b) in after.iter().zip(&before) { assert!(a.abs_diff_eq(delta.transform_point3(*b), 2e-4), "the FK correction must affect the skin exactly once"); }
    assert_eq!(serde_json::to_value(&h.app.state.scene.node(skin).unwrap().modifiers).unwrap(), before_skin);
    h.cmd(Command::UndoSceneOperation);
    assert_eq!(h.app.state.scene.node(control).unwrap().transform, before_control);
    for (a, b) in positions(&h).iter().zip(&before) { assert!(a.abs_diff_eq(*b, 1e-4)); }
}

#[test]
fn limb_proxy_pointer_selects_animator_input_inside_closed_character_group() {
    let mut h = Harness::new();
    let (mut state, mapping) = crate::loomrig_overlay::tests::rig();
    let group = state.scene.add_node(Node::new("character group", ObjectKind::Group { open: false }));
    state.scene.node_mut(mapping.root).unwrap().parent = Some(group);
    h.app.state = state;
    h.app.state.selection.clear();
    h.app.state.mesh_dirty = true;
    h.app.sync_after_command();
    framed(&mut h); h.cmd(Command::SelectObject);
    let proxy = &h.app.rig_controls[0]; let control = proxy.control;
    let centre = proxy.vertices.iter().copied().sum::<Vec3>() / 8.;
    h.click(h.screen_of(centre));
    assert_eq!(h.app.state.selection.iter().copied().collect::<Vec<_>>(), vec![control], "an animator input must remain directly selectable inside a closed character container");
    assert_eq!(h.app.state.selection_target(mapping.pelvis), group, "source-bone group behavior stays intact");
}
