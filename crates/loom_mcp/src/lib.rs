//! MCP tool catalogue generated from the command registry, so every
//! command the UI can run is also an agent tool (spec 6.8).

use serde_json::{json, Value};

/// `loom_` + snake_case of the action name.
pub fn tool_name(action: &str) -> String {
    let mut out = String::from("loom_");
    let mut last_us = true;
    let chars: Vec<char> = action.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_us = false;
        } else if (*c == '+' || *c == '-') && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit()) {
            // A signed number ("Rotate +90" / "Rotate -90") keeps its sign as a word.
            if !last_us {
                out.push('_');
            }
            out.push_str(if *c == '+' { "plus_" } else { "minus_" });
            last_us = true;
        } else if !last_us {
            out.push('_');
            last_us = true;
        }
    }
    out.trim_end_matches('_').to_owned()
}

pub fn tool_list() -> Vec<Value> {
    let mut tools: Vec<Value> = loom_commands::registry()
        .iter()
        .map(|i| {
            json!({
                "name": tool_name(i.name),
                "description": format!("{} (action: {})", i.description, i.name),
                "inputSchema": i.args_schema,
            })
        })
        .collect();
    tools.push(json!({
        "name": "loom_launch",
        "description": "Start 3dLoom with its control socket if it is not already running, and report whether it answers.",
        "inputSchema": { "type": "object", "properties": {
            "display": { "type": "string", "description": "X display to launch on, e.g. \":90\"" }
        }, "required": [] },
    }));
    tools.push(json!({
        "name": "loom_read_file",
        "description": "Read a .loom scene file without the GUI: nodes (name, kind, parent, layer, stack, counts), layers and settings. See docs/LOOM-FORMAT.md.",
        "inputSchema": { "type": "object", "properties": {
            "path": { "type": "string", "description": "Path of the .loom file" }
        }, "required": ["path"] },
    }));
    // The control socket's own requests (diagnostics, input, logs, ...).
    for (cmd, description, schema) in loom_commands::live::app_commands() {
        if cmd == "screenshot" {
            continue; // loom_screenshot below returns the image itself.
        }
        tools.push(json!({ "name": format!("loom_{cmd}"), "description": description, "inputSchema": schema }));
    }
    tools.push(json!({
        "name": "loom_instances",
        "description": "List the running 3dLoom instances (every run has a private control socket), newest first, with pid, socket, version and log file, and which one the tools talk to.",
        "inputSchema": { "type": "object", "properties": {}, "required": [] },
    }));
    tools.push(json!({
        "name": "loom_attach_instance",
        "description": "Talk to this instance from now on: by pid (its Unix socket) or by TCP port (LOOM_CONTROL_PORT). Without arguments, the newest instance.",
        "inputSchema": { "type": "object", "properties": {
            "pid": { "type": "integer" },
            "port": { "type": "integer" }
        }, "required": [] },
    }));
    tools.push(json!({
        "name": "loom_logs",
        "description": "Read log files directly, also of a run that crashed or is not answering: the last lines of the log of `pid` (default: the attached instance, else the newest log) and the list of kept log files.",
        "inputSchema": { "type": "object", "properties": {
            "pid": { "type": "integer" },
            "lines": { "type": "integer", "description": "How many lines (default 200)" }
        }, "required": [] },
    }));
    tools.push(json!({
        "name": "loom_screenshot",
        "description": "Capture the 3dLoom window as it is rendered right now and return the image.",
        "inputSchema": { "type": "object", "properties": {
            "path": { "type": "string", "description": "Where to save the PNG; defaults to a temp file" }
        }, "required": [] },
    }));
    tools
}

/// `loom_read_file`: a summary of a `.loom` file, read in this process.
pub fn read_file(path: &str) -> Result<Value, String> {
    let doc = loom_commands::file::read(std::path::Path::new(path))?;
    Ok(loom_commands::file::summary(&doc))
}

/// The control-socket request an app-command tool sends (`loom_diagnostics`
/// → `diagnostics`), if `tool` is one.
pub fn app_command(tool: &str) -> Option<&'static str> {
    loom_commands::live::app_commands().into_iter().map(|(c, _, _)| c).find(|c| tool.strip_prefix("loom_") == Some(*c))
}

/// `loom_logs`: the log of `pid` (or the newest) in `dir`, and the kept files.
pub fn read_logs(dir: &std::path::Path, pid: Option<u32>, lines: usize) -> Result<Value, String> {
    let files = loom_commands::live::log_files(dir);
    let file = match pid {
        Some(pid) => files.iter().find(|f| f.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(&format!("-{pid}.log")))),
        None => files.first(),
    }
    .ok_or_else(|| format!("no log file{} in {}", pid.map(|p| format!(" of pid {p}")).unwrap_or_default(), dir.display()))?;
    let text = loom_commands::live::tail(file, lines).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(json!({ "path": file, "text": text, "files": files }))
}
