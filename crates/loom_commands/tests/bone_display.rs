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
