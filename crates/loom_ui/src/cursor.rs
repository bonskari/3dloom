//! The mouse cursor for the current context: one resolution per frame from
//! what the pointer is over and what is being dragged (transform tools on the
//! gizmo, selection, navigation modes, creation, pick modes, paint brushes,
//! spinners, rendering). Places it does not decide (text fields, panel
//! splitters) keep the cursor their egui widget set.

use crate::gizmo::Handle;
use crate::LoomApp;
use eframe::egui::{self, CursorIcon};
use loom_commands::{PickMode, ScaleKind, SelectionMode, Tool};
use loom_input::NavMode;

/// What the pointer is over in the hovered viewport this frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ViewportHover {
    /// The transform gizmo handle under the pointer.
    pub gizmo: Option<Handle>,
    /// An object (object level) under the pointer.
    pub object: bool,
    /// A sub-object (vertex, edge, polygon) under the pointer.
    pub sub_object: bool,
}

fn spinners_id() -> egui::Id {
    egui::Id::new("cursor_spinner_ids")
}

/// Registers a spinner's widget id; its hover and drag show the spinner cursor.
pub fn note_spinner(ctx: &egui::Context, id: egui::Id) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Vec<egui::Id>>(spinners_id()).push(id));
}

/// The cursor of a transform tool (on its gizmo, or dragging with it).
fn tool_cursor(app: &LoomApp, handle: Option<Handle>) -> CursorIcon {
    match (app.state.active_tool, handle) {
        (_, Some(Handle::CenterBox)) | (Tool::Move, _) => CursorIcon::Move,
        (Tool::Rotate, _) | (_, Some(Handle::Screen | Handle::Free)) => CursorIcon::AllScroll,
        (Tool::Scale, _) | (_, Some(Handle::Uniform)) => match app.state.scale_kind {
            ScaleKind::Uniform => CursorIcon::ResizeNwSe,
            ScaleKind::NonUniform => CursorIcon::ResizeNeSw,
            ScaleKind::Squash => CursorIcon::ResizeVertical,
        },
        (Tool::Select, _) => CursorIcon::Default,
    }
}

/// A modal navigation tool's cursor; `dragging` while its drag runs.
fn nav_cursor(mode: NavMode, dragging: bool) -> CursorIcon {
    match mode {
        NavMode::None => CursorIcon::Default,
        NavMode::Pan | NavMode::TruckCamera => {
            if dragging {
                CursorIcon::Grabbing
            } else {
                CursorIcon::Grab
            }
        }
        NavMode::Zoom | NavMode::ZoomAll | NavMode::ZoomRegion | NavMode::Fov | NavMode::Perspective => CursorIcon::ZoomIn,
        NavMode::Orbit | NavMode::OrbitSelected | NavMode::OrbitSubObject | NavMode::OrbitCamera | NavMode::PanCamera | NavMode::RollCamera | NavMode::WalkThrough => CursorIcon::AllScroll,
        NavMode::DollyCamera | NavMode::DollyTarget | NavMode::DollyBoth | NavMode::LightHotspot | NavMode::LightFalloff => CursorIcon::ResizeVertical,
    }
}

/// A paint brush owns the left button (the ring shows its size).
fn painting(app: &LoomApp) -> bool {
    (app.state.soft_paint.is_some() && app.state.selection_mode != SelectionMode::Object)
        || app.state.paint_deform.is_some()
        || app.state.vertex_paint_brush
        || app.state.vertex_paint_pick
        || loom_commands::skin::painting(&app.state)
}

fn creating(app: &LoomApp) -> bool {
    app.create_kind.is_some() || app.create.is_some() || app.line.is_some() || app.camera_create.is_some() || app.helper_create.is_some()
}

/// The cursor for this frame. `hover` is the hovered viewport's state (None
/// off the viewports), `spinner` a spinner hovered or dragged, `busy` a
/// render in progress. None leaves egui's own cursor.
pub fn resolve(app: &LoomApp, hover: Option<&ViewportHover>, spinner: bool, busy: bool) -> Option<CursorIcon> {
    if busy {
        return Some(CursorIcon::Wait);
    }
    if spinner {
        return Some(CursorIcon::ResizeVertical);
    }
    if let Some(drag) = app.transform_drag {
        return Some(tool_cursor(app, Some(drag.handle)));
    }
    let hover = hover?;
    let nav = app.state.nav_mode;
    let nav_drag = app.camera_controller.drag_mode();
    if nav != NavMode::None {
        return Some(nav_cursor(nav, nav_drag.is_some()));
    }
    // Middle-button (and Alt) navigation outside a modal tool.
    match nav_drag {
        Some("pan") => return Some(CursorIcon::Grabbing),
        Some("orbit" | "roll") => return Some(CursorIcon::AllScroll),
        Some("zoom") => return Some(CursorIcon::ZoomIn),
        _ => {}
    }
    if painting(app) {
        return Some(CursorIcon::Crosshair);
    }
    match app.state.pick_mode {
        Some(PickMode::Link) => return Some(CursorIcon::Alias),
        Some(_) => return Some(CursorIcon::PointingHand),
        None => {}
    }
    if creating(app) {
        return Some(CursorIcon::Crosshair);
    }
    if app.state.active_tool != Tool::Select && app.has_selection() && hover.gizmo.is_some() {
        return Some(tool_cursor(app, hover.gizmo));
    }
    if hover.object || hover.sub_object {
        return Some(match app.state.active_tool {
            Tool::Select if hover.sub_object => CursorIcon::Cell,
            Tool::Select => CursorIcon::Crosshair,
            _ => tool_cursor(app, None),
        });
    }
    Some(CursorIcon::Default)
}

/// Sets this frame's cursor (end of the frame, after every widget).
pub fn apply(ctx: &egui::Context, app: &mut LoomApp) {
    let ids = ctx.data_mut(|d| d.remove_temp::<Vec<egui::Id>>(spinners_id())).unwrap_or_default();
    let spinner = ids.iter().any(|id| ctx.is_being_dragged(*id) || ctx.read_response(*id).is_some_and(|r| r.hovered()));
    let hover = app.viewport_hover.take();
    if let Some(icon) = resolve(app, hover.as_ref(), spinner, app.render_job.is_some()) {
        ctx.set_cursor_icon(icon);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_render_in_progress_shows_the_wait_cursor_everywhere() {
        let app = LoomApp::headless();
        assert_eq!(resolve(&app, None, false, true), Some(CursorIcon::Wait));
        assert_eq!(resolve(&app, Some(&ViewportHover::default()), true, true), Some(CursorIcon::Wait));
        assert_eq!(resolve(&app, None, false, false), None, "off the viewports egui's own cursor stays");
    }
}
