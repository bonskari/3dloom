//! Rendering > Render and Render SetUp. The command layer has no GPU: Render
//! records a request that the UI performs with its renderer.

use crate::command::Command as C;
use crate::state::RenderRequest;
use crate::{AppState, Dialog};
use serde_json::{json, Value};

/// Largest output side in pixels.
pub const MAX_RENDER_SIZE: u32 = 8192;

fn check_size(width: u32, height: u32) -> Result<(), String> {
    if width == 0 || height == 0 || width > MAX_RENDER_SIZE || height > MAX_RENDER_SIZE {
        return Err(format!("render size {width}x{height}: 1 to {MAX_RENDER_SIZE} pixels"));
    }
    Ok(())
}

pub fn execute(state: &mut AppState, cmd: C) -> Result<Value, String> {
    match cmd {
        C::Render { path, width, height } => {
            let width = width.unwrap_or(state.render_size.0);
            let height = height.unwrap_or(state.render_size.1);
            check_size(width, height)?;
            let Some(path) = path else {
                state.dialog = Some(Dialog::RenderFile);
                return Ok(json!({ "dialog": "RenderFile" }));
            };
            if path.trim().is_empty() {
                return Err("render path is empty".into());
            }
            let viewport = state.active_viewport;
            state.pending_render = Some(RenderRequest { path: path.clone(), width, height, viewport });
            Ok(json!({ "pending": true, "path": path, "width": width, "height": height, "viewport": viewport }))
        }
        C::RenderSetup { width, height } => {
            if width.is_none() && height.is_none() {
                state.dialog = Some(Dialog::RenderSetup);
                return Ok(json!({ "dialog": "RenderSetup" }));
            }
            let w = width.unwrap_or(state.render_size.0);
            let h = height.unwrap_or(state.render_size.1);
            check_size(w, h)?;
            state.render_size = (w, h);
            Ok(json!({ "width": w, "height": h }))
        }
        _ => unreachable!("routed by execute_inner"),
    }
}
