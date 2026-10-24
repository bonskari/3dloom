//! Maps control-socket JSON onto the command registry. The socket's own
//! requests (`loom_commands::live::app_commands`: ping, screenshot,
//! diagnostics, input, logs, ...) are handled here; everything else is a
//! registry command.

use eframe::egui;
use loom_ui::live::InputQueue;
use loom_ui::LoomApp;
use serde_json::{json, Value};

pub struct PendingShot {
    pub path: String,
}

pub enum Outcome {
    Done(Value),
    /// Response is deferred until the screenshot arrives.
    Screenshot(PendingShot),
    /// Response is deferred until the injected input was delivered.
    Input { steps: usize },
}

/// What the socket's own requests read and change besides the app.
#[derive(Default)]
pub struct Live {
    pub input: InputQueue,
    pub log_path: Option<std::path::PathBuf>,
    pub socket: Option<std::path::PathBuf>,
}

pub fn apply(app: &mut LoomApp, ctx: &egui::Context, live: &mut Live, body: &Value) -> Outcome {
    let cmd = body.get("cmd").and_then(Value::as_str).unwrap_or("");
    // The request's fields, inline or under `args` (as MCP sends them).
    let args = body.get("args").filter(|a| a.is_object()).unwrap_or(body);
    let done = |v: Value| Outcome::Done(v);
    let outcome = match cmd {
        "ping" => done(json!({
            "ok": true,
            "pid": std::process::id(),
            "version": env!("CARGO_PKG_VERSION"),
            "commit": env!("LOOM_COMMIT"),
            "socket": live.socket,
            "log": live.log_path,
        })),
        "screenshot" => {
            let path = args.get("path").and_then(Value::as_str).map(str::to_owned).unwrap_or_else(|| std::env::temp_dir().join(format!("loom-screenshot-{}.png", std::process::id())).to_string_lossy().into_owned());
            Outcome::Screenshot(PendingShot { path })
        }
        "diagnostics" => {
            let mut d = app.diagnostics(ctx);
            d["injected"] = json!({ "pending_input": !live.input.is_empty(), "held_modifiers": format!("{:?}", live.input.held()) });
            d["ok"] = json!(true);
            done(d)
        }
        "reset_interaction" => {
            let mut cleared = app.reset_interaction(ctx);
            if !live.input.is_empty() || live.input.held() != egui::Modifiers::NONE {
                live.input.clear();
                cleared.push("injected_input");
            }
            done(json!({ "ok": true, "cleared": cleared }))
        }
        "ui_hit" => match (args.get("x").and_then(Value::as_f64), args.get("y").and_then(Value::as_f64)) {
            (Some(x), Some(y)) => {
                let mut v = app.ui_hit(ctx, egui::pos2(x as f32, y as f32));
                v["ok"] = json!(true);
                done(v)
            }
            _ => done(json!({ "ok": false, "error": "`x` and `y` required" })),
        },
        "ui_tree" => {
            let mut v = app.ui_tree(ctx);
            v["ok"] = json!(true);
            done(v)
        }
        "input" => match loom_ui::live::parse_input(args.get("events").unwrap_or(&Value::Null)) {
            Ok(steps) => {
                let n = steps.len();
                live.input.push(steps);
                Outcome::Input { steps: n }
            }
            Err(e) => done(json!({ "ok": false, "error": e })),
        },
        "log_path" => done(json!({ "ok": live.log_path.is_some(), "path": live.log_path, "error": live.log_path.is_none().then_some("no log file") })),
        "log_tail" => {
            let lines = args.get("lines").and_then(Value::as_u64).unwrap_or(100) as usize;
            match live.log_path.as_deref().map(|p| loom_commands::live::tail(p, lines)) {
                Some(Ok(text)) => done(json!({ "ok": true, "path": live.log_path, "text": text })),
                Some(Err(e)) => done(json!({ "ok": false, "error": e.to_string() })),
                None => done(json!({ "ok": false, "error": "no log file" })),
            }
        }
        _ => return Outcome::Done(app.dispatch_json(body)),
    };
    if let Outcome::Done(v) = &outcome {
        if v["ok"] == false {
            log::warn!(target: "loom::control", "{cmd} -> {}", v["error"]);
        }
    } else {
        log::info!(target: "loom::control", "{cmd}");
    }
    outcome
}
