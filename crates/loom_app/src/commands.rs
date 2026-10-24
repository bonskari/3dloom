//! Maps control-socket JSON onto the command registry. Only `ping` and
//! `screenshot` are handled here; everything else is a registry command.

use loom_ui::LoomApp;
use serde_json::{json, Value};

pub struct PendingShot {
    pub path: String,
}

pub enum Outcome {
    Done(Value),
    /// Response is deferred until the screenshot arrives.
    Screenshot(PendingShot),
}

pub fn apply(app: &mut LoomApp, body: &Value) -> Outcome {
    match body.get("cmd").and_then(Value::as_str) {
        Some("ping") => Outcome::Done(json!({ "ok": true })),
        Some("screenshot") => match body.get("path").and_then(Value::as_str) {
            Some(path) => Outcome::Screenshot(PendingShot { path: path.to_string() }),
            None => Outcome::Done(json!({ "ok": false, "error": "`path` required" })),
        },
        _ => Outcome::Done(app.dispatch_json(body)),
    }
}
