//! The Edit UVWs window (Unwrap UVW > Edit...): menus,
//! toolbar, the texture-coordinate canvas and the bottom bar. Every action
//! is a command; the window only draws and turns mouse input into them.

use crate::LoomApp;
use eframe::egui;
use loom_commands::{Command, FreeformHit, SelectOp, TvLevel, TvMode, UvTexture};
use loom_mesh::unwrap as uw;
use loom_mesh::{FaceId, Mesh};
use loom_scene::UnwrapUvw;
use std::collections::BTreeSet;

/// A drag in progress on the canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UvDrag {
    /// Region select from a screen point.
    Region { start: egui::Pos2, add: bool, subtract: bool },
    /// Transforming the selection; `last` is the previous pointer position.
    Transform { start: egui::Pos2, last: egui::Pos2, pivot: egui::Pos2 },
    Pan { last: egui::Pos2 },
    /// Paint Select brush stroke.
    Paint,
    /// Sketch: drawing the stroke (`grouped`: Interactive Mode opened an undo group), or drag-picking vertices first.
    Sketch { grouped: bool },
    SketchPick,
    /// Freeform gizmo handle drag.
    Freeform { hit: FreeformHit, last: egui::Pos2 },
}

/// Canvas mapping between UV and screen.
#[derive(Clone, Copy)]
struct View {
    rect: egui::Rect,
    scale: f32,
    origin: egui::Pos2,
    plane: u32,
}

impl UvDrag {
    /// Whether the drag holds an open undo group (closed on release or when cut short).
    fn holds_undo(&self) -> bool {
        matches!(self, UvDrag::Transform { .. } | UvDrag::Paint | UvDrag::Freeform { .. } | UvDrag::Sketch { grouped: true })
    }
}

impl View {
    fn new(rect: egui::Rect, zoom: f32, pan: [f32; 2], plane: u32) -> Self {
        let scale = rect.height().min(rect.width()) * 0.8 * zoom;
        let origin = rect.center() - egui::vec2(pan[0] * scale, -pan[1] * scale) - egui::vec2(0.5 * scale, -0.5 * scale);
        Self { rect, scale, origin, plane }
    }
    /// The two shown coordinates of a tv.
    fn pair(self, v: glam::Vec3) -> egui::Vec2 {
        match self.plane {
            1 => egui::vec2(v.y, v.z),
            2 => egui::vec2(v.z, v.x),
            _ => egui::vec2(v.x, v.y),
        }
    }
    fn to_screen(self, uv: egui::Vec2) -> egui::Pos2 {
        self.origin + egui::vec2(uv.x * self.scale, -uv.y * self.scale)
    }
    fn to_uv(self, p: egui::Pos2) -> egui::Vec2 {
        let d = p - self.origin;
        egui::vec2(d.x / self.scale, -d.y / self.scale)
    }
}

/// The entry's working mesh (input + edits) and data.
fn working(app: &LoomApp) -> Option<(Mesh, u32, UnwrapUvw)> {
    let (id, index) = app.state.unwrap_entry()?;
    let node = app.state.scene.node(id)?;
    let loom_scene::Modifier::UnwrapUVW(u) = &node.modifiers[index].modifier else { return None };
    let input = match index.checked_sub(1) {
        Some(prev) => app.state.scene.evaluated(id, Some(prev)).value.mesh.clone(),
        None => node.object.display_output(),
    };
    let mesh = u.working_mesh(&input);
    Some((mesh, u.channel.get(), (**u).clone()))
}

pub fn show(ctx: &egui::Context, app: &mut LoomApp) {
    if !app.state.uv_editor.open {
        return;
    }
    let Some((mesh, ch, data)) = working(app) else {
        app.state.uv_editor.open = false;
        return;
    };
    let mut open = true;
    egui::Window::new("Edit UVWs").open(&mut open).default_size([760.0, 640.0]).resizable(true).show(ctx, |ui| {
        menus(ui, app);
        toolbar(ui, app);
        let bottom_h = 64.0;
        let avail = ui.available_size();
        let (response, painter) = ui.allocate_painter(egui::vec2(avail.x, (avail.y - bottom_h).max(120.0)), egui::Sense::click_and_drag());
        canvas(ui, app, &response, &painter, &mesh, ch, &data);
        bottom_bar(ui, app, &mesh, ch, &data);
    });
    sketch_dialog(ctx, app);
    if !open {
        if app.uv_drag.take().is_some_and(|d| d.holds_undo()) {
            app.state.undo.end();
        }
        let _ = app.dispatch(Command::CloseUvws);
    }
}

fn item(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, cmd: Command) {
    if ui.button(label).clicked() {
        let _ = app.dispatch(cmd);
        ui.close_menu();
    }
}

fn toggle_item(ui: &mut egui::Ui, app: &mut LoomApp, label: &str, on: bool, cmd: Command) {
    if ui.checkbox(&mut on.clone(), label).clicked() {
        let _ = app.dispatch(cmd);
        ui.close_menu();
    }
}

fn menus(ui: &mut egui::Ui, app: &mut LoomApp) {
    let e = app.state.uv_editor.clone();
    egui::menu::bar(ui, |ui| {
        ui.menu_button("File", |ui| {
            item(ui, app, "Load UVW...", Command::LoadUvw { path: None });
            item(ui, app, "Save UVW...", Command::SaveUvw { path: None });
            ui.separator();
            item(ui, app, "Reset UVWs", Command::ResetUvws);
            ui.separator();
            item(ui, app, "Render UVW Template...", Command::OpenRenderTemplateDialog);
        });
        ui.menu_button("Edit", |ui| {
            item(ui, app, "Copy", Command::CopyTv);
            item(ui, app, "Paste", Command::PasteTv);
            item(ui, app, "Paste Instance", Command::PasteInstance);
            item(ui, app, "Paste Symmetrical", Command::PasteSymmetrical);
            ui.separator();
            item(ui, app, "Select All", Command::SelectAllTv);
            item(ui, app, "Select None", Command::SelectNoneTv);
            item(ui, app, "Select Invert", Command::SelectInvertTv);
            ui.separator();
            item(ui, app, "Unwrap Options...", Command::UnwrapOptions);
        });
        ui.menu_button("Select", |ui| {
            item(ui, app, "Expand Selection", Command::TextureVertexExpandSelection);
            item(ui, app, "Contract Selection", Command::TextureVertexContractSelection);
            toggle_item(ui, app, "Select Element", e.select_element, Command::TvElementMode);
            ui.separator();
            item(ui, app, "Select By Material ID", Command::SelectByMaterialIdUnwrap);
            item(ui, app, "Select By Smoothing Group", Command::SelectBySmoothingGroupUnwrap);
            item(ui, app, "Loop", Command::TvLoopSelect);
            item(ui, app, "Ring", Command::TvRingSelect);
            item(ui, app, "Open Edges", Command::OpenEdgeSelect);
            item(ui, app, "UV Edges (seams)", Command::UvEdgeSelect);
            ui.separator();
            item(ui, app, "Select Overlapped Faces", Command::SelectOverlappedPolygons);
            item(ui, app, "Select Inverted Faces", Command::SelectInvertedPolygons);
            item(ui, app, "Select Pinned", Command::SelectPinnedVerts);
            ui.separator();
            item(ui, app, "Edge to Vertex", Command::EdgeToVertexSelect);
            item(ui, app, "Edge to Polygon", Command::EdgeToPolygonSelect);
            item(ui, app, "Polygon to Edge", Command::PolygonToEdgeSelect);
            item(ui, app, "Polygon to Vertex", Command::PolygonToVertexSelect);
            ui.separator();
            item(ui, app, "Get Selection From Viewport", Command::GetSelectionFromPolygons);
            item(ui, app, "Get Selection From Stack", Command::GetPolygonSelectionFromStack);
            toggle_item(ui, app, "Sync Selection", e.sync_selection, Command::SyncSelectionMode);
            toggle_item(ui, app, "Paint Select Mode", e.paint_select, Command::PaintSelectMode);
        });
        ui.menu_button("Tools", |ui| {
            item(ui, app, "Flip Horizontal", Command::FlipHorizontal);
            item(ui, app, "Flip Vertical", Command::FlipVertical);
            item(ui, app, "Mirror Horizontal", Command::MirrorHorizontal);
            item(ui, app, "Mirror Vertical", Command::MirrorVertical);
            ui.separator();
            item(ui, app, "Weld Selected", Command::TextureVertexWeldSelected);
            item(ui, app, "Target Weld", Command::TextureVertexTargetWeld);
            item(ui, app, "Break", Command::BreakSelectedVertices);
            item(ui, app, "Detach Edge Verts", Command::DetachEdgeVerts);
            item(ui, app, "Stitch Selected...", Command::StitchDialog);
            ui.separator();
            item(ui, app, "Pack UVs...", Command::PackDialog);
            item(ui, app, "Relax...", Command::RelaxDialog);
            item(ui, app, "Rescale Elements", Command::Rescale);
            ui.separator();
            item(ui, app, "Align Horizontal", Command::AlignSelectionHorizontal);
            item(ui, app, "Align Vertical", Command::AlignSelectionVertical);
            item(ui, app, "Linear Align", Command::LinearAlign);
            item(ui, app, "Space Horizontally", Command::SpaceHorizontally);
            item(ui, app, "Space Vertically", Command::SpaceVertically);
            item(ui, app, "Straighten Edges", Command::StraightenEdges);
            ui.separator();
            item(ui, app, "Sketch Vertices", Command::Sketch);
            item(ui, app, "Sketch Vertices Options...", Command::SketchDialog);
            ui.separator();
            item(ui, app, "Pin Selected", Command::PinSelected);
            item(ui, app, "Unpin Selected", Command::UnpinSelected);
            ui.separator();
            item(ui, app, "Render UVW Template...", Command::OpenRenderTemplateDialog);
        });
        ui.menu_button("Mapping", |ui| {
            item(ui, app, "Flatten Mapping...", Command::FlattenMapDialog);
            item(ui, app, "Normal Mapping...", Command::NormalMapDialog);
            item(ui, app, "Unfold Mapping...", Command::UnfoldMapDialog);
            ui.separator();
            item(ui, app, "Quick Planar Map", Command::QuickPlanarMap);
        });
        ui.menu_button("Options", |ui| {
            item(ui, app, "Load Defaults", Command::LoadDefaults);
            item(ui, app, "Save Current Settings As Default", Command::SaveCurrentSettingsAsDefault);
            ui.separator();
            toggle_item(ui, app, "Always Bring Up The Edit Window", e.always_bring_up, Command::AlwaysBringUpTheEditWindow);
            toggle_item(ui, app, "Constant Update", e.constant_update, Command::SetUnwrapOption { name: "Constant Update".into(), value: if e.constant_update { 0.0 } else { 1.0 } });
            ui.separator();
            toggle_item(ui, app, "Grid Snap", e.grid_snap, Command::GridSnap);
            toggle_item(ui, app, "Vertex Snap", e.vertex_snap, Command::VertexSnap);
            toggle_item(ui, app, "Edge Snap", e.edge_snap, Command::EdgeSnap);
            toggle_item(ui, app, "Freeform Snap", e.freeform_snap, Command::FreeformSnap);
            toggle_item(ui, app, "Allow Selections Inside Gizmo", e.allow_select_inside_gizmo, Command::AllowSelectionsInsideTranformGizmo);
            ui.separator();
            item(ui, app, "Preferences...", Command::UnwrapOptions);
        });
        ui.menu_button("Display", |ui| {
            toggle_item(ui, app, "Show Vertex Connections", e.show_vertex_connections, Command::ShowVertexConnections);
            toggle_item(ui, app, "Show Hidden Edges", e.show_hidden_edges, Command::ShowHiddenEdges);
            toggle_item(ui, app, "Show Seams", e.display_seams_in_editor, Command::DisplaySeams);
            toggle_item(ui, app, "Show Shared Sub-objects", e.show_shared, Command::ShowSharedSubObjects);
            toggle_item(ui, app, "Show Grid", e.grid_visible, Command::GridVisible);
            toggle_item(ui, app, "Show Map", e.show_map, Command::ShowMap);
            toggle_item(ui, app, "Show Edge Distortion", e.show_edge_distortion, Command::ShowEdgeDistortion);
            toggle_item(ui, app, "Show Local Distortion", e.show_local_distortion, Command::ShowLocalDistortion);
            toggle_item(ui, app, "Show Subobject Counter", e.show_counter, Command::ShowSubobjectCounter);
        });
    });
}

fn toolbar(ui: &mut egui::Ui, app: &mut LoomApp) {
    let e = app.state.uv_editor.clone();
    ui.horizontal_wrapped(|ui| {
        let mode = |ui: &mut egui::Ui, app: &mut LoomApp, label: &str, m: TvMode, cmd: Command| {
            if ui.selectable_label(e.mode == m, label).clicked() {
                let _ = app.dispatch(cmd);
            }
        };
        mode(ui, app, "Move", TvMode::Move, Command::TextureVertexMoveMode);
        mode(ui, app, "Move H", TvMode::MoveHorizontal, Command::MoveHorizontal);
        mode(ui, app, "Move V", TvMode::MoveVertical, Command::MoveVertical);
        mode(ui, app, "Rotate", TvMode::Rotate, Command::TextureVertexRotateMode);
        mode(ui, app, "Scale", TvMode::Scale, Command::TextureVertexScaleMode);
        mode(ui, app, "Scale H", TvMode::ScaleHorizontal, Command::ScaleHorizontal);
        mode(ui, app, "Scale V", TvMode::ScaleVertical, Command::ScaleVertical);
        mode(ui, app, "Freeform", TvMode::Freeform, Command::FreeformMode);
        mode(ui, app, "Pan", TvMode::Pan, Command::Pan);
        ui.separator();
        ui.menu_button("Mirror", |ui| {
            item(ui, app, "Mirror Horizontal", Command::MirrorHorizontal);
            item(ui, app, "Mirror Vertical", Command::MirrorVertical);
            item(ui, app, "Flip Horizontal", Command::FlipHorizontal);
            item(ui, app, "Flip Vertical", Command::FlipVertical);
        });
        ui.separator();
        if ui.selectable_label(e.show_map, "Show Map").clicked() {
            let _ = app.dispatch(Command::ShowMap);
        }
        let planes = ["UV", "VW", "WU"];
        egui::ComboBox::from_id_salt("uv_plane").selected_text(planes[e.plane.min(2) as usize]).width(50.0).show_ui(ui, |ui| {
            for (i, p) in planes.iter().enumerate() {
                if ui.selectable_label(e.plane == i as u32, *p).clicked() {
                    let _ = app.dispatch(Command::UvEditorPlane { plane: i as u32 });
                }
            }
        });
        let map_label = match &e.texture {
            UvTexture::Checker => "CheckerPattern".to_owned(),
            UvTexture::None => "(none)".to_owned(),
            UvTexture::Bitmap(p) => std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.clone()),
        };
        egui::ComboBox::from_id_salt("uv_map").selected_text(map_label).width(140.0).show_ui(ui, |ui| {
            if ui.selectable_label(e.texture == UvTexture::Checker, "CheckerPattern").clicked() {
                let _ = app.dispatch(Command::CheckerPattern);
            }
            if ui.selectable_label(false, "Pick Texture...").clicked() {
                app.state.dialog = Some(loom_commands::Dialog::PickUvTexture);
            }
            if ui.selectable_label(e.texture == UvTexture::None, "Remove Texture").clicked() {
                let _ = app.dispatch(Command::RemoveTexture);
            }
        });
        if ui.button("Update Map").clicked() {
            let _ = app.dispatch(Command::UpdateMap);
        }
    });
}

/// Nearest tv (within `px` pixels) in screen space.
fn nearest_tv(view: &View, mesh: &Mesh, ch: u32, data: &UnwrapUvw, p: egui::Pos2, px: f32) -> Option<u32> {
    let m = mesh.map(ch)?;
    let mut best: Option<(u32, f32)> = None;
    for (i, v) in m.verts.iter().enumerate() {
        let t = i as u32;
        if data.hidden_tvs.contains(&t) {
            continue;
        }
        let d = view.to_screen(view.pair(*v)).distance(p);
        if d <= px && best.is_none_or(|(_, b)| d < b) {
            best = Some((t, d));
        }
    }
    best.map(|(t, _)| t)
}

fn nearest_edge(view: &View, mesh: &Mesh, ch: u32, p: egui::Pos2, px: f32) -> Option<(u32, u32)> {
    let m = mesh.map(ch)?;
    let mut best: Option<((u32, u32), f32)> = None;
    for (a, b) in uw::tv_edges(mesh, ch) {
        let (pa, pb) = (view.to_screen(view.pair(m.verts[a as usize])), view.to_screen(view.pair(m.verts[b as usize])));
        let ab = pb - pa;
        let t = ((p - pa).dot(ab) / ab.length_sq().max(1e-9)).clamp(0.0, 1.0);
        let d = (pa + ab * t).distance(p);
        if d <= px && best.is_none_or(|(_, b)| d < b) {
            best = Some(((a, b), d));
        }
    }
    best.map(|(e, _)| e)
}

fn face_at(view: &View, mesh: &Mesh, ch: u32, p: egui::Pos2) -> Option<FaceId> {
    let m = mesh.map(ch)?;
    let uv = view.to_uv(p);
    (0..mesh.face_count() as u32).map(FaceId).find(|f| {
        let pts: Vec<egui::Vec2> = uw::face_tvs(mesh, ch, *f).iter().map(|t| view.pair(m.verts[*t as usize])).collect();
        pts.len() >= 3 && point_in_polygon(uv, &pts)
    })
}

fn point_in_polygon(p: egui::Vec2, poly: &[egui::Vec2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[allow(clippy::too_many_arguments)]
fn canvas(ui: &mut egui::Ui, app: &mut LoomApp, response: &egui::Response, painter: &egui::Painter, mesh: &Mesh, ch: u32, data: &UnwrapUvw) {
    let e = app.state.uv_editor.clone();
    let rect = response.rect;
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(70, 70, 70));
    let view = View::new(rect, e.zoom, e.pan, e.plane);
    // Map: checker tiles or the bitmap behind the unit square (and its tiles).
    if e.show_map {
        let tiles = if e.tile_bitmap { e.tiles as i32 } else { 1 };
        let range = if tiles > 1 { -(tiles / 2)..=(tiles / 2) } else { 0..=0 };
        for ty in range.clone() {
            for tx in range.clone() {
                let lo = view.to_screen(egui::vec2(tx as f32, ty as f32 + 1.0));
                let hi = view.to_screen(egui::vec2(tx as f32 + 1.0, ty as f32));
                let tile = egui::Rect::from_min_max(lo, hi).intersect(rect);
                if tile.is_negative() {
                    continue;
                }
                let dim = if (tx != 0 || ty != 0) && e.brightness_center_tile { e.tile_brightness } else { 1.0 };
                let alpha: u8 = if (tx != 0 || ty != 0) && e.blend_tiles { 110 } else { 255 };
                match &e.texture {
                    UvTexture::Checker => {
                        let cells = (8.0 * e.checker_tiling).max(1.0) as i32;
                        let full = egui::Rect::from_min_max(lo, hi);
                        let cw = full.width() / cells as f32;
                        let chh = full.height() / cells as f32;
                        for cy in 0..cells {
                            for cx in 0..cells {
                                let c = egui::Rect::from_min_size(full.min + egui::vec2(cx as f32 * cw, cy as f32 * chh), egui::vec2(cw, chh)).intersect(rect);
                                if c.is_negative() {
                                    continue;
                                }
                                let v = if (cx + cy) % 2 == 0 { 200.0 } else { 40.0 } * dim;
                                painter.rect_filled(c, 0.0, egui::Color32::from_rgba_unmultiplied(v as u8, v as u8, v as u8, alpha));
                            }
                        }
                    }
                    UvTexture::Bitmap(path) => {
                        if let Some(tex) = app.background_texture(ui.ctx(), path, e.map_version) {
                            let full = egui::Rect::from_min_max(lo, hi);
                            let tint = egui::Color32::from_rgba_unmultiplied((255.0 * dim) as u8, (255.0 * dim) as u8, (255.0 * dim) as u8, alpha);
                            painter.image(tex.id(), full, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), tint);
                        }
                    }
                    UvTexture::None => {}
                }
            }
        }
    }
    // Grid.
    if e.grid_visible {
        let step = e.grid_size.max(0.001);
        let lo = view.to_uv(rect.min);
        let hi = view.to_uv(rect.max);
        let (u0, u1) = (lo.x.min(hi.x), lo.x.max(hi.x));
        let (v0, v1) = (lo.y.min(hi.y), lo.y.max(hi.y));
        if (u1 - u0) / step < 400.0 {
            let faint = egui::Stroke::new(1.0_f32, egui::Color32::from_gray(95));
            let mut u = (u0 / step).floor() * step;
            while u <= u1 {
                let p = view.to_screen(egui::vec2(u, 0.0)).x;
                painter.line_segment([egui::pos2(p, rect.min.y), egui::pos2(p, rect.max.y)], faint);
                u += step;
            }
            let mut v = (v0 / step).floor() * step;
            while v <= v1 {
                let p = view.to_screen(egui::vec2(0.0, v)).y;
                painter.line_segment([egui::pos2(rect.min.x, p), egui::pos2(rect.max.x, p)], faint);
                v += step;
            }
        }
    }
    // The unit square.
    let sq = egui::Rect::from_min_max(view.to_screen(egui::vec2(0.0, 1.0)), view.to_screen(egui::vec2(1.0, 0.0)));
    painter.rect_stroke(sq, 0.0, egui::Stroke::new(1.5_f32, egui::Color32::from_gray(160)), egui::StrokeKind::Middle);
    let Some(m) = mesh.map(ch) else { return };
    // Faces (Face level fills) and edges.
    let tvf = uw::tv_faces(mesh, ch);
    let sel_edge = &data.tv_edge_selection;
    let is_sel_tv = |t: u32| data.tv_selection.contains(&t);
    // Filter Selected Polygons: only the faces selected in the viewport (Face level) show.
    let shown_faces: Option<BTreeSet<u32>> = e.filter_selected_polygons.then(|| data.face_selection.clone());
    let shown_tvs: Option<BTreeSet<u32>> = shown_faces.as_ref().map(|fs| fs.iter().flat_map(|f| tvf.get(*f as usize).cloned().unwrap_or_default()).collect());
    let face_shown = |f: usize| shown_faces.as_ref().is_none_or(|s| s.contains(&(f as u32)));
    let tv_shown = |t: u32| shown_tvs.as_ref().is_none_or(|s| s.contains(&t));
    // Show Local Distortion: faces tinted by their texture-area to surface-area ratio
    // against the mesh average (green = matches, red = stretched, blue = squashed).
    if e.show_local_distortion {
        let area2 = |pts: &[glam::Vec2]| -> f32 {
            let n = pts.len();
            (0..n).map(|i| { let (a, b) = (pts[i], pts[(i + 1) % n]); a.x * b.y - b.x * a.y }).sum::<f32>().abs() * 0.5
        };
        let mut ratios: Vec<(usize, f32)> = Vec::new();
        let (mut uv_total, mut w_total) = (0.0f32, 0.0f32);
        for (f, t) in tvf.iter().enumerate() {
            if t.len() < 3 || !face_shown(f) {
                continue;
            }
            let uvp: Vec<glam::Vec2> = t.iter().map(|x| view.pair(m.verts[*x as usize])).map(|v| glam::Vec2::new(v.x, v.y)).collect();
            let wa = mesh.face_positions(loom_mesh::FaceId(f as u32)).map(|pts| {
                let n = pts.len();
                (0..n).map(|i| pts[i].cross(pts[(i + 1) % n])).sum::<glam::Vec3>().length() * 0.5
            }).unwrap_or(0.0);
            let ua = area2(&uvp);
            uv_total += ua;
            w_total += wa;
            ratios.push((f, if wa > 1e-12 { ua / wa } else { 0.0 }));
        }
        let mean = if w_total > 1e-12 { uv_total / w_total } else { 1.0 };
        for (f, r) in ratios {
            let x = if mean > 1e-12 { (r / mean).ln().clamp(-1.0, 1.0) } else { 0.0 };
            let (rc, gc, bc) = if x >= 0.0 { (60.0 + 195.0 * x, 200.0 * (1.0 - x), 60.0) } else { (60.0, 200.0 * (1.0 + x), 60.0 - 195.0 * x) };
            let pts: Vec<egui::Pos2> = tvf[f].iter().map(|x| view.to_screen(view.pair(m.verts[*x as usize]))).collect();
            painter.add(egui::Shape::convex_polygon(pts, egui::Color32::from_rgba_unmultiplied(rc as u8, gc as u8, bc as u8, 90), egui::Stroke::NONE));
        }
    }
    if e.level == TvLevel::Face {
        for (f, t) in tvf.iter().enumerate() {
            let selected = data.face_selection.contains(&(f as u32)) || (!t.is_empty() && t.iter().all(|x| is_sel_tv(*x)));
            if selected && face_shown(f) {
                let pts: Vec<egui::Pos2> = t.iter().map(|x| view.to_screen(view.pair(m.verts[*x as usize]))).collect();
                painter.add(egui::Shape::convex_polygon(pts, egui::Color32::from_rgba_unmultiplied(255, 60, 60, 70), egui::Stroke::NONE));
            }
        }
    }
    let seams: BTreeSet<(u32, u32)> = if e.display_seams_in_editor {
        let mut count = std::collections::HashMap::new();
        for t in &tvf {
            for i in 0..t.len() {
                let (a, b) = (t[i], t[(i + 1) % t.len()]);
                *count.entry((a.min(b), a.max(b))).or_insert(0u32) += 1;
            }
        }
        count.into_iter().filter(|(_, c)| *c == 1).map(|(k, _)| k).collect()
    } else {
        BTreeSet::new()
    };
    // Show Hidden Edges: the polygons' internal triangulation, dotted.
    if e.show_hidden_edges {
        let dots = egui::Color32::from_gray(150);
        for (f, t) in tvf.iter().enumerate() {
            if t.len() < 4 || !face_shown(f) {
                continue;
            }
            let p0 = view.to_screen(view.pair(m.verts[t[0] as usize]));
            for x in &t[2..t.len() - 1] {
                let p1 = view.to_screen(view.pair(m.verts[*x as usize]));
                painter.add(egui::Shape::dashed_line(&[p0, p1], egui::Stroke::new(1.0_f32, dots), 3.0, 3.0));
            }
        }
    }
    let distortion = e.show_edge_distortion;
    for (a, b) in uw::tv_edges(mesh, ch) {
        if data.hidden_tvs.contains(&a) || data.hidden_tvs.contains(&b) || !tv_shown(a) || !tv_shown(b) {
            continue;
        }
        let (pa, pb) = (view.to_screen(view.pair(m.verts[a as usize])), view.to_screen(view.pair(m.verts[b as usize])));
        let selected = sel_edge.contains(&(a, b)) || (e.level == TvLevel::Vertex && is_sel_tv(a) && is_sel_tv(b));
        let colour = if selected {
            egui::Color32::from_rgb(255, 60, 60)
        } else if seams.contains(&(a, b)) {
            egui::Color32::from_rgb(90, 140, 255)
        } else if distortion {
            egui::Color32::from_rgb(120, 220, 120)
        } else {
            egui::Color32::from_gray(225)
        };
        painter.line_segment([pa, pb], egui::Stroke::new(if selected { 2.0_f32 } else { 1.0_f32 }, colour));
    }
    // Shared sub-objects: tvs on the same mesh vertex as a selected tv; vertex
    // connections: every such split vertex is numbered on each of its tvs.
    let mut tv_vertex: Vec<Option<u32>> = vec![None; m.verts.len()];
    for (he, c) in m.corners.iter().enumerate() {
        if let (Some(slot), Some(h)) = (tv_vertex.get_mut(*c as usize), mesh.half_edges().get(he)) {
            *slot = Some(h.origin.0);
        }
    }
    let mut by_vertex: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    for (t, v) in tv_vertex.iter().enumerate() {
        if let Some(v) = v {
            by_vertex.entry(*v).or_default().push(t as u32);
        }
    }
    let shared: BTreeSet<u32> = if e.show_shared {
        data.tv_selection.iter().filter_map(|t| tv_vertex.get(*t as usize).copied().flatten()).flat_map(|v| by_vertex.get(&v).cloned().unwrap_or_default()).filter(|t| !is_sel_tv(*t)).collect()
    } else {
        BTreeSet::new()
    };
    // Texture vertices.
    for (i, v) in m.verts.iter().enumerate() {
        let t = i as u32;
        if data.hidden_tvs.contains(&t) || !tv_shown(t) {
            continue;
        }
        let p = view.to_screen(view.pair(*v));
        if !rect.contains(p) {
            continue;
        }
        if e.show_vertex_connections {
            if let Some(vx) = tv_vertex[i] {
                if by_vertex.get(&vx).is_some_and(|l| l.len() > 1) {
                    painter.text(p + egui::vec2(4.0, -4.0), egui::Align2::LEFT_BOTTOM, vx.to_string(), egui::FontId::proportional(10.0), egui::Color32::from_rgb(255, 220, 120));
                }
            }
        }
        let colour = if is_sel_tv(t) {
            egui::Color32::from_rgb(255, 50, 50)
        } else if shared.contains(&t) {
            egui::Color32::from_rgb(80, 220, 220)
        } else if data.pins.contains(&t) {
            egui::Color32::from_rgb(200, 90, 255)
        } else if data.frozen_tvs.contains(&t) {
            egui::Color32::from_gray(120)
        } else {
            egui::Color32::from_rgb(90, 150, 255)
        };
        painter.rect_filled(egui::Rect::from_center_size(p, egui::vec2(5.0, 5.0)), 0.0, colour);
    }
    // Pivot.
    if !data.tv_selection.is_empty() {
        let c = uw::centre(mesh, ch, &data.tv_selection) + glam::Vec2::from(e.pivot);
        let p = view.to_screen(egui::vec2(c.x, c.y));
        let s = egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(255, 220, 60));
        painter.line_segment([p - egui::vec2(8.0, 0.0), p + egui::vec2(8.0, 0.0)], s);
        painter.line_segment([p - egui::vec2(0.0, 8.0), p + egui::vec2(0.0, 8.0)], s);
    }
    if e.show_counter {
        let text = format!("{} tvs, {} selected", m.verts.len(), data.tv_selection.len());
        painter.text(rect.left_bottom() + egui::vec2(6.0, -4.0), egui::Align2::LEFT_BOTTOM, text, egui::FontId::proportional(12.0), egui::Color32::from_gray(200));
    }
    interact(ui, app, response, &view, mesh, ch, data);
}

fn interact(ui: &mut egui::Ui, app: &mut LoomApp, response: &egui::Response, view: &View, mesh: &Mesh, ch: u32, data: &UnwrapUvw) {
    let e = app.state.uv_editor.clone();
    let pointer = ui.input(|i| i.pointer.clone());
    let mods = ui.input(|i| i.modifiers);
    // Wheel zoom around the cursor, middle drag pan.
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.0 {
            let factor = (1.0 + scroll * 0.002).clamp(0.5, 2.0);
            let before = pointer.hover_pos().map(|p| view.to_uv(p));
            let _ = app.dispatch(Command::SetUnwrapOption { name: "Zoom".into(), value: (e.zoom * factor) as f64 });
            if let Some(b) = before {
                let nv = View::new(view.rect, e.zoom * factor, e.pan, e.plane);
                let after = pointer.hover_pos().map(|p| nv.to_uv(p)).unwrap_or(b);
                let _ = app.dispatch(Command::SetUnwrapOption { name: "Pan U".into(), value: (e.pan[0] + (b.x - after.x)) as f64 });
                let _ = app.dispatch(Command::SetUnwrapOption { name: "Pan V".into(), value: (e.pan[1] + (b.y - after.y)) as f64 });
            }
        }
    }
    // A drag whose mode was switched off mid-way (a hotkey, a menu) ends here.
    let cut = match app.uv_drag {
        Some(UvDrag::Paint) => !(e.paint_select && e.plane == 0),
        Some(UvDrag::Sketch { .. } | UvDrag::SketchPick) => !(e.sketch && e.plane == 0),
        Some(UvDrag::Freeform { .. }) => e.mode != TvMode::Freeform || e.plane != 0 || data.tv_selection.is_empty(),
        _ => false,
    };
    if cut {
        if app.uv_drag.take().is_some_and(|d| d.holds_undo()) {
            app.state.undo.end();
        }
        app.uv_sketch.clear();
    }
    let Some(p) = pointer.interact_pos() else { return };
    let pan_drag = pointer.button_down(egui::PointerButton::Middle) || (e.mode == TvMode::Pan && pointer.primary_down());
    if pan_drag && response.hovered() {
        if let Some(UvDrag::Pan { last }) = app.uv_drag {
            let d = p - last;
            let _ = app.dispatch(Command::SetUnwrapOption { name: "Pan U".into(), value: (e.pan[0] - d.x / view.scale) as f64 });
            let _ = app.dispatch(Command::SetUnwrapOption { name: "Pan V".into(), value: (e.pan[1] + d.y / view.scale) as f64 });
        }
        app.uv_drag = Some(UvDrag::Pan { last: p });
        return;
    }
    if matches!(app.uv_drag, Some(UvDrag::Pan { .. })) && !pan_drag {
        app.uv_drag = None;
    }
    // Right-click quad: the common tools. Shown every frame (not only on the
    // right-click), or the menu would vanish the frame after it opened.
    response.clone().context_menu(|ui| {
        item(ui, app, "Break", Command::BreakSelectedVertices);
        item(ui, app, "Weld Selected", Command::TextureVertexWeldSelected);
        item(ui, app, "Target Weld", Command::TextureVertexTargetWeld);
        item(ui, app, "Detach Edge Verts", Command::DetachEdgeVerts);
        item(ui, app, "Stitch Selected", Command::Stitch);
        ui.separator();
        item(ui, app, "Flip Horizontal", Command::FlipHorizontal);
        item(ui, app, "Flip Vertical", Command::FlipVertical);
        item(ui, app, "Mirror Horizontal", Command::MirrorHorizontal);
        item(ui, app, "Mirror Vertical", Command::MirrorVertical);
        ui.separator();
        item(ui, app, "Relax", Command::RelaxUnwrap);
        item(ui, app, "Pack UVs", Command::Pack);
        item(ui, app, "Flatten Mapping", Command::FlattenMap);
    });
    if e.paint_select && e.plane == 0 {
        paint_select(ui, app, response, view, &pointer, mods, p);
        return;
    }
    if e.sketch && e.plane == 0 {
        sketch(ui, app, response, view, mesh, ch, data, &pointer, p);
        return;
    }
    app.uv_sketch_picked = false;
    if e.mode == TvMode::Freeform && e.plane == 0 && !data.tv_selection.is_empty() && freeform(ui, app, response, view, mesh, ch, data, &pointer, mods, p) {
        return;
    }
    if pointer.primary_pressed() && response.hovered() {
        let add = mods.ctrl;
        let subtract = mods.alt;
        let op = if add { SelectOp::Add } else if subtract { SelectOp::Subtract } else { SelectOp::Replace };
        // Target Weld: two clicks on tvs.
        if matches!(app.state.pick_mode, Some(loom_commands::PickMode::TvTargetWeldSource | loom_commands::PickMode::TvTargetWeldTarget)) {
            if let Some(t) = nearest_tv(view, mesh, ch, data, p, 8.0) {
                if app.state.pick_mode == Some(loom_commands::PickMode::TvTargetWeldSource) {
                    let node = app.state.unwrap_entry().map(|(id, _)| id).unwrap_or(loom_scene::NodeId(0));
                    app.state.pick_source = Some((node, t));
                    app.state.pick_mode = Some(loom_commands::PickMode::TvTargetWeldTarget);
                } else if let Some((_, from)) = app.state.pick_source.take() {
                    let _ = app.dispatch(Command::TargetWeldTv { from, to: t });
                    app.state.pick_mode = Some(loom_commands::PickMode::TvTargetWeldSource);
                }
            }
            return;
        }
        let hit_selected = match e.level {
            TvLevel::Vertex => nearest_tv(view, mesh, ch, data, p, 8.0).map(|t| (t, data.tv_selection.contains(&t))),
            TvLevel::Edge => nearest_edge(view, mesh, ch, p, 6.0).map(|(a, b)| (a, data.tv_edge_selection.contains(&(a, b)))),
            TvLevel::Face => face_at(view, mesh, ch, p).map(|f| (f.0, data.face_selection.contains(&f.0) || uw::face_tvs(mesh, ch, f).iter().all(|t| data.tv_selection.contains(t)))),
        };
        match hit_selected {
            Some((_, true)) if !add && !subtract && e.mode != TvMode::Pan => {
                // A drag on the selection transforms it.
                let centre = uw::centre(mesh, ch, &data.tv_selection) + glam::Vec2::from(e.pivot);
                let pivot = view.to_screen(egui::vec2(centre.x, centre.y));
                app.uv_drag = Some(UvDrag::Transform { start: p, last: p, pivot });
                app.state.undo.begin(match e.mode { TvMode::Rotate => "Rotate", TvMode::Scale | TvMode::ScaleHorizontal | TvMode::ScaleVertical => "Scale", _ => "Move" });
            }
            Some((hit, _)) => {
                let _ = match e.level {
                    TvLevel::Vertex => app.dispatch(Command::SelectTv { ids: vec![hit], op }),
                    TvLevel::Edge => {
                        let edge = nearest_edge(view, mesh, ch, p, 6.0).unwrap_or((hit, hit));
                        app.dispatch(Command::SelectTvEdges { edges: vec![[edge.0, edge.1]], op })
                    }
                    TvLevel::Face => app.dispatch(Command::SelectTvFaces { faces: vec![hit], op }),
                };
                if e.sync_selection && e.level == TvLevel::Face {
                    let _ = app.dispatch(Command::SyncTextureSelectionToViewport);
                }
            }
            None => app.uv_drag = Some(UvDrag::Region { start: p, add, subtract }),
        }
    }
    if pointer.primary_down() {
        if let Some(UvDrag::Transform { start, last, pivot }) = app.uv_drag {
            if (p - last).length() > 0.0 {
                match e.mode {
                    TvMode::Rotate => {
                        let a0 = (last - pivot).angle();
                        let a1 = (p - pivot).angle();
                        let deg = -(a1 - a0).to_degrees();
                        let _ = app.dispatch(Command::RotateTv { degrees: deg });
                    }
                    TvMode::Scale | TvMode::ScaleHorizontal | TvMode::ScaleVertical => {
                        let d0 = (last - pivot).length().max(1.0);
                        let d1 = (p - pivot).length().max(1.0);
                        let f = d1 / d0;
                        let factor = match e.mode { TvMode::ScaleHorizontal => [f, 1.0], TvMode::ScaleVertical => [1.0, f], _ => [f, f] };
                        let _ = app.dispatch(Command::ScaleTv { factor });
                    }
                    _ => {
                        let mut d = view.to_uv(p) - view.to_uv(last);
                        if e.edge_snap && !e.grid_snap && !e.vertex_snap {
                            // Edge Snap: the selection's centre lands on the texture edge under the cursor.
                            let centre = uw::centre(mesh, ch, &data.tv_selection);
                            let target = glam::Vec2::new(centre.x + d.x, centre.y + d.y);
                            if let Some(q) = uw::snap_to_edge(mesh, ch, &data.tv_selection, target, 8.0 / view.scale) {
                                d = egui::vec2(q.x - centre.x, q.y - centre.y);
                            }
                        }
                        if e.grid_snap || e.vertex_snap {
                            let centre = uw::centre(mesh, ch, &data.tv_selection);
                            let target = egui::vec2(centre.x, centre.y) + d;
                            let snapped = if e.grid_snap {
                                let s = e.grid_size.max(0.001);
                                Some(egui::vec2((target.x / s).round() * s, (target.y / s).round() * s))
                            } else {
                                let cursor = view.to_uv(p);
                                let mut best: Option<(f32, egui::Vec2)> = None;
                                if let Some(m) = mesh.map(ch) {
                                    for (i, v) in m.verts.iter().enumerate() {
                                        if data.tv_selection.contains(&(i as u32)) {
                                            continue;
                                        }
                                        let q = view.pair(*v);
                                        let dist = (view.to_screen(q) - p).length();
                                        if dist < 8.0 && best.is_none_or(|(bd, _)| dist < bd) {
                                            best = Some((dist, q));
                                        }
                                    }
                                }
                                best.map(|(_, q)| egui::vec2(centre.x, centre.y) + (q - cursor))
                            };
                            if let Some(s) = snapped {
                                d = s - egui::vec2(centre.x, centre.y);
                            }
                        }
                        let delta = match e.mode { TvMode::MoveHorizontal => [d.x, 0.0], TvMode::MoveVertical => [0.0, d.y], _ => [d.x, d.y] };
                        let _ = app.dispatch(Command::MoveTv { delta });
                    }
                }
                app.uv_drag = Some(UvDrag::Transform { start, last: p, pivot });
            }
        }
        if let Some(UvDrag::Region { start, .. }) = app.uv_drag {
            let r = egui::Rect::from_two_pos(start, p);
            ui.painter().rect_stroke(r, 0.0, egui::Stroke::new(1.0_f32, egui::Color32::from_gray(230)), egui::StrokeKind::Middle);
        }
    }
    if pointer.primary_released() {
        match app.uv_drag.take() {
            Some(UvDrag::Transform { .. }) => app.state.undo.end(),
            Some(UvDrag::Region { start, add, subtract }) => {
                let r = egui::Rect::from_two_pos(start, p);
                let op = if add { SelectOp::Add } else if subtract { SelectOp::Subtract } else { SelectOp::Replace };
                if r.width() > 2.0 || r.height() > 2.0 {
                    let (a, b) = (view.to_uv(r.min), view.to_uv(r.max));
                    let _ = app.dispatch(Command::SelectTvRegion { min: [a.x.min(b.x), a.y.min(b.y)], max: [a.x.max(b.x), a.y.max(b.y)], op });
                } else if op == SelectOp::Replace {
                    let _ = app.dispatch(Command::SelectNoneTv);
                }
            }
            _ => {}
        }
    }
}

fn bottom_bar(ui: &mut egui::Ui, app: &mut LoomApp, mesh: &Mesh, ch: u32, data: &UnwrapUvw) {
    let e = app.state.uv_editor.clone();
    ui.horizontal_wrapped(|ui| {
        // U / V / W of the selection (its centre): typing moves the selection there.
        let centre = if data.tv_selection.is_empty() { glam::Vec2::ZERO } else { uw::centre(mesh, ch, &data.tv_selection) };
        let w = data.tv_selection.iter().next().and_then(|t| mesh.map(ch).and_then(|m| m.verts.get(*t as usize).map(|v| v.z))).unwrap_or(0.0);
        for (label, val, key) in [("U:", centre.x, "u"), ("V:", centre.y, "v"), ("W:", w, "w")] {
            ui.label(label);
            let mut v = val;
            let r = ui.add(egui::DragValue::new(&mut v).speed(0.005).fixed_decimals(3));
            if r.changed() && (v - val).abs() > 1e-6 && !data.tv_selection.is_empty() {
                let delta = v - val;
                let _ = match key {
                    "u" => app.dispatch(Command::MoveTv { delta: [delta, 0.0] }),
                    "v" => app.dispatch(Command::MoveTv { delta: [0.0, delta] }),
                    _ => app.dispatch(Command::SetTvPosition { u: None, v: None, w: Some(v) }),
                };
            }
        }
        ui.separator();
        if ui.selectable_label(e.lock_selected, "Lock").on_hover_text("Lock Selected Vertices (Space)").clicked() {
            let _ = app.dispatch(Command::LockSelectedVertices);
        }
        if ui.selectable_label(e.filter_selected_polygons, "Filter Sel.").clicked() {
            let _ = app.dispatch(Command::FilterSelectedPolygons);
        }
        if ui.button("Hide").clicked() {
            let _ = app.dispatch(Command::HideSelectedUnwrap);
        }
        if ui.button("Unhide All").clicked() {
            let _ = app.dispatch(Command::UnhideAllUnwrap);
        }
        if ui.button("Freeze").clicked() {
            let _ = app.dispatch(Command::FreezeSelected);
        }
        if ui.button("Unfreeze All").clicked() {
            let _ = app.dispatch(Command::UnfreezeAllUnwrap);
        }
        ui.separator();
        if ui.button("Zoom Extents").clicked() {
            let _ = app.dispatch(Command::ZoomExtentsUv);
        }
        if ui.button("Zoom Sel.").clicked() {
            let _ = app.dispatch(Command::ZoomExtentsSelectedUv);
        }
        if ui.selectable_label(e.snap, "Snap").clicked() {
            let _ = app.dispatch(Command::Snap);
        }
    });
    ui.horizontal_wrapped(|ui| {
        for (label, level, cmd) in [("Vertex", TvLevel::Vertex, Command::TvVertexSubobjectMode), ("Edge", TvLevel::Edge, Command::TvEdgeSubobjectMode), ("Face", TvLevel::Face, Command::TvPolygonSubobjectMode)] {
            if ui.selectable_label(e.level == level, label).clicked() {
                let _ = app.dispatch(cmd);
            }
        }
        ui.separator();
        if ui.button("+").on_hover_text("Expand Selection").clicked() {
            let _ = app.dispatch(Command::TextureVertexExpandSelection);
        }
        if ui.button("-").on_hover_text("Contract Selection").clicked() {
            let _ = app.dispatch(Command::TextureVertexContractSelection);
        }
        if ui.selectable_label(e.select_element, "Select Element").clicked() {
            let _ = app.dispatch(Command::TvElementMode);
        }
        if ui.selectable_label(e.sync_selection, "Sync").clicked() {
            let _ = app.dispatch(Command::SyncSelectionMode);
        }
        ui.separator();
        if ui.button("Rotate +90").clicked() {
            let _ = app.dispatch(Command::RotatePlus90);
        }
        if ui.button("Rotate -90").clicked() {
            let _ = app.dispatch(Command::RotateMinus90);
        }
        if ui.button("Mirror H").clicked() {
            let _ = app.dispatch(Command::MirrorHorizontal);
        }
        if ui.button("Mirror V").clicked() {
            let _ = app.dispatch(Command::MirrorVertical);
        }
        if ui.button("Align H").clicked() {
            let _ = app.dispatch(Command::AlignSelectionHorizontal);
        }
        if ui.button("Align V").clicked() {
            let _ = app.dispatch(Command::AlignSelectionVertical);
        }
        ui.separator();
        if ui.selectable_label(e.soft_selection, "Soft Sel.").clicked() {
            let _ = app.dispatch(Command::SoftSelection);
        }
        let mut falloff = e.soft_falloff;
        if ui.add(egui::DragValue::new(&mut falloff).speed(0.01).range(0.0..=10.0).fixed_decimals(3).prefix("Falloff ")).changed() {
            let _ = app.dispatch(Command::SetUnwrapOption { name: "Soft Selection Falloff".into(), value: falloff as f64 });
        }
    });
}

/// Paint Select Mode: the brush (Paint Select size in pixels) adds the tvs
/// under it while the button is down; Alt removes.
#[allow(clippy::too_many_arguments)]
fn paint_select(ui: &mut egui::Ui, app: &mut LoomApp, response: &egui::Response, view: &View, pointer: &egui::PointerState, mods: egui::Modifiers, p: egui::Pos2) {
    let e = &app.state.uv_editor;
    let (size, radius) = (e.paint_size, e.paint_size / view.scale);
    if let Some(h) = pointer.hover_pos().filter(|h| view.rect.contains(*h)) {
        ui.painter().circle_stroke(h, size, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(255, 220, 60)));
    }
    if pointer.primary_pressed() && response.hovered() {
        app.state.undo.begin("Paint Select");
        app.uv_drag = Some(UvDrag::Paint);
    }
    if matches!(app.uv_drag, Some(UvDrag::Paint)) {
        if pointer.primary_down() {
            let uv = view.to_uv(p);
            let op = if mods.alt { SelectOp::Subtract } else { SelectOp::Add };
            let _ = app.dispatch(Command::PaintSelectTv { points: vec![[uv.x, uv.y]], radius, op });
        }
        if pointer.primary_released() {
            app.uv_drag = None;
            app.state.undo.end();
        }
    }
}

/// Sketch: with Select By Pick / Drag Pick, clicks (drags) on tvs pick them
/// in order; a drag from empty space draws the shape the picked (or
/// selected) tvs line up along on release (live with Interactive Mode).
#[allow(clippy::too_many_arguments)]
fn sketch(ui: &mut egui::Ui, app: &mut LoomApp, response: &egui::Response, view: &View, mesh: &Mesh, ch: u32, data: &UnwrapUvw, pointer: &egui::PointerState, p: egui::Pos2) {
    let e = app.state.uv_editor.clone();
    let painter = ui.painter();
    let colour = egui::Color32::from_rgb(255, 220, 60);
    let uv = view.to_uv(p);
    if pointer.primary_pressed() && response.hovered() {
        let on_tv = if e.sketch_select_by < 2 { nearest_tv(view, mesh, ch, data, p, 8.0) } else { None };
        if let Some(t) = on_tv {
            let op = if app.uv_sketch_picked { SelectOp::Add } else { SelectOp::Replace };
            let _ = app.dispatch(Command::SelectTv { ids: vec![t], op });
            app.uv_sketch_picked = true;
            if e.sketch_select_by == 1 {
                app.uv_drag = Some(UvDrag::SketchPick);
            }
        } else {
            app.uv_sketch = vec![[uv.x, uv.y]];
            app.uv_drag = Some(UvDrag::Sketch { grouped: e.sketch_interactive });
            if e.sketch_interactive {
                app.state.undo.begin("Sketch");
            }
        }
    }
    match app.uv_drag {
        Some(UvDrag::SketchPick) if pointer.primary_down() => {
            if let Some(t) = nearest_tv(view, mesh, ch, data, p, e.paint_size.max(4.0)) {
                if !data.tv_selection.contains(&t) {
                    let _ = app.dispatch(Command::SelectTv { ids: vec![t], op: SelectOp::Add });
                }
            }
        }
        Some(UvDrag::Sketch { .. }) if pointer.primary_down() => {
            let last = app.uv_sketch.last().copied().unwrap_or([uv.x, uv.y]);
            if view.to_screen(egui::vec2(last[0], last[1])).distance(p) > 2.0 {
                if e.sketch_align == 0 {
                    app.uv_sketch.push([uv.x, uv.y]);
                } else {
                    app.uv_sketch.truncate(1);
                    app.uv_sketch.push([uv.x, uv.y]);
                }
                if e.sketch_interactive {
                    let points = app.uv_sketch.clone();
                    let _ = app.dispatch(Command::SketchTv { tvs: None, align: e.sketch_align, points });
                    app.state.uv_editor.sketch = true;
                }
            }
        }
        _ => {}
    }
    if pointer.primary_released() {
        match app.uv_drag.take() {
            Some(UvDrag::Sketch { grouped }) => {
                let points = std::mem::take(&mut app.uv_sketch);
                if grouped {
                    app.state.undo.end();
                    app.state.uv_editor.sketch = false;
                } else if points.len() >= 2 || e.sketch_align == 0 {
                    let _ = app.dispatch(Command::SketchTv { tvs: None, align: e.sketch_align, points });
                }
                app.uv_sketch_picked = false;
            }
            other => app.uv_drag = other.filter(|d| !matches!(d, UvDrag::SketchPick)),
        }
    }
    // The stroke as it will be used.
    let pts: Vec<egui::Pos2> = app.uv_sketch.iter().map(|q| view.to_screen(egui::vec2(q[0], q[1]))).collect();
    let stroke = egui::Stroke::new(1.5_f32, colour);
    match (e.sketch_align, pts.as_slice()) {
        (0, _) if pts.len() >= 2 => {
            painter.add(egui::Shape::line(pts.clone(), stroke));
        }
        (1, [a, .., b]) => {
            painter.line_segment([*a, *b], stroke);
        }
        (2, [a, .., b]) => {
            painter.rect_stroke(egui::Rect::from_two_pos(*a, *b), 0.0, stroke, egui::StrokeKind::Middle);
        }
        (3, [a, .., b]) => {
            painter.circle_stroke(*a, a.distance(*b), stroke);
        }
        _ => {}
    }
    if e.sketch_show_order {
        let order: Vec<u32> = e.sketch_order.iter().copied().filter(|t| data.tv_selection.contains(t)).collect();
        if let Some(m) = mesh.map(ch) {
            for (i, t) in order.iter().enumerate() {
                if let Some(v) = m.verts.get(*t as usize) {
                    painter.text(view.to_screen(view.pair(*v)) + egui::vec2(5.0, -5.0), egui::Align2::LEFT_BOTTOM, (i + 1).to_string(), egui::FontId::proportional(11.0), colour);
                }
            }
        }
    }
}

/// Freeform mode: the gizmo box round the selection. Corners scale (Ctrl:
/// per axis), edge middles rotate, the cross moves the pivot (Freeform Snap:
/// onto tvs and the box's corners, middles and centre), inside moves.
/// Returns true when the gizmo took the input.
#[allow(clippy::too_many_arguments)]
fn freeform(ui: &mut egui::Ui, app: &mut LoomApp, response: &egui::Response, view: &View, mesh: &Mesh, ch: u32, data: &UnwrapUvw, pointer: &egui::PointerState, mods: egui::Modifiers, p: egui::Pos2) -> bool {
    let e = app.state.uv_editor.clone();
    let (lo, hi) = uw::bounds(mesh, ch, Some(&data.tv_selection));
    let centre = uw::centre(mesh, ch, &data.tv_selection);
    let pivot = centre + glam::Vec2::from(e.pivot);
    let s = |q: glam::Vec2| view.to_screen(egui::vec2(q.x, q.y));
    let painter = ui.painter();
    let col = egui::Color32::from_rgb(255, 160, 40);
    painter.rect_stroke(egui::Rect::from_two_pos(s(lo), s(hi)), 0.0, egui::Stroke::new(1.0_f32, col), egui::StrokeKind::Middle);
    let corners = [lo, glam::Vec2::new(hi.x, lo.y), hi, glam::Vec2::new(lo.x, hi.y)];
    for c in corners {
        painter.rect_filled(egui::Rect::from_center_size(s(c), egui::vec2(6.0, 6.0)), 0.0, col);
    }
    for i in 0..4 {
        let m = (corners[i] + corners[(i + 1) % 4]) * 0.5;
        painter.circle_stroke(s(m), 3.5, egui::Stroke::new(1.0_f32, col));
    }
    let uv = view.to_uv(p);
    let at = glam::Vec2::new(uv.x, uv.y);
    if pointer.primary_pressed() && response.hovered() {
        let hit = loom_commands::freeform_hit(lo, hi, pivot, at, 6.0 / view.scale, e.allow_select_inside_gizmo);
        if hit == FreeformHit::Outside {
            return false;
        }
        app.state.undo.begin(match hit { FreeformHit::Scale { .. } => "Scale", FreeformHit::Rotate => "Rotate", FreeformHit::Pivot => "Move Pivot", _ => "Move" });
        app.uv_drag = Some(UvDrag::Freeform { hit, last: p });
        return true;
    }
    let Some(UvDrag::Freeform { hit, last }) = app.uv_drag else { return false };
    if pointer.primary_down() && (p - last).length() > 0.0 {
        let prev = view.to_uv(last);
        let prev = glam::Vec2::new(prev.x, prev.y);
        match hit {
            FreeformHit::Move => {
                let d = at - prev;
                let _ = app.dispatch(Command::MoveTv { delta: [d.x, d.y] });
            }
            FreeformHit::Scale { .. } => {
                let (a, b) = (prev - pivot, at - pivot);
                let factor = if mods.ctrl {
                    let f = |n: f32, d: f32| if d.abs() < 1e-6 { 1.0 } else { n / d };
                    [f(b.x, a.x), f(b.y, a.y)]
                } else {
                    let f = b.length() / a.length().max(1e-6);
                    [f, f]
                };
                let _ = app.dispatch(Command::ScaleTv { factor });
            }
            FreeformHit::Rotate => {
                let deg = (at - pivot).to_angle() - (prev - pivot).to_angle();
                let _ = app.dispatch(Command::RotateTv { degrees: deg.to_degrees() });
            }
            FreeformHit::Pivot => {
                let mut target = at;
                if e.freeform_snap {
                    let mut cands: Vec<glam::Vec2> = corners.to_vec();
                    cands.push((lo + hi) * 0.5);
                    cands.extend((0..4).map(|i| (corners[i] + corners[(i + 1) % 4]) * 0.5));
                    if let Some(m) = mesh.map(ch) {
                        cands.extend(m.verts.iter().map(|v| glam::Vec2::new(v.x, v.y)));
                    }
                    if let Some(c) = cands.into_iter().filter(|c| s(*c).distance(p) <= 8.0).min_by(|a, b| s(*a).distance(p).total_cmp(&s(*b).distance(p))) {
                        target = c;
                    }
                }
                let off = target - centre;
                let _ = app.dispatch(Command::SetUnwrapOption { name: "Pivot U".into(), value: off.x as f64 });
                let _ = app.dispatch(Command::SetUnwrapOption { name: "Pivot V".into(), value: off.y as f64 });
            }
            FreeformHit::Outside => {}
        }
        app.uv_drag = Some(UvDrag::Freeform { hit, last: p });
    }
    if pointer.primary_released() {
        app.uv_drag = None;
        app.state.undo.end();
    }
    true
}

/// Tools > Sketch Vertices Options: the Sketch Tool dialog.
fn sketch_dialog(ctx: &egui::Context, app: &mut LoomApp) {
    if !app.state.uv_editor.sketch_dialog {
        return;
    }
    let e = app.state.uv_editor.clone();
    let mut open = true;
    let opt = |app: &mut LoomApp, name: &str, v: f64| {
        let _ = app.dispatch(Command::SetUnwrapOption { name: name.into(), value: v });
    };
    egui::Window::new("Sketch Tool").open(&mut open).resizable(false).show(ctx, |ui| {
        ui.label("Select By:");
        ui.horizontal(|ui| {
            for (i, l) in ["Pick", "Drag Pick", "Use Current Selection"].iter().enumerate() {
                if ui.radio(e.sketch_select_by == i as u32, *l).clicked() {
                    opt(app, "Sketch Select By", i as f64);
                }
            }
        });
        ui.label("Align To:");
        ui.horizontal(|ui| {
            for (i, l) in ["Free Form", "Line", "Box", "Circle"].iter().enumerate() {
                if ui.radio(e.sketch_align == i as u32, *l).clicked() {
                    opt(app, "Sketch Align", i as f64);
                }
            }
        });
        let mut show = e.sketch_show_order;
        if ui.checkbox(&mut show, "Show Vertex Order").changed() {
            opt(app, "Sketch Show Vertex Order", if show { 1.0 } else { 0.0 });
        }
        let mut inter = e.sketch_interactive;
        if ui.checkbox(&mut inter, "Interactive Mode").changed() {
            opt(app, "Sketch Interactive Mode", if inter { 1.0 } else { 0.0 });
        }
        ui.horizontal(|ui| {
            ui.label("Drag Cursor Size:");
            let mut size = e.paint_size;
            if ui.add(egui::DragValue::new(&mut size).speed(0.5).range(1.0..=200.0)).changed() {
                opt(app, "Paint Select Size", size as f64);
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Apply").on_hover_text("Start sketching").clicked() && !app.state.uv_editor.sketch {
                let _ = app.dispatch(Command::Sketch);
            }
            if ui.button("Reverse Vertex Order").clicked() {
                let _ = app.dispatch(Command::SketchReverseVertexOrder);
            }
        });
    });
    if !open {
        app.state.uv_editor.sketch_dialog = false;
    }
}
