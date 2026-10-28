use loom_commands::{execute,AppState,Command};
use loom_scene::{Node,ObjectKind};
#[test]
fn explicit_bone_display_modes_clear_the_conflicting_hide_bones_filter(){
 for command in [Command::DisplayBones,Command::DisplayBonesAndObjects]{
  let mut state=AppState::new();let bone=state.scene.add_node(Node::new("pelvis",ObjectKind::bone(10.)));
  state.display.hide_bones=true;
  assert!(!state.is_visible(state.scene.node(bone).unwrap()));
  execute(&mut state,command).unwrap();
  assert!(state.is_visible(state.scene.node(bone).unwrap()),"explicit Display Bones must restore visible bones");
 }
}

#[test]
fn explicit_bone_display_is_independent_of_the_geometry_category() {
    for command in [Command::DisplayBones, Command::DisplayBonesAndObjects] {
        let mut state = AppState::new();
        let bone = state.scene.add_node(Node::new("pelvis", ObjectKind::bone(10.)));
        let mesh = state.scene.add_node(Node::new("body", ObjectKind::box_object(1., 1., 1.)));
        state.hide_category.geometry = true;
        state.display.hide_bones = true;
        execute(&mut state, command).unwrap();
        assert!(state.is_visible(state.scene.node(bone).unwrap()), "Bone Objects has its own visibility category");
        assert!(!state.is_visible(state.scene.node(mesh).unwrap()), "showing bones must preserve the geometry filter");
    }
}

#[test]
fn explicit_bone_display_preserves_own_hidden_and_frozen_policies() {
    let mut state = AppState::new();
    let mut hidden = Node::new("hidden bone", ObjectKind::bone(10.));
    hidden.hidden = true;
    let hidden = state.scene.add_node(hidden);
    let mut frozen = Node::new("frozen bone", ObjectKind::bone(10.));
    frozen.frozen = true;
    let frozen = state.scene.add_node(frozen);
    state.display.hide_frozen = true;
    execute(&mut state, Command::DisplayBones).unwrap();
    assert!(!state.is_visible(state.scene.node(hidden).unwrap()));
    assert!(!state.is_visible(state.scene.node(frozen).unwrap()));
}

#[test]
fn helper_category_is_independent_of_geometry_and_bones() {
    let mut state = AppState::new();
    let helper = state.scene.add_node(Node::new("control", ObjectKind::point(4.)));
    let bone = state.scene.add_node(Node::new("bone", ObjectKind::bone(10.)));
    let mesh = state.scene.add_node(Node::new("skin", ObjectKind::box_object(1., 1., 1.)));
    state.hide_category.helpers = true;
    assert!(!state.is_visible(state.scene.node(helper).unwrap()), "Helpers hides control markers");
    assert!(state.is_visible(state.scene.node(bone).unwrap()));
    assert!(state.is_visible(state.scene.node(mesh).unwrap()));
    state.hide_category.helpers = false;
    state.hide_category.geometry = true;
    assert!(state.is_visible(state.scene.node(helper).unwrap()), "Geometry must not hide animator controls");
    assert!(state.is_visible(state.scene.node(bone).unwrap()));
    assert!(!state.is_visible(state.scene.node(mesh).unwrap()));
}
