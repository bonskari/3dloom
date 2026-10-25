//! Tools > Rename Objects: several objects renamed at once, one undo step.

use loom_commands::{execute, AppState, Command, PrimitiveKind};

fn names(s: &AppState) -> Vec<String> {
    let mut v: Vec<String> = s.scene.nodes().map(|n| n.name.clone()).collect();
    v.sort();
    v
}

fn rename(ids: Option<Vec<u64>>, base: Option<&str>, prefix: Option<&str>, first: Option<usize>, suffix: Option<&str>, last: Option<usize>, number: Option<(i64, i64)>) -> Command {
    Command::RenameObjects {
        ids,
        base_name: base.map(str::to_owned),
        prefix: prefix.map(str::to_owned),
        remove_first: first,
        suffix: suffix.map(str::to_owned),
        remove_last: last,
        base_number: number.map(|n| n.0),
        step: number.map(|n| n.1),
    }
}

#[test]
fn rename_objects_renames_the_selection_and_undoes_in_one_step() {
    let mut s = AppState::new();
    for x in [0.0, 20.0, 40.0] {
        execute(&mut s, Command::CreatePrimitive { kind: PrimitiveKind::Box, position: [x, 0.0, 0.0], size: [10.0; 3] }).unwrap();
    }
    execute(&mut s, Command::SelectAll).unwrap();
    let before = names(&s);
    execute(&mut s, rename(None, Some("Crate"), Some("SM_"), None, Some("_A"), None, Some((1, 1)))).unwrap();
    assert_eq!(names(&s), ["SM_Crate_A1", "SM_Crate_A2", "SM_Crate_A3"]);
    execute(&mut s, Command::UndoSceneOperation).unwrap();
    assert_eq!(names(&s), before, "one undo puts every name back");
    // Remove First / Last on the old names, on picked ids only.
    let first = s.scene.nodes().next().unwrap();
    let (id, old) = (first.id.0, first.name.clone());
    execute(&mut s, rename(Some(vec![id]), None, None, Some(1), None, Some(1), None)).unwrap();
    let chars: Vec<char> = old.chars().collect();
    let want: String = chars[1..chars.len() - 1].iter().collect();
    assert_eq!(s.scene.node(loom_scene::NodeId(id)).unwrap().name, want);
    execute(&mut s, Command::SelectNone).unwrap();
    assert!(execute(&mut s, rename(None, Some("X"), None, None, None, None, None)).is_err(), "nothing selected");
}
