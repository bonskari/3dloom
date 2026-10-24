//! MCP (stdio, JSON-RPC 2.0) server that drives a running 3dLoom instance
//! through its control socket.
//!
//! Env: `LOOM_CONTROL_PORT` (default 7833), `LOOM_BIN` (default `loom` on PATH),
//! `LOOM_DISPLAY` (X display used when launching).

use base64::Engine;
use loom_mcp::{tool_list, tool_name};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

fn main() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                respond(&mut out, &Value::Null, Err((-32700, e.to_string())));
                continue;
            }
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");
        // Notifications carry no id and must not be answered.
        if id.is_null() && method.starts_with("notifications/") {
            continue;
        }
        let result = handle(method, req.get("params").unwrap_or(&Value::Null));
        respond(&mut out, &id, result);
    }
}

fn respond(out: &mut impl Write, id: &Value, result: Result<Value, (i64, String)>) {
    let msg = match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        }
    };
    let _ = writeln!(out, "{msg}");
    let _ = out.flush();
}

fn handle(method: &str, params: &Value) -> Result<Value, (i64, String)> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "3dloom", "version": env!("CARGO_PKG_VERSION") },
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_list() })),
        "tools/call" => call_tool(params),
        other => Err((-32601, format!("unknown method `{other}`"))),
    }
}

fn call_tool(params: &Value) -> Result<Value, (i64, String)> {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    match name {
        "loom_launch" => launch(&args),
        "loom_screenshot" => screenshot(&args),
        "loom_read_file" => {
            let path = args.get("path").and_then(Value::as_str).ok_or((-32602, "path is required".to_owned()))?;
            let v = loom_mcp::read_file(path).map_err(|e| (-32000, e))?;
            Ok(json!({ "content": [{ "type": "text", "text": v.to_string() }] }))
        }
        _ => {
            let info = loom_commands::registry()
                .iter()
                .find(|i| tool_name(i.name) == name)
                .ok_or((-32602, format!("unknown tool `{name}`")))?;
            let body = json!({ "cmd": info.name, "args": args });
            let reply = send(&body).map_err(|e| (-32000, e))?;
            Ok(text_result(&reply))
        }
    }
}

fn screenshot(args: &Value) -> Result<Value, (i64, String)> {
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            std::env::temp_dir()
                .join("loom-screenshot.png")
                .to_string_lossy()
                .into_owned()
        });
    let reply = send(&json!({ "cmd": "screenshot", "path": path })).map_err(|e| (-32000, e))?;
    if reply.get("ok").and_then(Value::as_bool) != Some(true) {
        return Ok(error_result(&reply));
    }
    let bytes = std::fs::read(&path).map_err(|e| (-32000, format!("{path}: {e}")))?;
    Ok(json!({ "content": [
        { "type": "image",
          "data": base64::engine::general_purpose::STANDARD.encode(bytes),
          "mimeType": "image/png" },
        { "type": "text", "text": format!("Saved to {path}") },
    ]}))
}

fn launch(args: &Value) -> Result<Value, (i64, String)> {
    if send(&json!({ "cmd": "ping" })).is_ok() {
        return Ok(text("3dLoom is already running and answering on the control socket."));
    }
    let bin = std::env::var("LOOM_BIN").unwrap_or_else(|_| "loom".into());
    // Always a virtual X display: never the user's Wayland screen.
    let display = args
        .get("display")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| std::env::var("LOOM_DISPLAY").ok())
        .unwrap_or_else(|| ":90".to_owned());
    let mut cmd = std::process::Command::new(&bin);
    cmd.env("LOOM_CONTROL_PORT", port().to_string())
        .env("DISPLAY", &display)
        .env_remove("WAYLAND_DISPLAY")
        .env("XDG_SESSION_TYPE", "x11")
        .env("WINIT_UNIX_BACKEND", "x11")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    cmd.spawn()
        .map_err(|e| (-32000, format!("could not start `{bin}`: {e}")))?;
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if send(&json!({ "cmd": "ping" })).is_ok() {
            return Ok(text(&format!(
                "3dLoom started on display {display} and is answering on port {}.",
                port()
            )));
        }
    }
    Err((-32000, "3dLoom started but never answered the control socket".into()))
}

fn port() -> u16 {
    std::env::var("LOOM_CONTROL_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(7833)
}

/// One request, one response line, over a fresh connection.
fn send(body: &Value) -> Result<Value, String> {
    let stream = TcpStream::connect(("127.0.0.1", port())).map_err(|e| {
        format!(
            "3dLoom is not reachable on 127.0.0.1:{}: {e}. Use loom_launch first.",
            port()
        )
    })?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .ok();
    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    writeln!(writer, "{body}").map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&line).map_err(|e| format!("bad reply `{line}`: {e}"))
}

fn text_result(reply: &Value) -> Value {
    if reply.get("ok").and_then(Value::as_bool) == Some(true) {
        text(&reply.to_string())
    } else {
        error_result(reply)
    }
}

fn error_result(reply: &Value) -> Value {
    let msg = reply
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("command failed");
    json!({ "content": [{ "type": "text", "text": msg }], "isError": true })
}

fn text(s: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": s }] })
}
