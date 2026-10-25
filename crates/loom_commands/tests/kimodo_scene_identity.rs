use loom_commands::{execute,file_action,AppState,Command,PendingFileAction,PrimitiveKind};

#[test]
fn file_new_always_changes_project_identity_even_for_an_empty_saved_scene() {
    for populated in [false,true] {
        let mut s=AppState::new();
        if populated { execute(&mut s,Command::CreatePrimitive{kind:PrimitiveKind::Box,position:[0.;3],size:[1.;3]}).unwrap(); }
        s.file=Some(std::path::PathBuf::from("previous.loom"));
        let previous=s.scene.instance_id();
        file_action(&mut s,PendingFileAction::New).unwrap();
        assert_ne!(previous,s.scene.instance_id());
        assert!(s.file.is_none());
        assert_eq!(s.scene.len(),0);
    }
}
