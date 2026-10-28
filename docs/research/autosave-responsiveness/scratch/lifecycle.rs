pub use loom_commands::{file,state};
#[path="autosave.rs"] mod autosave;
#[test]
fn delayed_snapshot_acknowledges_only_captured_scene_version() {
 let mut state=loom_commands::AppState::new();
 let captured=state.undo.version();
 let job=autosave::prepare_now(&state);
 state.scene=loom_scene::Scene::new();
 let completion=job.write().unwrap();
 completion.apply(&mut state);
 assert_eq!(state.autosave.last_version,None);
 assert_eq!(state.autosave.next_index,2);
 assert_eq!(captured,0);
}
#[test]
fn delayed_worker_edits_failure_manual_order_and_shutdown() {
 use std::sync::mpsc;
 let mut state=loom_commands::AppState::new();
 state.prefs.auto_backup_name="Lifecycle".into();
 state.prefs.auto_backup_count=3;
 let job=autosave::prepare_now(&state);
 let first=job.path.clone();
 let (release,wait)=mpsc::channel();
 let worker=std::thread::spawn(move||{wait.recv().unwrap();job.write()});
 assert!(!worker.is_finished());
 assert!(!first.exists());
 loom_commands::execute(&mut state,loom_commands::Command::CreatePrimitive {kind:loom_commands::PrimitiveKind::Box,position:[0.;3],size:[1.;3]}).unwrap();
 let edited=state.undo.version();
 release.send(()).unwrap();
 worker.join().unwrap().unwrap().apply(&mut state);
 assert_eq!(state.autosave.last_version,Some(0));
 assert_ne!(edited,0);
 assert_eq!(loom_commands::file::read(&first).unwrap().scene.len(),0);
 // Manual backup is ordered after the joined automatic write.
 let second=autosave::backup_now(&mut state).unwrap();
 assert_ne!(first,second);
 assert_eq!(loom_commands::file::read(&second).unwrap().scene.len(),1);
 assert_eq!(state.autosave.last_version,Some(edited));
 // A real failing destination cannot acknowledge or advance retention.
 let mut job=autosave::prepare_now(&state);
 let blocked=first.parent().unwrap().join("blocked");
 std::fs::write(&blocked,b"not a directory").unwrap();
 job.path=blocked.join("failure.loom");
 let before=(state.autosave.next_index,state.autosave.last_version);
 assert!(job.write().is_err());
 assert_eq!((state.autosave.next_index,state.autosave.last_version),before);
 // Shutdown joins the started write before granting the caller exit.
 let job=autosave::prepare_now(&state);let path=job.path.clone();
 let worker=std::thread::spawn(move||job.write());
 worker.join().unwrap().unwrap().apply(&mut state);
 assert_eq!(loom_commands::file::read(&path).unwrap().scene.len(),1);
}
