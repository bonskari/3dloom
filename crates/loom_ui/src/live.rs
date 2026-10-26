//! Live debugging of a running app: what the input and interaction state is
//! right now, the last raw input events, clearing stuck interaction state,
//! what is under a screen point, and parsing injected input.

use crate::LoomApp;
use eframe::egui;
use serde_json::{json, Value};
use std::collections::VecDeque;

/// How many raw input events the ring buffer keeps.
const INPUT_LOG_LEN: usize = 200;

/// The last raw input events, with the frame time they arrived at. Kept in
/// memory only (typed text never reaches the log file).
#[derive(Debug, Default)]
pub struct InputLog {
    entries: VecDeque<(f64, String)>,
    last_modifiers: egui::Modifiers,
}

impl InputLog {
    fn push(&mut self, t: f64, entry: String) {
        if self.entries.len() == INPUT_LOG_LEN {
            self.entries.pop_front();
        }
        self.entries.push_back((t, entry));
    }

    /// Records one frame's raw input: modifier changes and every event
    /// (consecutive pointer moves collapse into the latest one).
    pub fn record(&mut self, raw: &egui::RawInput, t: f64) {
        if raw.modifiers != self.last_modifiers {
            self.last_modifiers = raw.modifiers;
            self.push(t, format!("Modifiers({})", modifier_names(raw.modifiers).join("+")));
        }
        for e in &raw.events {
            let text = match e {
                // The image would flood the buffer.
                egui::Event::Screenshot { .. } => "Screenshot".to_owned(),
                other => format!("{other:?}"),
            };
            if matches!(e, egui::Event::PointerMoved(_)) && self.entries.back().is_some_and(|(_, last)| last.starts_with("PointerMoved")) {
                self.entries.pop_back();
            }
            self.push(t, text);
        }
    }

    pub fn to_json(&self) -> Value {
        Value::Array(self.entries.iter().map(|(t, e)| json!({ "t": t, "event": e })).collect())
    }
}

fn modifier_names(m: egui::Modifiers) -> Vec<&'static str> {
    let mut v = Vec::new();
    if m.ctrl {
        v.push("ctrl");
    }
    if m.alt {
        v.push("alt");
    }
    if m.shift {
        v.push("shift");
    }
    if m.mac_cmd {
        v.push("cmd");
    }
    v
}

fn modifiers_json(m: egui::Modifiers) -> Value {
    json!({ "alt": m.alt, "ctrl": m.ctrl, "shift": m.shift, "command": m.command, "mac_cmd": m.mac_cmd })
}

fn rect_json(r: egui::Rect) -> Value {
    json!([r.min.x, r.min.y, r.max.x, r.max.y])
}

fn dbg<T: std::fmt::Debug>(v: &Option<T>) -> Value {
    v.as_ref().map_or(Value::Null, |v| Value::String(format!("{v:?}")))
}

fn layer_json(l: egui::LayerId) -> Value {
    json!({ "order": format!("{:?}", l.order), "id": l.id.short_debug_format() })
}

/// The app's top-level panels by id, in the order they are laid out.
const PANELS: [&str; 6] = ["menubar", "main_toolbar", "ribbon", "statusbar", "time_slider", "command_panel"];

fn panel_rects(ctx: &egui::Context) -> Vec<(&'static str, egui::Rect)> {
    PANELS
        .iter()
        .filter_map(|name| egui::containers::panel::PanelState::load(ctx, egui::Id::new(*name)).map(|s| (*name, s.rect)))
        .collect()
}

/// Logs a command the app ran (name, arguments, result). A command repeated
/// back to back (paint strokes, drags) is logged once with a repeat count.
pub fn log_command(body: &Value, result: Result<&Value, &str>) {
    use std::sync::Mutex;
    static LAST: Mutex<(String, u32)> = Mutex::new((String::new(), 0));
    let name = body["cmd"].as_str().unwrap_or("?");
    // Playback ticks run every frame while playing.
    if name == "Tick Playback" {
        return;
    }
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Err(e) = result {
        log::warn!(target: "loom::cmd", "{name} {} -> error: {e}", clip(&body["args"].to_string(), 300));
        return;
    }
    if last.0 == name {
        last.1 += 1;
        return;
    }
    if last.1 > 0 {
        log::info!(target: "loom::cmd", "{} repeated {} more times", last.0, last.1);
    }
    *last = (name.to_owned(), 0);
    let args = if body["args"].is_null() { String::new() } else { format!(" {}", clip(&body["args"].to_string(), 300)) };
    log::info!(target: "loom::cmd", "{name}{args} -> {}", clip(&result.map(Value::to_string).unwrap_or_default(), 200));
}

fn clip(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… ({} bytes)", &s[..end], s.len())
}

impl LoomApp {
    /// Everything that decides what the next click or key does: egui's view
    /// of the input, the app's drag and tool sessions, the active viewport
    /// and the modes and preferences that change input handling.
    pub fn diagnostics(&self, ctx: &egui::Context) -> Value {
        let s = &self.state;
        let frame_nr=ctx.data(|d|d.get_temp::<(u64,f64)>(egui::Id::new("live_frame_counter")).map_or(0,|v|v.0));
        let egui_state = ctx.input(|i| {
            let p = &i.pointer;
            let buttons: Vec<&str> = [(egui::PointerButton::Primary, "primary"), (egui::PointerButton::Secondary, "secondary"), (egui::PointerButton::Middle, "middle"), (egui::PointerButton::Extra1, "extra1"), (egui::PointerButton::Extra2, "extra2")]
                .into_iter()
                .filter(|(b, _)| p.button_down(*b))
                .map(|(_, n)| n)
                .collect();
            json!({
                "modifiers": modifiers_json(i.modifiers),
                "raw_modifiers": modifiers_json(i.raw.modifiers),
                "keys_down": i.keys_down.iter().map(|k| k.name()).collect::<Vec<_>>(),
                "pointer": {
                    "pos": p.latest_pos().map(|q| [q.x, q.y]),
                    "press_origin": p.press_origin().map(|q| [q.x, q.y]),
                    "buttons_down": buttons,
                    "decidedly_dragging": p.is_decidedly_dragging(),
                },
                "window_focused": i.focused,
                "screen_rect": rect_json(i.screen_rect),
                "time": i.time,
                "frame_nr": frame_nr,
                "stable_dt": i.stable_dt,
            })
        });
        let pointer = ctx.input(|i| i.pointer.latest_pos());
        let (focused_widget, popup_open, visible_layers) = ctx.memory(|m| {
            let mut layers: Vec<Value> = m
                .areas()
                .visible_layer_ids()
                .into_iter()
                .filter(|l| l.order != egui::Order::Background)
                .map(|l| json!({ "layer": layer_json(l), "rect": m.area_rect(l.id).map(rect_json) }))
                .collect();
            layers.sort_by_key(|v| v.to_string());
            (m.focused().map(|id| id.short_debug_format()), m.any_popup_open(), layers)
        });
        let interact = ctx.interaction_snapshot(|snap| {
            json!({
                "hovered": snap.hovered.iter().map(|id| id.short_debug_format()).collect::<Vec<_>>(),
                "dragged": snap.dragged.map(|id| id.short_debug_format()),
            })
        });
        let active = s.active();
        json!({
            "egui": {
                "input": egui_state,
                "wants_pointer_input": ctx.wants_pointer_input(),
                "wants_keyboard_input": ctx.wants_keyboard_input(),
                "is_using_pointer": ctx.is_using_pointer(),
                "pointer_over_area": ctx.is_pointer_over_area(),
                "layer_under_pointer": pointer.and_then(|q| ctx.layer_id_at(q)).map(layer_json),
                "focused_widget": focused_widget,
                "popup_open": popup_open,
                "floating_layers": visible_layers,
                "interaction": interact,
            },
            "interaction": {
                "nav_mode": format!("{:?}", s.nav_mode),
                "camera_drag": self.camera_controller.drag_mode(),
                "navigator_mode": format!("{:?}", self.camera_controller.mode()),
                "alt_click": self.alt_click.map(|q| [q.x, q.y]),
                "transform_drag": dbg(&self.transform_drag),
                "manip_drag": dbg(&self.manip_drag),
                "shift_clone": self.shift_clone.is_some(),
                "create_kind": dbg(&self.create_kind),
                "create": dbg(&self.create),
                "camera_create": dbg(&self.camera_create),
                "helper_create": dbg(&self.helper_create),
                "sun_create": dbg(&self.sun_create),
                "line": self.line.is_some(),
                "bone_points": self.bone_points.len(),
                "drag_select_start": self.drag_select_start.map(|q| [q.x, q.y]),
                "region_points": self.region_points.len(),
                "quad_menu": dbg(&self.quad_menu),
                "view_animation": self.view_animation.is_some(),
                "poly_dragging": self.poly_dragging,
                "soft_painting": self.soft_painting,
                "deform_painting": self.deform_painting,
                "uv_drag": dbg(&self.uv_drag),
                "tv_drag": dbg(&self.tv_drag),
                "undo_transaction_open": s.undo.in_transaction(),
            },
            "ui": {
                "dialog": dbg(&s.dialog),
                "pick_mode": dbg(&s.pick_mode),
                "panel_tab": format!("{:?}", self.panel_tab),
                "ui_locked": s.ui_locked,
                "last_error": self.last_error,
                "status_prompt": s.status_prompt,
                "uv_editor_open": s.uv_editor.open,
                "track_view_open": s.track_view.open,
                "panels": panel_rects(ctx).into_iter().map(|(n, r)| json!({ "name": n, "rect": rect_json(r) })).collect::<Vec<_>>(),
            },
            "viewports": {
                "active": s.active_viewport,
                "maximized": s.maximized,
                "layout": format!("{:?}", s.layout),
                "rects": self.viewport_rects.iter().map(|r| rect_json(*r)).collect::<Vec<_>>(),
                "active_view": {
                    "kind": format!("{:?}", active.kind),
                    "shading": format!("{:?}", active.shading),
                    "disabled": active.disabled,
                    "camera": { "target": active.camera.target.to_array(), "distance": active.camera.distance, "yaw": active.camera.yaw, "pitch": active.camera.pitch },
                    "fov_y_radians": active.fov_y_radians,
                },
            },
            "modes": {
                "active_tool": format!("{:?}", s.active_tool),
                "sub_object_level": format!("{:?}", s.selection_mode),
                "selection_locked": s.selection_locked,
                "keyboard_override": s.keyboard_override,
                "expert_mode": s.expert_mode,
                "manipulate": s.manipulate,
                "snaps": s.snaps_on,
                "angle_snap": s.angle_snap_on,
                "percent_snap": s.percent_snap_on,
                "axis_constraint": format!("{:?}", s.axis_constraint),
                "coord_system": format!("{:?}", s.coord_system),
                "center_mode": format!("{:?}", s.center_mode),
                "window_selection": s.window_selection,
                "selection_region": format!("{:?}", s.selection_region),
                "selection_filter": format!("{:?}", s.selection_filter),
                "poly_drag": dbg(&s.poly_drag),
                "selected_nodes": s.selection.len(),
                "selected_sub_objects": s.sub_selection.len(),
                "auto_key": s.auto_key,
                "playing": s.playing,
                "prefs": serde_json::to_value(&s.prefs).unwrap_or(Value::Null),
            },
            "input_events": self.input_log.to_json(),
        })
    }

    /// Clears drag, region, create and menu state that a lost button release
    /// or key-up can leave behind. Drags in progress end where they are (what
    /// they did so far stays as one undo step). Returns what was cleared.
    pub fn reset_interaction(&mut self, ctx: &egui::Context) -> Vec<&'static str> {
        let mut cleared = Vec::new();
        if self.transform_drag.is_some() {
            self.end_transform_drag();
            cleared.push("transform_drag");
        }
        if let Some(session) = self.create.take() {
            session.cancel(self);
            cleared.push("create");
        }
        macro_rules! clear {
            ($($field:ident),*) => {$(
                if self.$field.is_some() {
                    self.$field = None;
                    cleared.push(stringify!($field));
                }
            )*};
        }
        clear!(manip_drag, shift_clone, camera_create, helper_create, sun_create, line, alt_click, drag_select_start, quad_menu, view_animation, uv_drag, tv_drag);
        if !self.region_points.is_empty() {
            self.region_points.clear();
            cleared.push("region_points");
        }
        for (flag, name) in [(&mut self.poly_dragging, "poly_dragging"), (&mut self.soft_painting, "soft_painting"), (&mut self.deform_painting, "deform_painting")] {
            if *flag {
                *flag = false;
                cleared.push(name);
            }
        }
        if self.camera_controller.is_dragging() {
            self.camera_controller.cancel_drag();
            cleared.push("camera_drag");
        }
        // A camera viewport control drag's undo step is ended just below.
        self.camera_nav_open = false;
        self.cube_press = None;
        if self.state.undo.in_transaction() {
            self.state.undo.end();
            cleared.push("undo_transaction");
        }
        let popup = ctx.memory(|m| m.any_popup_open());
        let dragged = ctx.dragged_id().is_some();
        ctx.memory_mut(|m| {
            m.close_popup();
            m.stop_text_input();
        });
        if popup {
            cleared.push("egui_popup");
        }
        if dragged {
            ctx.stop_dragging();
            cleared.push("egui_drag");
        }
        self.sync_after_command();
        ctx.request_repaint();
        cleared
    }

    /// What is at a screen point: the panel, viewport and egui layer, and
    /// the widgets egui sees under the pointer when the pointer is there.
    pub fn ui_hit(&self, ctx: &egui::Context, pos: egui::Pos2) -> Value {
        let pointer_here = ctx.input(|i| i.pointer.latest_pos()).is_some_and(|q| q.distance(pos) < 0.5);
        let hovered: Vec<Value> = if pointer_here {
            ctx.interaction_snapshot(|s| s.hovered.iter().copied().collect::<Vec<_>>())
                .into_iter()
                .map(|id| json!({ "id": id.short_debug_format(), "rect": ctx.read_response(id).map(|r| rect_json(r.rect)) }))
                .collect()
        } else {
            Vec::new()
        };
        json!({
            "pos": [pos.x, pos.y],
            "layer": ctx.layer_id_at(pos).map(layer_json),
            "panel": panel_rects(ctx).into_iter().find(|(_, r)| r.contains(pos)).map(|(n, _)| n),
            "viewport": self.viewport_rects.iter().position(|r| r.contains(pos)),
            "pointer_here": pointer_here,
            "hovered": hovered,
        })
    }

    /// The app's layout: panels, viewports and the floating layers (menus,
    /// popups, windows) currently shown.
    pub fn ui_tree(&self, ctx: &egui::Context) -> Value {
        let floating: Vec<Value> = ctx.memory(|m| {
            m.areas()
                .visible_layer_ids()
                .into_iter()
                .filter(|l| l.order != egui::Order::Background)
                .map(|l| json!({ "layer": layer_json(l), "rect": m.area_rect(l.id).map(rect_json) }))
                .collect()
        });
        json!({
            "screen": rect_json(ctx.screen_rect()),
            "panels": panel_rects(ctx).into_iter().map(|(n, r)| json!({ "name": n, "rect": rect_json(r) })).collect::<Vec<_>>(),
            "viewports": self.viewport_rects.iter().enumerate().map(|(i, r)| json!({ "index": i, "kind": self.state.viewports.get(i).map(|v| format!("{:?}", v.kind)), "rect": rect_json(*r) })).collect::<Vec<_>>(),
            "floating": floating,
            "dialog": dbg(&self.state.dialog),
        })
    }
}

/// One frame of injected input.
#[derive(Debug, Clone, PartialEq)]
pub enum InputStep {
    /// An event, delivered with these modifiers held.
    Event(egui::Event, egui::Modifiers),
    /// Modifiers held from now on (until the next `Hold`).
    Hold(egui::Modifiers),
    /// A frame with no injected event.
    Wait,
}

fn parse_mods(v: &Value) -> Result<egui::Modifiers, String> {
    let mut m = egui::Modifiers::NONE;
    for name in v.as_array().into_iter().flatten() {
        match name.as_str().unwrap_or("") {
            "alt" => m.alt = true,
            "ctrl" => {
                m.ctrl = true;
                m.command = true;
            }
            "shift" => m.shift = true,
            other => return Err(format!("unknown modifier `{other}` (alt, ctrl, shift)")),
        }
    }
    Ok(m)
}

fn parse_pos(v: &Value, what: &str) -> Result<egui::Pos2, String> {
    match v.as_array().map(|a| (a.first().and_then(Value::as_f64), a.get(1).and_then(Value::as_f64))) {
        Some((Some(x), Some(y))) => Ok(egui::pos2(x as f32, y as f32)),
        _ => Err(format!("`{what}` must be [x, y]")),
    }
}

fn parse_button(v: &Value) -> Result<egui::PointerButton, String> {
    match v.as_str().unwrap_or("left") {
        "left" | "primary" => Ok(egui::PointerButton::Primary),
        "right" | "secondary" => Ok(egui::PointerButton::Secondary),
        "middle" => Ok(egui::PointerButton::Middle),
        other => Err(format!("unknown button `{other}` (left, right, middle)")),
    }
}

/// Parses `input {events: [...]}` into one step per frame. Event shapes
/// (`pos`, `from`, `to` are `[x, y]` in window points; `mods` a list of
/// "alt", "ctrl", "shift"):
///
/// - `{"type": "move", "pos"}`
/// - `{"type": "button", "pos", "button": "left|right|middle", "pressed", "mods"}`
/// - `{"type": "click", "pos", "button", "mods"}`: move, press, release
/// - `{"type": "drag", "from", "to", "button", "mods", "steps"}`: move, press, `steps` moves (default 8), release
/// - `{"type": "key", "key": "A|Escape|F3|Space|...", "pressed", "mods"}`: without `pressed`, press then release
/// - `{"type": "text", "text"}`
/// - `{"type": "scroll", "delta": [dx, dy], "mods"}` in lines
/// - `{"type": "modifiers", "mods"}`: hold these from now on (`[]` releases)
/// - `{"type": "leave"}`: the pointer leaves the window
/// - `{"type": "wait", "frames"}`
pub fn parse_input(events: &Value) -> Result<Vec<InputStep>, String> {
    let list = events.as_array().ok_or("`events` must be a list")?;
    let mut steps = Vec::new();
    for e in list {
        let mods = parse_mods(&e["mods"])?;
        let ev = |event| InputStep::Event(event, mods);
        match e["type"].as_str().unwrap_or("") {
            "move" => steps.push(ev(egui::Event::PointerMoved(parse_pos(&e["pos"], "pos")?))),
            "button" => {
                let pos = parse_pos(&e["pos"], "pos")?;
                let pressed = e["pressed"].as_bool().ok_or("`pressed` is required")?;
                steps.push(ev(egui::Event::PointerButton { pos, button: parse_button(&e["button"])?, pressed, modifiers: mods }));
            }
            "click" => {
                let pos = parse_pos(&e["pos"], "pos")?;
                let button = parse_button(&e["button"])?;
                steps.push(ev(egui::Event::PointerMoved(pos)));
                steps.push(ev(egui::Event::PointerButton { pos, button, pressed: true, modifiers: mods }));
                steps.push(ev(egui::Event::PointerButton { pos, button, pressed: false, modifiers: mods }));
            }
            "drag" => {
                let (a, b) = (parse_pos(&e["from"], "from")?, parse_pos(&e["to"], "to")?);
                let button = parse_button(&e["button"])?;
                let n = e["steps"].as_u64().unwrap_or(8).clamp(1, 500) as usize;
                steps.push(ev(egui::Event::PointerMoved(a)));
                steps.push(ev(egui::Event::PointerButton { pos: a, button, pressed: true, modifiers: mods }));
                for i in 1..=n {
                    steps.push(ev(egui::Event::PointerMoved(a + (b - a) * (i as f32 / n as f32))));
                }
                steps.push(ev(egui::Event::PointerButton { pos: b, button, pressed: false, modifiers: mods }));
            }
            "key" => {
                let name = e["key"].as_str().ok_or("`key` is required")?;
                let key = egui::Key::from_name(name).ok_or_else(|| format!("unknown key `{name}`"))?;
                let presses: Vec<bool> = match e["pressed"].as_bool() {
                    Some(p) => vec![p],
                    None => vec![true, false],
                };
                for pressed in presses {
                    steps.push(ev(egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: mods }));
                }
            }
            "text" => steps.push(ev(egui::Event::Text(e["text"].as_str().ok_or("`text` is required")?.to_owned()))),
            "scroll" => {
                let d = parse_pos(&e["delta"], "delta")?;
                steps.push(ev(egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: d.to_vec2(), modifiers: mods }));
            }
            "modifiers" => steps.push(InputStep::Hold(mods)),
            "leave" => steps.push(ev(egui::Event::PointerGone)),
            "wait" => steps.extend(std::iter::repeat_n(InputStep::Wait, e["frames"].as_u64().unwrap_or(1).min(600) as usize)),
            other => return Err(format!("unknown event type `{other}`")),
        }
    }
    Ok(steps)
}

/// Injected input waiting to be fed to egui, one step per frame.
#[derive(Debug, Default)]
pub struct InputQueue {
    steps: VecDeque<InputStep>,
    held: egui::Modifiers,
}

impl InputQueue {
    pub fn push(&mut self, steps: Vec<InputStep>) {
        self.steps.extend(steps);
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Modifiers held by an injected `modifiers` step.
    pub fn held(&self) -> egui::Modifiers {
        self.held
    }

    /// Drops what is still queued and releases held modifiers.
    pub fn clear(&mut self) {
        self.steps.clear();
        self.held = egui::Modifiers::NONE;
    }

    /// Feeds the next step into this frame's raw input. An event frame
    /// carries the event's modifiers; held modifiers apply to every frame.
    pub fn apply(&mut self, raw: &mut egui::RawInput) {
        match self.steps.pop_front() {
            Some(InputStep::Event(e, m)) => {
                raw.modifiers = m | self.held;
                raw.events.push(e);
            }
            Some(InputStep::Hold(m)) => self.held = m,
            Some(InputStep::Wait) | None => {}
        }
        raw.modifiers |= self.held;
    }
}
