//! Movable animation clips backed by native node keys, with one undo per drag.
use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, SelectOp};

#[derive(Default)]
pub(crate) struct State {
    selected: Option<u64>,
    drag: Option<Drag>,
}
struct Drag {
    scene: u64,
    id: u64,
    start: f32,
    pointer_x: f32,
    pixels_per_frame: f32,
    delta: f32,
}

pub(crate) fn preview_import(app: &mut LoomApp, imported: &serde_json::Value) -> Result<(), String> {
    let id = imported["clip"].as_u64().ok_or("Imported animation has no clip.")?;
    let clip = app.state.scene.animation_clips.iter().find(|c| c.id == id).ok_or("Imported clip is missing.")?;
    let frame = clip.start.floor() as i32;
    app.clip_timeline.selected = Some(id);
    app.dispatch(Command::SetFrame {frame})?;
    if app.state.display.hide_bones { app.dispatch(Command::HideBoneObjectsToggle)?; }
    app.dispatch(Command::ZoomExtentsSelected)?;
    if !app.state.playing { app.dispatch(Command::PlayAnimation)?; }
    Ok(())
}

pub(crate) fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if app.state.scene.animation_clips.is_empty() {
        app.clip_timeline.drag = None;
        return;
    }
    let scene = app.state.scene.instance_id();
    if app.clip_timeline.drag.as_ref().is_some_and(|d| d.scene != scene) || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.clip_timeline.drag = None;
    }
    let clips = app.state.scene.animation_clips.clone();
    let first = app.state.frame_range.0 as f32;
    let span = (app.state.frame_range.1 as f32 - first).max(1.0);
    let height = 24.0 + clips.len().min(4) as f32 * 28.0;
    let mut action = None;
    let mut selection = None;
    egui::TopBottomPanel::bottom("animation_clips").exact_height(height).show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.strong("Animation Clips");
            ui.small("Drag to move · Esc to cancel");
        });
        egui::ScrollArea::vertical().show(ui, |ui| {
            for clip in &clips {
                let (row, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::hover());
                let axis = app.track_bar.key_rect.unwrap_or(row);
                let pixels_per_frame = axis.width() / span;
                let delta = app.clip_timeline.drag.as_ref().filter(|d| d.scene == scene && d.id == clip.id).map(|d| d.delta).unwrap_or(0.0);
                let a = axis.left() + (clip.start + delta - first) * pixels_per_frame;
                let b = axis.left() + (clip.end + delta - first) * pixels_per_frame;
                let rect = egui::Rect::from_min_max(egui::pos2(a, row.top()+1.0), egui::pos2(b.max(a+8.0), row.bottom()-1.0));
                let resp = ui.interact(rect, egui::Id::new(("animation_clip", scene, clip.id)), egui::Sense::click_and_drag());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &clip.name));
                let selected = app.clip_timeline.selected == Some(clip.id);
                let color = if selected || resp.dragged() { egui::Color32::from_rgb(51,112,147) } else { egui::Color32::from_rgb(55,80,97) };
                ui.painter().rect_filled(rect,3.0,color);
                let text = format!("{}  ·  {:.0}–{:.0}",clip.name,clip.start+delta,clip.end+delta);
                ui.painter().with_clip_rect(rect.intersect(row)).text(rect.left_center()+egui::vec2(6.0,0.0),egui::Align2::LEFT_CENTER,text,egui::FontId::proportional(12.0),egui::Color32::WHITE);
                let current = axis.left() + (app.state.current_frame as f32-first)*pixels_per_frame;
                ui.painter().line_segment([egui::pos2(current,row.top()),egui::pos2(current,row.bottom())],egui::Stroke::new(1.0,egui::Color32::from_rgb(235,184,70)));
                if resp.clicked() {
                    app.clip_timeline.selected = Some(clip.id);
                    selection = Some(clip.nodes.iter().map(|n| n.0).collect());
                }
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    if let Some(origin) = ctx.input(|i| i.pointer.press_origin()) {
                        app.clip_timeline.selected = Some(clip.id);
                        app.clip_timeline.drag = Some(Drag {scene,id:clip.id,start:clip.start,pointer_x:origin.x,pixels_per_frame,delta:0.0});
                    }
                }
                if let Some(drag) = app.clip_timeline.drag.as_mut().filter(|d| d.scene==scene && d.id==clip.id) {
                    if let Some(pointer) = resp.interact_pointer_pos() {
                        drag.delta = ((pointer.x-drag.pointer_x)/drag.pixels_per_frame).round();
                    }
                    if resp.drag_stopped_by(egui::PointerButton::Primary) {
                        action=Some(Command::MoveAnimationClip {id:clip.id,start:drag.start+drag.delta});
                    }
                }
                resp.on_hover_text(format!("{}\nFrames {:.2}–{:.2}\nDrag the block to move this animation.",clip.name,clip.start+delta,clip.end+delta));
                ui.add_space(2.0);
            }
        });
    });
    if let Some(ids) = selection {
        if let Err(e) = app.dispatch(Command::SelectNodes {ids,op:SelectOp::Replace}) { app.state.status_prompt=e; }
    }
    if let Some(command)=action {
        app.clip_timeline.drag=None;
        if let Err(e)=app.dispatch(command) {app.state.status_prompt=e;}
    }
}
