use loom_mcp::{tool_list, tool_name};
use std::collections::HashSet;

#[test]
fn every_registry_command_has_an_mcp_tool() {
    let tools: HashSet<String> = tool_list()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect();
    let missing: Vec<&str> = loom_commands::registry()
        .iter()
        .map(|i| i.name)
        .filter(|n| !tools.contains(&tool_name(n)))
        .collect();
    assert!(missing.is_empty(), "commands without MCP tools: {missing:?}");
    assert!(tools.contains("loom_launch") && tools.contains("loom_screenshot") && tools.contains("loom_read_file"));
}

#[test]
fn read_file_summarises_a_loom_file_without_the_gui() {
    let dir = std::env::temp_dir().join("loom-mcp-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("read.loom");
    let mut s = loom_commands::AppState::new();
    loom_commands::execute(&mut s, loom_commands::Command::CreatePrimitive { kind: loom_commands::PrimitiveKind::Teapot, position: [0.0; 3], size: [10.0; 3] }).unwrap();
    loom_commands::execute(&mut s, loom_commands::Command::SaveFile { path: Some(path.to_string_lossy().into_owned()) }).unwrap();
    let v = loom_mcp::read_file(path.to_str().unwrap()).unwrap();
    assert_eq!(v["nodes"][0]["name"], "Teapot001");
    assert!(v["nodes"][0]["faces"].as_u64().unwrap() > 100);
    assert!(loom_mcp::read_file("/nonexistent/x.loom").is_err());
}

#[test]
fn tool_names_are_unique_and_snake_case() {
    let names: Vec<String> = tool_list()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect();
    let set: HashSet<&String> = names.iter().collect();
    assert_eq!(set.len(), names.len(), "duplicate tool names");
    for n in &names {
        assert!(
            n.starts_with("loom_") && n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "{n} is not snake_case"
        );
    }
    assert_eq!(tool_name("Wireframe / Smooth+Highlights Toggle"), "loom_wireframe_smooth_highlights_toggle");
    assert_eq!(tool_name("Sub-object Level Cycle"), "loom_sub_object_level_cycle");
    assert_eq!(tool_name("Rotate +90"), "loom_rotate_plus_90");
    assert_eq!(tool_name("Rotate -90"), "loom_rotate_minus_90");
}

#[test]
fn every_control_socket_request_has_a_tool() {
    let tools: HashSet<String> = tool_list().iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect();
    for (cmd, _, schema) in loom_commands::live::app_commands() {
        assert!(tools.contains(&format!("loom_{cmd}")), "no tool for `{cmd}`");
        assert_eq!(schema["type"], "object");
        if cmd != "screenshot" {
            assert_eq!(loom_mcp::app_command(&format!("loom_{cmd}")), Some(cmd));
        }
    }
    assert_eq!(loom_mcp::app_command("loom_state"), None, "registry commands are not app commands");
    for t in ["loom_instances", "loom_attach_instance", "loom_logs", "loom_diagnostics", "loom_input", "loom_log_tail", "loom_reset_interaction", "loom_ui_hit"] {
        assert!(tools.contains(t), "{t}");
    }
}

#[test]
fn logs_are_read_by_pid_or_newest() {
    let dir = std::env::temp_dir().join(format!("loom-mcp-logs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("loom-2026-01-01-12.log"), "a\nb\nc\n").unwrap();
    std::fs::write(dir.join("loom-2026-01-01-112.log"), "x\n").unwrap();
    let v = loom_mcp::read_logs(&dir, Some(12), 2).unwrap();
    assert_eq!(v["text"], "b\nc");
    assert!(v["path"].as_str().unwrap().ends_with("-12.log"));
    assert_eq!(v["files"].as_array().unwrap().len(), 2);
    assert!(loom_mcp::read_logs(&dir, Some(7), 2).is_err());
    assert!(loom_mcp::read_logs(&dir, None, 5).is_ok());
}
